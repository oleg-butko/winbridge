#!/usr/bin/env python3
"""Measure Debian publication -> shared -> a tested isolated Windows delta worker."""
import argparse
import base64
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import shutil
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('native_rpc', ROOT / 'scripts/validate-workflow-native.py')
rpc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rpc)


def write_new(path, value):
    with path.open('x', encoding='utf-8') as output:
        json.dump(value, output, ensure_ascii=False, indent=2)


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, stderr=subprocess.PIPE).decode('utf-8')


def check_artifacts(directory, ready):
    for item in ready['files']:
        if Path(item['path']).name != item['path']:
            raise RuntimeError('Unsafe report path')
        data = (directory / item['path']).read_bytes()
        if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
            raise RuntimeError('Report checksum mismatch: ' + item['path'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path, required=True)
    parser.add_argument('--native-run', required=True, help='Passed --delta-only run ID; reuses its tested isolated worker binary')
    parser.add_argument('--controller', type=Path, default=ROOT / 'target/debug/winbridge-controller')
    args = parser.parse_args()
    if not re.fullmatch(r'\d{8}-\d{6}-[a-f0-9]{8}', args.native_run):
        parser.error('native-run must be the exact owned native gate directory name')
    config = json.loads(args.controller_config.read_text())
    shared = Path(config['delivery']['root'])
    native = shared / config['delivery']['namespace'] / 'workflow-validation' / args.native_run
    check_artifacts(native, json.loads((native / 'report.ready.json').read_text(encoding='utf-8-sig')))
    native_report = json.loads((native / 'report.json').read_text(encoding='utf-8-sig'))
    if not native_report['passed'] or native_report['platform'] != 'windows' or native_report['sourceDirty'] or not (native / 'delta-worker-proof.json').is_file():
        raise RuntimeError('A passed, clean actual Windows delta gate is required')
    endpoint = Path(config['stateDirectory']) / ('controller-' + config['instanceId'] + '.sock')
    initial = rpc.call(endpoint, {'method': 'info'})['info']
    if initial['platform'] != 'windows' or initial.get('activeTaskCount') != 0 or initial.get('journalRecoveryRequired') or initial.get('maintenanceError'):
        raise RuntimeError('A healthy, idle installed Windows worker is required')
    project = next(p for p in initial['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
    run_id = time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()) + '-' + uuid.uuid4().hex[:8]
    local = ROOT / '.winbridge/delta-delivery' / run_id
    local.mkdir(parents=True)
    report = shared / config['delivery']['namespace'] / 'delta-validation' / run_id
    report.mkdir(parents=True)

    def windows(path):
        return config['delivery']['peerRoot'].rstrip('\\/') + '\\' + str(path.relative_to(shared)).replace('/', '\\')

    repo = local / 'source'
    shutil.copytree(ROOT / 'fixtures/snapshot-project', repo)
    for index in range(64):
        (repo / f'small-{index:03}.dat').write_bytes(bytes([index % 20]) * 4096)
    (repo / 'large.bin').write_bytes(bytes([23]) * (2 * 1024 * 1024))
    (repo / 'Привет.txt').write_text('base ' + run_id, encoding='utf-8')
    run('git', '-C', str(repo), 'init', '-q')

    def commit():
        run('git', '-C', str(repo), 'add', '.')
        run('git', '-C', str(repo), '-c', 'user.name=Fixture', '-c', 'user.email=fixture@local', 'commit', '-qm', 'delta delivery ' + run_id)
        return run('git', '-C', str(repo), 'rev-parse', 'HEAD').strip()

    commit()
    publish_config = dict(config)
    publish_config['projects'] = [{'id': 'fixture', 'sourceRoot': str(repo), 'sourceMode': 'commit'}]
    publish_config['stateDirectory'] = str(local / 'capture-state')
    publish_config['delivery'] = dict(config['delivery']) | {'root': str(report / 'packages'), 'peerRoot': windows(report / 'packages')}
    (report / 'packages').mkdir()
    config_path = local / 'publisher.local.json'
    write_new(config_path, publish_config)

    def publish(method, *arguments):
        return json.loads(run(str(args.controller), '--config', str(config_path), method, 'fixture', 'HEAD', *arguments))

    base = publish('publish')
    profile = {'schemaVersion': 1, 'deliveryRoot': windows(report / 'packages'), 'base': base, 'cases': []}
    for index in range(3):
        expected = f'Debian delta {index} · Привет ✓ · {run_id}'
        (repo / 'Привет.txt').write_text(expected, encoding='utf-8')
        source_commit = commit()
        # Alternate publisher order as well; captures are fresh, never package reuse.
        if index % 2:
            delta = publish('publish-delta', base['snapshotId'])
            full = publish('measure-publish')
        else:
            full = publish('measure-publish')
            delta = publish('publish-delta', base['snapshotId'])
        if full['reusedPackage'] or delta['reusedPackage']:
            raise RuntimeError('Fresh package measurement unexpectedly reused an existing package')
        packages = report / 'packages' / 'WinBridge' / 'deliveries' / 'fixture'
        full_manifest = json.loads((packages / full['ready']['snapshotId'] / 'manifest.json').read_bytes())
        delta_manifest = json.loads((packages / delta['ready']['snapshotId'] / 'manifest.json').read_bytes())
        if full_manifest['files'] != delta_manifest['files'] or len(delta_manifest['delta']['reusedFiles']) != len(delta_manifest['files']) - 1:
            raise RuntimeError('One-file delta differs from the full Git commit')
        profile['cases'].append({'index': index, 'commit': source_commit, 'expectedOutput': expected, 'full': full, 'delta': delta})
    profile_path = report / 'profile.json'
    write_new(profile_path, profile)
    profile_sha = hashlib.sha256(profile_path.read_bytes()).hexdigest()
    helper = report / 'test-delta-delivery.ps1'
    with helper.open('xb') as output:
        output.write((ROOT / 'scripts/test-delta-delivery.ps1').read_bytes())
    helper_sha = hashlib.sha256(helper.read_bytes()).hexdigest()
    native_root = PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / args.native_run
    worker = str(native_root / 'test-build-cache/debug/winbridge-worker.exe')
    cli = str(native_root / 'winbridge/scripts/worker-cli.ps1')
    cli_sha = hashlib.sha256((ROOT / 'scripts/worker-cli.ps1').read_bytes()).hexdigest()
    test_root = str(PureWindowsPath(project['workspaceRoot']) / 'delta-validation' / run_id)
    # Copy the hash-pinned helper locally before executing it. All paths are explicit.
    quote = rpc.ps_string
    script = f"""$ErrorActionPreference='Stop'
$helper={quote(windows(helper))}
if ((Get-FileHash -LiteralPath $helper -Algorithm SHA256).Hash.ToLowerInvariant() -ne '{helper_sha}') {{throw 'Helper checksum mismatch'}}
if ((Get-FileHash -LiteralPath {quote(cli)} -Algorithm SHA256).Hash.ToLowerInvariant() -ne '{cli_sha}') {{throw 'Native CLI helper checksum mismatch'}}
$root={quote(test_root)}
if (Test-Path -LiteralPath $root) {{throw 'Refusing to overwrite a test run'}}
New-Item -ItemType Directory -Path $root | Out-Null
$localHelper=Join-Path $root 'test-delta-delivery.ps1'
Copy-Item -LiteralPath $helper -Destination $localHelper
if ((Get-FileHash -LiteralPath $localHelper -Algorithm SHA256).Hash.ToLowerInvariant() -ne '{helper_sha}') {{throw 'Local helper checksum mismatch'}}
& pwsh.exe -NoProfile -NonInteractive -File $localHelper -WorkerExecutable {quote(worker)} -WorkerCliScript {quote(cli)} -ExpectedSourceCommit {quote(native_report['sourceCommit'])} -ProfilePath {quote(windows(profile_path))} -ProfileSha256 '{profile_sha}' -LocalDirectory (Join-Path $root 'runtime') -ReportDirectory {quote(windows(report))}
exit $LASTEXITCODE
"""
    request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': project['id'], 'idempotencyKey': 'delta-cross-host-' + run_id,
               'timeoutMs': 600000, 'command': {'kind': 'direct', 'executable': 'pwsh.exe', 'arguments': ['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()], 'workingDirectory': '.'}}
    write_new(local / 'request.json', request)
    task = rpc.call(endpoint, {'method': 'submit', 'request': request})['task']
    task_id = task['taskId']
    print(json.dumps({'taskId': task_id, 'reportDirectory': str(report), 'nativeSource': native_report['sourceCommit'], 'profileSha256': profile_sha}), flush=True)
    probes = []
    chunks = []
    sequence = 0
    deadline = time.monotonic() + 620
    while time.monotonic() < deadline:
        started = time.perf_counter()
        info = rpc.call(endpoint, {'method': 'info'})['info']
        probes.append((time.perf_counter() - started) * 1000)
        if info['generation'] != initial['generation'] or info.get('journalRecoveryRequired') or info.get('maintenanceError'):
            raise RuntimeError('Installed worker health/generation changed unexpectedly')
        task = rpc.call(endpoint, {'method': 'get_task', 'taskId': task_id})['task']
        page = rpc.call(endpoint, {'method': 'read_output', 'taskId': task_id, 'afterSequence': sequence})['chunks']
        chunks.extend(page)
        if page:
            sequence = page[-1]['sequence']
        if task['status'] not in {'queued', 'running'}:
            break
        time.sleep(0.5)
    else:
        raise TimeoutError('Cross-host task deadline; retained request must be reconciled explicitly')
    write_new(local / 'task-report.json', {'task': task, 'output': chunks})
    if task['status'] != 'succeeded' or task['exitCode'] != 0:
        diagnostic = b''.join(bytes(c['bytes']) for c in chunks).decode('utf-8', errors='replace')[-8000:]
        raise RuntimeError(f'Cross-host validation failed; stop and inspect task {task_id}: {diagnostic}')
    result = json.loads((report / 'windows-report.json').read_text(encoding='utf-8-sig'))
    if not result['passed'] or result['platform'] != 'windows' or len(result['cases']) != 3 or result['profileSha256'] != profile_sha:
        raise RuntimeError('Windows proof does not match the reviewed profile')
    final = rpc.call(endpoint, {'method': 'info'})['info']
    if final['generation'] != initial['generation'] or final.get('activeTaskCount') != 0 or final.get('journalRecoveryRequired') or final.get('maintenanceError'):
        raise RuntimeError('Installed worker is not healthy/idle after validation')
    identity = lambda info: {k: v for k, v in info.items() if k != 'projects'}
    write_new(report / 'report.json', {'schemaVersion': 1, 'passed': True, 'platform': 'debian-windows', 'runId': run_id, 'nativeRun': args.native_run,
              'nativeSourceCommit': native_report['sourceCommit'], 'publisher': json.loads(run(str(args.controller), '--describe'))['build'],
              'profileSha256': profile_sha, 'helperSha256': helper_sha, 'taskId': task_id, 'installedWorkerInitial': identity(initial), 'installedWorkerFinal': identity(final),
              'controlLatencyMs': probes, 'limitations': ['Three fresh one-file commits; OS/HGFS caches are not flushed.', 'Observed import duration includes local CLI startup and 80 ms polling.', 'Full/delta publication and import order alternates; no paired build comparison or general pipeline speedup claim.']})
    artifacts = []
    for path in sorted(report.iterdir()):
        if path.is_file():
            data = path.read_bytes()
            artifacts.append({'path': path.name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
    write_new(report / 'ready.json', {'schemaVersion': 1, 'passed': True, 'files': artifacts})
    check_artifacts(report, json.loads((report / 'ready.json').read_text()))
    print(json.dumps({'passed': True, 'taskId': task_id, 'reportDirectory': str(report), 'reportSha256': hashlib.sha256((report / 'report.json').read_bytes()).hexdigest(), 'cases': 3}), flush=True)


if __name__ == '__main__':
    main()
