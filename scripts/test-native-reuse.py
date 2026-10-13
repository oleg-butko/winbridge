#!/usr/bin/env python3
"""Fault tests for native gate source/cache reuse admission."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('native', Path(__file__).with_name('validate-workflow-native.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


class ReuseAdmission(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name) / 'validation'
        self.run = '20261013-164022-975ce2c4'
        self.report = self.base / self.run
        self.report.mkdir(parents=True)
        self.result = dict(passed=True, diagnosticOnly=True, sourceDirty=False,
                           platform='windows', sourceArchiveSha256='archive-sha',
                           sourceCommit='commit', commands=[
                               dict(name='process-resume-guard', exitCode=0),
                               dict(name='process-runtime-recovery-diagnostic', exitCode=0)])
        for name in ['process-resume-guard.log', 'process-runtime-recovery-diagnostic.log']:
            (self.report / name).write_text('native receipt\n')
        self.publish()

    def publish(self):
        (self.report / 'report.json').write_text(json.dumps(self.result))
        files = []
        for name in ['report.json', 'process-resume-guard.log', 'process-runtime-recovery-diagnostic.log']:
            data = (self.report / name).read_bytes()
            files.append(dict(path=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest()))
        (self.report / 'report.ready.json').write_text(json.dumps(dict(passed=True, platform='windows', files=files)))

    def verify(self, sha='archive-sha', run=None):
        return native.verify_reusable_diagnostic(self.base, run or self.run, sha)

    def test_valid_receipt_and_wrong_archive(self):
        self.assertEqual(self.verify()['sourceCommit'], 'commit')
        with self.assertRaisesRegex(ValueError, 'exact clean archive'):
            self.verify('different-archive')

    def test_tampered_artifact(self):
        (self.report / 'process-resume-guard.log').write_text('altered')
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            self.verify()

    def test_failed_dirty_full_or_incomplete_diagnostic(self):
        for field, value in [('passed', False), ('sourceDirty', True), ('diagnosticOnly', False)]:
            with self.subTest(field=field):
                old = self.result[field]
                self.result[field] = value
                self.publish()
                with self.assertRaises(ValueError):
                    self.verify()
                self.result[field] = old
        self.result['commands'].pop()
        self.publish()
        with self.assertRaisesRegex(ValueError, 'test groups'):
            self.verify()

    def test_paths_and_links_cannot_escape_report_namespace(self):
        with self.assertRaises(ValueError):
            self.verify(run='../other')
        ready = json.loads((self.report / 'report.ready.json').read_text())
        ready['files'][0]['path'] = '../outside'
        (self.report / 'report.ready.json').write_text(json.dumps(ready))
        with self.assertRaisesRegex(ValueError, 'Unsafe'):
            self.verify()
        self.publish()
        path = self.report / 'process-resume-guard.log'
        path.unlink()
        path.symlink_to(self.report / 'process-runtime-recovery-diagnostic.log')
        with self.assertRaisesRegex(ValueError, 'Unsafe'):
            self.verify()


if __name__ == '__main__':
    unittest.main()
