import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


class GoldenRasterTests(unittest.TestCase):
    def fixture(self, root, dpi=(96, 96), size=(800, 1067), rotation=0):
        from PIL import Image
        from pypdf import PdfWriter
        source, output = root / 'corpus' / 'sample', root / 'out' / 'sample'
        source.mkdir(parents=True)
        output.mkdir(parents=True)
        (source / 'source.docx').write_bytes(b'fixture document')
        image = Image.new('RGB', size, 'white')
        image.save(source / 'wps-0001.png', **({'dpi': dpi} if dpi else {}))
        writer = PdfWriter()
        page = writer.add_blank_page(width=800 if rotation else 600, height=600 if rotation else 800)
        if rotation:
            page.rotate(rotation)
        writer.write(source / 'reference.WPS.pdf')
        return source, output

    def test_pipeline_requests_consistent_reference_density_and_records_the_actual_scale(self):
        from PIL import Image
        from golden_pages import compare_case, digest
        from golden_word import compare_images
        with tempfile.TemporaryDirectory() as directory:
            source, output = self.fixture(Path(directory))
            with Image.open(source / 'wps-0001.png') as reference:
                expected = reference.info['dpi'][0] / 72
            case = {'id': 'sample', 'images': [{'number': 1}], 'reference_complete': True, 'pdf': 'present',
                    'hashes': {path.name: digest(path.read_bytes()) for path in source.iterdir()}}
            def renderer(command, **_):
                layout = {'page_count': 1, 'scale': float(command[-1]),
                          'pages': [{'width_pt': 600, 'height_pt': 800, 'text_spans': []}]}
                (output / 'layout.json').write_text(json.dumps(layout))
                (output / 'page-0001.png').write_bytes((source / 'wps-0001.png').read_bytes())
                return SimpleNamespace(returncode=0)
            with patch('golden_pages.subprocess.run', side_effect=renderer) as called:
                result = compare_case(case, source.parent, output.parent, Path('renderer'), 1, compare_images)
            self.assertEqual(result['status'], 'match')
            self.assertAlmostEqual(float(called.call_args.args[0][-1]), expected, places=6)
            self.assertAlmostEqual(result['raster_scale'], expected, places=6)
            self.assertEqual(result['raster_scale_method'], 'reference-png-density')

    def test_pipeline_rejects_a_renderer_that_reports_a_different_scale(self):
        from golden_pages import compare_case, digest
        from golden_word import compare_images
        with tempfile.TemporaryDirectory() as directory:
            source, output = self.fixture(Path(directory))
            case = {'id': 'sample', 'images': [{'number': 1}], 'reference_complete': True, 'pdf': 'present',
                    'hashes': {path.name: digest(path.read_bytes()) for path in source.iterdir()}}
            (output / 'layout.json').write_text(json.dumps({'page_count': 1, 'scale': 1,
                'pages': [{'width_pt': 600, 'height_pt': 800, 'text_spans': []}]}))
            (output / 'page-0001.png').write_bytes((source / 'wps-0001.png').read_bytes())
            with patch('golden_pages.subprocess.run', return_value=SimpleNamespace(returncode=0)):
                result = compare_case(case, source.parent, output.parent, Path('renderer'), 1, compare_images)
            self.assertEqual(result['status'], 'error')
            self.assertIn('scale', result['error'])

    def test_selection_falls_back_for_missing_inconsistent_or_anisotropic_density(self):
        from golden_raster import select_scale
        from pypdf import PdfReader
        for dpi, dimensions in [(None, (800, 1067)), ((96, 96), (600, 800)), ((96, 120), (800, 1333)), ((300, 300), (2500, 3333))]:
            with self.subTest(dpi=dpi), tempfile.TemporaryDirectory() as directory:
                source, _ = self.fixture(Path(directory), dpi=dpi, size=dimensions)
                scale, method = select_scale(source, [{'number': 1}], PdfReader(source / 'reference.WPS.pdf'))
                self.assertEqual(scale, 2)
                self.assertEqual(method, 'default-144dpi')

    def test_selection_honors_rotation_and_checks_every_reference_page(self):
        from golden_raster import select_scale
        from PIL import Image
        from pypdf import PdfReader, PdfWriter
        with tempfile.TemporaryDirectory() as directory:
            source, _ = self.fixture(Path(directory), rotation=90)
            scale, method = select_scale(source, [{'number': 1}], PdfReader(source / 'reference.WPS.pdf'))
            self.assertAlmostEqual(scale, 96 / 72, places=3)
            self.assertEqual(method, 'reference-png-density')
            writer = PdfWriter()
            for _ in range(2):
                writer.add_blank_page(width=600, height=800)
            writer.write(source / 'reference.WPS.pdf')
            Image.new('RGB', (1000, 1333), 'white').save(source / 'wps-0002.png', dpi=(120, 120))
            scale, method = select_scale(source, [{'number': 1}, {'number': 2}], PdfReader(source / 'reference.WPS.pdf'))
            self.assertEqual((scale, method), (2, 'default-144dpi'))

    def test_diff_does_not_compare_image_pass_totals_across_sampling_methods(self):
        from golden_word import main
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = {'summary': {}, 'results': [], 'sampled_pages_per_document': 1000,
                      'minimum_similarity': 0.999, 'comparison_width': 1200, 'maximum_rmse': 0.03}
            before, after, output = root / 'before.json', root / 'after.json', root / 'diff.json'
            before.write_text(json.dumps({**report, 'native_raster_sampling': 'fixed-144dpi'}))
            after.write_text(json.dumps({**report, 'native_raster_sampling': 'reference-density-when-consistent'}))
            with patch('sys.argv', ['golden_word.py', 'diff', '--before', str(before), '--after', str(after), '--out', str(output)]):
                self.assertEqual(main(), 0)
            self.assertFalse(json.loads(output.read_text())['image_pass_totals_comparable'])


if __name__ == '__main__':
    unittest.main()
