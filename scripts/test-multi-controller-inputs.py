#!/usr/bin/env python3
"""Preflight regressions after installing worker0.2.0; no Windows RPC or builds."""
import importlib.util
from pathlib import Path, PureWindowsPath
import subprocess
import sys
import unittest


spec = importlib.util.spec_from_file_location(
    'native_inputs', Path(__file__).with_name('validate-multi-controller-native.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


class MigrationReferenceTests(unittest.TestCase):
    def test_main25_requires_both_retained_path_and_hash_only_for_the_nineteen_profile(self):
        helper = native.references
        path = r'K:\saved\winbridge-worker.exe'
        self.assertEqual(helper.reference_for_profile('worker19-updater20', path, native.WORKER17_SHA256),
                         PureWindowsPath(path))
        self.assertIsNone(helper.reference_for_profile('worker17-updater20', None, None))
        for profile, value, digest in (
                ('worker19-updater20', None, native.WORKER17_SHA256),
                ('worker19-updater20', path, None),
                ('worker17-updater20', path, None),
                ('worker17-updater20', None, native.WORKER17_SHA256),
                ('future-profile', None, None)):
            with self.subTest(profile=profile, value=value, digest=digest), self.assertRaises(RuntimeError):
                helper.reference_for_profile(profile, value, digest)

    def test_retained_released_worker_remains_usable_after_the_runtime_is_upgraded(self):
        path = r'K:\VMs\shared\WinBridge\releases\20261026-182213-6cd81f3e\winbridge-worker.exe'
        self.assertEqual(native.worker17_reference(path), PureWindowsPath(path))
        self.assertEqual(native.WORKER17_SHA256,
                         '536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712')

    def test_installed_worker_is_not_a_migration_reference_even_with_a_case_alias(self):
        for path in (r'K:\_proj-26\winbridge\runtime\winbridge-worker.exe',
                     r'k:/_PROJ-26/WINBRIDGE/RUNTIME/./WINBRIDGE-WORKER.EXE'):
            with self.subTest(path=path), self.assertRaisesRegex(RuntimeError, 'retained separately'):
                native.worker17_reference(path)

    def test_ambiguous_shared_and_traversal_paths_are_rejected(self):
        for path in ('winbridge-worker.exe', r'K:winbridge-worker.exe',
                     r'\runtime\winbridge-worker.exe', r'\\host\share\winbridge-worker.exe',
                     r'K:\saved\..\runtime\winbridge-worker.exe', r'K:\saved\wrong.exe'):
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                native.worker17_reference(path)

    def test_missing_reference_stops_before_reading_sources_or_contacting_a_worker(self):
        result = subprocess.run([
            sys.executable, str(Path(__file__).with_name('validate-multi-controller-native.py')),
            '--source-archive', '/nonexistent-source', '--controller-config', '/nonexistent-config',
            '--controller-binary', '/nonexistent-controller', '--test-runtime-directory', r'K:\runtime-test-204',
            '--cache-run', '20261017-061527-f4639971',
        ], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('--worker17-reference', result.stderr)
        self.assertNotIn('Traceback', result.stderr)

    def test_main25_hash_alone_cannot_fall_back_to_the_installed_worker(self):
        result = subprocess.run([
            sys.executable, str(Path(__file__).with_name('run-update-service-main-native.py')),
            '--source-archive', '/nonexistent-source', '--target-archive', '/nonexistent-target',
            '--controller-config', '/nonexistent-config', '--controller-binary', '/nonexistent-controller',
            '--test-runtime-directory', r'K:\runtime-test-204', '--cache-run', '20261017-061527-f4639971',
            '--profile', 'worker19-updater20', '--worker17-reference-sha256', native.WORKER17_SHA256,
        ], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Explicit retained worker17 path required', result.stderr)
        self.assertNotIn('FileNotFoundError', result.stderr)


if __name__ == '__main__':
    unittest.main()
