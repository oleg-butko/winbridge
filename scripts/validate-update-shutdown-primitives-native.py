#!/usr/bin/env python3
"""Run fixed shutdown primitive tests via the installed worker, with no runtime update."""
import argparse
import base64
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('health', ROOT / 'scripts/validate-update-health-actor-native.py')
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)
native = health.native
GROUPS = {'shutdown-process-pin': 1, 'shutdown-data-state-journal': 12,
          'shutdown-worker-acceptance': 4, 'shutdown-worker15-refuses16': 1,
          'shutdown-updater11-refuses12': 1}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--cache-run', required=True)
    parser.add_argument('--reference-receipt', type=Path, required=True)
    parser.add_argument('--controller-config', type=Path, default=Path('.winbridge/controller-snapshot.local.json'))
    args = parser.parse_args()
    pin = health.archive_pin(args.archive)
    with zipfile.ZipFile(args.archive) as source:
        if (native.worker_schema_from_source(source.read('winbridge/apps/worker/src/store.rs')) != 16
                or native.updater_schema_from_source(source.read('winbridge/apps/update/src/journal.rs')) != 12):
            raise ValueError('This gate requires the reviewed worker16/updater12 source')
    if not re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.cache_run):
        raise ValueError('Exact retained cache run required')
    reference = json.loads(args.reference_receipt.read_text())
    if (reference.get('kind') != 'shutdown_base_reference' or reference.get('runtimeApplied') is not False
            or len(reference['files']) != 2
            or {(f['descriptor']['role'], f['schemaFromReviewedSource']) for f in reference['files']} != {('worker', 15), ('updater', 11)}):
        raise ValueError('Proven schema15/11 references required')
    cfg = json.loads(args.controller_config.read_text())
    endpoint = Path(cfg['stateDirectory']) / ('controller-' + cfg['instanceId'] + '.sock')
    info = native.call(endpoint, {'method': 'info'})['info']
    if (info['activeTaskCount'] or info.get('maintenanceError') or info.get('journalRecoveryRequired')
            or info.get('recoveryCoverage', {}).get('complete') is not True):
        raise ValueError('Healthy idle installed worker required')
    project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture')
    project_path = PureWindowsPath(project['workspaceRoot'])
    for item in reference['files']:
        path = PureWindowsPath(item['path'])
        if (not path.is_relative_to(project_path / 'workflow-validation') or '..' in path.parts
                or not re.fullmatch(r'[0-9a-f]{64}', item['sha256'])):
            raise ValueError('Reference must be retained in the registered local validation project')
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime()) + '-' + uuid.uuid4().hex[:8]
    report = Path('/mnt/hgfs/shared/WinBridge/workflow-validation') / run
    report.mkdir()
    local_report = ROOT / '.winbridge/workflow-validation' / run
    local_report.mkdir(parents=True)
    (report / 'reference.json').write_text(json.dumps(reference, indent=2))
    (report / 'source-pin.json').write_text(json.dumps(pin, indent=2))
    local = str(project_path / 'workflow-validation' / run)
    cache = str(project_path / 'workflow-validation' / args.cache_run)
    windows_report = str(PureWindowsPath('K:/VMs/shared/WinBridge/workflow-validation') / run)
    archive = str(PureWindowsPath('K:/VMs/shared') / args.archive.relative_to('/mnt/hgfs/shared'))
    ps = native.ps_string
    script = f'''# Isolated shutdown primitives; installed worker stays running; no UI windows.
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$archive={ps(archive)}
$local={ps(local)}
if(Test-Path -LiteralPath $local){{throw 'Existing validation preserved'}}
if([IO.DriveInfo]::new([IO.Path]::GetPathRoot($local)).AvailableFreeSpace -lt 6GB){{throw 'Less than 6 GiB; no cleanup'}}
New-Item -ItemType Directory -Path $local | Out-Null
$copy=Join-Path $local 'source.zip'
[IO.File]::Copy($archive,$copy,$false)
if((Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(pin['archiveSha256'])}){{throw 'Source copy SHA mismatch'}}
Expand-Archive -LiteralPath $copy -DestinationPath $local
$ref=Join-Path $local 'reference.json'
[IO.File]::Copy({ps(str(PureWindowsPath(windows_report) / 'reference.json'))},$ref,$false)
if((Get-FileHash -LiteralPath $ref -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(hashlib.sha256((report/'reference.json').read_bytes()).hexdigest())}){{throw 'Reference receipt SHA mismatch'}}
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $local 'winbridge/scripts/test-update-shutdown-primitives.ps1') -SourceDirectory (Join-Path $local 'winbridge') -SourceArchiveSha256 {ps(pin['archiveSha256'])} -ValidationCacheDirectory {ps(cache)} -ReportDirectory {ps(windows_report)} -ReferenceReceipt $ref
exit $LASTEXITCODE
'''
    (report / 'bootstrap.ps1').write_text(script)
    request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'snapshot-fixture',
               'idempotencyKey': 'shutdown-primitives-' + run, 'timeoutMs': 1200000,
               'command': {'kind': 'direct', 'executable': 'pwsh.exe', 'workingDirectory': '.',
                           'arguments': ['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()]}}
    (local_report / 'request.json').write_text(json.dumps(request, indent=2))
    task = native.call(endpoint, {'method': 'submit', 'request': request})['task']
    print(json.dumps({'runId': run, 'taskId': task['taskId'], 'reportDirectory': str(report)}), flush=True)
    chunks, sequence = [], 0
    deadline = time.monotonic() + 1250
    while True:
        task = native.call(endpoint, {'method': 'get_task', 'taskId': task['taskId']})['task']
        while True:
            page = native.call(endpoint, {'method': 'read_output', 'taskId': task['taskId'], 'afterSequence': sequence})['chunks']
            if not page:
                break
            chunks.extend(page)
            sequence = page[-1]['sequence']
        if task['status'] not in {'queued', 'running'}:
            break
        if time.monotonic() > deadline:
            raise TimeoutError('Observe original task; do not resubmit: ' + task['taskId'])
        time.sleep(1)
    (local_report / 'task-report.json').write_text(json.dumps({'task': task, 'output': chunks}))
    if not (report / 'report.ready.json').exists():
        diagnostic = ''.join(bytes(c['bytes']).decode('utf-8', errors='replace') for c in chunks)[-7000:]
        raise RuntimeError('Bootstrap failed: ' + diagnostic)
    ready = json.loads((report / 'report.ready.json').read_text(encoding='utf-8-sig'))
    names = set()
    for item in ready['files']:
        name = item['path']
        if (not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name) or name in names
                or (report / name).is_symlink()):
            raise ValueError('Unsafe report name')
        names.add(name)
        data = (report / name).read_bytes()
        if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
            raise ValueError('Report bytes differ: ' + name)
    if not {'report.json', 'reference.json', 'source-pin.json', 'bootstrap.ps1', *(g + '.log' for g in GROUPS)} <= names:
        raise ValueError('Incomplete report bundle')
    result = json.loads((report / 'report.json').read_text(encoding='utf-8-sig'))
    if (task['status'] != 'succeeded' or task['exitCode'] != 0 or result.get('passed') is not True
            or result.get('runtimeApplied') is not False or result.get('windowsOpened') is not False
            or result.get('kind') != 'shutdown_primitives_native_gate'
            or result['sourceCommit'] != pin['sourceCommit'] or result['sourceArchiveSha256'] != pin['archiveSha256']
            or len(result['commands']) != len(GROUPS)
            or {g['name'] for g in result['commands']} != set(GROUPS)
            or any(g['exitCode'] != 0 or g['passed'] < GROUPS[g['name']] for g in result['commands'])):
        raise RuntimeError({'taskId': task['taskId'], 'result': result})
    print(json.dumps({'passed': True, 'taskId': task['taskId'], 'reportDirectory': str(report),
                      'filesVerified': len(names), 'testsPassed': sum(g['passed'] for g in result['commands'])}), flush=True)


if __name__ == '__main__':
    main()
