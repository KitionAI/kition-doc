import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


class CorpusProvenanceTests(unittest.TestCase):
    def test_snapshot_includes_untracked_source_and_excludes_ignored_build_outputs(self):
        from corpus_provenance import source_snapshot, compare_snapshots
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            (root / '.gitignore').write_text('target/\n')
            (root / 'engine.rs').write_text('original')
            subprocess.run(['git', '-C', str(root), 'add', '.gitignore', 'engine.rs'], check=True)
            new_source = root / 'layout.rs'
            new_source.write_text('new module')
            output = root / 'target'
            output.mkdir()
            (output / 'report.json').write_text('first run')
            start = source_snapshot(root)
            self.assertTrue(compare_snapshots(start, start)['unchanged'])
            (output / 'report.json').write_text('second run')
            self.assertEqual(source_snapshot(root), start)
            new_source.write_text('changed module')
            end = source_snapshot(root)
            self.assertNotEqual(start['sha256'], end['sha256'])
            self.assertFalse(compare_snapshots(start, end)['unchanged'])
            self.assertEqual(compare_snapshots(start, end)['changed_files'], ['layout.rs'])
            (root / 'engine.rs').unlink()
            self.assertIn('engine.rs', compare_snapshots(end, source_snapshot(root))['changed_files'])

    def test_run_reports_source_changes_between_start_and_end(self):
        from corpus_word import run
        start = {'revision': 'fixture-head', 'sha256': 'first', 'files': {'layout.rs': 'first'}}
        end = {'revision': 'fixture-head', 'sha256': 'second', 'files': {'layout.rs': 'second'}}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            corpus = root / 'corpus'
            corpus.mkdir()
            (corpus / 'manifest.json').write_text(json.dumps({'cases': [{'id': 'fixture', 'images': [{'number': 1}]}]}))
            renderer = root / 'renderer'
            renderer.write_bytes(b'fixed renderer fixture')
            result = {'id': 'fixture', 'status': 'difference', 'reference_pages': 1,
                      'reference_complete': True, 'actual_pages': 1, 'page_delta': 0}
            with patch('corpus_word.source_snapshot', side_effect=[start, end]) as snapshots, \
                 patch('corpus_pages.compare_case', return_value=result):
                report = run(corpus, root / 'out', renderer, 1, 0, [])
            self.assertEqual(snapshots.call_count, 2)
            self.assertFalse(report['source_provenance']['unchanged'])
            self.assertEqual(report['source_provenance']['changed_files'], ['layout.rs'])
            self.assertEqual(report['source_provenance']['start']['sha256'], 'first')
            self.assertEqual(report['source_provenance']['end']['sha256'], 'second')
            self.assertEqual(report['summary']['matched'], 0)


if __name__ == '__main__':
    unittest.main()
