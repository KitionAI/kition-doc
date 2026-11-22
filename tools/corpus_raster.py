"""Choose native sampling from verified reference density, without changing fidelity criteria."""
import math

SAMPLING_METHOD = 'reference-png-density-when-consistent-otherwise-144dpi'
DEFAULT_SCALE = 2.0


def select_scale(folder, pages, reference_pdf):
    from PIL import Image
    fallback = DEFAULT_SCALE, 'default-144dpi'
    if not pages or reference_pdf is None:
        return fallback
    density = None
    for item in pages:
        number = item['number']
        if not 1 <= number <= len(reference_pdf.pages):
            return fallback
        with Image.open(folder / f'wps-{number:04}.png') as image:
            dpi = image.info.get('dpi')
            if not isinstance(dpi, (tuple, list)) or len(dpi) != 2:
                return fallback
            try:
                horizontal, vertical = (float(value) for value in dpi)
            except (TypeError, ValueError, OverflowError):
                return fallback
            if not (math.isfinite(horizontal) and math.isfinite(vertical)
                    and horizontal == vertical and 0.1 <= horizontal / 72 <= 2):
                return fallback
            page = reference_pdf.pages[number - 1]
            width, height = float(page.mediabox.width), float(page.mediabox.height)
            if page.rotation % 180:
                width, height = height, width
            if not all(math.isfinite(value) and value > 0 for value in (width, height)):
                return fallback
            if abs(image.width - width * horizontal / 72) > 1 or abs(image.height - height * vertical / 72) > 1:
                return fallback
            if density is not None and horizontal != density:
                return fallback
            density = horizontal
    return density / 72, 'reference-png-density'


def reported_scale(layout, requested):
    try:
        actual = float(layout.get('scale'))
    except (TypeError, ValueError, OverflowError) as error:
        raise ValueError('renderer scale metadata is missing or invalid') from error
    # The CLI stores its requested scale as f32. This check allows that conversion only;
    # image geometry and foreground similarity retain their independent strict gates.
    if not math.isfinite(actual) or not math.isclose(actual, requested, rel_tol=1e-6, abs_tol=1e-6):
        raise ValueError(f'renderer scale {actual} disagrees with requested scale {requested}')
    return actual
