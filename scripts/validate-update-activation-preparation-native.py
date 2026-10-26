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
import types
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


actor = load('support_actor', 'validate-update-support-actor-native.py')
checks = load('activation_checks', 'update-activation-preparation-proof.py')
replacement_audit = load('replacement_audit', 'test-update-replacement-proof.py')
file_checks = load('activation_file_checks', 'update-activation-files-proof.py')
process_checks = load('activation_process_checks', 'update-activation-process-proof.py')
state_checks = load('activation_state_checks', 'update-activation-state-proof.py')
writer_checks = load('activation_writer_checks', 'update-activation-writer-proof.py')
writer_references = load('writer_references', 'update-activation-writer-reference.py')
writer_migrations = load('writer_migrations', 'update-activation-writer-migration-proof.py')
support, health, native, need = actor.support, actor.health, actor.native, checks.need
PREFIX = 'native-activation-preparation-'


def startup_budget(archive):
    """Derive the admission default from checksum-verified source, never a proof."""
    with zipfile.ZipFile(archive) as package:
        source = package.read('winbridge/apps/update/src/activation.rs')
        factory = package.read('winbridge/apps/update/src/journal/activation.rs')
    values = re.findall(rb'(?m)^pub const STARTUP_TIMEOUT_MS: u64 = ([0-9_]+);\r?$', source)
    if not values:
        need(re.search(rb'crate::sha256\(nonce.as_bytes\(\)\),\s*120000\s*,?\s*\)', factory), 'Unknown legacy admission budget')
        return 120000
    need(len(values) == 1 and re.search(rb'crate::sha256\(nonce.as_bytes\(\)\),\s*STARTUP_TIMEOUT_MS\s*,?\s*\)', factory), 'Ambiguous startup admission budget')
    budget = int(values[0].replace(b'_', b''))
    need(budget in (120000, 240000), 'Unreviewed startup admission budget')
    return budget


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
    parser.add_argument('--task-timeout-ms', type=int, default=1800000,
                        help='Whole isolated experiment task budget, not startup admission')
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--activation-files',action='store_true',help='Explicit actor21 whole-set read binding; no worker launch')
    mode.add_argument('--activation-process',action='store_true',help='Explicit actor22 private receiver; no worker state writer')
    mode.add_argument('--activation-state',action='store_true',help='Explicit actor23 same-child physical lock handoff; no worker state writer')
    mode.add_argument('--activation-writer',action='store_true',help='Explicit actor24 same-child journal writer, actual18/16 migrations; no normal service or install')
    args = parser.parse_args()
    need(1800000 <= args.task_timeout_ms <= 7200000, 'Expected task budget between 30 and 120 minutes')
    state_mode = args.activation_state or args.activation_writer
    process_mode = args.activation_process or state_mode
    files_mode = args.activation_files or process_mode
    prefix = 'native-activation-writer-' if args.activation_writer else 'native-activation-state-' if args.activation_state else 'native-activation-process-' if process_mode else 'native-activation-files-' if files_mode else PREFIX
    source, target = (health.archive_pin(p) for p in (args.archive, args.target_archive))
    writer_budget = startup_budget(args.archive) if args.activation_writer else 120000
    if args.activation_writer:
        for archive in (args.archive, args.target_archive):
            with zipfile.ZipFile(archive) as package:
                need(native.updater_schema_from_source(package.read('winbridge/apps/update/src/journal.rs')) == 19
                     and native.worker_schema_from_source(package.read('winbridge/apps/worker/src/store.rs')) == 17,
                     'Writer mode requires exact19/17 in both clean archives')
                wire = package.read('winbridge/crates/protocol/src/update/activation_receiver.rs')
                need(b'WINBRIDGE_PRIVATE_JOURNAL_WRITER_V3' in wire
                     and re.search(rb'(?m)^pub const ACTIVATION_WRITER_CONTRACT: u16 = 3;\r?$', wire), 'Private writer3 required in both archives')
    else:
        health.require_runtime_schemas([args.archive, args.target_archive], 18)
    if state_mode:
        for archive in (args.archive, args.target_archive):
            with zipfile.ZipFile(archive) as package:
                data = package.read('winbridge/crates/protocol/src/update/activation_receiver.rs')
            need(re.search(rb'(?m)^pub const ACTIVATION_STATE_HANDOFF_CONTRACT: u16 = 2;\r?$', data),
                 'State handoff needs V2 in both clean source archives')
    need(source['sourceCommit'] != target['sourceCommit'], 'Distinct clean producer and receiver required')
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
    if args.activation_writer:
        reference = writer_references.from_pin(prior_pin, shared/'workflow-validation', project['workspaceRoot'],
            types.SimpleNamespace(support=support,state_checks=state_checks,checks=checks,health=health))
    else:
        reference = reference_from_pin(prior_pin, shared/'workflow-validation', project['workspaceRoot'])
    prior = native.call(endpoint, {'method': 'get_task', 'taskId': prior_pin['taskId']})['task']
    need(prior['status'] == 'succeeded' and prior['exitCode'] == 0
         and prior['submission']['projectId'] == 'snapshot-fixture'
         and prior['submission']['idempotencyKey'] == ('native-activation-state-' if args.activation_writer else 'native-compatible-drain-')+reference['referenceRun'], 'Reference task differs')
    run = args.verify_run or time.strftime('%Y%m%d-%H%M%S', time.gmtime())+'-'+uuid.uuid4().hex[:8]
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', run), 'Invalid run')
    local, report = ROOT/'.winbridge/workflow-validation'/run, shared/'workflow-validation'/run
    root = str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/run)
    meta_file = local/'activation-preparation-run.json'
    if args.verify_run:
        meta = json.loads(meta_file.read_text())
        need(meta['source'] == source and meta['target'] == target and meta['reference'] == reference
             and meta['cacheRun'] == args.cache_run and meta['run'] == run
             and meta.get('activationFiles',False) is files_mode
             and meta.get('activationProcess',False) is process_mode
             and meta.get('activationState',False) is state_mode
             and meta.get('activationWriter',False) is args.activation_writer, 'Saved scope changed')
        if meta['taskId'] is None:
            raw = (local/'request.json').read_bytes()
            need(hashlib.sha256(raw).hexdigest() == meta['requestSha256'], 'Saved request changed')
            request = json.loads(raw)
            need(request['idempotencyKey'] == prefix+run, 'Pending request changed')
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
        mode = '-ActivationWriterGate' if args.activation_writer else '-ActivationStateGate' if args.activation_state else '-ActivationProcessGate' if process_mode else '-ActivationFilesGate' if files_mode else '-ActivationPreparationGate'
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
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $root 'winbridge/scripts/test-update-health-actor.ps1') -SourceDirectory (Join-Path $root 'winbridge') -SourceArchiveSha256 {ps(source['archiveSha256'])} -TargetArchivePath {ps(str(target_archive))} -TargetArchiveSha256 {ps(target['archiveSha256'])} -ValidationCacheDirectory {ps(str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/args.cache_run))} -ReportDirectory {ps(str(peer/'workflow-validation'/run))} {mode} -MigrationReferenceFile {ps(str(peer/'workflow-validation'/run/'reference.json'))} -MigrationReferenceSha256 {ps(reference_sha)}
exit $LASTEXITCODE
"""
        request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId='snapshot-fixture', idempotencyKey=prefix+run,
                       timeoutMs=args.task_timeout_ms, command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
                       arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()]))
        raw = json.dumps(request).encode()
        with (local/'request.json').open('xb') as f:
            f.write(raw)
            f.flush()
            os.fsync(f.fileno())
        meta = dict(taskId=None, run=run, source=source, target=target, reference=reference, cacheRun=args.cache_run, activationFiles=files_mode, activationProcess=process_mode,
                    requestSha256=hashlib.sha256(raw).hexdigest(), debianFreeBytesBefore=shutil.disk_usage(ROOT).free)
        if state_mode:
            meta['activationState'] = True
        if args.activation_writer:
            meta['activationWriter'] = True
        support.save_meta(meta_file, meta)
        print(json.dumps(dict(taskId=None, run=run, recovery='--verify-run '+run)), flush=True)
        meta['taskId'] = native.call(endpoint, {'method': 'submit', 'request': request})['task']['taskId']
        support.save_meta(meta_file, meta)
    print(json.dumps(dict(taskId=meta['taskId'], run=run)), flush=True)
    deadline = time.monotonic()+args.task_timeout_ms/1000+50
    while True:
        task = native.call(endpoint, {'method': 'get_task', 'taskId': meta['taskId']})['task']
        need(task['submission']['projectId'] == 'snapshot-fixture' and task['submission']['idempotencyKey'] == prefix+run, 'Task scope differs')
        if task['status'] not in ('queued', 'running'):
            break
        need(time.monotonic() < deadline, 'Observation expired; inspect this same task/run, never resubmit')
        time.sleep(1)
    (local/'task-report.json').write_text(json.dumps({'task': task}))
    files = support.ready_files(report)
    need(support.ready_files(report) == files, 'Ready files changed')
    result = json.loads(files['report.json'].decode('utf-8-sig'))
    need(task['status'] == 'succeeded' and task['exitCode'] == 0 and result['passed'] is True
         and result['kind'] == ('activation_writer_main_native_gate' if args.activation_writer else 'activation_state_main_native_gate' if args.activation_state else 'activation_process_main_native_gate' if process_mode else 'activation_files_main_native_gate' if files_mode else 'activation_preparation_main_native_gate') and result['error'] is None
         and result['sourceCommit'] == source['sourceCommit'] and result['sourceArchiveSha256'] == source['archiveSha256']
         and result['target'] == target and result['runtimeApplied'] is True
         and result['installedRuntimeApplied'] is False and result['windowsOpened'] is False,
         'Native preparation failed; inspect '+str(report))
    groups = [('activation-data-ledger', 24 if files_mode else 23), ('activation-contract', 2), ('activation-mailbox', 1), ('activation-actual-migration', 1),
              ('runtime-replacement-observer-sharing', 1), ('runtime-replacement-model-ledger-and-directory', 15),
              ('runtime-replacement-original-custody', 1), ('runtime-replacement-contract', 3),
              ('runtime-preparation-driver-and-ledger', 16), ('runtime-preparation-directory-pins', 1),
              ('backup-empty-wal-fingerprint', 1), ('backup-crash-observation', 1), ('backup-data-and-ledger', 15),
              ('backup-sqlite-wal-and-pins', 3), ('backup-exact-entry', 2), ('shutdown-main-pipeline', 1)]
    if files_mode:
        groups=[('activation-files-primitives',4),('activation-marker-search',2),('activation-files-contract',2),('activation-files-mailbox',1)]+groups
    if process_mode:
        groups=[('activation-process-atomic-create',1),('activation-process-contract',2),('activation-process-mailbox',1),('activation-process-owner-io',2),('activation-process-owner-state',1)]+groups
    if state_mode:
        groups=[('activation-state-boundary-probes',3),('activation-state-contract',2),('activation-state-mailbox',1),
                ('activation-state-owner',1),('activation-state-journal',2),('activation-state-release',5),
                ('activation-state-original-pins',2)]+groups
    if args.activation_writer:
        groups=[('writer-model-and-ledger',19),('writer-worker-sql',11),('writer-existing-open',7),
                ('writer-view-contract',2),('writer-mailbox',1),('writer-send-trace',1),('writer-actual-worker-migration',1)]+groups
    need([c['name'] for c in result['commands']] == ['health-supervisor-build']+(['activation-process-fixture-build'] if process_mode else [])+[n for n,_ in groups[:-1]]+['shutdown-old-real-set',groups[-1][0]]
         and all(c['exitCode'] == 0 for c in result['commands']), 'Selected native commands changed')
    counts = {name: sum(int(m[1]) for m in re.finditer(r'test result: ok\. ([0-9]+) passed; 0 failed;',files[name+'.log'].decode('utf-8-sig'))) for name,_ in groups}
    need(all(counts[n] >= minimum for n,minimum in groups), 'Expected native tests were not selected')
    proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
    state_rejected = 0
    writer_rejected = 0
    if args.activation_writer:
        writer_rejected=writer_checks.audit(proof,source,target,root,checks,state_checks,health,startup_timeout_ms=writer_budget)
        rejected=len(replacement_audit.audit(dict(proof,kind='runtime_replacement_main_native_proof'),source,target,True,worker_schema=17))
        file_rejected=process_rejected=0
    elif args.activation_state:
        state_rejected=state_checks.audit(proof,source,target,root,checks,health)
        rejected=len(replacement_audit.audit(dict(proof,kind='runtime_replacement_main_native_proof'),source,target,True))
        file_rejected=process_rejected=0
    elif process_mode:
        process_rejected=process_checks.audit(proof,source,target,root,checks,health)
        rejected=len(replacement_audit.audit(dict(proof,kind='runtime_replacement_main_native_proof'),source,target,True))
        file_rejected=0
    else:
        component=dict(proof,kind='activation_preparation_main_native_proof') if files_mode else proof
        rejected=checks.audit(component,source,target,root,health)
        file_rejected=file_checks.audit(proof,source,target,root,checks,health) if files_mode else 0
        rejected+=len(replacement_audit.audit(dict(proof,kind='runtime_replacement_main_native_proof'),source,target))
        process_rejected=0
    need(hashlib.sha256(files['reference.json']).hexdigest() == result['migrationReferenceSha256'], 'Migration reference changed')
    migration = json.loads(files['activation-migration-proof.json'].decode('utf-8-sig'))
    if args.activation_writer:
        worker_migration = json.loads(files['worker-journal-activation-migration-proof.json'].decode('utf-8-sig'))
        migration_rejected = writer_migrations.audit(migration,worker_migration,source,reference,checks,state_checks,root)
    else:
        migration_rejected = checks.audit_migration(migration, source, reference)
    verified = dict(meta, passed=True, filesVerifiedTwice=len(files), nativeTestsPassed=sum(counts.values()), testCounts=counts,
                    negativeMutationsRejected=rejected, activationFilesMutationsRejected=file_rejected, migrationMutationsRejected=migration_rejected, processMutationsRejected=process_rejected,
                    proofSha256=hashlib.sha256(files['candidate-health-actor-proof.json']).hexdigest(),
                    migrationProofSha256=hashlib.sha256(files['activation-migration-proof.json']).hexdigest(),
                    reportSha256=hashlib.sha256(files['report.json']).hexdigest(), storage=result['storage'], debianFreeBytesAfter=shutil.disk_usage(ROOT).free)
    if args.activation_state:
        verified['stateMutationsRejected'] = state_rejected
    if args.activation_writer:
        verified['writerMutationsRejected'] = writer_rejected
        verified['workerMigrationProofSha256'] = hashlib.sha256(files['worker-journal-activation-migration-proof.json']).hexdigest()
    (local/'activation-preparation-verified.json').write_text(json.dumps(verified, indent=2))
    print(json.dumps(verified), flush=True)


if __name__ == '__main__':
    main()
