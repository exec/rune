import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest

import editor_compare as compare


class Fixtures(unittest.TestCase):
    def test_tasks_have_checkable_positions_and_unique_search_target(self):
        cases = {case.name: case for case in compare.cases([1000])}
        typed = cases['type_1000']
        self.assertEqual(typed.expected, b'x' * 256 + typed.data)
        navigation = cases['navigate_1000']
        lines = navigation.expected.splitlines()
        self.assertTrue(lines[256].startswith(b'Xrecord'))
        self.assertFalse(lines[255].startswith(b'X'))
        found = cases['find_1000']
        self.assertEqual(found.data.count(b'@'), 1)
        self.assertTrue(found.expected.endswith(b'X@ unique target at EOF\n'))
        longline = cases['longline_append']
        self.assertEqual(longline.expected.splitlines()[1], b'a' * 1000000 + b'x' * 256)
        self.assertTrue(cases['unicode_wrap'].wrap)

    def test_clean_configs_do_not_use_user_home_or_network_checks(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            case = compare.cases([1000])[0]
            command = compare.launch('rune', Path('/rune'), case, directory / 'fixture.txt', directory)
            self.assertEqual(command[1:3], ['--config', str(directory / 'rune.toml')])
            self.assertIn('check_for_updates = false', (directory / 'rune.toml').read_text())
            self.assertIn('--ignorercfiles', compare.launch('nano', Path('/nano'), case, directory / 'fixture.txt', directory))
            vim = compare.launch('vim', Path('/vim'), case, directory / 'fixture.txt', directory)
            self.assertIn('--noplugin', vim)
            self.assertEqual(vim[vim.index('-i') + 1], 'NONE')

    def test_pico_is_rejected_before_executing_unsupported_version_flag(self):
        with tempfile.TemporaryDirectory() as directory:
            pico = Path(directory) / 'pico'
            pico.write_text('not GNU nano')
            pico.chmod(0o755)
            nano = Path(directory) / 'nano'
            nano.symlink_to(pico)
            args = SimpleNamespace(profile='nano', nano=str(nano), rune='', vim='')
            with self.assertRaisesRegex(ValueError, 'Pico'):
                compare.editor_binaries(args)


class Pty(unittest.TestCase):
    def test_large_input_and_output_are_drained_without_losing_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'saved'
            code = '''
import os, sys, tty
from pathlib import Path
tty.setraw(0)
os.write(1, b'BENCH_READY')
data = bytearray()
while True:
    chunk = os.read(0, 1024)
    if b'Q' in chunk:
        data.extend(chunk.split(b'Q')[0])
        break
    data.extend(chunk)
    os.write(1, b'.' * (len(chunk) * 10))
Path(sys.argv[1]).write_bytes(data)
'''
            session = compare.Session([sys.executable, '-c', code, str(target)], 80, 24, 5)
            try:
                session.ready()
                result = session.finish(b'x' * 80000 + b'Q')
                self.assertEqual(target.read_bytes(), b'x' * 80000)
                self.assertGreater(result['output_bytes'], 80000)
                self.assertGreater(result['child_cpu_ms'], 0)
                self.assertGreater(result['peak_rss_mib'], 0)
            finally:
                session.close()

    def test_startup_timeout_reaps_the_child(self):
        session = compare.Session([sys.executable, '-c', 'import time; time.sleep(30)'], 80, 24, 0.05)
        try:
            with self.assertRaises(TimeoutError):
                session.ready()
        finally:
            session.close()
        with self.assertRaises(ChildProcessError):
            os.waitpid(session.pid, os.WNOHANG)

    def test_nonzero_exit_is_an_error_even_if_the_terminal_closes_cleanly(self):
        code = 'import os,tty; tty.setraw(0); os.write(1,b"BENCH_READY"); os.read(0,1); os._exit(7)'
        session = compare.Session([sys.executable, '-c', code], 80, 24, 2)
        try:
            session.ready()
            with self.assertRaisesRegex(RuntimeError, 'exit status'):
                session.finish(b'Q')
        finally:
            session.close()


if __name__ == '__main__':
    unittest.main()
