import tempfile
import unittest
import zipfile
import json
from types import SimpleNamespace
from unittest.mock import patch
from pathlib import Path

from golden_word import collect_cases, compare_images, page_matches


class GoldenWordTests(unittest.TestCase):
    def test_pairs_originals_without_reverse_conversions_or_temporary_files(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / 'samples.zip'
            with zipfile.ZipFile(archive, 'w') as stream:
                for name in ['golden_word/paragraphs/sample.docx',
                             'golden_word/paragraphs/sample.WORDTOPDF.WPS.PDFTOWORD.WPS.docx',
                             'golden_word/paragraphs/~$sample.docx',
                             'golden_word/paragraphs/sample_WORD_TO_PNG_WPS/page10.png',
                             'golden_word/paragraphs/sample_WORD_TO_PNG_WPS/page2.png',
                             'golden_word/paragraphs/sample_WORD_TO_PNG_WPS/page1.png']:
                    stream.writestr(name, b'fixture')
            cases = collect_cases(archive)
            self.assertEqual(len(cases), 1)
            self.assertEqual([page['number'] for page in cases[0]['images']], [1, 2, 10])
            self.assertEqual(cases[0]['source'], 'golden_word/paragraphs/sample.docx')
            self.assertFalse(cases[0]['reference_complete'])

    def test_identical_pages_match_but_a_missing_or_shifted_paragraph_does_not(self):
        from PIL import Image, ImageDraw
        page = Image.new('RGB', (600, 800), 'white')
        draw = ImageDraw.Draw(page)
        draw.rectangle((80, 100, 420, 120), fill='black')
        self.assertEqual(compare_images(page, page)['ink_f1'], 1)
        missing = Image.new('RGB', page.size, 'white')
        self.assertEqual(compare_images(page, missing)['ink_f1'], 0)
        shifted = Image.new('RGB', page.size, 'white')
        ImageDraw.Draw(shifted).rectangle((80, 180, 420, 200), fill='black')
        self.assertLess(compare_images(page, shifted)['ink_f1'], 0.1)

    def test_white_background_cannot_hide_a_missing_paragraph(self):
        from PIL import Image, ImageDraw
        reference = Image.new('RGB', (600, 800), 'white')
        ImageDraw.Draw(reference).rectangle((80, 100, 420, 120), fill='black')
        result = compare_images(reference, Image.new('RGB', reference.size, 'white'))
        self.assertEqual(result['ink_f1'], 0)
        self.assertGreater(result['rmse'], 0)

    def test_wrong_colors_fail_even_when_the_ink_geometry_is_identical(self):
        from PIL import Image
        result = compare_images(Image.new('RGB', (600, 800), 'red'), Image.new('RGB', (600, 800), 'blue'))
        self.assertEqual(result['ink_f1'], 1)
        self.assertFalse(page_matches(result))

    def test_diagnostic_similarity_still_sees_pixel_drift_that_acceptance_ignores(self):
        # Maintainer decision: font rendering and pixel-level drift never decide a page.
        from PIL import Image, ImageDraw, ImageChops
        page = Image.new('RGB', (600, 800), 'white')
        ImageDraw.Draw(page).rectangle((80, 100, 420, 120), fill='black')
        exact = compare_images(page, page)
        self.assertEqual(exact['similarity'], 1)
        self.assertTrue(page_matches(exact))
        shifted = compare_images(page, ImageChops.offset(page, 1, 0))
        self.assertLess(shifted['similarity'], 0.999)
        self.assertTrue(page_matches(shifted))

    def test_acceptance_ignores_stroke_weight_but_rejects_displaced_or_missing_lines(self):
        from PIL import Image, ImageDraw
        # Text-like glyph boxes at the comparison width; heavier strokes are one pixel wider.
        def page(lines, weight=0, top=200):
            image = Image.new('RGB', (1200, 1600), 'white')
            draw = ImageDraw.Draw(image)
            for k in range(lines):
                for x in range(160, 840, 24):
                    draw.rectangle((x, top + k * 40, x + 16 + weight, top + k * 40 + 18), fill='black')
            return image
        reference = page(10)
        self.assertTrue(page_matches(compare_images(reference, page(10, weight=1))))
        self.assertFalse(page_matches(compare_images(reference, page(9))))
        self.assertFalse(page_matches(compare_images(reference, page(10, top=220))))

    def test_a_missing_highlight_behind_text_is_a_large_difference(self):
        from PIL import Image, ImageDraw
        def page(highlight):
            image = Image.new('RGB', (1200, 1600), 'white')
            draw = ImageDraw.Draw(image)
            for k in range(10):
                top = 300 + k * 40
                if highlight:
                    draw.rectangle((160, top, 840, top + 30), fill=(255, 240, 120))
                for x in range(160, 840, 24):
                    draw.rectangle((x, top + 6, x + 16, top + 24), fill='black')
            return image
        metrics = compare_images(page(True), page(False))
        self.assertEqual(metrics['layout_f1'], 1)
        self.assertFalse(page_matches(metrics))

    def test_a_missing_light_watermark_is_a_large_difference(self):
        from PIL import Image, ImageDraw
        reference = Image.new('RGB', (600, 800), 'white')
        ImageDraw.Draw(reference).rectangle((100, 250, 500, 550), fill=(207, 207, 207))
        metrics = compare_images(reference, Image.new('RGB', reference.size, 'white'))
        self.assertEqual(metrics['layout_f1'], 1)
        self.assertGreater(metrics['tone_rmse'], 0.03)
        self.assertFalse(page_matches(metrics))

    def test_equal_page_counts_do_not_hide_wrong_page_allocation_or_page_numbers(self):
        from golden_fidelity import compare_page_text
        self.assertTrue(compare_page_text('Body text\n1', ['Body ', 'text', '1'])['page_text_equal'])
        self.assertFalse(compare_page_text('Body text\n1', ['Body text', '2'])['page_text_equal'])
        self.assertFalse(compare_page_text('First paragraph', ['Second paragraph'])['page_text_equal'])

    def test_sampled_or_empty_page_runs_cannot_pass_a_full_document(self):
        from golden_fidelity import document_matches
        page = {**compare_images(self.blank(), self.blank()), 'page_text_equal': True}
        result = {'reference_complete': True, 'page_delta': 0, 'reference_pages': 2, 'pages': [page]}
        self.assertFalse(document_matches(result))
        result['pages'].append(page)
        self.assertTrue(document_matches(result))
        result['pages'][1] = {**page, 'page_text_equal': False}
        self.assertFalse(document_matches(result))

    @staticmethod
    def blank():
        from PIL import Image
        return Image.new('RGB', (600, 800), 'white')

    def test_faint_content_and_large_white_margins_do_not_inflate_similarity(self):
        # The diagnostic similarity still sees faint content; acceptance (maintainer decision)
        # only fails large differences, and a near-white bar is not one.
        from PIL import Image, ImageDraw
        reference = Image.new('RGB', (1200, 1600), 'white')
        ImageDraw.Draw(reference).rectangle((80, 100, 420, 120), fill=(251, 251, 251))
        metrics = compare_images(reference, Image.new('RGB', reference.size, 'white'))
        self.assertLess(metrics['similarity'], 0.999)
        self.assertTrue(page_matches(metrics))

    def test_physical_page_sizes_remove_only_raster_dimension_rounding(self):
        from PIL import Image
        result = compare_images(Image.new('RGB', (793, 1122), 'white'), Image.new('RGB', (1191, 1684), 'white'),
                                reference_size=(595.3, 841.9), actual_size=(595.3, 841.9))
        self.assertGreater(result['aspect_error'], 0.0001)
        self.assertEqual(result['physical_aspect_error'], 0)
        self.assertTrue(page_matches(result))

    def test_pixel_density_preserves_content_geometry_when_page_edges_are_rounded(self):
        from PIL import Image, ImageDraw
        reference = Image.new('RGB', (1190, 1683), 'white')
        actual = Image.new('RGB', (1191, 1684), 'white')
        for page in (reference, actual):
            ImageDraw.Draw(page).rectangle((100, 120, 1050, 400), fill='black')
            page.info['dpi'] = (144, 144)
        metrics = compare_images(reference, actual, reference_size=(595.3, 841.9), actual_size=(595.3, 841.9))
        self.assertEqual(metrics['reference_ink_bounds'], metrics['actual_ink_bounds'])
        self.assertEqual(metrics['similarity'], 1)

    def test_case_pipeline_rejects_extra_page_number_and_modified_input(self):
        from PIL import Image
        from pypdf import PdfWriter
        from golden_pages import compare_case, digest
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, output = root / 'corpus' / 'sample', root / 'out' / 'sample'
            source.mkdir(parents=True); output.mkdir(parents=True)
            (source / 'source.docx').write_bytes(b'fake renderer input')
            image = Image.new('RGB', (600, 800), 'white')
            image.save(source / 'wps-0001.png'); image.save(output / 'page-0001.png')
            writer = PdfWriter(); writer.add_blank_page(width=600, height=800)
            writer.write(source / 'reference.WPS.pdf')
            case = {'id': 'sample', 'images': [{'number': 1}], 'reference_complete': True, 'pdf': 'present',
                    'hashes': {p.name: digest(p.read_bytes()) for p in source.iterdir()}}
            layout = {'page_count': 1, 'scale': 2, 'pages': [{'width_pt': 600, 'height_pt': 800, 'text_spans': []}]}
            (output / 'layout.json').write_text(json.dumps(layout))
            with patch('golden_pages.subprocess.run', return_value=SimpleNamespace(returncode=0)) as renderer:
                good = compare_case(case, source.parent, output.parent, Path('renderer'), 1, compare_images)
                self.assertEqual(good['status'], 'match')
                layout['pages'][0]['text_spans'] = [{'text': '2'}]
                (output / 'layout.json').write_text(json.dumps(layout))
                wrong_number = compare_case(case, source.parent, output.parent, Path('renderer'), 1, compare_images)
                self.assertEqual(wrong_number['status'], 'difference')
                self.assertEqual(wrong_number['minimum_similarity'], 1)
                self.assertFalse(wrong_number['pages'][0]['page_text_equal'])
                renderer.reset_mock()
                (source / 'source.docx').write_bytes(b'changed')
                changed = compare_case(case, source.parent, output.parent, Path('renderer'), 1, compare_images)
                self.assertEqual(changed['status'], 'error')
                renderer.assert_not_called()


if __name__ == '__main__':
    unittest.main()
