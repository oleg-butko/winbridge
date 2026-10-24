#!/usr/bin/env python3
"""One isolated Main18 preparation and copy-only17 migration gate; never install."""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PureWindowsPath
import re
import shutil
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


actor = load('support_actor', 'validate-update-support-actor-native.py')
checks = load('activation_checks', 'update-activation-preparation-proof.py')
replacement_audit = load('replacement_audit', 'test-update-replacement-proof.py')
support, health, native, need = actor.support, actor.health, actor.native, checks.need
PREFIX = 'native-activation-preparation-'


def reference_from_pin(pin, reports, workspace):
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', pin['run']) and pin['passed'] is True, 'Invalid reference pin')
    files = support.ready_files(reports/pin['run'])
    need(support.ready_files(reports/pin['run']) == files
         and hashlib.sha256(files['report.json']).hexdigest() == pin['reportSha256']
         and hashlib.sha256(files['candidate-support-actor-proof.json']).hexdigest() == pin['proofSha256'], 'Reference bytes changed')
    proof = json.loads(files['candidate-support-actor-proof.json'].decode('utf-8-sig'))
    actor.verify(proof, pin['source'], pin['target'], str(PureWindowsPath(workspace)/'workflow-validation'/pin['run']), 17, True)
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    need(report['passed'] is True and report['kind'] == 'compatible_drain_main_native_gate'
         and report['sourceCommit'] == pin['source']['sourceCommit']
         and report['sourceArchiveSha256'] == pin['source']['archiveSha256'] and report['target'] == pin['target'],
         'Reference is not the terminal production17 report')
    record = proof['record']
    return dict(schemaVersion=1, kind='activation_schema17_reference', referenceRun=pin['run'],
                reportSha256=pin['reportSha256'], proofSha256=pin['proofSha256'], source=pin['source'],
                image=proof['oldRuntimeImages'][3], descriptor=record['intent']['executor'],
                manifestSha256=record['result']['finalWorker']['identity']['manifestSha256'],
                updateId=record['intent']['updateId'], supportSha256=hashlib.sha256(checks.canonical(record)).hexdigest(),
                compatibleSha256=hashlib.sha256(checks.canonical(proof['beforeCleanup']['compatible']['record'])).hexdigest())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--target-archive', type=Path, required=True)
    parser.add_argument('--cache-run', required=True)
    parser.add_argument('--reference-pin', type=Path, required=True)
    parser.add_argument('--controller-config', type=Path, default=Path('.winbridge/controller-snapshot.local.json'))
    parser.add_argument('--verify-run', help='Observe the saved task, never resubmit')
    args = parser.parse_args()
    source, target = (health.archive_pin(p) for p in (args.archive, args.target_archive))
    health.require_runtime_schemas([args.archive, args.target_archive], 18)
    need(source['sourceCommit'] != target['sourceCommit'], 'Distinct producer18 and fixture18 required')
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.cache_run), 'Invalid cache run')
    cfg = json.loads(args.controller_config.read_text())
    shared = Path(cfg['delivery']['root'])/cfg['delivery']['namespace']
    peer = PureWindowsPath(cfg['delivery']['peerRoot'])/cfg['delivery']['namespace']
    endpoint = Path(cfg['stateDirectory'])/('controller-'+cfg['instanceId']+'.sock')
    info = native.call(endpoint, {'method': 'info'})['info']
    need(info['platform'] == 'windows' and not info.get('maintenanceError') and not info.get('journalRecoveryRequired')
         and info.get('recoveryCoverage', {}).get('complete') is True, 'Healthy installed Windows worker required')
    project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
    if not args.verify_run:
        need(info['activeTaskCount'] == 0 and shutil.disk_usage(ROOT).free >= 6*1024**3, 'Idle worker and 6 GiB Debian required')
    prior_pin = json.loads(args.reference_pin.read_text())
    reference = reference_from_pin(prior_pin, shared/'workflow-validation', project['workspaceRoot'])
    prior = native.call(endpoint, {'method': 'get_task', 'taskId': prior_pin['taskId']})['task']
    need(prior['status'] == 'succeeded' and prior['exitCode'] == 0
         and prior['submission']['projectId'] == 'snapshot-fixture'
         and prior['submission']['idempotencyKey'] == 'native-compatible-drain-'+reference['referenceRun'], 'Reference task differs')
    run = args.verify_run or time.strftime('%Y%m%d-%H%M%S', time.gmtime())+'-'+uuid.uuid4().hex[:8]
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', run), 'Invalid run')
    local, report = ROOT/'.winbridge/workflow-validation'/run, shared/'workflow-validation'/run
    root = str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/run)
    meta_file = local/'activation-preparation-run.json'
    if args.verify_run:
        meta = json.loads(meta_file.read_text())
        need(meta['source'] == source and meta['target'] == target and meta['reference'] == reference
             and meta['cacheRun'] == args.cache_run and meta['run'] == run, 'Saved scope changed')
        if meta['taskId'] is None:
            raw = (local/'request.json').read_bytes()
            need(hashlib.sha256(raw).hexdigest() == meta['requestSha256'], 'Saved request changed')
            request = json.loads(raw)
            need(request['idempotencyKey'] == PREFIX+run, 'Pending request changed')
            meta['taskId'] = support.recover_task(endpoint, request)['taskId']
            support.save_meta(meta_file, meta)
    else:
        local.mkdir(parents=True)
        report.mkdir()
        archive = peer/'sources'/args.archive.resolve().relative_to((shared/'sources').resolve())
        target_archive = peer/'sources'/args.target_archive.resolve().relative_to((shared/'sources').resolve())
        raw_reference = json.dumps(reference, indent=2).encode()
        (report/'reference.json').write_bytes(raw_reference)
        reference_sha = hashlib.sha256(raw_reference).hexdigest()
        ps = native.ps_string
        script = f"""$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$root={ps(root)}
if(Test-Path -LiteralPath $root){{throw 'Existing run preserved; no replay'}}
if([IO.DriveInfo]::new([IO.Path]::GetPathRoot($root)).AvailableFreeSpace -lt 6GB){{throw 'Less than 6 GiB Windows free; no cleanup'}}
New-Item -ItemType Directory -Path $root|Out-Null
$archive={ps(str(archive))}
if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(source['archiveSha256'])}){{throw 'Source ZIP changed'}}
$copy=Join-Path $root 'source.zip'
[IO.File]::Copy($archive,$copy,$false)
if((Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(source['archiveSha256'])}){{throw 'Source ZIP copy changed'}}
Expand-Archive -LiteralPath $copy -DestinationPath $root
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $root 'winbridge/scripts/test-update-health-actor.ps1') -SourceDirectory (Join-Path $root 'winbridge') -SourceArchiveSha256 {ps(source['archiveSha256'])} -TargetArchivePath {ps(str(target_archive))} -TargetArchiveSha256 {ps(target['archiveSha256'])} -ValidationCacheDirectory {ps(str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/args.cache_run))} -ReportDirectory {ps(str(peer/'workflow-validation'/run))} -ActivationPreparationGate -MigrationReferenceFile {ps(str(peer/'workflow-validation'/run/'reference.json'))} -MigrationReferenceSha256 {ps(reference_sha)}
exit $LASTEXITCODE
"""
        request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId='snapshot-fixture', idempotencyKey=PREFIX+run,
                       timeoutMs=1800000, command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
                       arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()]))
        raw = json.dumps(request).encode()
        with (local/'request.json').open('xb') as f:
            f.write(raw)
            f.flush()
            os.fsync(f.fileno())
        meta = dict(taskId=None, run=run, source=source, target=target, reference=reference, cacheRun=args.cache_run,
                    requestSha256=hashlib.sha256(raw).hexdigest(), debianFreeBytesBefore=shutil.disk_usage(ROOT).free)
        support.save_meta(meta_file, meta)
        print(json.dumps(dict(taskId=None, run=run, recovery='--verify-run '+run)), flush=True)
        meta['taskId'] = native.call(endpoint, {'method': 'submit', 'request': request})['task']['taskId']
        support.save_meta(meta_file, meta)
    print(json.dumps(dict(taskId=meta['taskId'], run=run)), flush=True)
    deadline = time.monotonic()+1850
    while True:
        task = native.call(endpoint, {'method': 'get_task', 'taskId': meta['taskId']})['task']
        need(task['submission']['projectId'] == 'snapshot-fixture' and task['submission']['idempotencyKey'] == PREFIX+run, 'Task scope differs')
        if task['status'] not in ('queued', 'running'):
            break
        need(time.monotonic() < deadline, 'Observation expired; inspect this same task/run, never resubmit')
        time.sleep(1)
    (local/'task-report.json').write_text(json.dumps({'task': task}))
    files = support.ready_files(report)
    need(support.ready_files(report) == files, 'Ready files changed')
    result = json.loads(files['report.json'].decode('utf-8-sig'))
    need(task['status'] == 'succeeded' and task['exitCode'] == 0 and result['passed'] is True
         and result['kind'] == 'activation_preparation_main_native_gate' and result['error'] is None
         and result['sourceCommit'] == source['sourceCommit'] and result['sourceArchiveSha256'] == source['archiveSha256']
         and result['target'] == target and result['runtimeApplied'] is True
         and result['installedRuntimeApplied'] is False and result['windowsOpened'] is False,
         'Native preparation failed; inspect '+str(report))
    groups = [('activation-data-ledger', 23), ('activation-contract', 2), ('activation-mailbox', 1), ('activation-actual-migration', 1),
              ('runtime-replacement-observer-sharing', 1), ('runtime-replacement-model-ledger-and-directory', 15),
              ('runtime-replacement-original-custody', 1), ('runtime-replacement-contract', 3),
              ('runtime-preparation-driver-and-ledger', 16), ('runtime-preparation-directory-pins', 1),
              ('backup-empty-wal-fingerprint', 1), ('backup-crash-observation', 1), ('backup-data-and-ledger', 15),
              ('backup-sqlite-wal-and-pins', 3), ('backup-exact-entry', 2), ('shutdown-main-pipeline', 1)]
    need([c['name'] for c in result['commands']] == ['health-supervisor-build']+[n for n,_ in groups[:-1]]+['shutdown-old-real-set',groups[-1][0]]
         and all(c['exitCode'] == 0 for c in result['commands']), 'Selected native commands changed')
    counts = {name: sum(int(m[1]) for m in re.finditer(r'test result: ok\. ([0-9]+) passed; 0 failed;',files[name+'.log'].decode('utf-8-sig'))) for name,_ in groups}
    need(all(counts[n] >= minimum for n,minimum in groups), 'Expected native tests were not selected')
    proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
    rejected = checks.audit(proof, source, target, root, health)
    rejected += len(replacement_audit.audit(dict(proof, kind='runtime_replacement_main_native_proof'), source, target))
    need(hashlib.sha256(files['reference.json']).hexdigest() == result['migrationReferenceSha256'], 'Migration reference changed')
    migration = json.loads(files['activation-migration-proof.json'].decode('utf-8-sig'))
    migration_rejected = checks.audit_migration(migration, source, reference)
    verified = dict(meta, passed=True, filesVerifiedTwice=len(files), nativeTestsPassed=sum(counts.values()), testCounts=counts,
                    negativeMutationsRejected=rejected, migrationMutationsRejected=migration_rejected,
                    proofSha256=hashlib.sha256(files['candidate-health-actor-proof.json']).hexdigest(),
                    migrationProofSha256=hashlib.sha256(files['activation-migration-proof.json']).hexdigest(),
                    reportSha256=hashlib.sha256(files['report.json']).hexdigest(), storage=result['storage'], debianFreeBytesAfter=shutil.disk_usage(ROOT).free)
    (local/'activation-preparation-verified.json').write_text(json.dumps(verified, indent=2))
    print(json.dumps(verified), flush=True)


if __name__ == '__main__':
    main()
