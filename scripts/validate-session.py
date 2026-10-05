#!/usr/bin/env python3
"""Real controller/WebSocket/worker checks on the Unix development adapter."""
import base64
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

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / '.winbridge' / 'session' / (time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8])
RUN.mkdir(parents=True)
PROJECT = RUN / 'project'
PROJECT.mkdir()
INSTANCE = 'pair-' + uuid.uuid4().hex[:6]
with socket.socket() as probe:
    probe.bind(('127.0.0.1', 0))
    PORT = probe.getsockname()[1]
CONTROLLER_CFG = RUN / 'controller.local.json'
WORKER_CFG = RUN / 'worker.local.json'
CONTROLLER_SOCKET = RUN / 'c' / ('controller-' + INSTANCE + '.sock')
WORKER_SOCKET = RUN / 'w' / ('worker-' + INSTANCE + '.sock')
FIXTURE = ROOT / 'target/debug/winbridge-fixture'
CONTROLLER_CFG.write_text(json.dumps({'schemaVersion': 1, 'instanceId': INSTANCE, 'nodeId': 'debian', 'stateDirectory': str(RUN / 'c'),
    'controller': {'listenAddress': '127.0.0.1:' + str(PORT), 'allowedWorkerAddresses': ['127.0.0.1'], 'workerNodeId': 'windows'}}))
WORKER_CFG.write_text(json.dumps({'schemaVersion': 1, 'instanceId': INSTANCE, 'nodeId': 'windows', 'stateDirectory': str(RUN / 'w'),
    'worker': {'controllerUrl': 'ws://127.0.0.1:' + str(PORT), 'keepRunningOnUiExit': True},
    'projects': [{'id': 'fixture', 'workspaceRoot': str(PROJECT), 'allowDirectCommands': True, 'maxConcurrentMutations': 1,
                  'recipes': [{'id': 'fixture-success', 'executable': str(FIXTURE), 'arguments': ['success'], 'workingDirectory': '.', 'timeoutMs': 10000}]}]}))


def exact(stream, count):
    data = bytearray()
    while len(data) < count:
        part = stream.recv(count - len(data))
        if not part:
            raise RuntimeError('IPC response ended early')
        data.extend(part)
    return bytes(data)


def call(request, endpoint=CONTROLLER_SOCKET, error=False, lose_ack=False):
    payload = json.dumps(request).encode()
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(12)
        stream.connect(str(endpoint))
        stream.sendall(struct.pack('<I', len(payload)) + payload)
        if lose_ack:
            return
        length, = struct.unpack('<I', exact(stream, 4))
        assert 0 < length <= 1048576
        result = json.loads(exact(stream, length))
    assert (result['kind'] == 'error') == error, result
    return result


def until(check, message, seconds=15):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = check()
            if value:
                return value
        except (OSError, RuntimeError):
            pass
        time.sleep(.05)
    raise AssertionError(message)


def start(role):
    config = CONTROLLER_CFG if role == 'controller' else WORKER_CFG
    log = open(RUN / (role + '-' + uuid.uuid4().hex[:6] + '.log'), 'wb')
    process = subprocess.Popen([str(ROOT / ('target/debug/winbridge-' + role)), '--config', str(config), 'serve'], stdout=log, stderr=log)
    log.close()
    endpoint = CONTROLLER_SOCKET if role == 'controller' else WORKER_SOCKET
    method = 'controller_status' if role == 'controller' else 'info'
    until(lambda: process.poll() is None and call({'method': method}, endpoint), role + ' unavailable')
    return process


def stop(process, sig=signal.SIGINT):
    if process is None or process.poll() is not None:
        return
    process.send_signal(sig)
    try:
        process.wait(10)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
        raise AssertionError('process shutdown exceeded deadline')


def request(key, mode, timeout=10000):
    return {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'fixture', 'idempotencyKey': key, 'timeoutMs': timeout,
            'command': {'kind': 'direct', 'executable': str(FIXTURE), 'arguments': [mode], 'workingDirectory': '.'}}


def submit(value, **kwargs):
    return call({'method': 'submit', 'request': value}, **kwargs)


def get(task_id, endpoint=CONTROLLER_SOCKET):
    return call({'method': 'get_task', 'taskId': task_id}, endpoint)['task']


def terminal(task_id, expected, endpoint=CONTROLLER_SOCKET):
    def check():
        task = get(task_id, endpoint)
        if task['status'] in ['queued', 'running']:
            return False
        assert task['status'] == expected, task
        return task
    return until(check, 'task did not finish')


def connected():
    return call({'method': 'controller_status'})['status']['connected']


def output(task_id):
    chunks = []
    after = 0
    while True:
        page = call({'method': 'read_output', 'taskId': task_id, 'afterSequence': after})['chunks']
        if not page:
            return chunks
        chunks.extend(page)
        after = page[-1]['sequence']


class RawWebSocket:
    """A deliberately simple fixture peer for malformed/slow session checks."""
    def __init__(self, source='127.0.0.1'):
        self.stream = socket.create_connection(('127.0.0.1', PORT), timeout=12, source_address=(source, 0))
        key = base64.b64encode(uuid.uuid4().bytes).decode()
        self.stream.sendall(('GET / HTTP/1.1\r\nHost: 127.0.0.1:' + str(PORT) + '\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ' + key + '\r\nSec-WebSocket-Version: 13\r\n\r\n').encode())
        headers = bytearray()
        while not headers.endswith(b'\r\n\r\n'):
            headers.extend(exact(self.stream, 1))
            assert len(headers) < 16384
        assert headers.startswith(b'HTTP/1.1 101'), headers

    def send(self, value, opcode=1):
        data = json.dumps(value).encode() if opcode == 1 else value
        mask = os.urandom(4)
        size = len(data)
        header = bytes([0x80 | opcode, 0x80 | size]) if size < 126 else bytes([0x80 | opcode, 0x80 | 126]) + struct.pack('!H', size) if size <= 65535 else bytes([0x80 | opcode, 0x80 | 127]) + struct.pack('!Q', size)
        self.stream.sendall(header + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def receive(self):
        while True:
            first, second = exact(self.stream, 2)
            opcode = first & 15
            size = second & 127
            if size == 126:
                size, = struct.unpack('!H', exact(self.stream, 2))
            elif size == 127:
                size, = struct.unpack('!Q', exact(self.stream, 8))
            assert size <= 1048576
            data = exact(self.stream, size)
            if opcode == 8:
                raise RuntimeError('WebSocket closed')
            if opcode == 9:
                self.send(data, 10)
                continue
            if opcode == 10:
                continue
            assert opcode == 1
            return json.loads(data)

    def hello(self, **changes):
        peer = {'protocolVersion': 1, 'applicationVersion': '0.1.0', 'component': 'worker', 'instanceId': INSTANCE,
                'nodeId': 'windows', 'generation': str(uuid.uuid4()), 'capabilities': ['task_submit', 'task_cancel']}
        peer.update(changes)
        self.send({'type': 'hello', 'peer': peer})
        return self.receive()

    def close(self):
        self.stream.close()


controller = worker = watch = None
crash_pid = None
checks = []
try:
    controller = start('controller')
    assert stat.S_IMODE(CONTROLLER_SOCKET.stat().st_mode) == 0o600
    assert stat.S_IMODE(CONTROLLER_SOCKET.parent.stat().st_mode) == 0o700
    assert call({'method': 'info'}, error=True)['message'].find('offline') >= 0
    worker = start('worker')
    until(connected, 'outbound worker handshake failed')
    info = call({'method': 'info'})['info']
    assert info['controllerConnection']['state'] == 'connected'
    checks.append('private-controller-ipc-outbound-versioned-handshake')

    duplicate = RawWebSocket()
    assert 'already' in duplicate.hello()['message']
    duplicate.close()
    assert connected()
    checks.append('duplicate-session-does-not-displace-live-worker')

    value = request('dedup', 'success')
    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:
        accepted = list(pool.map(lambda _: submit(value)['task'], range(12)))
    task_id = accepted[0]['taskId']
    assert {task['taskId'] for task in accepted} == {task_id}
    task = terminal(task_id, 'succeeded')
    chunks = output(task_id)
    assert 'Привет ✓' in b''.join(bytes(c['bytes']) for c in chunks if c['stream'] == 'stdout').decode()
    assert 'diagnostic' in b''.join(bytes(c['bytes']) for c in chunks if c['stream'] == 'stderr').decode()
    assert len((PROJECT / 'runs.log').read_text().splitlines()) == 1
    (RUN / 'success.task-report.json').write_text(json.dumps({'schemaVersion': 1, 'platform': 'linux', 'task': task, 'output': chunks, 'outputComplete': True}))
    conflict = dict(value, timeoutMs=20000)
    assert 'different' in submit(conflict, error=True)['message'].lower()
    checks.append('correlated-concurrent-dedup-conflict-unicode-output')

    lost = request('lost-ack', 'success')
    submit(lost, lose_ack=True)
    accepted = submit(lost)['task']
    terminal(accepted['taskId'], 'succeeded')
    assert len((PROJECT / 'runs.log').read_text().splitlines()) == 2
    checks.append('lost-controller-client-ack-same-key-one-process')

    tree = submit(request('tree', 'tree', 60000))['task']
    until(lambda: get(tree['taskId'])['processId'], 'tree not started')
    call({'method': 'cancel', 'taskId': tree['taskId']})
    terminal(tree['taskId'], 'cancelled')
    checks.append('remote-managed-tree-cancel')

    long = submit(request('reconnect', 'long', 60000))['task']
    until(lambda: get(long['taskId'])['processId'], 'long task not started')
    old_status = call({'method': 'controller_status'})['status']
    watch_log = open(RUN / 'watch.jsonl', 'wb')
    watch = subprocess.Popen([str(ROOT / 'target/debug/winbridge-controller'), '--config', str(CONTROLLER_CFG), 'watch', long['taskId']], stdout=watch_log, stderr=watch_log)
    watch_log.close()
    until(lambda: (RUN / 'watch.jsonl').stat().st_size > 0, 'watch did not emit events')
    stop(controller)
    controller = None
    until(lambda: call({'method': 'info'}, WORKER_SOCKET)['info']['controllerConnection']['state'] != 'connected', 'worker did not show controller disconnected')
    local = get(long['taskId'], WORKER_SOCKET)
    assert local['status'] == 'running'
    assert not local['cancelRequested']
    controller = start('controller')
    until(connected, 'worker did not reconnect')
    new_status = call({'method': 'controller_status'})['status']
    assert old_status['generation'] != new_status['generation']
    assert old_status['sessionId'] != new_status['sessionId']
    assert old_status['workerGeneration'] == new_status['workerGeneration']
    assert get(long['taskId'])['taskId'] == long['taskId']
    assert submit(request('reconnect', 'long', 60000))['task']['taskId'] == long['taskId']
    call({'method': 'cancel', 'taskId': long['taskId']})
    terminal(long['taskId'], 'cancelled')
    watch.wait(15)
    assert watch.returncode == 0
    updates = [json.loads(line) for line in (RUN / 'watch.jsonl').read_text().splitlines()]
    assert updates[-1]['outputComplete'] and updates[-1]['task']['taskId'] == long['taskId']
    event_sequences = [e['sequence'] for u in updates if u['kind'] == 'task_update' for e in u['events']]
    output_sequences = [c['sequence'] for u in updates if u['kind'] == 'task_update' for c in u['output']]
    assert event_sequences == sorted(set(event_sequences))
    assert output_sequences == sorted(set(output_sequences))
    checks.append('controller-exit-task-continues-reconnect-same-id-watch-sequence-replay')

    crashed = submit(request('crash', 'long', 60000))['task']
    crash_pid = until(lambda: get(crashed['taskId'])['processId'], 'crash task not started')
    stop(worker, signal.SIGKILL)
    worker = None
    # Unix harness owns forced-crash cleanup; this is not evidence for Windows Job Objects.
    try:
        os.killpg(crash_pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    crash_pid = None
    until(lambda: not connected(), 'controller did not detect worker disconnect')
    assert 'offline' in call({'method': 'get_task', 'taskId': crashed['taskId']}, error=True)['message']
    worker = start('worker')
    until(connected, 'worker restart did not reconnect')
    terminal(crashed['taskId'], 'indeterminate')
    assert submit(request('crash', 'long', 60000))['task']['taskId'] == crashed['taskId']
    assert get(task_id)['status'] == 'succeeded'
    checks.append('worker-crash-recovery-indeterminate-no-retry-preserved-terminal-result')

    terminal(submit(request('timeout', 'long', 200))['task']['taskId'], 'failed')
    bad = request('wrong-version', 'success')
    bad['protocolVersion'] = 999
    assert 'protocolVersion' in submit(bad, error=True)['message']
    bad = request('traversal', 'success')
    bad['command']['workingDirectory'] = '..'
    submit(bad, error=True)
    checks.append('remote-timeout-wire-version-and-project-policy')


    stop(worker)
    worker = None
    until(lambda: not connected(), 'worker did not leave its session')
    for fields, expected in [({'protocolVersion': 999}, 'protocolVersion'), ({'instanceId': 'wrong-pair'}, 'mismatch'), ({'capabilities': []}, 'capabilities')]:
        fake = RawWebSocket()
        rejected = fake.hello(**fields)
        assert rejected['type'] == 'rejected' and expected in rejected['message'], rejected
        fake.close()
    try:
        denied = RawWebSocket('127.0.0.2')
        denied.close()
        raise AssertionError('unregistered source IP accepted')
    except (OSError, RuntimeError):
        pass
    checks.append('handshake-rejects-wire-version-pair-capabilities-and-unregistered-ip')

    fake = RawWebSocket()
    assert fake.hello()['type'] == 'welcome'
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        futures = [pool.submit(call, {'method': method}, error=True) for method in ['info', 'list_tasks']]
        received = []
        while len(received) < 2:
            message = fake.receive()
            if message['type'] == 'ping':
                fake.send({'type': 'pong', 'nonce': message['nonce']})
            else:
                assert message['type'] == 'request', message
                received.append(message)
        for message in reversed(received):
            fake.send({'type': 'response', 'id': message['id'], 'response': {'kind': 'error', 'message': message['request']['method']}})
        assert [future.result()['message'] for future in futures] == ['info', 'list_tasks']
    fake.close()
    until(lambda: not connected(), 'fake session did not close')
    checks.append('out-of-order-responses-correlate-to-the-correct-caller')

    fake = RawWebSocket()
    assert fake.hello()['type'] == 'welcome'
    # A declared oversized frame is rejected before the payload is sent/allocated.
    fake.stream.sendall(bytes([0x81, 0xff]) + struct.pack('!Q', 1048577) + b'abcd')
    try:
        while True:
            fake.receive()
    except (OSError, RuntimeError):
        pass
    fake.close()
    until(lambda: not connected(), 'oversized frame session remained active')
    assert 'too long' in call({'method': 'controller_status'})['status']['lastError'].lower()
    checks.append('oversized-websocket-frame-rejected-before-payload')

    fake = RawWebSocket()
    assert fake.hello()['type'] == 'welcome'
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        pending = pool.submit(call, {'method': 'info'}, error=True)
        request_count = 0
        try:
            while True:
                message = fake.receive()
                if message['type'] == 'ping':
                    fake.send({'type': 'pong', 'nonce': message['nonce']})
                elif message['type'] == 'request':
                    request_count += 1
                    # Keep heartbeat alive but deliberately lose the RPC ACK.
        except (OSError, RuntimeError):
            pass
        result = pending.result()
        assert request_count == 1 and 'unknown' in result['message'], result
    fake.close()
    until(lambda: not connected(), 'RPC timeout did not end the session')
    assert 'response timeout' in call({'method': 'controller_status'})['status']['lastError'].lower()
    checks.append('rpc-deadline-live-heartbeat-no-request-retry')

    fake = RawWebSocket()
    assert fake.hello()['type'] == 'welcome'
    first_ping = fake.receive()
    assert first_ping['type'] == 'ping'
    began = time.monotonic()
    try:
        # WebSocket control traffic must not extend the application heartbeat deadline.
        while time.monotonic() - began < 12:
            fake.send(b'noise', opcode=9)
            fake.stream.settimeout(.1)
            try:
                fake.receive()
            except socket.timeout:
                pass
    except (OSError, RuntimeError):
        pass
    fake.close()
    until(lambda: not connected(), 'control frames extended heartbeat deadline', seconds=2)
    assert 9 <= time.monotonic() - began < 12
    assert 'heartbeat timeout' in call({'method': 'controller_status'})['status']['lastError'].lower()
    checks.append('single-heartbeat-deadline-survives-irrelevant-control-traffic')
    print(json.dumps({'platform': 'linux', 'checks': checks, 'reportRoot': str(RUN)}, indent=2))
    (RUN / 'validation-report.json').write_text(json.dumps({'platform': 'linux', 'checks': checks, 'passed': True}, indent=2))
finally:
    stop(watch)
    stop(worker)
    stop(controller)
    if crash_pid:
        try:
            os.killpg(crash_pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
