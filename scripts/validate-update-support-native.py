#!/usr/bin/env python3
"""Run bounded readonly updater probes via the approved Windows worker; never install."""
import argparse
import base64
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PureWindowsPath
import re
import shutil
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


native = load('native', 'validate-workflow-native.py')
health = load('health', 'validate-update-health-actor-native.py')


def need(ok, message):
    if not ok:
        raise ValueError(message)


def ready_files(directory):
    raw = (directory/'report.ready.json').read_bytes()
    ready = json.loads(raw.decode('utf-8-sig'))
    need(ready.get('schemaVersion') == 1 and type(ready['schemaVersion']) is int, 'Ready schema differs')
    files = {}
    for entry in ready['files']:
        name = entry['path']
        need(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name) and name not in files
             and not (directory/name).is_symlink(), 'Unsafe report entry')
        data = (directory/name).read_bytes()
        need(type(entry['bytes']) is int and len(data) == entry['bytes']
             and hashlib.sha256(data).hexdigest() == entry['sha256'], 'Report bytes differ: '+name)
        files[name] = data
    need('report.json' in files and (directory/'report.ready.json').read_bytes() == raw,
         'Ready/report changed during verification')
    return files


def reference_from_pin(pin, reports, migration=False):
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', pin['run']), 'Invalid reference run')
    files = ready_files(reports/pin['run'])
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    raw = files['candidate-health-actor-proof.json']
    need(hashlib.sha256(files['report.json']).hexdigest() == pin['reportSha256']
         and hashlib.sha256(raw).hexdigest() == pin['proofSha256']
         and report['kind'] == 'runtime_replacement_main_native_gate' and report['passed'] is True
         and report['sourceCommit'] == pin['source']['sourceCommit']
         and report['sourceArchiveSha256'] == pin['source']['archiveSha256']
         and report['target'] == pin['target'], 'Reference differs from passed pinned gate')
    proof = json.loads(raw.decode('utf-8-sig'))
    health.verify_runtime_replacement_proof(proof, pin['source'], pin['target'])
    case = proof['cases'][0]['replacement']
    image = case['actualInstalledFiles'][3]
    artifact = case['actualManifest']['artifacts'][3]
    need(artifact['name'] == 'winbridge-update.exe' and artifact['sha256'] == image['sha256']
         and artifact['bytes'] == image['bytes'] and artifact['descriptor']['role'] == 'updater',
         'Reference updater role/image differs')
    reference = dict(schemaVersion=1, kind='retained_updater_support_reference', referenceRun=pin['run'],
                reportSha256=pin['reportSha256'], proofSha256=pin['proofSha256'],
                source=pin['target'], image=image, descriptor=artifact['descriptor'])
    if migration:
        replacement = case['record']
        reference.update(kind='candidate_support_schema15_reference',
                         manifest=case['actualInstalledFiles'][4],
                         updateId=replacement['intent']['preparation']['plan']['updateId'],
                         replacementSha256=hashlib.sha256(json.dumps(replacement, sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode()).hexdigest())
    return reference


def verify_migration(proof, source, reference):
    need(type(proof.get('schemaVersion')) is int and proof['schemaVersion'] == 1
         and proof.get('kind') == 'candidate_support_schema_migration_proof'
         and proof.get('passed') is True and proof.get('platform') == 'windows'
         and proof['build']['sourceCommit'] == source['sourceCommit']
         and proof['build']['sourceManifestSha256'] == source['manifestSha256']
         and proof['build']['sourceDirty'] is False
         and proof['referenceProofSha256'] == reference['proofSha256']
         and proof['referenceImage'] == reference['image'] and proof['referenceManifest'] == reference['manifest'],
         'Migration proof/source/reference differs')
    before, after, backup = (proof[k] for k in ('before', 'after', 'backupInfo'))
    need(type(before['schemaVersion']) is int and before['schemaVersion'] == 15
         and type(after['schemaVersion']) is int and after['schemaVersion'] == 16
         and backup == before
         and type(before['ownerEpoch']) is int and type(after['ownerEpoch']) is int
         and 0 <= before['ownerEpoch'] < after['ownerEpoch'] <= 9007199254740991
         and after == dict(before, schemaVersion=16, ownerEpoch=before['ownerEpoch']+1),
         'Migration invented metadata or did not fence the previous owner')
    replacement = proof['retainedReplacement']
    digest = hashlib.sha256(json.dumps(replacement, sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
    need(digest == reference['replacementSha256'] and replacement['revision'] == 22
         and replacement['state']['phase'] == 'files_moved'
         and replacement['intent']['preparation']['plan']['updateId'] == reference['updateId']
         and before['activeUpdateId'] == reference['updateId'], 'Actual retained replacement15 changed')
    need(re.fullmatch(r'[0-9a-f]{64}', proof['rowsBefore'])
         and proof['rowsBefore'] == proof['rowsAfter'] == proof['backupRows']
         and len(proof['supportRows']) == 3 and all(type(n) is int and n == 0 for n in proof['supportRows'])
         and proof['replacementView']['physicalCustodyHeld'] is False
         and proof['replacementView']['phase'] == 'files_moved'
         and proof['replacementView']['attemptId'] == replacement['intent']['attemptId']
         and proof['replacementView']['revision'] == 22
         and all(proof.get(k) is True for k in ('oldFifteenInitRefused', 'oldFifteenStatusRefused',
             'sourceBytesUnchanged', 'copyBytesUnchangedByOld', 'supportAbsent', 'newSupportAttemptDenied', 'migrationBackupVerified'))
         and all(proof.get(k) is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened')),
         'Migration lost rows/backup, adopted custody or let old binaries modify16')


def audit_migration(proof, source, reference):
    verify_migration(proof, source, reference)
    edits=[('schemaVersion', True), ('passed', False), ('runtimeApplied', True), ('installedRuntimeApplied', True),
           ('windowsOpened', True), ('oldFifteenInitRefused', False), ('oldFifteenStatusRefused', False),
           ('sourceBytesUnchanged', False), ('copyBytesUnchangedByOld', False), ('supportAbsent', False),
           ('newSupportAttemptDenied', False), ('migrationBackupVerified', False), ('rowsAfter', '0'*64),
           ('backupRows', '0'*64), ('supportRows', [1,0,0]), ('retainedReplacement/revision', 21),
           ('replacementView/physicalCustodyHeld', True), ('replacementView/revision', 21),
           ('after/ownerEpoch', proof['before']['ownerEpoch']), ('after/schemaVersion', 15),
           ('referenceImage/sha256', '0'*64), ('referenceManifest/fileIdentity', 'different')]
    for path,value in edits:
        bad=copy.deepcopy(proof);at=bad;parts=path.split('/')
        for part in parts[:-1]:at=at[part]
        at[parts[-1]]=value
        try:verify_migration(bad,source,reference)
        except (ValueError,KeyError,TypeError):continue
        raise ValueError('Accepted mutated migration proof: '+path)
    return len(edits)


def windows_path(path):
    return PureWindowsPath(path.removeprefix('\\\\?\\'))


def save_meta(path, value):
    """Atomic local observer state; save the pending ID before any network submit."""
    temporary = path.with_name(path.name+'.'+uuid.uuid4().hex+'.new')
    with temporary.open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)
    if os.name == 'posix':
        descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def recover_task(endpoint, request, max_pages=20):
    """Lost ACK recovery is readonly. Missing/incomplete history never resends."""
    cursor = None
    seen = set()
    deadline = time.monotonic()+30
    for _ in range(max_pages):
        need(time.monotonic() < deadline, 'Readonly ACK recovery time limit; no request resent')
        page = native.call(endpoint, {'method': 'history', 'query': dict(
            projectId=request['projectId'], kind='task', status=None, search='', before=cursor)})['page']
        need(len(page['records']) <= 50, 'Oversized recovery page; no request resent')
        for entry in page['records']:
            need(time.monotonic() < deadline and len(seen) < 200,
                 'Readonly ACK recovery bound reached; no request resent')
            task_id = entry['operationId']
            need(task_id not in seen, 'History repeated a task while recovering ACK')
            seen.add(task_id)
            task = native.call(endpoint, {'method': 'get_task', 'taskId': task_id})['task']
            submission = task['submission']
            if submission.get('idempotencyKey') == request['idempotencyKey']:
                need(submission == request, 'Saved key has a different payload; never resubmit')
                return task
        if page['hasMore'] is False:
            break
        next_cursor = page['nextCursor']
        need(next_cursor is not None and next_cursor != cursor, 'Incomplete recovery history cursor')
        cursor = next_cursor
    raise ValueError('Accepted task not found in bounded readonly history; outcome unknown, no request resent')


def stamp(process):
    need(set(process) == {'processId', 'birth', 'hostId', 'sessionId'}
         and type(process['processId']) is int and 0 < process['processId'] <= 0xffffffff
         and isinstance(process['birth'], str) and re.fullmatch(r'[0-9]{1,32}', process['birth'])
         and re.fullmatch(r'[0-9a-f]{64}', process['hostId'])
         and type(process['sessionId']) is int and 0 <= process['sessionId'] <= 0xffffffff,
         'Invalid actual process stamp')
    return process['hostId'], process['sessionId'], process['processId'], process['birth']


def verify_proof(proof, source, reference, images, runtime_root, updater_schema):
    need(type(proof.get('schemaVersion')) is int and proof['schemaVersion'] == 1
         and proof.get('kind') == 'updater_runtime_support_native_proof'
         and proof.get('platform') == 'windows' and proof.get('passed') is True
         and all(proof.get(k) is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened'))
         and proof['build']['sourceCommit'] == source['sourceCommit']
         and proof['build']['sourceManifestSha256'] == source['manifestSha256']
         and proof['build']['sourceDirty'] is False, 'Support proof source/scope differs')
    need(proof['oldImage'] == proof['oldImageAfter'] == reference['image']
         and proof['oldDescriptor'] == reference['descriptor'] and proof['oldSupport'] is None,
         'Retained old image was changed, executed through fallback or rebound')
    seen = set()
    identities, hashes = set(), set()
    for flavor in ('standard', 'validation'):
        case = proof[flavor]
        artifact, inspection = case['artifact'], case['inspection']
        image, descriptor = artifact['image'], artifact['descriptor']
        declared = images[flavor]
        expected = windows_path(runtime_root)/'support-images'/flavor/'winbridge-update.exe'
        need(windows_path(image['path']) == windows_path(declared['path']) == expected
             and image['sha256'] == declared['sha256'] and type(image['bytes']) is int
             and image['bytes'] == declared['bytes'] and 0 < image['bytes'] <= 256*1024*1024
             and image['querySupported'] is True and image['fileIdentity']
             and image['fileIdentity'] not in identities and image['sha256'] not in hashes,
             'Support image is not the distinct pinned flavor')
        identities.add(image['fileIdentity']); hashes.add(image['sha256'])
        need(all(type(descriptor[k]) is int and descriptor[k] == 1 for k in ('schemaVersion', 'queryContract'))
             and descriptor['kind'] == 'winbridge_artifact' and descriptor['role'] == 'updater'
             and descriptor['targetOs'] == 'windows' and descriptor['targetArch'] == 'x86_64'
             and descriptor['build']['sourceCommit'] == source['sourceCommit']
             and descriptor['build']['sourceManifestSha256'] == source['manifestSha256']
             and descriptor['build']['sourceDirty'] is False,
             'Support descriptor differs from source/role')
        need(type(artifact['schemaVersion']) is int and artifact['schemaVersion'] == 1 and artifact['kind'] == 'artifact_query'
             and artifact['readOnly'] is True and artifact['candidateVerified'] is False
             and artifact['cleanupVerified'] is True, 'Artifact cleanup/scope differs')
        need(type(inspection['schemaVersion']) is int and inspection['schemaVersion'] == 1 and inspection['kind'] == 'updater_support_inspection'
             and inspection['image'] == image and inspection['readOnly'] is True
             and inspection['cleanupVerified'] is True and inspection['journalBound'] is False
             and inspection['candidateVerified'] is False, 'Inspection invented approval/cleanup')
        support = inspection['support']
        expected_fields = {'schemaVersion', 'kind', 'queryContract', 'descriptor', 'process',
            'updaterJournalSchema', 'buildFlavor', 'finalStateBackupContract', 'runtimePreparationContract',
            'runtimeReplacementContract', 'activationContract', 'controllerConfirmationContract', 'desktopUpdateContract'}
        versions = {'schemaVersion': 1, 'queryContract': 1, 'updaterJournalSchema': updater_schema,
                    'finalStateBackupContract': 15, 'runtimePreparationContract': 16, 'runtimeReplacementContract': 17}
        need(set(support) == expected_fields and all(type(support[k]) is int and support[k] == v for k, v in versions.items())
             and support['kind'] == 'winbridge_updater_runtime_support'
             and support['descriptor'] == descriptor and support['buildFlavor'] == flavor
             and all(support[k] is None for k in ('activationContract', 'controllerConfirmationContract', 'desktopUpdateContract')),
             'Support declared absent/incorrect execution contracts')
        for process in (artifact['process'], support['process']):
            identity = stamp(process)
            need(identity not in seen, 'Support process was reused/rebound')
            if seen:
                need(identity[:2] == next(iter(seen))[:2], 'Support probes changed host/session')
            seen.add(identity)
        config = case['configBefore']
        invalid = b'invalid configuration: do not load'
        need(config == case['configAfter'] and windows_path(config['path']) == expected.with_name('winbridge-update.local.json')
             and config['bytes'] == len(invalid) and config['sha256'] == hashlib.sha256(invalid).hexdigest()
             and config['fileIdentity'] and config['querySupported'] is False
             and case['directoryEntries'] == ['winbridge-update.exe', 'winbridge-update.local.json'],
             'Query opened/modified config or created journal/files')


def audit(proof, source, reference, images, root, schema):
    verify_proof(proof, source, reference, images, root, schema)
    edits = [('passed', False), ('schemaVersion', True), ('runtimeApplied', True),
             ('installedRuntimeApplied', True), ('windowsOpened', True), ('oldSupport', {}),
             ('oldImageAfter/sha256', '0'*64), ('build/sourceDirty', True)]
    for flavor in ('standard', 'validation'):
        edits += [(flavor+'/artifact/image/fileIdentity', 'different'),
                  (flavor+'/artifact/schemaVersion', True),
                  (flavor+'/artifact/descriptor/queryContract', True),
                  (flavor+'/inspection/schemaVersion', True),
                  (flavor+'/artifact/cleanupVerified', False), (flavor+'/inspection/journalBound', True),
                  (flavor+'/inspection/candidateVerified', True), (flavor+'/inspection/cleanupVerified', False),
                  (flavor+'/inspection/support/buildFlavor', 'wrong'),
                  (flavor+'/inspection/support/queryContract', True),
                  (flavor+'/inspection/support/activationContract', 18),
                  (flavor+'/inspection/support/process/processId', 0),
                  (flavor+'/inspection/support/descriptor/build/buildId', 'different'),
                  (flavor+'/configAfter/sha256', '0'*64)]
    for path, value in edits:
        bad = copy.deepcopy(proof)
        at = bad
        parts = path.split('/')
        for part in parts[:-1]:
            at = at[part]
        at[parts[-1]] = value
        try:
            verify_proof(bad, source, reference, images, root, schema)
        except (ValueError, KeyError, TypeError):
            continue
        raise ValueError('Accepted mutated support proof: '+path)
    return len(edits)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--reference-pin', type=Path, required=True)
    parser.add_argument('--cache-run', required=True)
    parser.add_argument('--controller-config', type=Path, default=Path('.winbridge/controller-snapshot.local.json'))
    parser.add_argument('--verify-run', help='Observe/recheck the same task; never resubmit')
    parser.add_argument('--candidate-support-migration', action='store_true', help='Also run support metadata/policy and actual15-to16 migration/refusal on a fresh consistent clone')
    args = parser.parse_args()
    source = health.archive_pin(args.archive)
    with zipfile.ZipFile(args.archive) as z:
        schema = native.updater_schema_from_source(z.read('winbridge/apps/update/src/journal.rs'))
        need('winbridge/scripts/test-update-support.ps1' in z.namelist(), 'Support runner missing in source')
        if args.candidate_support_migration:
            need(schema == 16 and 'winbridge/apps/update/tests/candidate_support_migration.rs' in z.namelist(),
                 'Explicit schema16 migration source required')
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.cache_run), 'Invalid cache run')
    cfg = json.loads(args.controller_config.read_text())
    shared = Path(cfg['delivery']['root'])
    namespace = cfg['delivery']['namespace']
    reports = shared/namespace/'workflow-validation'
    archive_relative = args.archive.resolve().relative_to((shared/namespace/'sources').resolve())
    endpoint = Path(cfg['stateDirectory'])/('controller-'+cfg['instanceId']+'.sock')
    pin = json.loads(args.reference_pin.read_text())
    reference = reference_from_pin(pin, reports, args.candidate_support_migration)
    task = native.call(endpoint, {'method': 'get_task', 'taskId': pin['taskId']})['task']
    need(task['status'] == 'succeeded' and task['exitCode'] == 0
         and task['submission']['projectId'] == 'snapshot-fixture'
         and task['submission']['idempotencyKey'] == 'native-health-'+pin['run'], 'Reference task is not terminal')
    info = native.call(endpoint, {'method': 'info'})['info']
    need(info['platform'] == 'windows' and not info.get('maintenanceError') and not info.get('journalRecoveryRequired')
         and info.get('recoveryCoverage', {}).get('complete') is True, 'Healthy Windows worker required')
    project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
    free = shutil.disk_usage(ROOT).free
    if not args.verify_run:
        need(info['activeTaskCount'] == 0 and free >= 6*1024**3, 'Idle worker and 6 GiB Debian free required')
    run = args.verify_run or time.strftime('%Y%m%d-%H%M%S', time.gmtime())+'-'+uuid.uuid4().hex[:8]
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', run), 'Invalid run')
    local = ROOT/'.winbridge/workflow-validation'/run
    report = reports/run
    windows_report = str(PureWindowsPath(cfg['delivery']['peerRoot'])/namespace/'workflow-validation'/run)
    root = str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/run)
    if args.verify_run:
        meta = json.loads((local/'support-run.json').read_text())
        need(meta['source'] == source and meta['reference'] == reference, 'Saved source/reference differs')
        if meta.get('taskId') is None:
            raw = (local/'request.json').read_bytes()
            need(hashlib.sha256(raw).hexdigest() == meta['requestSha256'], 'Saved request changed after lost ACK')
            request = json.loads(raw)
            need(request['projectId'] == 'snapshot-fixture' and request['idempotencyKey'] == 'native-support-'+run,
                 'Saved request differs from pending run')
            task = recover_task(endpoint, request)
            meta['taskId'] = task['taskId']
            save_meta(local/'support-run.json', meta)
    else:
        local.mkdir(parents=True); report.mkdir()
        raw = json.dumps(reference, indent=2).encode()
        (report/'reference.json').write_bytes(raw)
        reference_sha = hashlib.sha256(raw).hexdigest()
        archive = str(PureWindowsPath(cfg['delivery']['peerRoot'])/namespace/'sources'/archive_relative)
        cache = str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/args.cache_run)
        ps = native.ps_string
        mode = ' -CandidateSupportMigration' if args.candidate_support_migration else ''
        script = f'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$local={ps(root)}
if(Test-Path -LiteralPath $local){{throw 'Existing run preserved; no replay'}}
if([IO.DriveInfo]::new([IO.Path]::GetPathRoot($local)).AvailableFreeSpace -lt 6GB){{throw 'Less than 6 GiB Windows free; no cleanup'}}
New-Item -ItemType Directory -Path $local | Out-Null
$archive={ps(archive)}
if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(source['archiveSha256'])}){{throw 'Source ZIP changed'}}
$copy=Join-Path $local 'source.zip'
[IO.File]::Copy($archive,$copy,$false)
if((Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(source['archiveSha256'])}){{throw 'Source ZIP copy changed'}}
Expand-Archive -LiteralPath $copy -DestinationPath $local
$reference=Join-Path $local 'reference.json'
[IO.File]::Copy({ps(str(PureWindowsPath(windows_report)/'reference.json'))},$reference,$false)
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $local 'winbridge/scripts/test-update-support.ps1') -SourceDirectory (Join-Path $local 'winbridge') -SourceArchiveSha256 {ps(source['archiveSha256'])} -ValidationCacheDirectory {ps(cache)} -ReferenceFile $reference -ReferenceSha256 {ps(reference_sha)} -ReportDirectory {ps(windows_report)}{mode}
exit $LASTEXITCODE
'''
        request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId='snapshot-fixture',
                       idempotencyKey='native-support-'+run, timeoutMs=900000,
                       command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
                                    arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()]))
        raw = json.dumps(request).encode()
        with (local/'request.json').open('xb') as stream:
            stream.write(raw); stream.flush(); os.fsync(stream.fileno())
        meta = dict(taskId=None, run=run, source=source, reference=reference,
                    referenceSha256=reference_sha, debianFreeBytesBefore=free, cacheRun=args.cache_run,
                    requestSha256=hashlib.sha256(raw).hexdigest())
        save_meta(local/'support-run.json', meta)
        print(json.dumps(dict(taskId=None, run=run, reportDirectory=str(report),
                              recovery='--verify-run '+run)), flush=True)
        task = native.call(endpoint, {'method': 'submit', 'request': request})['task']
        meta['taskId'] = task['taskId']
        save_meta(local/'support-run.json', meta)
        print(json.dumps(dict(taskId=task['taskId'], run=run, reportDirectory=str(report))), flush=True)
    deadline = time.monotonic()+950
    while True:
        task = native.call(endpoint, {'method': 'get_task', 'taskId': meta['taskId']})['task']
        need(task['submission']['projectId'] == 'snapshot-fixture'
             and task['submission']['idempotencyKey'] == 'native-support-'+run,
             'Observation task differs from the saved native run')
        if task['status'] not in ('queued', 'running'):
            break
        if time.monotonic() >= deadline:
            raise TimeoutError('Observe same task via --verify-run '+run+'; do not resubmit')
        time.sleep(1)
    (local/'task-report.json').write_text(json.dumps({'task': task}))
    files = ready_files(report)
    need(ready_files(report) == files, 'Ready files changed between reads')
    result = json.loads(files['report.json'].decode('utf-8-sig'))
    need(task['status'] == 'succeeded' and task['exitCode'] == 0 and result['passed'] is True
         and type(result['schemaVersion']) is int and result['schemaVersion'] == 1 and result['platform'] == 'windows'
         and result['kind'] == ('candidate_support_migration_native_gate' if args.candidate_support_migration else 'updater_runtime_support_native_gate') and result['error'] is None
         and result['sourceCommit'] == source['sourceCommit'] and result['sourceArchiveSha256'] == source['archiveSha256']
         and result['referenceSha256'] == meta['referenceSha256']
         and hashlib.sha256(files['reference.json']).hexdigest() == meta['referenceSha256']
         and all(result[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened')),
         'Native task/report failed or changed scope; inspect '+str(report))
    expected_commands=['standard-build', 'standard-cli', 'validation-build', 'validation-cli', 'support-binding', 'support-actual']
    if args.candidate_support_migration:
        expected_commands+=['candidate-support-model-ledger','candidate-support-policy','candidate-support-actual-migration']
    need(all(c['exitCode'] == 0 for c in result['commands'])
         and [c['name'] for c in result['commands']] == expected_commands,
         'Missing native support command groups')
    proof = json.loads(files['updater-support-proof.json'].decode('utf-8-sig'))
    rejected = audit(proof, source, reference, result['images'], root, schema)
    counts = {}
    groups=[('standard-cli', 3), ('validation-cli', 3), ('support-binding', 1), ('support-actual', 1)]
    migration_rejected=None
    if args.candidate_support_migration:
        migration_proof=json.loads(files['candidate-support-migration-proof.json'].decode('utf-8-sig'))
        migration_rejected=audit_migration(migration_proof,source,reference)
        groups += [('candidate-support-model-ledger',10),('candidate-support-policy',6),('candidate-support-actual-migration',1)]
    for name, minimum in groups:
        log = files[name+'.log'].decode('utf-8-sig')
        counts[name] = sum(int(m[1]) for m in re.finditer(r'test result: ok\. ([0-9]+) passed; 0 failed;', log))
        need(counts[name] >= minimum, 'Native support tests were not selected: '+name)
    summary = dict(meta, passed=True, filesVerifiedTwice=len(files), negativeMutationsRejected=rejected,
                   nativeTestsPassed=sum(counts.values()), groupsPassed=len(counts), testCounts=counts,
                   reportSha256=hashlib.sha256(files['report.json']).hexdigest(),
                   proofSha256=hashlib.sha256(files['updater-support-proof.json']).hexdigest(),
                   storage=result['storage'], debianFreeBytesAfter=shutil.disk_usage(ROOT).free)
    if args.candidate_support_migration:
        summary['migrationProofSha256']=hashlib.sha256(files['candidate-support-migration-proof.json']).hexdigest()
        summary['migrationNegativeMutationsRejected']=migration_rejected
    (local/'support-verified.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()
