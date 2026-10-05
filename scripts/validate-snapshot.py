#!/usr/bin/env python3
"""Actual IPC -> WebSocket -> worker snapshot/build gate. Never emulates task success."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import socket
import shutil
import struct
import subprocess
import time
import uuid
ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--controller-config', type=Path, required=True)
p.add_argument('--expected-platform', choices=('windows', 'linux'), default='windows')
p.add_argument('--report-directory', type=Path, default=Path('/mnt/hgfs/shared/WinBridge/snapshot-validation'))
a = p.parse_args()
cfg = json.loads(a.controller_config.read_text())
endpoint = Path(cfg['stateDirectory']) / ('controller-' + cfg['instanceId'] + '.sock')
run_id = 'debian-' + time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
local = ROOT / '.winbridge/snapshot-validation' / run_id
local.mkdir(parents=True)
report = {'schemaVersion': 1, 'runId': run_id, 'platform': a.expected_platform, 'passed': False, 'checks': [], 'imports': [], 'tasks': [], 'error': None}

def exact(stream, length):
    result = bytearray()
    while len(result) < length:
        chunk = stream.recv(length - len(result))
        if not chunk:
            raise RuntimeError('IPC ended before response; reconcile the same operation key')
        result.extend(chunk)
    return bytes(result)

def call(request, allow_error=False):
    payload = json.dumps(request, ensure_ascii=False).encode()
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(15)
        stream.connect(str(endpoint))
        stream.sendall(struct.pack('<I', len(payload)) + payload)
        length, = struct.unpack('<I', exact(stream, 4))
        assert 0 < length <= 1048576
        response = json.loads(exact(stream, length))
    if response['kind'] == 'error' and not allow_error:
        raise RuntimeError(response['message'])
    return response

def wait(kind, identity, timeout=180):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        key = 'importId' if kind == 'import' else 'taskId'
        record = call({'method': 'get_import' if kind == 'import' else 'get_task', key: identity})[kind]
        if record['status'] not in ('queued', 'running'):
            report['imports' if kind == 'import' else 'tasks'].append(record)
            return record
        time.sleep(0.15)
    raise TimeoutError(f'{kind} {identity}: deadline exceeded; retained for reconciliation')

def check(name, condition):
    report['checks'].append({'name': name, 'passed': bool(condition)})
    assert condition, name

def import_request(snapshot, manifest_hash, key=None):
    return {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'snapshot-fixture', 'snapshotId': snapshot,
            'manifestSha256': manifest_hash, 'idempotencyKey': key or str(uuid.uuid4()), 'timeoutMs': 180000}

def task(command, snapshot=None):
    request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'snapshot-fixture', 'idempotencyKey': str(uuid.uuid4()), 'timeoutMs': 180000, 'command': command}
    if snapshot:
        request['snapshotId'] = snapshot
    accepted = call({'method': 'submit', 'request': request})['task']
    result = wait('task', accepted['taskId'])
    chunks = []
    after = 0
    while True:
        page = call({'method': 'read_output', 'taskId': result['taskId'], 'afterSequence': after})['chunks']
        if not page:
            break
        chunks.extend(page)
        after = page[-1]['sequence']
    with (local / (result['taskId'] + '.task-report.json')).open('x') as f:
        json.dump({'schemaVersion': 1, 'platform': a.expected_platform, 'task': result, 'output': chunks, 'outputComplete': sum(len(c['bytes']) for c in chunks) == result['outputBytes']}, f, ensure_ascii=False, indent=2)
    text = b''.join(bytes(c['bytes']) for c in chunks if c['stream'] == 'stdout').decode('utf-8', 'replace')
    return result, text

def fault_package(original, kind):
    snapshot = 'gate-' + run_id + '-' + kind
    root = Path(cfg['delivery']['root']) / cfg['delivery']['namespace'] / 'deliveries/snapshot-fixture'
    original_path = root / original['snapshotId']
    package = root / snapshot
    package.mkdir()  # Own unique package; original immutable delivery is never edited.
    manifest = json.loads((original_path / 'manifest.json').read_bytes())
    manifest['snapshotId'] = snapshot
    data = json.dumps(manifest, ensure_ascii=False, indent=2).encode()
    ready = dict(original, snapshotId=snapshot, manifestSha256=hashlib.sha256(data).hexdigest())
    with (package / 'manifest.json').open('xb') as f:
        f.write(data)
    if kind != 'missing-ready':
        with (package / 'ready.json').open('xb') as f:
            f.write(json.dumps(ready).encode())
    if kind == 'corrupt':
        with (package / 'payload.zip').open('xb') as f:
            f.write(b'not the declared ZIP')
    return import_request(snapshot, ready['manifestSha256'])

try:
    info = call({'method': 'info'})['info']
    report['worker'] = info
    check('native worker platform and import capability', info['platform'] == a.expected_platform and info.get('snapshotImportAvailable') is True)
    project = next(pr for pr in info['projects'] if pr['id'] == 'snapshot-fixture')
    published = json.loads(subprocess.check_output([str(ROOT / 'target/debug/winbridge-controller'), '--config', str(a.controller_config), 'publish', 'snapshot-fixture', 'HEAD']))
    report['ready'] = published
    preview = call({'method': 'preview_snapshot', 'projectId': 'snapshot-fixture', 'snapshotId': published['snapshotId'], 'offset': 0})['preview']
    check('manifest preview and Unicode source paths', not preview['payloadVerified'] and any(f['path'] == 'Привет.txt' for f in preview['files']))
    request = import_request(published['snapshotId'], published['manifestSha256'])
    with ThreadPoolExecutor(max_workers=4) as pool:
        responses = list(pool.map(lambda _: call({'method': 'import_snapshot', 'request': dict(request, requestId=str(uuid.uuid4()))})['import'], range(4)))
    check('concurrent duplicate accepts one durable import', len({r['importId'] for r in responses}) == 1)
    imported = wait('import', responses[0]['importId'])
    check('verified local generation', imported['status'] == 'succeeded' and imported['completedFiles'] == preview['totalFiles'] and imported['completedBytes'] == preview['totalBytes'])
    again = call({'method': 'import_snapshot', 'request': dict(request, requestId=str(uuid.uuid4()))})['import']
    check('lost ACK/reconnect key keeps import ID', again['importId'] == imported['importId'])
    conflict = call({'method': 'import_snapshot', 'request': dict(request, manifestSha256='a' * 64)}, allow_error=True)
    check('duplicate key rejects changed payload', conflict['kind'] == 'error')
    if a.expected_platform == 'windows':
        command = {'kind': 'recipe', 'recipeId': 'snapshot-build', 'arguments': []}
        executable = str(Path(project['workspaceRoot'])) + '\\build-cache\\debug\\snapshot-fixture.exe'
    else:
        command = {'kind': 'direct', 'executable': '/home/user1/.local/bin/cargo', 'arguments': ['build', '--locked', '--offline', '--target-dir', str(Path(project['workspaceRoot']) / 'build-cache')], 'workingDirectory': '.'}
        executable = str(Path(project['workspaceRoot']) / 'build-cache/debug/snapshot-fixture')
    built, _ = task(command, published['snapshotId'])
    check('actual build from pinned generation', built['status'] == 'succeeded' and built['exitCode'] == 0 and published['snapshotId'] in built['resolvedCommand']['workingDirectory'] and built['snapshot']['manifestSha256'] == published['manifestSha256'])
    executed, stdout = task({'kind': 'direct', 'executable': executable, 'arguments': [], 'workingDirectory': '.'}, published['snapshotId'])
    check('compiled program prints delivered Unicode commit content', executed['exitCode'] == 0 and 'Проверенный commit → Windows build ✓' in stdout)
    for fault in ('missing-ready', 'corrupt', 'unavailable'):
        invalid = fault_package(published, fault) if fault != 'unavailable' else import_request('gate-' + run_id + '-unavailable', 'b' * 64)
        rejected = wait('import', call({'method': 'import_snapshot', 'request': invalid})['import']['importId'])
        check(f'{fault} package rejected before buildable generation', rejected['status'] == 'failed')
        refused = call({'method': 'submit', 'request': {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'snapshot-fixture', 'idempotencyKey': str(uuid.uuid4()), 'timeoutMs': 10000, 'snapshotId': invalid['snapshotId'], 'command': {'kind': 'direct', 'executable': executable, 'arguments': [], 'workingDirectory': '.'}}}, allow_error=True)
        check(f'{fault} cannot launch command', refused['kind'] == 'error')

    # A real committed 64 MiB source gives cancellation time to reach active I/O.
    large_source = local / 'large-source'
    shutil.copytree(ROOT / 'fixtures/snapshot-project', large_source)
    with (large_source / 'padding.dat').open('xb') as f:
        for _ in range(64):
            f.write(bytes(1024 * 1024))
    for args in [('init', '-q'), ('add', '.'), ('-c', 'user.name=Fixture', '-c', 'user.email=fixture@local', 'commit', '-qm', 'Active import cancellation fixture')]:
        subprocess.run(['git', '-C', str(large_source), *args], check=True)
    large_cfg = dict(cfg, projects=[{'id': 'snapshot-fixture', 'sourceRoot': str(large_source), 'sourceMode': 'commit'}])
    large_config_path = local / 'large-controller.local.json'
    with large_config_path.open('x') as f:
        json.dump(large_cfg, f)
    large = json.loads(subprocess.check_output([str(ROOT / 'target/debug/winbridge-controller'), '--config', str(large_config_path), 'publish', 'snapshot-fixture', 'HEAD']))
    report['largeReady'] = large
    large_request = import_request(large['snapshotId'], large['manifestSha256'])
    active = call({'method': 'import_snapshot', 'request': large_request})['import']
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        current = call({'method': 'get_import', 'importId': active['importId']})['import']
        if current['status'] == 'running':
            break
        if current['status'] not in ('queued', 'running'):
            raise RuntimeError('Large import finished before cancellation observation; retained for review')
        time.sleep(.01)
    else:
        raise TimeoutError('Large import did not enter running')
    call({'method': 'cancel_import', 'importId': active['importId']})
    cancelled = wait('import', active['importId'])
    check('active snapshot import cancels without publishing incomplete sources', cancelled['status'] == 'cancelled')
    no_build = call({'method': 'submit', 'request': {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'snapshot-fixture', 'idempotencyKey': str(uuid.uuid4()), 'timeoutMs': 10000, 'snapshotId': large['snapshotId'], 'command': {'kind': 'direct', 'executable': executable, 'arguments': [], 'workingDirectory': '.'}}}, allow_error=True)
    check('cancelled import has no buildable generation', no_build['kind'] == 'error')
    if a.expected_platform == 'windows':
        probe_id = 'gate-' + run_id + '-junction'
        # Only uniquely named canary and junction under this registered fixture root.
        ps = "$ErrorActionPreference='Stop'; $root=(Get-Location).Path; $canary=Join-Path $root 'canary-" + run_id + "'; [IO.Directory]::CreateDirectory($canary)|Out-Null; [IO.File]::WriteAllText((Join-Path $canary 'canary.txt'),'unchanged'); [IO.Directory]::CreateDirectory((Join-Path $root 'snapshots'))|Out-Null; New-Item -ItemType Junction -Path (Join-Path $root 'snapshots/" + probe_id + "') -Target $canary|Out-Null; Get-Content -Raw -LiteralPath (Join-Path $canary 'canary.txt')"
        created, _ = task({'kind': 'direct', 'executable': 'powershell.exe', 'arguments': ['-NoProfile', '-Command', ps], 'workingDirectory': '.'})
        check('owned NTFS junction probe created', created['exitCode'] == 0)
        probe = import_request(probe_id, published['manifestSha256'])
        refused = wait('import', call({'method': 'import_snapshot', 'request': probe})['import']['importId'])
        check('native NTFS junction rejected before workspace writes', refused['status'] == 'failed' and 'reparse' in refused.get('reason', '').lower())
        verified, stdout = task({'kind': 'direct', 'executable': 'powershell.exe', 'arguments': ['-NoProfile', '-Command', "Get-Content -Raw -LiteralPath (Join-Path (Get-Location).Path 'canary-" + run_id + "/canary.txt')"], 'workingDirectory': '.'})
        check('junction target canary preserved', verified['exitCode'] == 0 and stdout.strip() == 'unchanged')
    report['passed'] = True
except Exception as exc:
    report['error'] = str(exc)
    raise
finally:
    with (local / 'validation-report.json').open('x') as f:
        json.dump(report, f, ensure_ascii=False, indent=2)
    destination = a.report_directory / run_id
    try:
        destination.mkdir(parents=True, exist_ok=False)
        exported = []
        for file in local.iterdir():
            if file.name != 'validation-report.json' and not file.name.endswith('.task-report.json'):
                continue
            data = file.read_bytes()
            with (destination / file.name).open('xb') as f:
                f.write(data)
            exported.append({'path': file.name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
        with (destination / 'report.ready.json').open('x') as f:
            json.dump({'schemaVersion': 1, 'files': exported}, f, indent=2)
        print(f'Report: {destination / "validation-report.json"}')
    except Exception as exc:
        print(f'Shared report unavailable: {exc}; local report: {local}')
