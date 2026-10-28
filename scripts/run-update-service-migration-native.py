#!/usr/bin/env python3
"""One actual19 ->20 consistent-copy gate through the installed worker; never install."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import shutil
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main(profile='Service19To20'):
    if profile not in ('Service19To20', 'Controller20To21'):
        raise ValueError('Unknown fixed migration profile')
    controller = profile == 'Controller20To21'
    prefix = 'controller' if controller else 'service'
    parser = argparse.ArgumentParser(description=f'One actual{20 if controller else 19} ->{21 if controller else 20} consistent-copy gate; never install.')
    for name in ('source-archive', 'reference-source-archive', 'reference-target-archive', 'controller-config'):
        parser.add_argument('--' + name, required=True, type=Path)
    for name in (('cache-run',) if controller else ('reference-run', 'reference-task-id', 'reference-report-sha256', 'reference-proof-sha256', 'cache-run')):
        parser.add_argument('--' + name, required=True)
    if controller:
        parser.add_argument('--reference-run-info', required=True, type=Path)
    args = parser.parse_args()
    runner = load('service_migration_transport', 'run-update-service-main-native.py')
    legacy = load('service_migration_legacy', 'validate-update-activation-preparation-native.py')
    evidence = load('service_migration_evidence', 'update-activation-service-migration-proof.py')
    native, health, checks = legacy.native, legacy.health, legacy.checks
    if controller:
        evidence = load('controller_migration_evidence', 'update-activation-controller-migration-proof.py')
        reference, archive_location, reference_audit, old_meta = evidence.reference_from_main25(
            args.reference_run_info, args.reference_source_archive, args.reference_target_archive)
        args.reference_run, args.reference_task_id = old_meta['run'], old_meta['taskId']
        args.reference_report_sha256, args.reference_proof_sha256 = reference['reportSha256'], reference['proofSha256']
    for run in (args.reference_run, args.cache_run):
        checks.need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', run), 'Explicit retained run required')
    for sha in (args.reference_report_sha256, args.reference_proof_sha256):
        checks.need(re.fullmatch(r'[0-9a-f]{64}', sha), 'Explicit retained SHA required')
    checks.need(str(uuid.UUID(args.reference_task_id)) == args.reference_task_id, 'Canonical reference task required')
    source, old_source, old_target = map(health.archive_pin, (args.source_archive, args.reference_source_archive, args.reference_target_archive))
    schemas = (21, 20, 20) if controller else (20, 19, 19)
    for path, expected in zip((args.source_archive, args.reference_source_archive, args.reference_target_archive), schemas):
        with zipfile.ZipFile(path) as package:
            checks.need(native.updater_schema_from_source(package.read('winbridge/apps/update/src/journal.rs')) == expected,
                        'Explicit migration schema differs from archive')
    checks.need(shutil.disk_usage(ROOT).free >= 6 * 1024**3, '6 GiB Debian free required')
    cfg = json.loads(args.controller_config.read_text())
    endpoint = Path(cfg['stateDirectory']) / ('controller-' + cfg['instanceId'] + '.sock')
    before = native.call(endpoint, {'method': 'info'})['info']
    checks.need(before['platform'] == 'windows' and before['activeTaskCount'] == 0
                and not before.get('maintenanceError') and not before.get('journalRecoveryRequired'), 'Healthy idle installed worker required')
    project = next(p for p in before['projects'] if p['id'] == 'snapshot-fixture')
    reports = Path('/mnt/hgfs/shared/WinBridge/workflow-validation')
    files = legacy.support.ready_files(reports / args.reference_run)
    checks.need(files == legacy.support.ready_files(reports / args.reference_run), 'Reference files changed')
    checks.need(hashlib.sha256(files['report.json']).hexdigest() == args.reference_report_sha256
                and hashlib.sha256(files['candidate-health-actor-proof.json']).hexdigest() == args.reference_proof_sha256,
                'Reference report/proof SHA differs')
    old_report = json.loads(files['report.json'].decode('utf-8-sig'))
    checks.need(old_report['kind'] == ('activation_service_main_native_gate' if controller else 'activation_writer_main_native_gate') and old_report['passed'] is True
                and old_report['error'] is None and old_report['sourceCommit'] == old_source['sourceCommit']
                and old_report['sourceArchiveSha256'] == old_source['archiveSha256'] and old_report['target'] == old_target,
                'Not the pinned successful Main24 report')
    old_task = native.call(endpoint, {'method': 'get_task', 'taskId': args.reference_task_id})['task']
    checks.need(old_task['status'] == 'succeeded' and old_task['exitCode'] == 0
                and old_task['submission']['projectId'] == project['id']
                and old_task['submission']['idempotencyKey'] == ('native-main25-204-' if controller else 'native-activation-writer-') + args.reference_run,
                'Reference terminal task differs')
    old_proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
    root = str(PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / args.reference_run)
    if controller:
        old_audits = reference_audit['negativeAudits'] + reference_audit['nativeMarkerNegativeAudits']
    else:
        old_audits = legacy.writer_checks.audit(old_proof, old_source, old_target, root, checks, legacy.state_checks,
                                               health, startup_timeout_ms=legacy.startup_budget(args.reference_source_archive))
        old_audits += len(legacy.replacement_audit.audit(dict(old_proof, kind='runtime_replacement_main_native_proof'),
                                                       old_source, old_target, True, worker_schema=17))
        pin = dict(run=args.reference_run, target=old_target, reportSha256=args.reference_report_sha256,
                   proofSha256=args.reference_proof_sha256)
        reference = evidence.derive_reference(pin, old_proof, checks.canonical)
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime()) + '-' + uuid.uuid4().hex[:8]
    local = ROOT / '.winbridge/update-preparation' / (prefix + '-migration-' + run)
    local.mkdir(parents=True)
    shared = reports / run
    shared.mkdir()
    runner.write_new(shared / 'reference-input.json', reference)
    reference_sha = hashlib.sha256((shared / 'reference-input.json').read_bytes()).hexdigest()
    work = PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / ('tmp-' + prefix + '-migration-' + run)
    peer = PureWindowsPath('K:/VMs/shared/WinBridge/workflow-validation') / run
    archive = PureWindowsPath('K:/VMs/shared') / args.source_archive.resolve().relative_to('/mnt/hgfs/shared')
    cache = PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / args.cache_run
    ps = native.ps_string
    script = f'''$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'
$work={ps(str(work))};$archive={ps(str(archive))}
if(Test-Path -LiteralPath $work){{throw 'Existing migration preserved'}}
if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(source['archiveSha256'])}){{throw 'Source changed'}}
if([IO.DriveInfo]::new('K:\\').AvailableFreeSpace -lt 6GB){{throw 'Less than 6 GiB; no build'}}
New-Item -ItemType Directory -Path $work|Out-Null
[IO.File]::Copy($archive,(Join-Path $work 'source.zip'),$false)
Expand-Archive -LiteralPath (Join-Path $work 'source.zip') -DestinationPath $work
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $work 'winbridge/scripts/test-update-{prefix}-migration.ps1') -SourceDirectory (Join-Path $work 'winbridge') -SourceArchiveSha256 {ps(source['archiveSha256'])} -ReferenceFile {ps(str(peer/'reference-input.json'))} -ReferenceSha256 {ps(reference_sha)} -ValidationCacheDirectory {ps(str(cache))} -ReportDirectory {ps(str(peer/'result'))}
exit $LASTEXITCODE
'''
    context = before['fencedRequests']['context']
    submission = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId=project['id'],
                      idempotencyKey='native-' + prefix + '-migration-' + run, timeoutMs=900000,
                      command=dict(kind='direct', executable='pwsh.exe', arguments=['-NoProfile', '-NonInteractive', '-Command', script], workingDirectory='.'))
    request = dict(requestContractVersion=2, context=context, request=dict(method='submit', request=submission))
    runner.write_new(local / 'request.json', request)
    runner.write_new(local / 'info-before.json', before)
    runner.write_new(local / 'reference-terminal.json', old_task)
    metadata = dict(run=run, source=source, reference=reference,
                     referenceSource=old_source, referenceTarget=old_target, referenceTaskId=old_task['taskId'],
                     referenceNegativeAudits=old_audits, reportDirectory=str(shared / 'result'),
                     requestSha256=hashlib.sha256((local / 'request.json').read_bytes()).hexdigest(),
                     debianFreeBytesBefore=shutil.disk_usage(ROOT).free)
    if controller:
        metadata.update(profile=profile, referenceRunInfo=str(args.reference_run_info.resolve()), referenceArchiveLocation=archive_location)
    runner.write_new(local / 'run-info.json', metadata)
    reply = runner.submit_fenced(native, endpoint, request)
    runner.write_new(local / 'acceptance.json', reply)
    checks.need(reply.get('requestContractVersion') == 2 and reply.get('context') == context
                and reply.get('outcome') == 'responded' and reply['response']['kind'] == 'task',
                'Acceptance unknown; inspect saved request/key, never submit again')
    task = reply['response']['task']
    print(json.dumps(dict(taskId=task['taskId'], local=str(local))), flush=True)
    sequence = 0
    with (local / 'output.jsonl').open('x') as output:
        deadline = time.monotonic() + 960
        while True:
            task = native.call(endpoint, dict(method='get_task', taskId=task['taskId']))['task']
            while True:
                page = native.call(endpoint, dict(method='read_output', taskId=task['taskId'], afterSequence=sequence))['chunks']
                if not page:
                    break
                for chunk in page:
                    output.write(json.dumps(chunk) + '\n')
                    print(bytes(chunk['bytes']).decode('utf-8', errors='replace'), end='', flush=True)
                output.flush()
                sequence = page[-1]['sequence']
            if task['status'] not in ('queued', 'running'):
                break
            checks.need(time.monotonic() < deadline, 'Observer deadline; inspect same task ID')
            time.sleep(1)
    runner.write_new(local / 'terminal.json', task)
    after = native.call(endpoint, {'method': 'info'})['info']
    runner.write_new(local / 'info-after.json', after)
    checks.need(task['status'] == 'succeeded' and task['exitCode'] == 0 and task['submission'] == submission, 'Native migration failed; preserve original/copy')
    verifier = load('service_migration_independent', 'verify-update-service-migration-native.py')
    result = verifier.verify(local / 'run-info.json', args.source_archive, args.reference_source_archive, args.reference_target_archive, profile=profile)
    runner.write_new(local / 'verified.json', result)
    print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
