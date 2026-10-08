#!/usr/bin/env python3
"""Exercise real MCP stdio -> controller -> worker. Windows mode never updates runtime."""
import argparse
import hashlib
import json
import queue
import socket
import struct
import subprocess
import tempfile
import threading
import time
import uuid
from pathlib import Path, PureWindowsPath

ROOT = Path(__file__).resolve().parents[1]
CONTROLLER = ROOT / 'target/debug/winbridge-controller'
FIXTURE = ROOT / 'target/debug/winbridge-fixture'


def exact(stream, length):
    result = bytearray()
    while len(result) < length:
        chunk = stream.recv(length - len(result))
        if not chunk:
            raise RuntimeError('IPC reply ended before a complete frame')
        result.extend(chunk)
    return bytes(result)


def rpc(endpoint, request):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(15)
        stream.connect(str(endpoint))
        data = json.dumps(request).encode()
        stream.sendall(struct.pack('<I', len(data)) + data)
        length, = struct.unpack('<I', exact(stream, 4))
        if not 0 < length <= 1048576:
            raise RuntimeError('Invalid IPC frame size')
        return json.loads(exact(stream, length))


class Client:
    def __init__(self, config, run, name):
        self.log = (run / (name + '.stderr.log')).open('xb')
        self.process = subprocess.Popen([str(CONTROLLER), '--config', str(config), 'mcp'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log)
        self.messages = queue.Queue()
        self.identifier = 0
        self.exchanges = []
        self.transcript = run / (name + '.calls.jsonl')
        self.transcript.touch(exist_ok=False)
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        self.initialized = self.call('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'WinBridge automatic validation', 'version': '1'}})['result']
        assert self.initialized['protocolVersion'] == '2025-11-25'
        self.notify('notifications/initialized')

    def _read(self):
        try:
            while True:
                line = self.process.stdout.readline(3 * 1048576 + 2)
                if not line:
                    raise RuntimeError('MCP stdout closed before a reply')
                if not line.endswith(b'\n'):
                    raise RuntimeError('MCP reply exceeded limit or ended without newline')
                self.messages.put(json.loads(line))
        except Exception as error:
            self.messages.put(error)

    def send(self, message):
        self.process.stdin.write(json.dumps(message, ensure_ascii=False).encode() + b'\n')
        self.process.stdin.flush()

    def notify(self, method, params=None):
        message = {'jsonrpc': '2.0', 'method': method}
        if params is not None:
            message['params'] = params
        self.send(message)

    def call(self, method, params=None):
        self.identifier += 1
        message = {'jsonrpc': '2.0', 'id': self.identifier, 'method': method}
        if params is not None:
            message['params'] = params
        correlation = {'id': self.identifier, 'method': method, 'tool': (params or {}).get('name')}
        request = (params or {}).get('arguments', {}).get('request', {})
        if 'idempotencyKey' in request:
            correlation['idempotencyKey'] = request['idempotencyKey']
        with self.transcript.open('a') as stream:
            stream.write(json.dumps({**correlation, 'stage': 'sent'}) + '\n')
        self.send(message)
        response = self.messages.get(timeout=30)
        if isinstance(response, Exception):
            raise response
        assert response.get('jsonrpc') == '2.0' and response['id'] == self.identifier
        self.exchanges.append({'request': message, 'response': response})
        content = response.get('result', {}).get('structuredContent', {})
        record = content.get('task', content.get('import', {}))
        with self.transcript.open('a') as stream:
            stream.write(json.dumps({**correlation, 'stage': 'received', 'kind': content.get('kind'),
                                     'taskId': record.get('taskId'), 'importId': record.get('importId'),
                                     'protocolErrorCode': response.get('error', {}).get('code')}) + '\n')
        return response

    def tool(self, method, arguments=None, expected_error=False):
        response = self.call('tools/call', {'name': 'winbridge_' + method, 'arguments': arguments or {}})
        if 'error' in response:
            raise RuntimeError(response['error'])
        result = response['result']
        value = json.loads(result['content'][0]['text'])
        assert value == result['structuredContent']
        if bool(result.get('isError')) != expected_error:
            raise RuntimeError(value)
        return value

    def close(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
                raise RuntimeError('MCP did not exit after stdin EOF')
        self.log.close()
        assert self.process.returncode == 0


def wait_task(client, task_id, running=False):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        task = client.tool('get_task', {'taskId': task_id})['task']
        if (running and task['status'] == 'running' and task['processId']) or (not running and task['status'] not in {'running', 'queued'}):
            return task
        time.sleep(.05)
    raise TimeoutError('Task state deadline: ' + task_id)


def output(client, task_id):
    sequence = 0
    streams = {'stdout': bytearray(), 'stderr': bytearray()}
    while True:
        chunks = client.tool('read_output', {'taskId': task_id, 'afterSequence': sequence})['chunks']
        if not chunks:
            return {k: v.decode('utf-8', errors='replace') for k, v in streams.items()}
        for chunk in chunks:
            streams[chunk['stream']].extend(chunk['bytes'])
        sequence = chunks[-1]['sequence']


def submission(key, project, executable, arguments, cwd='.'):
    return {'protocolVersion': 1, 'requestId': key, 'projectId': project, 'idempotencyKey': key, 'timeoutMs': 30000,
            'command': {'kind': 'direct', 'executable': str(executable), 'arguments': arguments, 'workingDirectory': str(cwd)}}


def assert_catalog(client):
    tools = client.call('tools/list')['result']['tools']
    names = [t['name'] for t in tools]
    assert len(names) == len(set(names)) and 'winbridge_publish' in names and 'winbridge_history' in names
    assert next(t for t in tools if t['name'] == 'winbridge_get_task')['annotations']['readOnlyHint']
    assert client.call('tools/call', {'name': 'winbridge_info', 'arguments': {'method': 'submit'}})['error']['code'] == -32602
    return len(tools)


class LostAckProxy:
    """Forward to a real controller, discard exactly one already committed submit reply."""
    def __init__(self, endpoint, upstream):
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(str(endpoint))
        self.listener.listen()
        self.listener.settimeout(.2)
        self.upstream = upstream
        self.submits = 0
        self.accepted = None
        self.error = None
        self.stopped = threading.Event()
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    def serve(self):
        try:
            while not self.stopped.is_set():
                try:
                    stream, _ = self.listener.accept()
                except socket.timeout:
                    continue
                with stream:
                    stream.settimeout(15)
                    length, = struct.unpack('<I', exact(stream, 4))
                    assert length <= 1048576
                    request = json.loads(exact(stream, length))
                    reply = rpc(self.upstream, request)
                    if request['method'] == 'submit':
                        self.submits += 1
                        if self.accepted is None:
                            self.accepted = reply['task']['taskId']
                            continue
                    data = json.dumps(reply).encode()
                    stream.sendall(struct.pack('<I', len(data)) + data)
        except Exception as error:
            self.error = error

    def close(self):
        self.stopped.set()
        self.thread.join(timeout=2)
        self.listener.close()
        assert not self.thread.is_alive()
        if self.error:
            raise self.error


def local_check(run):
    project = run / 'project'
    source = run / 'source'
    shared = run / 'shared'
    for path in [project, source, shared]:
        path.mkdir()
    (source / 'Привет.txt').write_text('MCP publication ✓', encoding='utf-8')
    for args in [['init', '-q'], ['add', '.'], ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@local', 'commit', '-qm', 'MCP fixture']]:
        subprocess.run(['git', '-C', str(source), *args], check=True)
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        port = probe.getsockname()[1]
    cc = {'schemaVersion': 1, 'instanceId': 'mcp-test', 'nodeId': 'debian-test', 'stateDirectory': str(run / 'controller'),
          'controller': {'listenAddress': f'127.0.0.1:{port}', 'workerNodeId': 'worker-test'},
          'delivery': {'adapter': 'shared-folder', 'root': str(shared), 'namespace': 'WinBridge'},
          'projects': [{'id': 'fixture', 'sourceRoot': str(source), 'sourceMode': 'commit'}]}
    wc = {'schemaVersion': 1, 'instanceId': 'mcp-test', 'nodeId': 'worker-test', 'stateDirectory': str(run / 'state'),
          'worker': {'controllerUrl': f'ws://127.0.0.1:{port}', 'keepRunningOnUiExit': True},
          'delivery': {'adapter': 'shared-folder', 'root': str(shared), 'namespace': 'WinBridge'},
          'projects': [{'id': 'fixture', 'workspaceRoot': str(project), 'allowDirectCommands': True, 'maxConcurrentMutations': 1, 'recipes': []}]}
    cp, wp = run / 'controller.local.json', run / 'worker.local.json'
    cp.write_text(json.dumps(cc)); wp.write_text(json.dumps(wc))
    processes, clients, logs = [], [], []
    proxy = None
    try:
        for executable, config, name in [(CONTROLLER, cp, 'controller'), (ROOT / 'target/debug/winbridge-worker', wp, 'worker')]:
            log = (run / (name + '.log')).open('xb'); logs.append(log)
            processes.append(subprocess.Popen([str(executable), '--config', str(config), 'serve'], stdout=log, stderr=log))
        endpoint = Path(cc['stateDirectory']) / 'controller-mcp-test.sock'
        deadline = time.monotonic() + 15
        while True:
            try:
                assert rpc(endpoint, {'method': 'info'})['kind'] == 'info'
                break
            except Exception:
                if time.monotonic() >= deadline:
                    raise
                time.sleep(.05)
        client = Client(cp, run, 'mcp'); clients.append(client)
        catalog_count = assert_catalog(client)
        ready = client.tool('publish', {'projectId': 'fixture', 'revision': 'HEAD'})['ready']
        imported = client.tool('import_snapshot', {'request': {'protocolVersion': 1, 'requestId': 'mcp-import', 'projectId': 'fixture',
                              'idempotencyKey': 'mcp-import', 'snapshotId': ready['snapshotId'], 'manifestSha256': ready['manifestSha256'], 'timeoutMs': 10000}})['import']
        deadline = time.monotonic() + 15
        while client.tool('get_import', {'importId': imported['importId']})['import']['status'] != 'succeeded':
            if time.monotonic() > deadline:
                raise TimeoutError('MCP snapshot import')
            time.sleep(.05)
        (run / 'proxy').mkdir()
        proxy_config = {**cc, 'stateDirectory': str(run / 'proxy')}
        pp = run / 'proxy.local.json'; pp.write_text(json.dumps(proxy_config))
        proxy = LostAckProxy(run / 'proxy/controller-mcp-test.sock', endpoint)
        lossy = Client(pp, run, 'mcp-lost-ack'); clients.append(lossy)
        request = submission('mcp-lost-ack', 'fixture', FIXTURE, ['arguments', 'Привет ✓'])
        refused = lossy.tool('submit', {'request': request}, expected_error=True)
        assert 'Do not automatically replay' in refused['message']
        assert proxy.accepted and proxy.submits == 1
        task = wait_task(client, proxy.accepted); assert task['status'] == 'succeeded'
        time.sleep(.2); assert proxy.submits == 1
        retry = lossy.tool('submit', {'request': request})['task']; assert retry['taskId'] == proxy.accepted
        assert proxy.submits == 2
        assert len((project / 'runs.log').read_text().splitlines()) == 1
        assert 'Привет ✓' in output(client, task['taskId'])['stdout']
        history = client.tool('history', {'query': {'projectId': None, 'kind': None, 'status': None, 'search': '', 'before': None}})['page']
        assert {r['kind'] for r in history['records']} == {'task', 'import'}
        preview = client.tool('prepare_task_diagnostic', {'request': {'taskId': task['taskId'], 'includeCommand': False, 'includeOutput': False, 'redactValues': []}})['preview']
        assert json.loads(preview['json'])['commandIncluded'] is False
        long_request = submission('mcp-client-exit', 'fixture', FIXTURE, ['long'])
        long_task = client.tool('submit', {'request': long_request})['task']
        running = wait_task(client, long_task['taskId'], running=True)
        client.close(); clients.remove(client)
        successor = Client(cp, run, 'mcp-reopened'); clients.append(successor)
        still = successor.tool('get_task', {'taskId': long_task['taskId']})['task']
        assert still['status'] == 'running' and still['processId'] == running['processId']
        successor.tool('cancel', {'taskId': long_task['taskId']})
        assert wait_task(successor, long_task['taskId'])['status'] == 'cancelled'
        return {'platform': 'linux', 'passed': True, 'tools': catalog_count, 'taskId': task['taskId'], 'clientExitTaskId': long_task['taskId'],
                'lostAckDidNotReplay': True, 'sameKeyRetryOneProcess': True, 'unicodeOutput': True, 'publishImport': True,
                'historyDiagnostic': True, 'clientExitKeptTaskAndPid': True, 'explicitCancel': True}
    finally:
        for client in clients:
            client.close()
        if proxy:
            proxy.close()
        for process in processes:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
        for log in logs:
            log.close()


def windows_check(config_path, windows_fixture, run, run_id):
    config = json.loads(config_path.read_text())
    client = Client(config_path, run, 'mcp-windows')
    long_id = None
    try:
        assert not client.initialized['_meta']['winbridgeBuild']['sourceDirty'], 'Build a committed clean MCP adapter before Windows validation'
        info = client.tool('info')['info']
        assert info['platform'] == 'windows' and not info.get('journalRecoveryRequired')
        if not info.get('historyAvailable'):
            refusal = client.tool('history', {'query': {'projectId': None, 'kind': None, 'status': None, 'search': '', 'before': None}}, expected_error=True)
            assert 'history_read' in refusal['message'] and 'not transmitted' in refusal['message']
        project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
        root = PureWindowsPath(project['workspaceRoot'])
        source_exe = PureWindowsPath(windows_fixture)
        source_exe.relative_to(root)  # Only the already tested, owned fixture is read.
        owned = root / 'mcp-validation' / run_id
        quote = lambda s: "'" + str(s).replace("'", "''") + "'"
        setup = f"$ErrorActionPreference='Stop'; $dir={quote(owned)}; if(Test-Path -LiteralPath $dir){{throw 'Refusing to reuse MCP test directory'}}; New-Item -ItemType Directory -Path $dir | Out-Null; Copy-Item -LiteralPath {quote(source_exe)} -Destination (Join-Path $dir 'fixture.exe'); Write-Output 'MCP isolated fixture prepared'"
        setup_task = client.tool('submit', {'request': submission('mcp-setup-' + run_id, project['id'], 'pwsh.exe', ['-NoProfile', '-Command', setup])})['task']
        assert wait_task(client, setup_task['taskId'])['status'] == 'succeeded'
        relative = owned.relative_to(root)
        request = submission('mcp-success-' + run_id, project['id'], owned / 'fixture.exe', ['arguments', 'Привет ✓'], relative)
        accepted = client.tool('submit', {'request': request})['task']
        task = wait_task(client, accepted['taskId']); assert task['status'] == 'succeeded' and task['exitCode'] == 0
        repeated = client.tool('submit', {'request': request})['task']
        assert repeated['taskId'] == task['taskId'] and repeated['processId'] == task['processId']
        captured = output(client, task['taskId']); assert 'Привет ✓' in captured['stdout']
        long_task = client.tool('submit', {'request': submission('mcp-long-' + run_id, project['id'], owned / 'fixture.exe', ['long'], relative)})['task']
        long_id = long_task['taskId']; running = wait_task(client, long_id, running=True)
        adapter_build = client.initialized['_meta']['winbridgeBuild']
        client.close()
        client = Client(config_path, run, 'mcp-windows-reopened')
        still = client.tool('get_task', {'taskId': long_id})['task']
        assert still['status'] == 'running' and still['processId'] == running['processId']
        client.tool('cancel', {'taskId': long_id})
        cancelled = wait_task(client, long_id); assert cancelled['status'] == 'cancelled'
        long_id = None
        after = client.tool('info')['info']
        assert after['generation'] == info['generation'] and after['build'] == info['build'] and not after.get('journalRecoveryRequired')
        return {'platform': 'windows', 'passed': True, 'adapterBuild': adapter_build, 'workerBuild': info['build'],
                'workerGeneration': info['generation'], 'setupTaskId': setup_task['taskId'], 'task': task, 'output': captured,
                'sameKeySameTaskAndPid': True, 'clientExitKeptTaskAndPid': True, 'cancelledTask': cancelled, 'ownedDirectory': str(owned),
                'olderWorkerCapabilityRefusal': not info.get('historyAvailable'),
                'workerActiveTaskCountAfter': after['activeTaskCount']}
    finally:
        # Unexpected worker behavior is retained and surfaced, never automatically retried.
        if long_id:
            (run / 'unfinished-task.json').write_text(json.dumps({'taskId': long_id, 'message': 'Inspect before any further worker operation'}))
        client.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path, help='Connected Windows mode; otherwise use an isolated real Unix controller/worker')
    parser.add_argument('--windows-fixture', help='Read-only path to the fixture built by the completed Windows gate')
    args = parser.parse_args()
    if bool(args.controller_config) != bool(args.windows_fixture):
        parser.error('Windows mode requires both controller-config and windows-fixture')
    run_id = time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
    run = ROOT / '.winbridge/mcp-validation' / run_id
    run.mkdir(parents=True)
    if args.controller_config:
        result = windows_check(args.controller_config, args.windows_fixture, run, run_id)
    else:
        # Unix socket paths have a small OS limit; source/log/config remain owned here.
        with tempfile.TemporaryDirectory(prefix='winbridge-mcp-') as directory:
            result = local_check(Path(directory))
    result['adapterExecutableSha256'] = hashlib.sha256(CONTROLLER.read_bytes()).hexdigest()
    result['validatedAt'] = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
    data = json.dumps(result, ensure_ascii=False, indent=2).encode()
    (run / 'report.json').write_bytes(data)
    if args.controller_config:
        config = json.loads(args.controller_config.read_text())
        report = Path(config['delivery']['root']) / config['delivery']['namespace'] / 'mcp-validation' / run_id
        report.mkdir(parents=True, exist_ok=False)
        with (report / 'report.json').open('xb') as stream:
            stream.write(data)
        actual = (report / 'report.json').read_bytes(); assert actual == data
        ready = {'files': [{'path': 'report.json', 'bytes': len(actual), 'sha256': hashlib.sha256(actual).hexdigest()}]}
        with (report / 'report.ready.json').open('x') as stream:
            json.dump(ready, stream)
        print(json.dumps({'passed': True, 'platform': 'windows', 'reportDirectory': str(report), 'reportSha256': ready['files'][0]['sha256']}))
    else:
        print(json.dumps({**result, 'reportPath': str(run / 'report.json')}))


if __name__ == '__main__':
    main()
