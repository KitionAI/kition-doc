"""Strict page fidelity criteria, independent of corpus inventory and rendering."""
from collections import Counter
import hashlib
import json
import math
import unicodedata

# Strict pixel similarity, reported for diagnosis only.
MINIMUM_SIMILARITY = 0.999
COMPARISON_WIDTH = 1200

# Acceptance ignores font rendering (maintainer decision): glyph shapes, stroke weight and
# anti-aliasing never decide a page; identical page counts, page numbers, text allocation and the
# absence of large layout differences do. The thresholds were calibrated so that WPS's own
# reference PDF, rasterized by an independent renderer (identical layout, different font
# rendering), passes, while content displaced by half a line, a missing watermark or a missing
# highlight fails. Ink is any pixel darker than 180 in luminance, so dark and coloured text count
# as ink and light fills, highlights and watermarks do not.
LAYOUT_INK = 180
# Content occupancy: 8-pixel cells (about 4 pt at the comparison width), one cell of tolerance.
LAYOUT_CELL = 8
MINIMUM_LAYOUT_F1 = 0.98
# Background tone: mean colour of 48-pixel blocks (about 24 pt), ignoring pixels within 3 pixels
# of thin ink (glyph strokes) in either page; solid ink areas of 5 by 5 cells or more stay in.
TONE_BLOCK = 48
TONE_INK_MARGIN = 3
MAXIMUM_TONE_RMSE = 0.03


def normalize_page(image, physical_size=None, width=COMPARISON_WIDTH):
    """Map physical coordinates without stretching integer-rounded raster edges."""
    from PIL import Image
    dpi = image.info.get('dpi')
    rgb = image.convert('RGB')
    aspect = physical_size[1] / physical_size[0] if physical_size else image.height / image.width
    dimensions = width, max(1, round(width * aspect))
    if physical_size and dpi and len(dpi) == 2:
        scales = [value / 72 for value in dpi]
        consistent = (all(math.isfinite(value) and value > 0 for value in scales)
                      and abs(image.width - physical_size[0] * scales[0]) <= 1
                      and abs(image.height - physical_size[1] * scales[1]) <= 1)
        if consistent:
            target_scale = width / physical_size[0]
            return rgb.transform(dimensions, Image.Transform.AFFINE,
                                 (scales[0] / target_scale, 0, 0, 0, scales[1] / target_scale, 0),
                                 resample=Image.Resampling.BICUBIC, fillcolor='white')
    return rgb.resize(dimensions, Image.Resampling.LANCZOS)


def foreground_similarity(a, b):
    """The worse of exact ink overlap and color agreement on the union of content."""
    import numpy as np
    ink_a, ink_b = np.min(a, axis=2) < 180, np.min(b, axis=2) < 180
    ink_total = int(ink_a.sum()) + int(ink_b.sum())
    overlap = float(2 * (ink_a & ink_b).sum() / ink_total) if ink_total else 1.0
    foreground = (np.min(a, axis=2) < 255) | (np.min(b, axis=2) < 255)
    color_rmse = float(np.sqrt(np.mean(((a[foreground] - b[foreground]) / 255) ** 2))) if foreground.any() else 0.0
    return {'similarity': min(overlap, 1 - color_rmse), 'exact_ink_f1': overlap,
            'foreground_rmse': color_rmse}


def layout_similarity(a, b):
    """Large-scale agreement that font rendering cannot change: where content is, and the tone
    of everything that is not a glyph stroke."""
    import numpy as np
    from PIL import Image, ImageFilter
    height = min(a.shape[0], b.shape[0])
    a, b = a[:height], b[:height]

    def ink(image):
        return image[..., 0] * 0.299 + image[..., 1] * 0.587 + image[..., 2] * 0.114 < LAYOUT_INK

    def coverage(mask):
        rows, cols = -(-mask.shape[0] // LAYOUT_CELL), -(-mask.shape[1] // LAYOUT_CELL)
        grid = np.zeros((rows * LAYOUT_CELL, cols * LAYOUT_CELL), bool)
        grid[:mask.shape[0], :mask.shape[1]] = mask
        return grid.reshape(rows, LAYOUT_CELL, cols, LAYOUT_CELL).mean(axis=(1, 3))

    def spread(grid, radius, any_of=True):
        padded = np.pad(grid, radius, constant_values=False)
        shifted = [padded[radius + dy:radius + dy + grid.shape[0], radius + dx:radius + dx + grid.shape[1]]
                   for dy in range(-radius, radius + 1) for dx in range(-radius, radius + 1)]
        return np.logical_or.reduce(shifted) if any_of else np.logical_and.reduce(shifted)

    masks = ink(a), ink(b)
    ref, act = (coverage(mask) > 0 for mask in masks)
    counts = int(ref.sum()), int(act.sum())
    recall = float((ref & spread(act, 1)).sum() / counts[0]) if counts[0] else float(counts[1] == 0)
    precision = float((act & spread(ref, 1)).sum() / counts[1]) if counts[1] else float(counts[0] == 0)
    f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0

    def strokes(mask):
        solid = spread(coverage(mask) >= 0.98, 2, any_of=False)
        solid = np.repeat(np.repeat(solid, LAYOUT_CELL, 0), LAYOUT_CELL, 1)[:mask.shape[0], :mask.shape[1]]
        size = 2 * TONE_INK_MARGIN + 1
        near = np.asarray(Image.fromarray(mask.astype('uint8') * 255).filter(ImageFilter.MaxFilter(size))) > 0
        return near & ~solid

    ignored = strokes(masks[0]) | strokes(masks[1])
    a, b = a.copy(), b.copy()
    a[ignored] = 255
    b[ignored] = 255

    def tone(image):
        rows, cols = image.shape[0] // TONE_BLOCK, image.shape[1] // TONE_BLOCK
        return image[:rows * TONE_BLOCK, :cols * TONE_BLOCK].reshape(rows, TONE_BLOCK, cols, TONE_BLOCK, 3).mean(axis=(1, 3))

    ta, tb = tone(a), tone(b)
    content = (np.min(ta, axis=2) < 254) | (np.min(tb, axis=2) < 254)
    tone_rmse = float(np.sqrt(np.mean(((ta[content] - tb[content]) / 255) ** 2))) if content.any() else 0.0
    return {'layout_f1': f1, 'tone_rmse': tone_rmse}


def compare_page_text(reference, spans):
    # PDF producers may emit labels and bidirectional runs in a different order.
    # Character multisets pin allocation to a physical page, including page-number
    # digits. Pixel geometry separately guards order and placement within that page.
    def characters(text):
        return Counter(c for c in unicodedata.normalize('NFC', text) if not c.isspace() and c.isprintable())
    expected, actual = characters(reference), characters(''.join(spans))
    def signature(counts):
        return hashlib.sha256(json.dumps(sorted(counts.items()), ensure_ascii=True).encode()).hexdigest()
    return {'page_text_equal': expected == actual, 'reference_text_sha256': signature(expected),
            'actual_text_sha256': signature(actual), 'reference_characters': sum(expected.values()),
            'actual_characters': sum(actual.values()), 'missing_characters': sum((expected - actual).values()),
            'extra_characters': sum((actual - expected).values())}


def page_matches(metrics):
    return (not metrics.get('missing') and metrics.get('layout_f1', 0) >= MINIMUM_LAYOUT_F1
            and metrics.get('tone_rmse', 1) <= MAXIMUM_TONE_RMSE
            and metrics.get('physical_aspect_error', metrics.get('aspect_error', 1)) < 0.0001
            and max(metrics.get('width_error_pt', 0), metrics.get('height_error_pt', 0)) <= 0.05)


def document_matches(result):
    pages = result.get('pages', [])
    return (result['reference_complete'] and result.get('page_delta') == 0
            and len(pages) == result['reference_pages']
            and all(page_matches(page) and page.get('page_text_equal') is True for page in pages))
