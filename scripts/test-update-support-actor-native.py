#!/usr/bin/env python3
"""Reject malformed JSON counters before accepting any native actor proof."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('actor', Path(__file__).with_name('validate-update-support-actor-native.py'))
actor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(actor)


class CounterChecks(unittest.TestCase):
    def test_nested_optional_counters_preserve_real_boolean_status(self):
        actor.strict_counters(dict(passed=True, records=[dict(revision=None, cleanupVerified=False),
                                                       dict(ownerEpoch=1, admittedAtMs=9007199254740991)]))

    def test_boolean_string_negative_and_overflow_are_not_counters(self):
        for key in ('schemaVersion', 'queryContract', 'revision', 'ownerEpoch', 'completedAtMs', 'bytes',
                    'supportRevision', 'beginDrainCount', 'gateRows'):
            for value in (True, False, '1', -1, 9007199254740992, 1.0):
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    actor.strict_counters(dict(records=[{key: value}]))

    def test_non_counter_bools_do_not_hide_nested_malformed_evidence(self):
        with self.assertRaises(ValueError):
            actor.strict_counters(dict(passed=True, cleanupVerified=True, record=dict(process=dict(processId=True))))


if __name__ == '__main__':
    unittest.main()
