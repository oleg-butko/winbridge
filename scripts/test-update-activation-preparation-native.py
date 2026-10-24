#!/usr/bin/env python3
"""Validate typed evidence and reject secrets before any gate proof is accepted."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('preparation', Path(__file__).with_name('validate-update-activation-preparation-native.py'))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class EvidenceChecks(unittest.TestCase):
    def test_preparation_counters_do_not_accept_boolean_or_fractional_success(self):
        for key in ('workerLaunchCount', 'stateHandoffCount', 'workerConfigSchema', 'workerJournalSchema',
                    'updaterJournalSchema', 'protocolVersion', 'startupTimeoutMs', 'gateRows', 'ownerEpoch'):
            for value in (True, False, '0', -1, 1.0, 9007199254740992):
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    gate.checks.strict({'nested': [{key: value}]})

    def test_private_material_and_fabricated_permissions_are_rejected_in_every_copy(self):
        for field in ('nonce', 'channelNonce', 'token', 'permit', 'canSpawn', 'canInstall'):
            with self.subTest(field=field), self.assertRaises(ValueError):
                gate.checks.strict({'history': [{'intent': {field: 'not-exportable'}}]})
        gate.checks.strict({'schemaVersion': 1, 'workerLaunchCount': 0, 'revision': None,
                            'physicalCustodyHeld': False, 'channelNonceSha256': 'a'*64})

    def test_utf8_canonical_binding_is_independent_of_object_key_order(self):
        self.assertEqual(gate.checks.canonical({'b': 'Привет ✓', 'a': 1}),
                         '{"a":1,"b":"Привет ✓"}'.encode())

    def test_extended_local_path_does_not_alias_network_path(self):
        self.assertEqual(gate.checks.windows_path(r'\\?\K:\isolated\runtime'), gate.checks.windows_path(r'K:\isolated\runtime'))
        self.assertNotEqual(gate.checks.windows_path(r'\\server\runtime'), gate.checks.windows_path(r'K:\runtime'))


if __name__ == '__main__':
    unittest.main()
