#!/usr/bin/env python3
"""Type/privacy tests for real process evidence; never a substitute native proof."""
import importlib.util
from pathlib import Path
import unittest
s=importlib.util.spec_from_file_location('process_gate',Path(__file__).with_name('validate-update-activation-preparation-native.py'))
gate=importlib.util.module_from_spec(s);s.loader.exec_module(gate)
class ProcessEvidenceTests(unittest.TestCase):
    def test_unknown_live_observations_are_typed_nullable_booleans(self):
        for key in ('processCustodyHeld','processAlive','outputIncomplete','captureComplete'):
            for value in (None,True,False):gate.process_checks.strict({key:value},gate.checks)
            for value in (0,1,'true',[],{}):
                with self.subTest(key=key,value=value),self.assertRaises(ValueError):gate.process_checks.strict({key:value},gate.checks)
    def test_windows_filetime_is_exact_u64_and_not_a_json_safe_counter(self):
        for value in (0,134000000000000000,2**64-1):gate.process_checks.strict({'creationTime':value},gate.checks)
        for value in (False,True,-1,2**64,1.0,'1'):
            with self.subTest(value=value),self.assertRaises(ValueError):gate.process_checks.strict({'creationTime':value},gate.checks)
    def test_no_serialized_state_permission_in_any_retained_copy(self):
        for key in ('stateGrant','canHandoff','writerOwned','canWriteState','nonce','token','permit'):
            with self.subTest(key=key),self.assertRaises(ValueError):gate.process_checks.strict({'history':[{'nested':{key:True}}]},gate.checks)
if __name__=='__main__':unittest.main()
