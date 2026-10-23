#!/usr/bin/env python3
"""Report/path/process admission checks without submitting Windows work."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('support', Path(__file__).with_name('validate-update-support-native.py'))
support = importlib.util.module_from_spec(spec)
spec.loader.exec_module(support)


class ReadyChecks(unittest.TestCase):
    def package(self, root):
        raw = b'{"passed":true}'
        (root/'report.json').write_bytes(raw)
        ready = dict(schemaVersion=1, files=[dict(path='report.json', bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())])
        (root/'report.ready.json').write_text(json.dumps(ready))
        return ready

    def test_changed_bytes_and_duplicate_entries_are_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            ready = self.package(root)
            self.assertEqual(support.ready_files(root)['report.json'], b'{"passed":true}')
            (root/'report.json').write_bytes(b'{"passed":false}')
            with self.assertRaises(ValueError):
                support.ready_files(root)
            self.package(root)
            ready['files'].append(dict(ready['files'][0]))
            (root/'report.ready.json').write_text(json.dumps(ready))
            with self.assertRaises(ValueError):
                support.ready_files(root)

    def test_traversal_symlinks_and_boolean_counters_are_rejected(self):
        for mutation in ('traversal', 'symlink', 'bool'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as d:
                root = Path(d)
                ready = self.package(root)
                if mutation == 'traversal':
                    ready['files'][0]['path'] = '../report.json'
                elif mutation == 'symlink':
                    (root/'original.json').write_bytes((root/'report.json').read_bytes())
                    (root/'report.json').unlink()
                    (root/'report.json').symlink_to(root/'original.json')
                else:
                    ready['schemaVersion'] = True
                (root/'report.ready.json').write_text(json.dumps(ready))
                with self.assertRaises(ValueError):
                    support.ready_files(root)


class ProcessChecks(unittest.TestCase):
    def test_stamp_preserves_birth_and_session_and_rejects_boolean_pid(self):
        process = dict(processId=123, sessionId=1, birth='134372474158590666', hostId='a'*64)
        self.assertNotEqual(support.stamp(process), support.stamp(dict(process, birth='134372474158590667')))
        self.assertNotEqual(support.stamp(process), support.stamp(dict(process, sessionId=2)))
        for changed in ({'processId': True}, {'sessionId': True}, {'birth': ''}, {'hostId': 'other'}, {'extra': True}):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                support.stamp(dict(process, **changed))

    def test_local_extended_path_matches_only_same_windows_path(self):
        self.assertEqual(support.windows_path(r'\\?\K:\fixture\image.exe'), support.windows_path(r'K:\fixture\image.exe'))
        self.assertNotEqual(support.windows_path(r'\\host\fixture\image.exe'), support.windows_path(r'K:\fixture\image.exe'))


class LostAckChecks(unittest.TestCase):
    def test_recovery_finds_exact_accepted_request_without_submit(self):
        request = dict(projectId='snapshot-fixture', idempotencyKey='saved-key', command={'executable': 'pwsh.exe'})
        task = dict(taskId='existing', submission=request)
        def call(endpoint, q):
            if q['method'] == 'history':
                return {'page': dict(records=[dict(operationId='existing')], hasMore=False, nextCursor=None)}
            self.assertEqual(q, dict(method='get_task', taskId='existing'))
            return {'task': task}
        with patch.object(support.native, 'call', call):
            self.assertEqual(support.recover_task('unused', request), task)

    def test_different_payload_or_missing_task_never_falls_back_to_submit(self):
        request = dict(projectId='snapshot-fixture', idempotencyKey='saved-key')
        for exists in (False, True):
            def call(endpoint, q):
                if q['method'] == 'history':
                    return {'page': dict(records=[dict(operationId='different')] if exists else [], hasMore=False, nextCursor=None)}
                self.assertEqual(q['method'], 'get_task')
                return {'task': dict(submission=dict(request, command='changed'))}
            with patch.object(support.native, 'call', call), self.assertRaises(ValueError):
                support.recover_task('unused', request)

    def test_recovery_is_bounded_and_atomic_pending_metadata_survives_missing_ack(self):
        request = dict(projectId='snapshot-fixture', idempotencyKey='saved-key')
        calls = []
        def call(endpoint, q):
            self.assertEqual(q['method'], 'history')
            calls.append(q)
            return {'page': dict(records=[], hasMore=True, nextCursor=dict(createdAtMs=len(calls), operationId=str(len(calls))))}
        with patch.object(support.native, 'call', call), self.assertRaises(ValueError):
            support.recover_task('unused', request, max_pages=2)
        self.assertEqual(len(calls), 2)
        with patch.object(support.time, 'monotonic', side_effect=[0, 31]), patch.object(support.native, 'call') as network:
            with self.assertRaises(ValueError):
                support.recover_task('unused', request)
            network.assert_not_called()
        with tempfile.TemporaryDirectory() as d:
            path = Path(d)/'pending.json'
            support.save_meta(path, dict(taskId=None, request=request))
            self.assertIsNone(json.loads(path.read_text())['taskId'])
            support.save_meta(path, dict(taskId='accepted', request=request))
            self.assertEqual(json.loads(path.read_text())['taskId'], 'accepted')
            self.assertEqual(list(path.parent.iterdir()), [path])


if __name__ == '__main__':
    unittest.main()
