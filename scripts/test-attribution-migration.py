#!/usr/bin/env python3
"""Actual Linux schema17 history ->18; create-only fixtures, no Windows/config reuse."""
import argparse
import hashlib
import json
from pathlib import Path
import signal
import socket
import sqlite3
import struct
import subprocess
import tempfile
import time


def call(endpoint, request):
    payload = json.dumps(request).encode()
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(3)
        stream.connect(str(endpoint))
        stream.sendall(struct.pack('<I', len(payload)) + payload)

        def exact(size):
            result = b''
            while len(result) < size:
                part = stream.recv(size - len(result))
                if not part:
                    raise RuntimeError('Incomplete fixture IPC reply')
                result += part
            return result

        size = struct.unpack('<I', exact(4))[0]
        assert size <= 1024 * 1024
        return json.loads(exact(size))


def until(check):
    deadline = time.monotonic() + 12
    while True:
        result = check()
        if result:
            return result
        assert time.monotonic() < deadline, 'Owned fixture deadline'
        time.sleep(.02)


class Worker:
    def __init__(self, exe, config, endpoint, root, name):
        self.log = (root / (name + '.log')).open('w')
        self.child = subprocess.Popen([str(exe), '--config', str(config), 'serve'], stdout=self.log, stderr=subprocess.STDOUT)
        self.endpoint = endpoint

    def ready(self):
        def observe():
            assert self.child.poll() is None, 'Fixture worker exited before readiness'
            try:
                return call(self.endpoint, {'method': 'info'}).get('info')
            except OSError:
                return None
        return until(observe)

    def close(self):
        if self.child.poll() is None:
            self.child.send_signal(signal.SIGINT)
            try:
                self.child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.child.kill()
                self.child.wait()
                raise AssertionError('Owned fixture required force termination')
        self.log.close()


def rows(path):
    with sqlite3.connect(f'file:{path}?mode=ro', uri=True) as db:
        return db.execute('SELECT id,payload,record FROM tasks ORDER BY id').fetchall()


def main():
    args = argparse.ArgumentParser()
    args.add_argument('--reference', type=Path, required=True, help='Pinned schema17 reference.json from a private rehearsal')
    args.add_argument('--worker', type=Path, required=True, help='Current Linux worker executable')
    options = args.parse_args()
    reference = json.loads(options.reference.read_text())
    old = Path(reference['path'])
    assert reference['report']['workerJournalSchema'] == 17
    assert hashlib.sha256(old.read_bytes()).hexdigest() == reference['sha256']
    with tempfile.TemporaryDirectory(prefix='wb-attr-') as temporary:
        root = Path(temporary)
        config = root / 'worker.json'
        state = root / 'state'
        endpoint = state / 'worker-attr-fixture.sock'
        config.write_text(json.dumps({'schemaVersion': 1, 'instanceId': 'attr-fixture', 'nodeId': 'worker', 'stateDirectory': str(state),
                                     'worker': {'keepRunningOnUiExit': True}, 'projects': [{'id': 'fixture', 'workspaceRoot': str(root), 'allowDirectCommands': True, 'maxConcurrentMutations': 1, 'recipes': []}]}))
        submission = {'protocolVersion': 1, 'requestId': 'original', 'projectId': 'fixture', 'idempotencyKey': 'old-key', 'timeoutMs': 5000,
                      'command': {'kind': 'direct', 'executable': '/bin/sh', 'arguments': ['-c', "printf 'old\\n' >> runs.log"], 'workingDirectory': '.'}}
        db = state / 'journal.sqlite'
        processes = []
        try:
            first = Worker(old, config, endpoint, root, 'old17'); processes.append(first)
            info = first.ready(); context = info['fencedRequests']['context']

            def submit(request):
                reply = call(endpoint, {'requestContractVersion': 2, 'context': context, 'request': {'method': 'submit', 'request': request}})
                assert reply['outcome'] == 'responded', reply
                return reply['response']['task']

            def finish(task):
                def terminal():
                    record = call(endpoint, {'method': 'get_task', 'taskId': task['taskId']})['task']
                    return record if record['status'] not in ['running', 'queued'] else None
                result = until(terminal)
                assert result['status'] == 'succeeded', result

            task = submit(submission); finish(task); first.close()
            before = rows(db)
            with sqlite3.connect(db) as connection:
                assert connection.execute('PRAGMA user_version').fetchone()[0] == 17
            second = Worker(options.worker, config, endpoint, root, 'new18'); processes.append(second)
            new_info = second.ready(); assert new_info['fencedRequests']['context'] == context

            def attribution(task_id):
                response = call(endpoint, {'extensionVersion': 1, 'contract': {'id': 'winbridge.operation-attribution', 'version': 1}, 'context': context,
                                           'payload': {'kind': 'task', 'operationId': task_id}})
                assert response['result']['kind'] == 'data', response
                return response['result']['payload']

            assert attribution(task['taskId'])['creator'] is None
            retry = dict(submission, requestId='retry-new-worker')
            assert submit(retry)['taskId'] == task['taskId']
            assert attribution(task['taskId'])['creator'] is None
            assert rows(db) == before
            backups = list(state.glob('journal-v17-*.sqlite')); assert len(backups) == 1
            assert rows(backups[0]) == before
            fresh = json.loads(json.dumps(submission)); fresh['idempotencyKey'] = 'new-key'; fresh['command']['arguments'][1] = "printf 'new\\n' >> runs.log"
            new_task = submit(fresh); finish(new_task)
            creator = attribution(new_task['taskId'])['creator']
            assert creator == {'kind': 'local', 'workerGeneration': new_info['generation']}
            second.close()
            third = Worker(options.worker, config, endpoint, root, 'reopened18'); processes.append(third)
            reopened = third.ready(); assert reopened['generation'] != new_info['generation']
            assert attribution(new_task['taskId'])['creator'] == creator
            assert submit(fresh)['taskId'] == new_task['taskId']
            assert (root / 'runs.log').read_text().splitlines() == ['old', 'new']
            third.close()
            frozen = hashlib.sha256(db.read_bytes()).hexdigest()
            incompatible = subprocess.run([str(old), '--config', str(config), 'serve'], capture_output=True, text=True, timeout=8)
            assert incompatible.returncode != 0 and 'schema 18' in incompatible.stderr, incompatible.stderr
            assert hashlib.sha256(db.read_bytes()).hexdigest() == frozen
            with sqlite3.connect(db) as connection:
                assert connection.execute('PRAGMA user_version').fetchone()[0] == 18
            print(json.dumps({'passed': True, 'platform': 'linux', 'fromSchema': 17, 'toSchema': 18, 'oldHistoryPreserved': True,
                              'verifiedMigrationBackup': True, 'legacyCreatorUnknownAfterRetry': True, 'creatorSurvivesRestart': True,
                              'externalExecutions': 2, 'oldWorkerRefusesWithoutJournalWrite': True}, indent=2))
        finally:
            for process in processes:
                process.close()


if __name__ == '__main__':
    main()
