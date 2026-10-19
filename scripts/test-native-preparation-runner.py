#!/usr/bin/env python3
"""Check admission ordering without a Windows process or a real controller."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SCRIPT = Path(__file__).with_name('validate-workflow-native.py')

class SubmissionBoundary(Exception):
    pass

class PreparationAdmission(unittest.TestCase):
    def test_updater_schema_follows_pinned_source_and_rejects_ambiguity(self):
        spec = importlib.util.spec_from_file_location('native_schema', SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for version in (6, 7, 8, 9):
            self.assertEqual(module.updater_schema_from_source(f'pub const UPDATE_SCHEMA_VERSION: u32 = {version};\r\n'.encode()), version)
        for source in (b'', b'pub const UPDATE_SCHEMA_VERSION: u32 = 0;\n',
                       b'pub const UPDATE_SCHEMA_VERSION: u32 = 4294967296;\n',
                       b'pub const UPDATE_SCHEMA_VERSION: u32 = 7;\npub const UPDATE_SCHEMA_VERSION: u32 = 8;\n'):
            with self.assertRaises(ValueError):
                module.updater_schema_from_source(source)

    def run_to_submission(self, active):
        spec = importlib.util.spec_from_file_location('native_runner', SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            shared = root / 'shared'
            shared.mkdir()
            archive = shared / 'candidate.zip'
            with zipfile.ZipFile(archive, 'w') as z:
                # Exercise the same feature flag that previously read the final
                # proof before any Windows task had been accepted.
                z.writestr('winbridge/apps/update/src/artifact.rs', '// fixture')
            config = root / 'controller.json'
            config.write_text(json.dumps({'stateDirectory': str(root / 'state'), 'instanceId':'unit', 'delivery':{'root':str(shared), 'peerRoot':'K:\\shared', 'namespace':'WinBridge'}}))
            info = {'platform':'windows', 'activeTaskCount':active, 'projects':[{'id':'snapshot-fixture','allowDirectCommands':True,'workspaceRoot':'K:\\fixture'}]}
            submissions = []
            def call(endpoint, request):
                if request['method'] == 'info':
                    return {'info':info}
                self.assertEqual(request['method'], 'submit')
                submissions.append(request['request'])
                raise SubmissionBoundary()
            argv = [str(SCRIPT), '--controller-config', str(config), '--archive', str(archive), '--update-preparation-only']
            with patch.object(module, 'ROOT', root), patch.object(module, 'call', call), patch.object(sys, 'argv', argv):
                if active:
                    with self.assertRaisesRegex(RuntimeError, 'healthy, idle'):
                        module.main()
                    self.assertEqual(submissions, [])
                else:
                    with self.assertRaises(SubmissionBoundary):
                        module.main()
                    self.assertEqual(len(submissions), 1)
                    self.assertEqual(submissions[0]['projectId'], 'snapshot-fixture')
                    self.assertEqual(submissions[0]['command']['executable'], 'pwsh.exe')
                    self.assertTrue(submissions[0]['idempotencyKey'].startswith('native-workflow-'))

    def test_artifact_preparation_reaches_submit_without_reading_final_proofs(self):
        self.run_to_submission(0)

    def test_active_user_work_stops_before_submit(self):
        self.run_to_submission(1)

if __name__ == '__main__':
    unittest.main()
