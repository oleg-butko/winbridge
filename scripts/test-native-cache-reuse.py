"""Verify cache admission with failed tests, tampered receipts and incomplete builds."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('native', Path(__file__).with_name('validate-workflow-native.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


class CacheReceiptTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.run = '20261014-042057-8075f8f1'
        self.report = self.base / self.run
        self.report.mkdir()
        self.result = dict(platform='windows', passed=False, sourceDirty=False, diagnosticOnly=False,
                           sourceCommit='a' * 40, sourceArchiveSha256='b' * 64,
                           commands=[dict(name=n, exitCode=101 if n.endswith('writers') else 0)
                                     for n in ['task-cas-pure-fsm', 'task-cas-process-resume-guard',
                                               'task-cas-fenced-contract', 'task-cas-frozen-contracts',
                                               'task-cas-native-writers']])
        self.log = 'Finished `test` profile [unoptimized + debuginfo]\ntest result: FAILED. 7 passed; 3 failed\n'
        self.publish()

    def publish(self):
        (self.report / 'report.json').write_text(json.dumps(self.result))
        (self.report / 'task-cas-native-writers.log').write_text(self.log)
        files = []
        for name in ['report.json', 'task-cas-native-writers.log']:
            data = (self.report / name).read_bytes()
            files.append(dict(path=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest()))
        (self.report / 'report.ready.json').write_text(json.dumps(dict(platform='windows', passed=self.result['passed'], files=files)))

    def check(self):
        return native.verify_reusable_build_cache(self.base, self.run)

    def test_test_failure_can_supply_only_compiler_cache(self):
        receipt = self.check()
        self.assertEqual(receipt['sourceCommit'], 'a' * 40)
        self.assertNotIn('passed', receipt)
        self.assertEqual(receipt['reportSha256'], hashlib.sha256((self.report / 'report.json').read_bytes()).hexdigest())

    def test_tampered_log_is_refused(self):
        (self.report / 'task-cas-native-writers.log').write_text('tampered')
        with self.assertRaisesRegex(ValueError, 'checksum'):
            self.check()

    def test_compiler_failure_without_execution_is_refused(self):
        self.log = 'error: could not compile winbridge-worker\n'
        self.publish()
        with self.assertRaisesRegex(ValueError, 'compilation'):
            self.check()

    def test_missing_group_and_dirty_source_are_refused(self):
        for key, value in [('commands', self.result['commands'][:4]), ('sourceDirty', True)]:
            original = self.result[key]
            self.result[key] = value
            self.publish()
            with self.assertRaises(ValueError):
                self.check()
            self.result[key] = original

    def test_traversal_and_duplicate_artifact_are_refused(self):
        with self.assertRaisesRegex(ValueError, 'run ID'):
            native.verify_reusable_build_cache(self.base, '../' + self.run)
        ready_path = self.report / 'report.ready.json'
        ready = json.loads(ready_path.read_text())
        ready['files'].append(ready['files'][0])
        ready_path.write_text(json.dumps(ready))
        with self.assertRaisesRegex(ValueError, 'Unsafe'):
            self.check()

    def test_linked_cache_receipt_is_refused(self):
        linked_run = '20261014-042057-00000000'
        (self.base / linked_run).symlink_to(self.report, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'namespace'):
            native.verify_reusable_build_cache(self.base, linked_run)

    def borrowed_run(self, parent_receipt):
        self.run = '20261014-044152-738951fb'
        self.report = self.base / self.run
        self.report.mkdir()
        self.result['buildCacheReusedFrom'] = parent_receipt
        self.result['buildCacheDirectory'] = 'K:\\project\\workflow-validation\\' + parent_receipt['runId'] + '\\test-build-cache'
        self.publish()

    def test_borrowed_cache_resolves_to_verified_original_owner(self):
        owner = self.check()
        self.borrowed_run(owner)
        self.assertEqual(self.check(), owner)

    def test_tampered_parent_and_wrong_directory_are_refused(self):
        owner = self.check()
        self.borrowed_run(dict(owner, reportSha256='0' * 64))
        with self.assertRaisesRegex(ValueError, 'identity mismatch'):
            self.check()
        self.result['buildCacheReusedFrom'] = owner
        self.result['buildCacheDirectory'] = 'K:\\project\\workflow-validation\\' + self.run + '\\test-build-cache'
        self.publish()
        with self.assertRaisesRegex(ValueError, 'directory differs'):
            self.check()

    def test_cycle_in_cache_provenance_is_refused(self):
        owner = self.check()
        self.result['buildCacheReusedFrom'] = owner
        self.publish()
        with self.assertRaisesRegex(ValueError, 'Cyclic'):
            self.check()

    def test_missing_cache_fallback_requires_a_new_scoped_directory_and_explicit_reason(self):
        owner = self.check()
        local = 'K:\\project\\workflow-validation\\20261014-060000-aaaaaaaa'
        result = dict(buildCacheRequested=owner, buildCacheReuseSkipped='cache_missing', buildCacheDirectory=local+'\\test-build-cache')
        native.verify_build_cache_usage(result, owner, local)
        for field, value in [('buildCacheReuseSkipped', 'ignored'), ('buildCacheDirectory', 'K:\\other\\test-build-cache'), ('buildCacheRequested', None)]:
            wrong = dict(result, **{field: value})
            with self.assertRaises(RuntimeError):
                native.verify_build_cache_usage(wrong, owner, local)


if __name__ == '__main__':
    unittest.main()
