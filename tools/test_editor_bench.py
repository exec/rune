import hashlib
import io
import math
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import editor_bench as bench
import editor_compare as compare


class Scoring(unittest.TestCase):
    def samples(self):
        rows = []
        for editor in ['rune', 'nano', 'vim']:
            for action in ['key', 'burst', 'navigate', 'search', 'save', 'paste']:
                for count in bench.SIZES:
                    rows.append(dict(editor=editor, action=action, count=count, iteration=0,
                                     verified=True, warmup=False, task_ms=.01, startup_ms=.01))
            for action in ['unicode', 'longline']:
                rows.append(dict(editor=editor, action=action, count=0, iteration=0,
                                 verified=True, warmup=False, task_ms=.01, startup_ms=.01))
        return rows

    def test_weights_total_100_and_faster_than_budget_gets_no_extra_credit(self):
        results = bench.summarize(self.samples(), 1)
        for result in results.values():
            self.assertAlmostEqual(sum(c['weight'] for c in result['components']), 100)
            self.assertEqual(result['score'], 100)
        self.assertEqual(sum(bench.SIZES.values()), 1)

    def test_weighted_penalty_and_warmup_exclusion(self):
        samples = self.samples()
        samples[0]['task_ms'] = 1  # Twice the .5 ms key budget, with 20% * 60% weight.
        samples.append(dict(samples[0], warmup=True, task_ms=1e9))
        results = bench.summarize(samples, 1)
        self.assertAlmostEqual(results['rune']['score'], 100 * math.exp(.12 * math.log(.5)))
        self.assertEqual(results['nano']['score'], 100)

    def test_missing_wrong_duplicate_and_invalid_samples_cannot_score(self):
        for change in ['missing', 'wrong', 'duplicate', 'nan', 'infinity', 'zero']:
            with self.subTest(change=change):
                samples = self.samples()
                if change == 'missing':
                    samples.pop()
                elif change == 'wrong':
                    samples[0]['verified'] = False
                elif change == 'duplicate':
                    samples.append(samples[0])
                else:
                    samples[0]['task_ms'] = {'nan': float('nan'), 'infinity': float('inf'), 'zero': 0}[change]
                with self.assertRaises(ValueError):
                    bench.summarize(samples, 1)


class Protocol(unittest.TestCase):
    def test_fixtures_have_exact_edits_and_paste_is_one_mib(self):
        for action in ['key', 'burst', 'navigate', 'search', 'save', 'paste', 'unicode', 'longline']:
            case, text = bench.fixture(action, 1000)
            self.assertNotIn(bench.MARKER, case.data)
            if action != 'save':
                self.assertEqual(case.expected.count(b'~' if action == 'key' else bench.MARKER), 1)
            if action == 'paste':
                self.assertEqual(len(text), 1048576)
                self.assertEqual(case.expected, text + case.data)
                for editor in ['rune', 'nano', 'vim']:
                    payload = bench.payload(editor, action, text)
                    self.assertIn(b'\x1b[200~' + text + b'\x1b[201~', payload)
            if action == 'navigate':
                self.assertTrue(case.expected.splitlines()[256].startswith(bench.MARKER))
            if action == 'search':
                self.assertTrue(case.expected.endswith(bench.MARKER + b'quartz target at EOF\n'))
            if action == 'longline':
                self.assertEqual(case.expected.splitlines()[1], b'a' * 1000000 + text)

    def test_observation_waits_for_split_sync_end_after_large_frame(self):
        code = '''
import os,time,tty
tty.setraw(0)
os.write(1,b'BENCH_READY')
os.read(0,1)
os.write(1,b'\x1b[?2026h' + b'x' * 70000 + b'QZWVUGJKF')
time.sleep(.06)
os.write(1,b'\x1b[?202')
time.sleep(.02)
os.write(1,b'6l')
os.read(0,1)
'''
        session = compare.Session([sys.executable, '-c', code], 120, 40, 3)
        try:
            session.ready()
            result = session.observe(b'X', bench.MARKER)
            self.assertGreaterEqual(result['task_ms'], 70)
            self.assertFalse(session.sync_active)
            session.finish(b'Q')
        finally:
            session.close()

    def test_observation_timeout_and_exit_never_acknowledge_missing_marker(self):
        for ending, error in [('os.read(0,1)', TimeoutError), ('pass', RuntimeError)]:
            code = f'import os,tty; tty.setraw(0); os.write(1,b"BENCH_READY"); os.read(0,1); {ending}'
            session = compare.Session([sys.executable, '-c', code], 80, 24, .1)
            try:
                session.ready()
                with self.assertRaises(error):
                    session.observe(b'X', bench.MARKER)
            finally:
                session.close()


class Downloads(unittest.TestCase):
    def test_failed_setup_replaces_old_score_with_failed_status(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'result.json'
            output.write_text('{"status":"complete","scores":{"rune":100}}')
            with (patch.object(sys, 'argv', ['editor_bench.py', '--output', str(output)]),
                  patch.object(bench, 'prepare', side_effect=ValueError('bad source'))):
                with self.assertRaisesRegex(ValueError, 'bad source'):
                    bench.main()
            import json
            result = json.loads(output.read_text())
            self.assertEqual(result['status'], 'failed')
            self.assertIsNone(result['scores'])

    def test_checksum_failure_prevents_extraction_or_building(self):
        with tempfile.TemporaryDirectory() as directory:
            with (patch.object(bench, 'CACHE', Path(directory)),
                  patch.object(bench.platform, 'platform', return_value='test-platform'),
                  patch.object(bench, 'SOURCES', {'nano': ('https://example.invalid/source', '0' * 64, [])}),
                  patch.object(bench.subprocess, 'check_output', return_value='cc version'),
                  patch.object(bench, 'urlopen', return_value=io.BytesIO(b'wrong archive')),
                  patch.object(bench.subprocess, 'run') as build):
                with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                    bench.prepare('nano')
                build.assert_not_called()

    def test_corrupt_cached_archive_is_rejected_without_using_network(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'downloads').mkdir()
            checksum = hashlib.sha256(b'expected archive').hexdigest()
            (root / 'downloads' / checksum).write_bytes(b'bad archive')
            with (patch.object(bench, 'CACHE', root),
                  patch.object(bench.platform, 'platform', return_value='test-platform'),
                  patch.object(bench, 'SOURCES', {'nano': ('https://example.invalid/source', checksum, [])}),
                  patch.object(bench.subprocess, 'check_output', return_value='cc version'),
                  patch.object(bench, 'urlopen') as network):
                with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                    bench.prepare('nano')
                network.assert_not_called()


if __name__ == '__main__':
    unittest.main()
