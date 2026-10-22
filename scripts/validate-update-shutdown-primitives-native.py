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

COLD_GROUPS = {'shutdown-reader-cli': 1, 'shutdown-reader-report': 1, 'shutdown-reader-v6-contract': 2}


def verify_migration_proof(proof, pin, reference):
    build=proof.get('build', {})
    if (proof.get('kind') != 'state_backup_schema_migration_proof' or proof.get('passed') is not True
            or proof.get('platform') != 'windows' or build.get('sourceCommit') != pin['sourceCommit']
            or build.get('sourceManifestSha256') != pin['manifestSha256'] or build.get('sourceDirty') is not False
            or proof.get('sourceProofSha256') != reference['sourceProofSha256']
            or proof.get('referenceManifestSha256') != reference['manifestSha256']
            or proof.get('referenceUpdaterSha256') != reference['updaterSha256']
            or proof.get('updateId') != reference['updateId']
            or proof.get('fromSchema') != 12 or proof.get('toSchema') != 13
            or not isinstance(proof.get('epochBefore'), int) or not isinstance(proof.get('epochAfter'), int)
            or proof['epochAfter'] <= proof['epochBefore']
            or not re.fullmatch(r'[0-9a-f]{64}',proof.get('retainedRowsSha256',''))
            or any(proof.get(k) is not True for k in ('backupRowsPreserved','shutdownHistoryPreserved','migrationBackupVerified','noBackupAttemptInvented','noCustodyAdopted','oldTwelveInitRefused','oldTwelveStatusRefused','sourceBytesUnchanged'))
            or proof.get('runtimeApplied') is not False or proof.get('windowsOpened') is not False):
        raise ValueError('Actual schema12 history/migration/refusal/source preservation differs')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--cache-run', required=True)
    modes=parser.add_mutually_exclusive_group()
    modes.add_argument('--cold-reader-only', action='store_true')
    modes.add_argument('--backup-migration-only', action='store_true')
    modes.add_argument('--runtime-file-moves-only', action='store_true')
    parser.add_argument('--reference-receipt', type=Path)
    parser.add_argument('--controller-config', type=Path, default=Path('.winbridge/controller-snapshot.local.json'))
    args = parser.parse_args()
    groups = {'runtime-owned-file-moves': 3, 'runtime-file-exchange-data': 6} if args.runtime_file_moves_only else {'backup-schema12-history-migration': 1} if args.backup_migration_only else COLD_GROUPS if args.cold_reader_only else GROUPS
    kind = 'runtime_file_moves_native_gate' if args.runtime_file_moves_only else 'state_backup_migration_native_gate' if args.backup_migration_only else 'shutdown_reader_native_gate' if args.cold_reader_only else 'shutdown_primitives_native_gate'
    mode_switch = ' -RuntimeFileMovesOnly' if args.runtime_file_moves_only else ' -BackupMigrationOnly' if args.backup_migration_only else ' -ColdReaderOnly' if args.cold_reader_only else ''
    pin = health.archive_pin(args.archive)
    with zipfile.ZipFile(args.archive) as source:
        if not args.runtime_file_moves_only and (native.worker_schema_from_source(source.read('winbridge/apps/worker/src/store.rs')) != 16
                or native.updater_schema_from_source(source.read('winbridge/apps/update/src/journal.rs')) != (13 if args.backup_migration_only else 12)):
            raise ValueError('This gate requires reviewed worker16 and matching updater schema source')
    if not re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.cache_run):
        raise ValueError('Exact retained cache run required')
    if args.runtime_file_moves_only:
        if args.reference_receipt:
            parser.error('Runtime file moves use generated fixtures, no old binary reference')
        reference = {'kind': 'runtime_file_move_validation', 'runtimeApplied': False, 'windowsOpened': False}
    else:
        if not args.reference_receipt:
            parser.error('--reference-receipt is required for shutdown/migration gates')
        reference = json.loads(args.reference_receipt.read_text())
    if args.runtime_file_moves_only:
        pass
    elif args.backup_migration_only:
        if reference.get('kind') != 'state_backup_schema12_reference' or any(not re.fullmatch(r'[0-9a-f]{64}', reference.get(k, '')) for k in ('sourceProofSha256', 'manifestSha256', 'updaterSha256', 'sourceManifestSha256')):
            raise ValueError('Proven schema12 archive/manifest/updater pins required')
    else:
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
    if args.runtime_file_moves_only:
        pass
    elif args.backup_migration_only:
        path=PureWindowsPath(reference['runtimeDirectory'])
        if not path.is_relative_to(project_path/'workflow-validation') or '..' in path.parts:
            raise ValueError('Schema12 reference must be a retained local validation runtime')
    else:
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
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $local 'winbridge/scripts/test-update-shutdown-primitives.ps1') -SourceDirectory (Join-Path $local 'winbridge') -SourceArchiveSha256 {ps(pin['archiveSha256'])} -ValidationCacheDirectory {ps(cache)} -ReportDirectory {ps(windows_report)} -ReferenceReceipt $ref{mode_switch}
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
    if not {'report.json', 'reference.json', 'source-pin.json', 'bootstrap.ps1'} <= names:
        raise ValueError('Incomplete report header bundle')
    result = json.loads((report / 'report.json').read_text(encoding='utf-8-sig'))
    if task['status'] != 'succeeded' or task['exitCode'] != 0 or result.get('passed') is not True:
        raise RuntimeError({'taskId': task['taskId'], 'error': result.get('error'), 'reportDirectory': str(report), 'filesVerified': len(names)})
    if not {g + '.log' for g in groups} <= names:
        raise ValueError('Successful report is missing test logs')
    if (task['status'] != 'succeeded' or task['exitCode'] != 0 or result.get('passed') is not True
            or result.get('runtimeApplied') is not False or result.get('windowsOpened') is not False
            or result.get('kind') != kind
            or result['sourceCommit'] != pin['sourceCommit'] or result['sourceArchiveSha256'] != pin['archiveSha256']
            or len(result['commands']) != len(groups)
            or {g['name'] for g in result['commands']} != set(groups)
            or any(g['exitCode'] != 0 or g['passed'] < groups[g['name']] for g in result['commands'])):
        raise RuntimeError({'taskId': task['taskId'], 'result': result})
    if args.backup_migration_only:
        if 'state-backup-migration-proof.json' not in names:
            raise ValueError('Migration report missing actual proof')
        verify_migration_proof(json.loads((report/'state-backup-migration-proof.json').read_text(encoding='utf-8-sig')), pin, reference)
    print(json.dumps({'passed': True, 'taskId': task['taskId'], 'reportDirectory': str(report),
                      'filesVerified': len(names), 'testsPassed': sum(g['passed'] for g in result['commands'])}), flush=True)


if __name__ == '__main__':
    main()
