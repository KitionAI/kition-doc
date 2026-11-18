#!/usr/bin/env python3
"""Inventory immutable WPS references and compare them with Kition Doc page renders."""
import argparse
import hashlib
import html
import json
import re
import shutil
import sys
import zipfile
from pathlib import Path, PurePosixPath

from golden_fidelity import (COMPARISON_WIDTH, MAXIMUM_TONE_RMSE, MINIMUM_LAYOUT_F1, MINIMUM_SIMILARITY, foreground_similarity, layout_similarity,
                             normalize_page, page_matches)
from golden_provenance import compare_snapshots, source_snapshot
from golden_raster import SAMPLING_METHOD


def archive_name(info):
    if info.flag_bits & 0x800:
        return info.filename
    try:
        return info.filename.encode('cp437').decode('utf-8')
    except UnicodeError:
        return info.filename


def collect_cases(archive):
    with zipfile.ZipFile(archive) as stream:
        names = {archive_name(info): info for info in stream.infolist()
                 if not info.is_dir() and not info.filename.startswith('__MACOSX/')}
    cases = []
    for source in sorted(names):
        name = PurePosixPath(source).name
        if not name.lower().endswith('.docx') or '.PDFTOWORD.' in name.upper() or name.startswith('~$'):
            continue
        prefix = source[:-5] + '_WORD_TO_PNG_WPS/'
        images = []
        for candidate in names:
            if candidate.startswith(prefix) and candidate.lower().endswith('.png'):
                numbers = re.findall(r'\d+', PurePosixPath(candidate).stem)
                if numbers:
                    images.append({'number': int(numbers[-1]), 'source': candidate})
        images.sort(key=lambda page: page['number'])
        pdf = source[:-5] + '.WORDTOPDF.WPS.pdf'
        cases.append({'id': 'word-' + hashlib.sha256(source.encode()).hexdigest()[:12],
                      'source': source, 'images': images, 'pdf': pdf if pdf in names else None,
                      'reference_complete': bool(images) and [p['number'] for p in images] == list(range(1, len(images) + 1))})
    return cases


def digest(data):
    return hashlib.sha256(data).hexdigest()


def inventory(archive, corpus):
    corpus.mkdir(parents=True, exist_ok=True)
    cases = collect_cases(archive)
    with zipfile.ZipFile(archive) as stream:
        members = {archive_name(info): info for info in stream.infolist()}
        for case in cases:
            folder = corpus / case['id']
            folder.mkdir(exist_ok=True)
            items = [(case['source'], 'source.docx')]
            items += [(page['source'], f"wps-{page['number']:04}.png") for page in case['images']]
            if case['pdf']:
                items.append((case['pdf'], 'reference.WPS.pdf'))
            case['hashes'] = {}
            for original, local in items:
                data = stream.read(members[original])
                case['hashes'][local] = digest(data)
                target = folder / local
                if target.exists() and digest(target.read_bytes()) != digest(data):
                    raise ValueError(f"Refusing to overwrite changed corpus input: {case['id']}/{local}")
                if not target.exists():
                    target.write_bytes(data)
    manifest = {'format': 1, 'cases': cases}
    (corpus / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({'documents': len(cases), 'with_wps_images': sum(bool(c['images']) for c in cases),
                      'with_wps_pdf': sum(bool(c['pdf']) for c in cases), 'wps_pages': sum(len(c['images']) for c in cases)}))


def compare_images(reference, actual, reference_size=None, actual_size=None):
    import numpy as np
    from PIL import Image, ImageFilter
    # Match DPI, preserving page proportions: a wrong page size must not be stretched away.
    width = COMPARISON_WIDTH
    left, right = normalize_page(reference, reference_size), normalize_page(actual, actual_size)
    height = max(left.height, right.height)
    def padded(image):
        page = Image.new('RGB', (width, height), 'white')
        page.paste(image, (0, 0))
        return page
    left, right = padded(left), padded(right)
    a, b = np.asarray(left, dtype=np.float32), np.asarray(right, dtype=np.float32)
    masks = [np.min(array, axis=2) < 180 for array in (a, b)]
    dilated = [np.asarray(Image.fromarray(mask.astype('uint8') * 255).filter(ImageFilter.MaxFilter(5))) > 0 for mask in masks]
    counts = [int(mask.sum()) for mask in masks]
    recall = float((masks[0] & dilated[1]).sum() / counts[0]) if counts[0] else float(counts[1] == 0)
    precision = float((masks[1] & dilated[0]).sum() / counts[1]) if counts[1] else float(counts[0] == 0)
    f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
    def bounds(mask):
        ys, xs = np.where(mask)
        return [int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1] if len(xs) else None
    physical = ({'physical_aspect_error': abs(reference_size[0] / reference_size[1] - actual_size[0] / actual_size[1])}
                if reference_size and actual_size else {})
    return {**foreground_similarity(a, b), **layout_similarity(a, b), **physical, 'ink_f1': round(f1, 6), 'rmse': round(float(np.sqrt(np.mean(((a - b) / 255) ** 2))), 6),
            'changed_pixels': round(float((np.max(np.abs(a - b), axis=2) > 38).mean()), 6),
            'reference_ink_bounds': bounds(masks[0]), 'actual_ink_bounds': bounds(masks[1]),
            'aspect_error': round(abs(reference.width / reference.height - actual.width / actual.height), 6)}


def run(corpus, output, renderer, pages, limit, selected, jobs=1):
    if output == corpus or corpus in output.parents:
        raise ValueError('comparison output must be outside the immutable corpus')
    source_start = source_snapshot()
    renderer_hash = digest(renderer.read_bytes())
    output.mkdir(parents=True, exist_ok=True)
    pinned_renderer = output / ('renderer-' + renderer_hash)
    if not pinned_renderer.exists():
        shutil.copy2(renderer, pinned_renderer)
    if digest(pinned_renderer.read_bytes()) != renderer_hash:
        raise ValueError('renderer snapshot checksum mismatch')
    renderer = pinned_renderer
    manifest = json.loads((corpus / 'manifest.json').read_text())
    cases = [case for case in manifest['cases'] if case['images'] and (not selected or case['id'] in selected)]
    if limit:
        cases = cases[:limit]
    results = []
    from concurrent.futures import ThreadPoolExecutor
    from golden_pages import compare_case
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        compared = pool.map(lambda case: compare_case(case, corpus, output, renderer, pages, compare_images), cases)
        for index, result in enumerate(compared):
            results.append(result)
            print(f"[{index + 1}/{len(cases)}] {result['id']}: {result['status']}", flush=True)
    if not results:
        raise ValueError('no paired documents selected')
    summary = {'documents': len(results), 'matched': sum(r['status'] == 'match' for r in results),
               'same_page_count': sum(r.get('page_delta') == 0 and r['reference_complete'] for r in results),
               'errors': sum(r['status'] == 'error' for r in results)}
    source_end = source_snapshot()
    report = {'format': 1, 'summary': summary, 'source_provenance': compare_snapshots(source_start, source_end), 'renderer_sha256': renderer_hash,
              'sampled_pages_per_document': pages, 'comparison_width': COMPARISON_WIDTH,
              'acceptance': {'minimum_layout_f1': MINIMUM_LAYOUT_F1, 'maximum_tone_rmse': MAXIMUM_TONE_RMSE,
                             'method': 'content occupancy F1 on about 4 pt cells and background tone RMSE on about 24 pt blocks away from glyph strokes; font rendering ignored (maintainer decision)'},
              'diagnostic_minimum_similarity': MINIMUM_SIMILARITY, 'similarity_method': 'min(exact-ink-f1, 1-foreground-rmse), diagnostic only',
              'page_text_method': 'NFC non-whitespace character multiset per physical page',
              'raster_normalization': 'physical pixel density when consistent; proportional raster otherwise',
              'native_raster_sampling': SAMPLING_METHOD,
              'ink_neighborhood_pixels': 2, 'results': results}
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    rows = []
    for result in sorted(results, key=lambda r: (r.get('mean_ink_f1', -1), r['id'])):
        identifier = html.escape(result['id'])
        preview = f"<a href='{identifier}/index.html'>Compared pages and differences</a>" if result['status'] != 'error' else html.escape(result.get('error', ''))
        rows.append(f"<tr><td>{identifier}</td><td>{result['status']}</td><td>{result['reference_pages']}</td><td>{result.get('actual_pages', '-')}</td><td>{result.get('minimum_layout_f1', '-')}</td><td>{result.get('maximum_tone_rmse', '-')}</td><td>{result.get('minimum_similarity', '-')}</td><td>{result.get('same_page_text', '-')}</td><td>{preview}</td></tr>")
    (output / 'report.html').write_text('<!doctype html><meta charset="utf-8"><title>WPS layout comparison</title><style>body{font:15px system-ui;margin:32px}td,th{padding:8px;text-align:left;border-bottom:1px solid #ddd}</style><h1>WPS layout comparison</h1><p>' + html.escape(json.dumps(summary)) + '</p><p>Acceptance: identical page count, exact per-page text allocation including page numbers, and no large layout differences on any page (content occupancy F1 at least 0.98, background tone RMSE at most 0.03); font rendering is ignored. Pixel similarity is diagnostic only. Sampled runs cannot pass a complete document. WPS agreement is not a claim of Microsoft Word parity.</p><table><tr><th>Case</th><th>Status</th><th>WPS pages</th><th>Kition pages</th><th>Minimum layout F1</th><th>Maximum tone RMSE</th><th>Minimum pixel similarity (diagnostic)</th><th>Pages with equal text</th><th>Preview</th></tr>' + ''.join(rows) + '</table>')
    print(json.dumps(summary))
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    inventory_parser = commands.add_parser('inventory')
    inventory_parser.add_argument('--archive', type=Path, required=True)
    inventory_parser.add_argument('--corpus', type=Path, required=True)
    run_parser = commands.add_parser('run')
    run_parser.add_argument('--corpus', type=Path, required=True)
    run_parser.add_argument('--out', type=Path, required=True)
    run_parser.add_argument('--renderer', type=Path, required=True)
    run_parser.add_argument('--pages', type=int, default=1)
    run_parser.add_argument('--limit', type=int, default=0)
    run_parser.add_argument('--case', action='append', default=[])
    run_parser.add_argument('--jobs', type=int, choices=range(1, 5), default=1, help='bounded parallel document comparisons')
    run_parser.add_argument('--check', action='store_true', help='exit nonzero on any difference; never bless the current output')
    diff_parser = commands.add_parser('diff')
    diff_parser.add_argument('--before', type=Path, required=True)
    diff_parser.add_argument('--after', type=Path, required=True)
    diff_parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'inventory':
        inventory(args.archive.resolve(), args.corpus.resolve())
    elif args.command == 'diff':
        before = json.loads(args.before.read_text())
        after = json.loads(args.after.read_text())
        old = {case['id']: case for case in before['results']}
        changes = []
        for case in after['results']:
            previous = old.get(case['id'])
            if previous is None:
                continue
            changes.append({'id': case['id'], 'before_pages': previous.get('actual_pages'),
                            'after_pages': case.get('actual_pages'), 'reference_pages': case['reference_pages'],
                            'before_ink_f1': previous.get('mean_ink_f1'), 'after_ink_f1': case.get('mean_ink_f1')})
        comparable = (before['sampled_pages_per_document'] == after['sampled_pages_per_document']
                      and before.get('acceptance') == after.get('acceptance')
                      and before.get('comparison_width') == after.get('comparison_width')
                      and before.get('raster_normalization') == after.get('raster_normalization')
                      and before.get('native_raster_sampling') == after.get('native_raster_sampling')
                      and before.get('page_text_method') == after.get('page_text_method'))
        comparison = {'before': before['summary'], 'after': after['summary'],
                      'image_pass_totals_comparable': comparable,
                      'note': 'Page counts cover complete documents. Image pass totals require the same sampling and criteria.',
                      'changes': changes}
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(comparison, indent=2) + '\n')
        print(json.dumps({'before': before['summary'], 'after': after['summary']}))
    else:
        if args.pages < 1 or args.limit < 0:
            parser.error('pages must be positive and limit must be nonnegative')
        report = run(args.corpus.resolve(), args.out.resolve(), args.renderer.resolve(), args.pages, args.limit, args.case, args.jobs)
        if args.check and report['summary']['matched'] != report['summary']['documents']:
            return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
