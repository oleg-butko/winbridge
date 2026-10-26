#!/usr/bin/env python3
"""Reject incomplete/misbound native preparation reports; no Windows execution."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('native', Path(__file__).with_name('validate-workflow-native.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def fixture():
    groups, files = [], {}
    for name, package, selector, count in (
            ('service-history-native', 'winbridge-update', 'journal::activation_service', 13),
            ('service-transfer-native', 'winbridge-worker', 'store::activation_service_open', 6)):
        log = name + '.log'
        groups.append(dict(name=name, executable=r'C:\Rust\bin\cargo.exe',
                           arguments=['test', '-p', package, '--features', 'bundled-sqlite', '--lib', '--locked', '--offline', selector, '--', '--test-threads=1', '--nocapture'],
                           exitCode=0, log=log))
        files[log] = ('SERVICE_ADMISSION_BUILD ' + json.dumps(dict(sourceCommit='a'*40, sourceDirty=False, sourceManifestSha256='b'*64, buildId='fixture-build')) + '\n').encode() + f'test result: ok. {count} passed; 0 failed; 0 ignored; 80 filtered out; finished in 0.2s\n'.encode()
    return dict(passed=True, serviceAdmissionOnly=True, platform='windows', sourceDirty=False, sourceCommit='a'*40, commands=groups), files


class Proof(unittest.TestCase):
    def test_counts_sync_and_async_native_tests_from_pinned_source(self):
        source = b'#[test]\nfn sync() {}\n#[tokio::test(flavor = "multi_thread", worker_threads = 2)]\nasync fn ipc() {}\n#[tokio::test]\r\nasync fn other() {}\nfn helper() {}'
        self.assertEqual(native.service_source_test_count(source), 3)

    def test_accepts_exact_complete_preparation_only(self):
        report, files = fixture()
        native.verify_service_admission(report, files, [13, 6], 'b'*64)

    def test_accepts_inline_libtest_prefix_on_compiled_build_marker(self):
        report, files = fixture()
        for name in files:
            files[name] = b'test module::case ... ' + files[name]
        native.verify_service_admission(report, files, [13, 6], 'b'*64)

    def test_rejects_untyped_flags_exit_codes_wrong_scope_and_commands(self):
        for index in range(9):
            report, files = fixture()
            if index == 0:
                report['passed'] = 1
            elif index == 1:
                report['serviceAdmissionOnly'] = False
            elif index == 2:
                report['sourceDirty'] = 0
            elif index == 3:
                report['commands'][0]['exitCode'] = False
            elif index == 4:
                report['commands'].reverse()
            elif index == 5:
                report['commands'][0]['arguments'].append('--ignored')
            elif index == 6:
                report['commands'][0]['log'] = 'other.log'
                files['other.log'] = files['service-history-native.log']
            elif index == 7:
                report['commands'][0]['executable'] = r'C:\Rust\bin\pwsh.exe'
            else:
                report['commands'][0]['name'] = 'actual-service-ready'
            with self.subTest(index=index), self.assertRaises(ValueError):
                native.verify_service_admission(report, files, [13, 6], 'b'*64)

    def test_rejects_stale_missing_duplicate_or_untyped_compiled_identity(self):
        for index in range(7):
            report, files = fixture()
            name = 'service-history-native.log'
            marker, summary = files[name].split(b'\n', 1)
            build = json.loads(marker.removeprefix(b'SERVICE_ADMISSION_BUILD '))
            if index == 0:
                build['sourceCommit'] = 'c'*40
            elif index == 1:
                build['sourceManifestSha256'] = 'c'*64
            elif index == 2:
                build['sourceDirty'] = 0
            elif index == 3:
                build['buildId'] = True
            elif index == 4:
                files[name] = summary
            elif index == 5:
                files[name] += marker + b'\n'
            else:
                report['sourceCommit'] = None
                build['sourceCommit'] = None
            if index not in (4, 5):
                files[name] = ('SERVICE_ADMISSION_BUILD ' + json.dumps(build) + '\n').encode() + summary
            with self.subTest(index=index), self.assertRaises(ValueError):
                native.verify_service_admission(report, files, [13, 6], 'b'*64)

    def test_rejects_skipped_failed_missing_extra_or_untyped_test_counts(self):
        report, original = fixture()
        for text in ('', 'test result: ok. 0 passed; 0 failed; 0 ignored;',
                     'test result: ok. 13 passed; 0 failed; 1 ignored;',
                     'test result: FAILED. 12 passed; 1 failed; 0 ignored;',
                     'test result: ok. 13 passed; 0 failed; 0 ignored;\ntest result: ok. 1 passed; 0 failed; 0 ignored;'):
            files = copy.deepcopy(original)
            files['service-history-native.log'] = original['service-history-native.log'].splitlines(keepends=True)[0] + text.encode()
            with self.subTest(text=text), self.assertRaises(ValueError):
                native.verify_service_admission(report, files, [13, 6], 'b'*64)
        for counts in ([True, 6], [13, '6'], [13, 0], [13], [12, 6]):
            with self.subTest(counts=counts), self.assertRaises(ValueError):
                native.verify_service_admission(report, original, counts, 'b'*64)


if __name__ == '__main__':
    unittest.main()
