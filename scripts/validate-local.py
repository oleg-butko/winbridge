#!/usr/bin/env python3
"""Real Unix development-adapter/IPC checks; no Windows runtime claims."""
import concurrent.futures
import json
import os
from pathlib import Path
import signal
import socket
import stat
import struct
import subprocess
import time
import uuid

REPO = Path(__file__).resolve().parents[1]
RUN = REPO / '.winbridge' / 'validation' / (time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8])
PROJECT = RUN / 'project with spaces'
CONFIG = RUN / 'worker.local.json'
WORKER = REPO / 'target/debug/winbridge-worker'
FIXTURE = REPO / 'target/debug/winbridge-fixture'
PROJECT.mkdir(parents=True)
INSTANCE = 'local-' + uuid.uuid4().hex[:12]
SOCKET = RUN / 'state' / ('worker-' + INSTANCE + '.sock')
CONFIG.write_text(json.dumps({
    'schemaVersion': 1, 'instanceId': INSTANCE, 'nodeId': 'linux-validation',
    'stateDirectory': str(RUN / 'state'), 'maxOutputBytesPerTask': 4096,
    'worker': {'keepRunningOnUiExit': True},
    'projects': [{'id': 'fixture', 'workspaceRoot': str(PROJECT), 'allowDirectCommands': True,
                  'maxConcurrentMutations': 1, 'recipes': []}],
}), encoding='utf-8')


def receive_exact(stream, length):
    data = bytearray()
    while len(data) < length:
        chunk = stream.recv(length - len(data))
        if not chunk:
            raise RuntimeError('IPC ended before the response was complete')
        data.extend(chunk)
    return bytes(data)


def call(request, expect_error=False, disconnect=False):
    data = json.dumps(request).encode()
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(10)
        stream.connect(str(SOCKET))
        stream.sendall(struct.pack('<I', len(data)) + data)
        if disconnect:
            return None
        length, = struct.unpack('<I', receive_exact(stream, 4))
        assert 0 < length <= 1024 * 1024
        response = json.loads(receive_exact(stream, length))
    assert (response['kind'] == 'error') == expect_error, response
    return response


def request(key, mode, timeout=10000):
    return {'protocolVersion': 1, 'requestId': 'request-' + uuid.uuid4().hex,
            'projectId': 'fixture', 'idempotencyKey': key, 'timeoutMs': timeout,
            'command': {'kind': 'direct', 'executable': str(FIXTURE), 'arguments': [mode], 'workingDirectory': '.'}}


def submit(value, **options):
    return call({'method': 'submit', 'request': value}, **options)


def get(task_id):
    return call({'method': 'get_task', 'taskId': task_id})['task']


def wait(task_id, expected):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        task = get(task_id)
        if task['status'] not in ['queued', 'running']:
            assert task['status'] == expected, task
            return task
        time.sleep(.025)
    raise RuntimeError('task deadline exceeded')


def output(task_id):
    chunks = []
    after = 0
    while True:
        page = call({'method': 'read_output', 'taskId': task_id, 'afterSequence': after})['chunks']
        if not page:
            return chunks
        chunks.extend(page)
        after = page[-1]['sequence']


def save_report(task, name):
    chunks = output(task['taskId'])
    report = {'schemaVersion': 1, 'platform': 'linux', 'task': task, 'output': chunks, 'outputComplete': True}
    (RUN / (name + '.task-report.json')).write_text(json.dumps(report, ensure_ascii=False), encoding='utf-8')
    return {stream: b''.join(bytes(c['bytes']) for c in chunks if c['stream'] == stream).decode('utf-8') for stream in ['stdout', 'stderr']}


def start():
    log = open(RUN / ('worker-' + uuid.uuid4().hex[:8] + '.log'), 'wb')
    process = subprocess.Popen([str(WORKER), '--config', str(CONFIG), 'serve'], stdout=log, stderr=log)
    log.close()
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        assert process.poll() is None, 'worker exited before becoming ready'
        try:
            call({'method': 'info'})
            return process
        except (OSError, RuntimeError):
            time.sleep(.05)
    raise RuntimeError('IPC startup deadline exceeded')


process = None
crash_pid = None
checks = []
try:
    process = start()
    assert stat.S_IMODE(SOCKET.stat().st_mode) == 0o600
    assert stat.S_IMODE(SOCKET.parent.stat().st_mode) == 0o700
    checks.append('user-owned IPC permissions')
    duplicate_worker = subprocess.run([str(WORKER), '--config', str(CONFIG), 'serve'], capture_output=True, timeout=5)
    assert duplicate_worker.returncode != 0
    checks.append('exclusive worker ownership')

    with concurrent.futures.ThreadPoolExecutor(max_workers=16) as pool:
        accepted = list(pool.map(lambda _: submit(request('concurrent', 'success'))['task'], range(16)))
    assert len({task['taskId'] for task in accepted}) == 1
    success = wait(accepted[0]['taskId'], 'succeeded')
    streams = save_report(success, 'success')
    assert 'stdout: success Привет ✓' in streams['stdout'] and 'stderr: diagnostic' in streams['stderr']
    assert len((PROJECT / 'runs.log').read_text().splitlines()) == 1
    submit(request('concurrent', 'failure'), expect_error=True)
    checks.append('concurrent dedup, literal output, payload conflict')

    submit(request('lost-ack', 'success'), disconnect=True)
    same = submit(request('lost-ack', 'success'))['task']
    wait(same['taskId'], 'succeeded')
    assert len((PROJECT / 'runs.log').read_text().splitlines()) == 2
    checks.append('disconnected acceptance response, no second launch')

    failure = wait(submit(request('failure', 'failure'))['task']['taskId'], 'failed')
    assert failure['exitCode'] == 7
    save_report(failure, 'failure')
    flood = wait(submit(request('flood', 'flood'))['task']['taskId'], 'succeeded')
    assert flood['outputBytes'] == 4096 and flood['outputTruncated']
    timed = wait(submit(request('timeout', 'tree', 500))['task']['taskId'], 'failed')
    assert 'timeout' in timed['reason']
    checks.append('failure, stdout/stderr pressure, quota, tree timeout')

    tree = submit(request('cancel', 'tree'))['task']
    deadline = time.monotonic() + 5
    while get(tree['taskId'])['outputBytes'] < 60:
        assert time.monotonic() < deadline
        time.sleep(.025)
    queued = submit(request('queued', 'success'))['task']
    assert queued['status'] == 'queued'
    assert call({'method': 'cancel', 'taskId': queued['taskId']})['task']['status'] == 'cancelled'
    call({'method': 'cancel', 'taskId': tree['taskId']})
    save_report(wait(tree['taskId'], 'cancelled'), 'cancelled')
    checks.append('one mutating command per project, queued/tree cancellation')

    for cwd in ['..', 'NUL.txt', 'C:\\outside', '\\\\host\\share']:
        invalid = request('invalid-' + uuid.uuid4().hex, 'success')
        invalid['command']['workingDirectory'] = cwd
        submit(invalid, expect_error=True)
    os.symlink(RUN, PROJECT / 'escape')
    invalid = request('symlink', 'success')
    invalid['command']['workingDirectory'] = 'escape'
    submit(invalid, expect_error=True)
    checks.append('traversal/reserved/UNC/symlink rejection')

    for size in [0, 1024 * 1024 + 1]:
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(3)
            stream.connect(str(SOCKET))
            stream.sendall(struct.pack('<I', size))
            assert stream.recv(1) == b''
    checks.append('bounded IPC frames')

    crash_request = request('crash', 'long', 60000)
    crashed = submit(crash_request)['task']
    deadline = time.monotonic() + 5
    while not get(crashed['taskId'])['processId']:
        assert time.monotonic() < deadline
        time.sleep(.025)
    crash_pid = get(crashed['taskId'])['processId']
    process.kill()
    process.wait()
    # The Unix test adapter does not promise Windows Job Object crash cleanup.
    os.killpg(crash_pid, signal.SIGKILL)
    crash_pid = None
    before = len((PROJECT / 'runs.log').read_text().splitlines())
    process = start()
    recovered = get(crashed['taskId'])
    assert recovered['status'] == 'indeterminate'
    assert submit(crash_request)['task']['taskId'] == crashed['taskId']
    time.sleep(.1)
    assert len((PROJECT / 'runs.log').read_text().splitlines()) == before
    assert get(success['taskId'])['status'] == 'succeeded'
    save_report(recovered, 'indeterminate')
    checks.append('crash journal recovery; no automatic retry; terminal result persisted')
    (RUN / 'validation-report.json').write_text(json.dumps({'platform': 'linux', 'checks': checks, 'windowsVerified': False}, indent=2))
    print(json.dumps({'passed': checks, 'reports': str(RUN)}, ensure_ascii=False, indent=2))
finally:
    if crash_pid:
        try:
            os.killpg(crash_pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if process and process.poll() is None:
        process.send_signal(signal.SIGINT)
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            raise RuntimeError('worker did not complete graceful shutdown within 10 seconds')
