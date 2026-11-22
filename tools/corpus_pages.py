"""Render and compare one immutable WPS corpus document."""
import hashlib
import html
import json
import subprocess

from corpus_fidelity import compare_page_text, document_matches, normalize_page
from corpus_raster import reported_scale, select_scale


def digest(data):
    return hashlib.sha256(data).hexdigest()


def compare_case(case, corpus, output, renderer, pages, compare_images):
    from PIL import Image, ImageChops
    from pypdf import PdfReader
    identifier = case['id']
    folder = output / identifier
    folder.mkdir(exist_ok=True)
    result = {'id': identifier, 'reference_pages': len(case['images']), 'reference_complete': case['reference_complete']}
    try:
        for name, expected in case['hashes'].items():
            if digest((corpus / identifier / name).read_bytes()) != expected:
                raise ValueError('corpus checksum mismatch: ' + name)
        reference_pdf = PdfReader(corpus / identifier / 'reference.WPS.pdf') if case['pdf'] else None
        requested_scale, scale_method = select_scale(corpus / identifier, case['images'], reference_pdf)
        rendered = subprocess.run([str(renderer), 'render-pages', str(corpus / identifier / 'source.docx'), str(folder), str(pages), str(requested_scale)],
                                  capture_output=True, text=True, timeout=max(60, pages * 5))
        if rendered.returncode:
            raise ValueError(rendered.stderr.strip() or 'renderer failed')
        layout = json.loads((folder / 'layout.json').read_text())
        scale = reported_scale(layout, requested_scale)
        result.update(raster_scale=scale, raster_scale_method=scale_method)
        result['reference_pdf_pages'] = len(reference_pdf.pages) if reference_pdf else None
        result['reference_complete'] = case['reference_complete'] and (reference_pdf is None or len(reference_pdf.pages) == len(case['images']))
        result['actual_pages'] = layout['page_count']
        result['page_delta'] = layout['page_count'] - len(case['images'])
        result['pages'] = []
        for page in case['images'][:pages]:
            number = page['number']
            actual_path = folder / f'page-{number:04}.png'
            if number > layout['page_count'] or not actual_path.exists():
                result['pages'].append({'page': number, 'missing': True, 'ink_f1': 0})
                continue
            with Image.open(corpus / identifier / f'wps-{number:04}.png') as reference, Image.open(actual_path) as actual:
                actual.info['dpi'] = (72 * scale, 72 * scale)
                reference_size, actual_size = None, None
                if reference_pdf and number <= len(reference_pdf.pages):
                    reference_page = reference_pdf.pages[number - 1]
                    width, height = float(reference_page.mediabox.width), float(reference_page.mediabox.height)
                    if reference_page.rotation % 180:
                        width, height = height, width
                    actual_page = layout['pages'][number - 1]
                    reference_size = width, height
                    actual_size = actual_page['width_pt'], actual_page['height_pt']
                metrics = compare_images(reference, actual, reference_size=reference_size, actual_size=actual_size)
                if reference_size:
                    metrics['width_error_pt'] = round(abs(actual_page['width_pt'] - width), 4)
                    metrics['height_error_pt'] = round(abs(actual_page['height_pt'] - height), 4)
                    metrics.update(compare_page_text(reference_page.extract_text(), [span['text'] for span in actual_page['text_spans']]))
                result['pages'].append({'page': number, **metrics})
                # Difference previews use the same physical page proportions as the metrics.
                ref = normalize_page(reference, reference_size, width=600)
                act = normalize_page(actual, actual_size, width=600)
                height = max(ref.height, act.height)
                left = Image.new('RGB', (600, height), 'white'); left.paste(ref, (0, 0))
                right = Image.new('RGB', (600, height), 'white'); right.paste(act, (0, 0))
                preview = Image.new('RGB', (1800, height), 'white')
                preview.paste(left, (0, 0)); preview.paste(right, (600, 0))
                preview.paste(ImageChops.difference(left, right).point(lambda value: min(255, value * 3)), (1200, 0))
                preview.save(folder / f'compare-{number:04}.png')
        result['mean_ink_f1'] = round(sum(p['ink_f1'] for p in result['pages']) / max(1, len(result['pages'])), 6)
        result['minimum_similarity'] = min((p.get('similarity', 0) for p in result['pages']), default=0)
        result['minimum_layout_f1'] = min((p.get('layout_f1', 0) for p in result['pages']), default=0)
        result['maximum_tone_rmse'] = max((p.get('tone_rmse', 1) for p in result['pages']), default=1)
        result['same_page_text'] = sum(p.get('page_text_equal') is True for p in result['pages'])
        result['status'] = 'match' if document_matches(result) else 'difference'
        page_links = []
        for page in result['pages']:
            number = page['page']
            preview = (f"<img loading='lazy' width='1200' src='compare-{number:04}.png' alt='WPS, Kition, and pixel difference'>"
                       if not page.get('missing') else '<p>The corresponding Kition page is missing.</p>')
            page_links.append(f"<h2>Page {number}</h2><pre>{html.escape(json.dumps(page, indent=2))}</pre>{preview}")
        (folder / 'index.html').write_text('<!doctype html><meta charset="utf-8"><title>' + identifier + '</title><style>body{font:14px system-ui;margin:24px}img{max-width:100%;height:auto;border:1px solid #ddd}</style><h1>' + identifier + '</h1><p>Left: WPS. Centre: Kition. Right: amplified pixel difference.</p>' + ''.join(page_links))
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        result.update(status='error', error=str(error))
    return result
