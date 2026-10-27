#!/usr/bin/env python3
"""Exercise the real process/filesystem boundary without a physical camera."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

FAKE = r'''#!/usr/bin/env python3
import os, sys, time
from pathlib import Path
a = sys.argv[1:]
assert '--delete-file' not in a and '--delete-all-files' not in a
with open(os.environ['FAKE_PROCESS_LOG'], 'a') as f:
    f.write(str(os.getpid()) + ' ' + ('download' if '--get-file' in a else 'list') + '\n')
if '--auto-detect' in a:
    print('Model                          Port')
    print('----------------------------------------------------------')
    print('Nikon Z fc                     usb:001,002')
    print('Canon EOS R                    usb:001,003')
elif '--list-files' in a:
    print('/store/DCIM/100NIKON/a b.NEF')
    print('/store/DCIM/101NIKON/a.JPG')
    if os.environ.get('FAKE_LARGE'):
        print('/store/DCIM/101NIKON/large.NEF')
    if os.environ.get('FAKE_DUPLICATES'):
        print('/store/DCIM/102NIKON/a.JPG')
        print('/store/DCIM/102NIKON/1-a.JPG')
elif '--get-file' in a:
    assert a[a.index('--camera')+1] == 'Nikon Z fc'
    assert a[a.index('--port')+1] == 'usb:001,002'
    assert a[a.index('--folder')+1] == '/'
    assert '--stdout-size' in a
    names = [a[i+1] for i in range(len(a)) if a[i] == '--get-file']
    for index, name in enumerate(names):
        with open(os.environ['FAKE_LOG'], 'a') as f: f.write(name + '\n')
        if os.environ.get('FAKE_RACE_TARGET'):
            Path(os.environ['FAKE_RACE_TARGET']).write_bytes(b'existing-photo')
        if os.environ.get('FAKE_DELAY'):
            time.sleep(float(os.environ['FAKE_DELAY']))
        data = b'photo-data:' + name.encode()
        if name.endswith('/large.NEF'):
            data = b'x' * (1024 * 1024 + 17)
        if os.environ.get('FAKE_BINARY'):
            data = b'0\n12345\x00\xff\n' + data
        if os.environ.get('FAKE_BAD_HEADER'):
            sys.stdout.buffer.write(b'not-a-size\n')
            sys.exit(1)
        fail = os.environ.get('FAKE_FAIL') or (os.environ.get('FAKE_FAIL_AFTER') and index >= int(os.environ['FAKE_FAIL_AFTER']))
        sys.stdout.buffer.write(str(len(data)).encode() + b'\n')
        if fail:
            sys.stdout.buffer.write(data[:3])
            if os.environ.get('FAKE_STDERR_BURST'):
                sys.stderr.write('diagnostic ' * 20000)
            print('USB camera disconnected', file=sys.stderr)
            sys.exit(1)
        if os.environ.get('FAKE_SLOW_CHUNKS') and name.endswith('/large.NEF'):
            for offset in range(0, len(data), 65536):
                sys.stdout.buffer.write(data[offset:offset+65536])
                sys.stdout.buffer.flush()
                time.sleep(0.02)
        else:
            sys.stdout.buffer.write(data)
            sys.stdout.buffer.flush()
    if os.environ.get('FAKE_EXTRA'):
        sys.stdout.buffer.write(b'unexpected output')
    if os.environ.get('FAKE_EXIT_FAIL'):
        print('camera exit failed', file=sys.stderr)
        sys.exit(1)
else:
    raise AssertionError(a)
'''

class BackendTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.build = tempfile.TemporaryDirectory(prefix='camera-tests-')
        build = Path(cls.build.name)
        subprocess.run(['cargo', 'build', '--locked', '--example', 'backend-driver'], cwd=ROOT, check=True)
        cls.driver = ROOT / 'target/debug/examples/backend-driver'
        fake = build / 'gphoto2'
        fake.write_text(FAKE)
        fake.chmod(0o755)

    @classmethod
    def tearDownClass(cls):
        cls.build.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='camera-output-')
        self.addCleanup(self.temp.cleanup)
        self.destination = Path(self.temp.name)
        self.log = self.destination / 'calls.log'
        self.env = dict(os.environ, PATH=self.build.name + os.pathsep + os.environ['PATH'], FAKE_LOG=str(self.log), FAKE_PROCESS_LOG=str(self.destination / 'processes.log'))

    def run_import(self, mode='new', destination=None, **extra):
        return subprocess.run([str(self.driver), mode, str(destination or self.destination)], env=dict(self.env, **extra), capture_output=True, text=True, timeout=10)

    def test_multiple_cameras(self):
        result = self.run_import('detect')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Nikon Z fc', result.stdout)
        self.assertIn('Canon EOS R', result.stdout)

    def test_download_skip_and_unique_copy(self):
        result = self.run_import()
        self.assertEqual(result.returncode, 0, result.stderr)
        target = self.destination / 'a b.NEF'
        self.assertEqual(target.read_bytes(), b'photo-data:store/DCIM/100NIKON/a b.NEF')
        self.assertEqual(len(self.log.read_text().splitlines()), 2)
        processes = (self.destination / 'processes.log').read_text().splitlines()
        self.assertEqual(len(processes), 2)
        self.assertEqual(sum(line.endswith('download') for line in processes), 1)
        self.assertIn('skipped 2', self.run_import().stdout)
        self.assertEqual(len(self.log.read_text().splitlines()), 2)
        self.assertEqual(sum(line.endswith('download') for line in (self.destination / 'processes.log').read_text().splitlines()), 1)
        self.assertEqual(self.run_import('all').returncode, 0)
        self.assertEqual((target.parent / '1-a b.NEF').read_bytes(), target.read_bytes())
        self.assertFalse(list(self.destination.rglob('*.part')))
        self.assertFalse((self.destination / 'store').exists())

    def test_optional_camera_folders(self):
        result = self.run_import(TEST_PRESERVE_FOLDERS='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        target = self.destination / 'store/DCIM/100NIKON/a b.NEF'
        self.assertEqual(target.read_bytes(), b'photo-data:store/DCIM/100NIKON/a b.NEF')
        self.assertFalse((self.destination / 'a b.NEF').exists())

    def test_export_date_folder_with_either_layout(self):
        for preserve in (False, True):
            with self.subTest(preserve=preserve):
                destination = self.destination / str(preserve) / '2026-10-27'
                destination.mkdir(parents=True)
                options = {'TEST_PRESERVE_FOLDERS': '1'} if preserve else {}
                result = self.run_import(destination=destination, **options)
                self.assertEqual(result.returncode, 0, result.stderr)
                relative = 'store/DCIM/100NIKON/a b.NEF' if preserve else 'a b.NEF'
                self.assertTrue((destination / relative).is_file())

    def test_duplicate_camera_names_are_preserved_and_skipped_on_retry(self):
        result = self.run_import(FAKE_DUPLICATES='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.destination / 'a.JPG').read_bytes(), b'photo-data:store/DCIM/101NIKON/a.JPG')
        self.assertEqual((self.destination / '2-a.JPG').read_bytes(), b'photo-data:store/DCIM/102NIKON/a.JPG')
        self.assertEqual((self.destination / '1-a.JPG').read_bytes(), b'photo-data:store/DCIM/102NIKON/1-a.JPG')
        self.assertIn('skipped 4', self.run_import(FAKE_DUPLICATES='1').stdout)
        self.assertEqual(len(self.log.read_text().splitlines()), 4)

    def test_partial_download_never_published(self):
        result = self.run_import(FAKE_FAIL='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('disconnected', result.stderr)
        self.assertIn('Metrics: imported=0 skipped=0 bytes=3 frozen=true', result.stderr)
        self.assertFalse(list(self.destination.rglob('*.NEF')))
        self.assertFalse(list(self.destination.rglob('*.part')))

    def test_byte_metrics_include_large_files_and_freeze_on_completion(self):
        result = self.run_import(FAKE_LARGE='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = sum(len(path.read_bytes()) for path in self.destination.iterdir()
                       if path.suffix in ('.NEF', '.JPG'))
        self.assertIn(f'Metrics: imported=3 skipped=0 bytes={expected} frozen=true', result.stderr)
        self.assertIn('fraction=1', result.stderr)
        result = self.run_import(FAKE_LARGE='1')
        self.assertIn('Metrics: imported=0 skipped=3 bytes=0 frozen=true', result.stderr)
        self.assertNotIn('remaining', result.stderr)

    def test_progress_updates_during_a_file(self):
        result = self.run_import(FAKE_LARGE='1', FAKE_SLOW_CHUNKS='1', TEST_OBSERVE_PROGRESS='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Observed partial progress: true', result.stderr)

    def test_stream_framing_handles_binary_data(self):
        result = self.run_import(FAKE_BINARY='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.destination / 'a b.NEF').read_bytes(), b'0\n12345\x00\xff\nphoto-data:store/DCIM/100NIKON/a b.NEF')
        self.assertEqual((self.destination / 'a.JPG').read_bytes(), b'0\n12345\x00\xff\nphoto-data:store/DCIM/101NIKON/a.JPG')

    def test_failure_keeps_completed_files_and_cleans_partial(self):
        result = self.run_import(FAKE_FAIL_AFTER='1', FAKE_STDERR_BURST='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('disconnected', result.stderr)
        self.assertTrue((self.destination / 'a b.NEF').is_file())
        self.assertFalse((self.destination / 'a.JPG').exists())
        self.assertFalse(list(self.destination.rglob('*.part')))

    def test_invalid_size_header_is_rejected(self):
        result = self.run_import(FAKE_BAD_HEADER='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('size header', result.stderr)
        self.assertFalse(list(self.destination.rglob('*.NEF')))
        self.assertFalse(list(self.destination.rglob('*.part')))

    def test_extra_output_and_nonzero_exit_are_reported(self):
        for variable in ('FAKE_EXTRA', 'FAKE_EXIT_FAIL'):
            with self.subTest(variable=variable):
                result = self.run_import('all', **{variable: '1'})
                self.assertNotEqual(result.returncode, 0)

    def test_cancel_reaps_shared_process(self):
        result = self.run_import(TEST_CANCEL_AFTER_MS='100', FAKE_DELAY='0.3')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Cancelled. Imported 1', result.stdout)
        self.assertTrue((self.destination / 'a b.NEF').is_file())
        self.assertFalse((self.destination / 'a.JPG').exists())
        self.assertFalse(list(self.destination.rglob('*.part')))
        for line in (self.destination / 'processes.log').read_text().splitlines():
            with self.assertRaises(ProcessLookupError):
                os.kill(int(line.split()[0]), 0)

    def test_concurrent_destination_creation_never_overwritten(self):
        target = self.destination / 'a b.NEF'
        result = self.run_import(FAKE_RACE_TARGET=str(target))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(target.read_bytes(), b'existing-photo')
        self.assertFalse(list(self.destination.rglob('*.part')))

    def test_missing_gphoto2_is_reported(self):
        result = self.run_import(PATH='')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Cannot run gphoto2', result.stderr)

    def test_cancel_before_download(self):
        self.assertIn('Cancelled', self.run_import('cancel').stdout)
        self.assertFalse(self.log.exists())

    def test_symlink_escape_rejected(self):
        with tempfile.TemporaryDirectory() as outside:
            (self.destination / 'store').symlink_to(outside, target_is_directory=True)
            result = self.run_import(TEST_PRESERVE_FOLDERS='1')
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(list(Path(outside).rglob('*.NEF')))

if __name__ == '__main__':
    unittest.main()
