#!/usr/bin/env python3
"""Run a journal-bound support query through isolated Windows Main; never install."""
import argparse
import base64
import copy
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
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


support = load('support_observer', 'validate-update-support-native.py')
native, health = support.native, support.health
need = support.need


def strict_counters(value):
    if isinstance(value, list):
        for item in value:
            strict_counters(item)
    elif isinstance(value, dict):
        names = {'schemaVersion', 'contractVersion', 'queryContract', 'revision', 'ownerEpoch',
                 'parentRevision', 'expectedUpdaterJournalSchema', 'updaterJournalSchema',
                 'processId', 'sessionId', 'timeoutMs', 'bytes', 'requestShutdownCount'}
        for key, item in value.items():
            if key in names or key.endswith('AtMs'):
                need(item is None or type(item) is int and 0 <= item <= 9007199254740991,
                     'Invalid typed counter: '+key)
            strict_counters(item)


def verify(proof, source, target):
    strict_counters(proof)
    need(type(proof.get('schemaVersion')) is int and proof['schemaVersion'] == 1
         and proof.get('kind') == 'candidate_support_actor_main_native_proof'
         and proof.get('platform') == 'windows' and proof.get('passed') is True
         and proof['target'] == target, 'Support actor proof scope differs')
    build = proof['build']
    need(build['sourceCommit'] == source['sourceCommit']
         and build['sourceManifestSha256'] == source['manifestSha256']
         and build['sourceDirty'] is False, 'Producer differs')
    need(all(proof.get(k) is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened', 'productionDrainEnforced'))
         and proof['actualNormalIpc'] is True and proof['oldRuntimeUnchanged'] is True, 'Proof changed execution scope')
    before = proof['beforeCleanup']
    need(before['kind'] == 'candidate_support_main_before_cleanup' and before['passed'] is True
         and before['normalMain'] is True and before['normalServe'] is True
         and type(before['requestShutdownCount']) is int and before['requestShutdownCount'] == 0
         and before['lostAckSentOnce'] is True and before['legacyHealthFieldAbsent'] is True
         and all(before[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened', 'productionDrainEnforced')),
         'Main validation did not preserve old worker/no-drain scope')
    root = before['before']['update']
    after, duplicate = before['after'], before['duplicate']
    need(root['phase'] == 'candidate_verified' and root['request']['source'] == target
         and after['update'] == duplicate['update'] == root
         and after['schemaVersion'] == duplicate['schemaVersion'] == 10
         and after['driverActive'] is False and duplicate['driverActive'] is False
         and after['driverError'] is None and duplicate['driverError'] is None,
         'Support changed root or repeated a driver')
    record = proof['record']
    need(record == proof['coldRecord'] == before['record']
         and type(record['revision']) is int and record['revision'] == 5
         and record['schemaVersion'] == 1 and record['phase'] == 'finished', 'Support terminal record changed')
    intent, probe, result = (record[k] for k in ('intent', 'probe', 'result'))
    supervisor, worker = before['supervisor'], before['worker']
    need(intent['updateId'] == root['updateId']
         and intent['installationId'] == root['request']['expected']['installationId']
         and intent['scopeSha256'] == root['request']['expected']['scopeSha256']
         and intent['launchId'] == supervisor['spec']['launchId']
         and intent['ownerId'] == supervisor['ownerId'] and intent['ownerEpoch'] == supervisor['ownerEpoch']
         and intent['parentRevision'] == root['revision']
         and intent['expectedUpdaterJournalSchema'] == 16
         and intent['configSha256'] == supervisor['spec']['configSha256']
         and intent['supervisorImageSha256'] == supervisor['spec']['imageSha256']
         and proof['normalIpcGeneration'] == worker['generation']
         and proof['normalIpcBuild'] == worker['build'] == proof['build'],
         'Intent/normal IPC did not bind actual Main and owner')
    for name in ('sourceSha256', 'healthSha256', 'artifactsSha256'):
        need(re.fullmatch(r'[0-9a-f]{64}', intent[name]) is not None, 'Saved prerequisite digest missing')
    descriptor = intent['descriptor']
    executor = intent['executor']
    need(descriptor['role'] == executor['role'] == 'updater'
         and descriptor['targetOs'] == executor['targetOs'] == 'windows'
         and descriptor['targetArch'] == executor['targetArch'] == 'x86_64'
         and descriptor['build']['sourceDirty'] is executor['build']['sourceDirty'] is False
         and descriptor['build']['sourceCommit'] == target['sourceCommit']
         and descriptor['build']['sourceManifestSha256'] == target['manifestSha256']
         and executor['build']['sourceCommit'] == source['sourceCommit']
         and executor['build']['sourceManifestSha256'] == source['manifestSha256'],
         'Candidate/executor identity differs')
    command = probe['command']
    need(support.windows_path(command['executable']) == support.windows_path(intent['image']['path'])
         and command['arguments'] == ['--runtime-support'] and command['timeoutMs'] == 10000
         and support.windows_path(command['workingDirectory']) == support.windows_path(command['executable']).parent
         and probe['resumeGranted'] is True and probe['resumeAcknowledged'] is True
         and re.fullmatch(r'Global\\WinBridge-UpdaterSupport-[0-9a-f]{64}', probe['jobName']) is not None,
         'Probe command/grants differ')
    process = probe['process']
    support.stamp(process)
    cleanup = probe['cleanup']
    need(cleanup['process'] == process and cleanup['jobName'] == probe['jobName']
         and all(cleanup[k] is True for k in ('creationConfirmed', 'resumeRequested', 'treeEmpty', 'captureComplete'))
         and cleanup['noProcessCreated'] is False and cleanup['outputExceeded'] is False
         and type(cleanup['exitCode']) is int and cleanup['exitCode'] == 0 and cleanup['failure'] is None,
         'PID/Job/cleanup differs')
    report = result['report']
    need(result['outcome'] == 'checked' and report['process'] == process and report['descriptor'] == descriptor
         and report['queryContract'] == report['schemaVersion'] == 1
         and report['kind'] == 'winbridge_updater_runtime_support'
         and report['updaterJournalSchema'] == 16 and report['buildFlavor'] == 'standard'
         and type(result['completedAtMs']) is int and result['completedAtMs'] >= intent['admittedAtMs'],
         'Report is not the admitted process/candidate')
    need([report[k] for k in ('finalStateBackupContract', 'runtimePreparationContract', 'runtimeReplacementContract',
                              'activationContract', 'controllerConfirmationContract', 'desktopUpdateContract')]
         == [15, 16, 17, None, None, None], 'Query advertises unavailable execution contracts')
    expected_blockers = [dict(kind=kind, contract=contract)
                         for contract in ('activation', 'controller_confirmation', 'desktop_update')
                         for kind in ('missing_contract', 'driver_not_implemented')]
    need(intent['drivers'] == dict(schemaVersion=1, buildFlavor='standard', finalStateBackup=15,
                                  runtimePreparation=16, runtimeReplacement=17, activation=None,
                                  controllerConfirmation=None, desktopUpdate=None)
         and result['assessment'] == dict(schemaVersion=1, policy='same_schema_install_v1',
              expectedUpdaterJournalSchema=16, blockers=expected_blockers), 'Frozen policy blockers differ')
    view = after['candidateSupport']
    need(view == duplicate['candidateSupport'] == proof['coldView']['candidateSupport']
         and view['phase'] == 'checked' and view['revision'] == 5
         and view['attemptId'] == intent['attemptId'] and view['process'] == process
         and view['command'] == command and view['report'] == report
         and view['assessment'] == result['assessment'] and view['cleanupVerified'] is True,
         'Actor/cold/duplicate projections differ')
    history = proof['history']
    need(len(history) == 6 and history[-1] == record, 'Causal history missing')
    for index, state in enumerate(history):
        need(state['intent'] == intent and type(state['revision']) is int and state['revision'] == index
             and state['phase'] == ('admitted', 'creating', 'resume_granted', 'running', 'captured', 'finished')[index]
             and (state['result'] == result if index == 5 else state['result'] is None),
             'Causal history rewrote intent/revision/result')
        p = state['probe']
        if index == 0:
            need(p is None, 'Process existed before creation intent')
        else:
            need(p['command'] == command and p['jobName'] == probe['jobName']
                 and p['process'] == (None if index == 1 else process)
                 and p['resumeGranted'] is (index >= 2) and p['resumeAcknowledged'] is (index >= 3)
                 and p['cleanup'] == (cleanup if index >= 4 else None), 'Causal process/grant/cleanup changed')
    lease = before['sourceLease']
    need(lease['phase'] == 'source_held' and lease['revision'] == 1 and lease['spec']['updateId'] == root['updateId']
         and proof['coldView']['update']['phase'] == 'failed_before_cutover'
         and proof['cleanup']['abort']['phase'] == 'completed' and proof['cleanup']['supportUnchanged'] is True
         and proof['cleanup']['requestShutdownCount'] == 0 and proof['cleanup']['fixtureCleanupStop'] is True,
         'Fixture cleanup changed support or requested worker shutdown')


def audit(proof, source, target):
    verify(proof, source, target)
    edits = [
        ('passed', False), ('schemaVersion', True), ('runtimeApplied', True),
        ('installedRuntimeApplied', True), ('windowsOpened', True), ('productionDrainEnforced', True),
        ('actualNormalIpc', False), ('oldRuntimeUnchanged', False),
        ('beforeCleanup/lostAckSentOnce', False), ('beforeCleanup/requestShutdownCount', 1),
        ('beforeCleanup/after/driverActive', True), ('beforeCleanup/after/update/phase', 'draining'),
        ('record/schemaVersion', True), ('record/revision', 4), ('record/intent/ownerEpoch', 0), ('record/probe/process/birth', '0'),
        ('record/probe/cleanup/treeEmpty', False), ('record/result/assessment/blockers', []),
        ('coldRecord/result/outcome', 'unsupported'), ('coldView/candidateSupport/cleanupVerified', False),
        ('cleanup/supportUnchanged', False), ('normalIpcGeneration', 'other'), ('history', proof['history'][:5]),
    ]
    for path, value in edits:
        changed = copy.deepcopy(proof)
        obj = changed
        parts = path.split('/')
        for part in parts[:-1]:
            obj = obj[part]
        obj[parts[-1]] = value
        try:
            verify(changed, source, target)
        except (ValueError, KeyError, TypeError, IndexError):
            continue
        raise ValueError('Accepted mutated actor proof: '+path)
    return len(edits)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--target-archive', type=Path, required=True)
    parser.add_argument('--cache-run', required=True)
    parser.add_argument('--controller-config', type=Path, default=Path('.winbridge/controller-snapshot.local.json'))
    parser.add_argument('--verify-run', help='Observe/recheck the same task; never resubmit')
    args = parser.parse_args()
    source, target = (health.archive_pin(p) for p in (args.archive, args.target_archive))
    need(source['sourceCommit'] != target['sourceCommit'], 'Distinct source/candidate required')
    for path in (args.archive, args.target_archive):
        with zipfile.ZipFile(path) as z:
            need(native.updater_schema_from_source(z.read('winbridge/apps/update/src/journal.rs')) == 16,
                 'This actor gate requires schema16 on both sides')
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.cache_run), 'Invalid cache run')
    cfg = json.loads(args.controller_config.read_text())
    shared = Path(cfg['delivery']['root'])/cfg['delivery']['namespace']
    endpoint = Path(cfg['stateDirectory'])/('controller-'+cfg['instanceId']+'.sock')
    info = native.call(endpoint, {'method': 'info'})['info']
    need(info['platform'] == 'windows' and not info.get('maintenanceError') and not info.get('journalRecoveryRequired')
         and info.get('recoveryCoverage', {}).get('complete') is True, 'Healthy Windows worker required')
    project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
    if not args.verify_run:
        need(info['activeTaskCount'] == 0 and shutil.disk_usage(ROOT).free >= 6*1024**3, 'Idle worker and 6 GiB Debian free required')
    run = args.verify_run or time.strftime('%Y%m%d-%H%M%S', time.gmtime())+'-'+uuid.uuid4().hex[:8]
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', run), 'Invalid run')
    local, report = ROOT/'.winbridge/workflow-validation'/run, shared/'workflow-validation'/run
    root = str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/run)
    peer = PureWindowsPath(cfg['delivery']['peerRoot'])/cfg['delivery']['namespace']
    if args.verify_run:
        meta = json.loads((local/'support-actor-run.json').read_text())
        need(meta['source'] == source and meta['target'] == target, 'Saved source/target differs')
        if meta.get('taskId') is None:
            raw = (local/'request.json').read_bytes()
            need(hashlib.sha256(raw).hexdigest() == meta['requestSha256'], 'Saved request changed')
            request = json.loads(raw)
            need(request['idempotencyKey'] == 'native-support-actor-'+run, 'Pending run differs')
            meta['taskId'] = support.recover_task(endpoint, request)['taskId']
            support.save_meta(local/'support-actor-run.json', meta)
    else:
        local.mkdir(parents=True)
        report.mkdir()
        archive = peer/'sources'/args.archive.resolve().relative_to((shared/'sources').resolve())
        target_archive = peer/'sources'/args.target_archive.resolve().relative_to((shared/'sources').resolve())
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
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $root 'winbridge/scripts/test-update-support-actor.ps1') -SourceDirectory (Join-Path $root 'winbridge') -SourceArchiveSha256 {ps(source['archiveSha256'])} -TargetArchivePath {ps(str(target_archive))} -TargetArchiveSha256 {ps(target['archiveSha256'])} -ValidationCacheDirectory {ps(str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/args.cache_run))} -ReportDirectory {ps(str(peer/'workflow-validation'/run))}
exit $LASTEXITCODE
"""
        request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId='snapshot-fixture',
                       idempotencyKey='native-support-actor-'+run, timeoutMs=1800000,
                       command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
                                    arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand',
                                               base64.b64encode(script.encode('utf-16le')).decode()]))
        raw = json.dumps(request).encode()
        with (local/'request.json').open('xb') as stream:
            stream.write(raw)
            stream.flush()
            support.os.fsync(stream.fileno())
        meta = dict(taskId=None, run=run, source=source, target=target, cacheRun=args.cache_run,
                    requestSha256=hashlib.sha256(raw).hexdigest(), debianFreeBytesBefore=shutil.disk_usage(ROOT).free)
        support.save_meta(local/'support-actor-run.json', meta)
        print(json.dumps(dict(taskId=None, run=run, recovery='--verify-run '+run)), flush=True)
        meta['taskId'] = native.call(endpoint, {'method': 'submit', 'request': request})['task']['taskId']
        support.save_meta(local/'support-actor-run.json', meta)
        print(json.dumps(dict(taskId=meta['taskId'], run=run)), flush=True)
    deadline = time.monotonic()+1850
    while True:
        task = native.call(endpoint, {'method': 'get_task', 'taskId': meta['taskId']})['task']
        need(task['submission']['projectId'] == 'snapshot-fixture'
             and task['submission']['idempotencyKey'] == 'native-support-actor-'+run, 'Task scope differs')
        if task['status'] not in ('queued', 'running'):
            break
        need(time.monotonic() < deadline, 'Observation expired; recheck the same run, never resubmit')
        time.sleep(1)
    (local/'task-report.json').write_text(json.dumps({'task': task}))
    files = support.ready_files(report)
    need(support.ready_files(report) == files, 'Ready files changed')
    result = json.loads(files['report.json'].decode('utf-8-sig'))
    need(task['status'] == 'succeeded' and task['exitCode'] == 0 and result['passed'] is True
         and result['kind'] == 'candidate_support_actor_native_gate' and result['error'] is None
         and result['sourceCommit'] == source['sourceCommit'] and result['sourceArchiveSha256'] == source['archiveSha256']
         and result['target'] == target
         and all(result[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened', 'productionDrainEnforced')),
         'Native actor task/report failed; inspect '+str(report))
    groups = [('support-model-ledger', 12), ('support-contract', 2), ('support-policy', 6),
              ('support-frozen-views', 1), ('support-worker-mailbox', 1), ('support-main-pipeline', 1)]
    need([c['name'] for c in result['commands']] == ['support-supervisor-build']+[n for n, _ in groups[:-1]]
         + ['support-old-real-set', 'support-main-pipeline'] and all(c['exitCode'] == 0 for c in result['commands']),
         'Native command selection differs')
    counts = {}
    for name, minimum in groups:
        counts[name] = sum(int(m[1]) for m in re.finditer(r'test result: ok\. ([0-9]+) passed; 0 failed;',
                                                       files[name+'.log'].decode('utf-8-sig')))
        need(counts[name] >= minimum, 'Tests were not selected: '+name)
    proof = json.loads(files['candidate-support-actor-proof.json'].decode('utf-8-sig'))
    rejected = audit(proof, source, target)
    verified = dict(meta, passed=True, filesVerifiedTwice=len(files), nativeTestsPassed=sum(counts.values()),
                    testCounts=counts, negativeMutationsRejected=rejected, groupsPassed=len(groups),
                    proofSha256=hashlib.sha256(files['candidate-support-actor-proof.json']).hexdigest(),
                    reportSha256=hashlib.sha256(files['report.json']).hexdigest(), storage=result['storage'],
                    debianFreeBytesAfter=shutil.disk_usage(ROOT).free)
    (local/'support-actor-verified.json').write_text(json.dumps(verified, indent=2))
    print(json.dumps(verified), flush=True)


if __name__ == '__main__':
    main()
