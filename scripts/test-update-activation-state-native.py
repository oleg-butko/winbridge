#!/usr/bin/env python3
"""Local type/privacy/admission regressions; not a native StateOwned proof."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest

s = importlib.util.spec_from_file_location('state_gate', Path(__file__).with_name('validate-update-activation-preparation-native.py'))
gate = importlib.util.module_from_spec(s)
s.loader.exec_module(gate)


class StateEvidenceTests(unittest.TestCase):
    def test_unknown_lock_and_process_observations_are_nullable_typed_booleans(self):
        for key in ('originalLockHeld', 'processAlive', 'processCustodyHeld'):
            for value in (None, False, True):
                gate.state_checks.strict({key: value}, gate.checks)
            for value in (0, 1, 'true', [], {}):
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    gate.state_checks.strict({key: value}, gate.checks)

    def test_physical_success_cannot_be_numeric_or_unknown(self):
        for key in ('lockAvailable', 'stateTraceOnly', 'sameInstanceLockAvailable', 'childInstanceLockOccupied',
                    'actualRetainedProcessExited', 'writeDenied', 'deleteDenied', 'writableStoreStarted'):
            for value in (None, 0, 1, 'false'):
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    gate.state_checks.strict({key: value}, gate.checks)

    def test_no_permission_or_secret_in_any_retained_copy(self):
        for key in ('stateGrant', 'canHandoff', 'writerOwned', 'canWriteState', 'nonce', 'token', 'permit'):
            with self.subTest(key=key), self.assertRaises(ValueError):
                gate.state_checks.strict({'cold': {'history': [{'nested': {key: False}}]}}, gate.checks)

    def test_actual_filetime_preserves_u64_without_accepting_booleans(self):
        for value in (0, 134000000000000000, 2**64 - 1):
            gate.state_checks.strict({'actualJob': {'creationTime': value}}, gate.checks)
        for value in (True, False, -1, 2**64, 1.0, '1'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                gate.state_checks.strict({'actualJob': {'creationTime': value}}, gate.checks)

    def test_cli_rejects_mixed_modes_before_reading_or_submitting(self):
        script = str(Path(__file__).with_name('validate-update-activation-preparation-native.py'))
        result = subprocess.run([sys.executable, script, '--archive', 'absent.zip', '--target-archive', 'absent-target.zip',
                                 '--cache-run', 'absent', '--reference-pin', 'absent.json',
                                 '--activation-state', '--activation-process'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('not allowed with argument', result.stderr)


if __name__ == '__main__':
    unittest.main()
