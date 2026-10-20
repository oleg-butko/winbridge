#!/usr/bin/env python3
"""Run the owned Windows health actor fixture through the approved installed worker."""
import argparse
import base64
import hashlib
import importlib.util
import io
import json
from pathlib import Path, PureWindowsPath
import re
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('native', ROOT / 'scripts/validate-workflow-native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def archive_pin(path):
    if path.is_symlink() or path.stat().st_size > 512*1024*1024:
        raise ValueError('Bounded regular source ZIP required')
    data = path.read_bytes()
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        names = [e.filename for e in entries]
        if len(set(n.casefold() for n in names)) != len(names):
            raise ValueError('Duplicate/case-colliding source ZIP entries')
        for entry in entries:
            name = entry.filename
            parts = name.split('/')
            mode = (entry.external_attr >> 16) & 0o170000
            if (entry.is_dir() or mode not in (0, 0o100000) or entry.file_size > 64*1024*1024
                    or parts[0] != 'winbridge' or len(parts) < 2
                    or any(p in ('', '.', '..') or p.endswith((' ', '.'))
                           or re.search(r'[<>:"\\|?*\x00-\x1f]', p) for p in parts)):
                raise ValueError('Unsafe source ZIP entry')
        raw = archive.read('winbridge/winbridge-source-manifest.json')
        manifest = json.loads(raw)
        if (manifest['dirty'] or not manifest['includesBuiltUi'] or manifest['schemaVersion'] != 1
                or not re.fullmatch(r'[0-9a-f]{40}', manifest['sourceCommit'])):
            raise ValueError('Clean exported source with built UI required')
        expected = {'winbridge/winbridge-source-manifest.json'}
        for item in manifest['files']:
            name = 'winbridge/'+item['path']
            if name in expected or not re.fullmatch(r'[0-9a-f]{64}', item['sha256']):
                raise ValueError('Invalid manifest source entry')
            expected.add(name)
            content = archive.read(name)
            if len(content) != item['bytes'] or hashlib.sha256(content).hexdigest() != item['sha256']:
                raise ValueError('Source file differs from manifest')
        if expected != set(names):
            raise ValueError('Source ZIP contains missing or unlisted entries')
    return {'archiveName': path.name, 'archiveSha256': hashlib.sha256(data).hexdigest(),
            'manifestSha256': hashlib.sha256(raw).hexdigest(), 'sourceCommit': manifest['sourceCommit']}


def verify_proof(proof, source, target):
    if (proof.get('kind') != 'candidate_health_actor_proof' or proof.get('passed') is not True
            or proof.get('platform') != 'windows' or proof.get('target') != target
            or proof.get('build', {}).get('sourceCommit') != source['sourceCommit']
            or proof['build'].get('sourceDirty') is not False
            or proof['build'].get('sourceManifestSha256') != source['manifestSha256']):
        raise ValueError('Actual helper/target generation proof mismatch')
    for key in ['actualCandidateBuild', 'actualCandidateHelpers', 'workerEntryPointIsTestHarness',
                'syntheticOldOtherRoleDescriptors', 'intentBeforeAck', 'duplicateObserved',
                'pidBeforeResume', 'abortWaitedForDriver', 'newEmptyJournalOnly']:
        if proof.get(key) is not True:
            raise ValueError('Missing native evidence: '+key)
    if proof.get('runtimeApplied') is not False or proof.get('windowsOpened') is not False:
        raise ValueError('Isolated gate unexpectedly changed runtime/opened GUI')
    cases = proof.get('cases', [])
    if [c.get('case') for c in cases] != ['pid-receipt-rejected', 'cancel-before-helper', 'cancel-running-rehearsal', 'verified']:
        raise ValueError('Missing ordered health cases')
    for case in cases:
        health, abort = case['health'], case['abort']
        expected = {'pid-receipt-rejected': 'rejected', 'cancel-before-helper': 'cancelled',
                    'cancel-running-rehearsal': 'cancelled', 'verified': 'verified'}[case['case']]
        if health['result']['outcome'] != expected or abort['phase'] != 'completed' or not abort['cleanupVerified'] or abort['workerReleased'] is None:
            raise ValueError('Health outcome/abort release proof incomplete')
        for p in health['probes']:
            c = p['cleanup']
            if not c or not c['treeEmpty'] or not c['captureComplete'] or c['process'] != p['process']:
                raise ValueError('Helper cleanup/PID proof incomplete')
        if case['case'] == 'pid-receipt-rejected':
            p, = health['probes']
            if p['resumeGranted'] or p['resumeAcknowledged'] or p['cleanup']['resumeRequested'] or not p['process']:
                raise ValueError('Denied PID receipt resumed a process')
        elif case['case'] == 'cancel-before-helper':
            if health['probes']:
                raise ValueError('Blocked callback cancellation unexpectedly launched helper')
        else:
            if len(health['probes']) != 2:
                raise ValueError('Both actual helpers required')
            if case['case'] == 'verified':
                if health['result']['parentRevision'] != health['intent']['parentRevision']+1 or abort['spec']['originalPhase'] != 'candidate_verified':
                    raise ValueError('Atomic acceptance/root lineage differs')
                result, context = health['result'], health['context']
                if result['config']['descriptor']['build']['sourceManifestSha256'] != target['manifestSha256'] or result['rehearsal']['descriptor'] != result['config']['descriptor']:
                    raise ValueError('Reports do not belong to the actual target')
                for report, probe in zip([result['config'], result['rehearsal']], health['probes']):
                    if report['process'] != probe['process'] or not probe['resumeAcknowledged'] or not probe['resumeGranted'] or probe['cleanup']['failure'] is not None or probe['cleanup']['exitCode'] != 0:
                        raise ValueError('Typed report process/cleanup differs')
                if (not result['rehearsal']['freshJournal'] or not result['rehearsal']['ipcReady'] or not result['rehearsal']['serverStopped']
                        or result['rehearsal']['productionStateOpened'] or result['rehearsal']['controllerEnabled']
                        or result['config']['workerJournalOpened'] or result['config']['schedulerStarted']
                        or result['rehearsal']['workerJournalSchema'] != context['compatibility']['workerJournalSchema']
                        or result['rehearsal']['updaterJournalSchema'] != context['compatibility']['updaterJournalSchema']):
                    raise ValueError('Fresh-state schema/isolation proof differs')



def verify_drain_proof(proof, source, target):
    if (proof.get('kind') != 'candidate_drain_actor_proof' or proof.get('passed') is not True
            or proof.get('platform') != 'windows' or proof.get('target') != target
            or proof.get('build', {}).get('sourceCommit') != source['sourceCommit']
            or proof['build'].get('sourceDirty') is not False
            or proof['build'].get('sourceManifestSha256') != source['manifestSha256']
            or proof.get('runtimeApplied') is not False or proof.get('windowsOpened') is not False
            or proof.get('seededTargetSource') is not True):
        raise ValueError('Drain source/target/isolation proof differs')
    if [c.get('case') for c in proof.get('cases', [])] != ['verified', 'lost-drain-ack', 'cancel-drain', 'deny-drain-receipt']:
        raise ValueError('Missing full drain cases')
    for case in proof['cases']:
        health, abort, drain = case['health'], case['abort'], case['drain']
        result, context = health['result'], health['context']
        config, rehearsal = result['config'], result['rehearsal']
        if (len(health['probes']) != 2 or config['descriptor']['build']['sourceManifestSha256'] != target['manifestSha256']
                or config['descriptor'] != rehearsal['descriptor']
                or result['parentRevision'] != health['intent']['parentRevision']+1
                or config['config'] != context['workerConfig']
                or rehearsal['updaterJournalSchema'] != context['compatibility']['updaterJournalSchema']
                or rehearsal['updaterJournalSchema'] != 11
                or rehearsal['productionStateOpened'] or rehearsal['controllerEnabled']
                or not rehearsal['freshJournal'] or not rehearsal['serverStopped'] or not rehearsal['ipcReady']):
            raise ValueError('Actual same-schema target health reports differ')
        if (health['result']['outcome'] != 'verified' or abort['phase'] != 'completed'
                or abort['cleanupVerified'] is not True or not abort.get('workerReleased')
                or drain['userPausePreserved'] is not True or drain['newRootRejected'] is not True
                or drain['runtimeApplied'] is not False or drain.get('previousSchemaRefused') is not True):
            raise ValueError('Missing health, queue or owned release evidence')
        for p in health['probes']:
            cleanup = p.get('cleanup') or {}
            if not (cleanup.get('treeEmpty') and cleanup.get('captureComplete')
                    and cleanup.get('exitCode') == 0 and p.get('resumeAcknowledged')):
                raise ValueError('Health helper cleanup incomplete')
        waiting = drain['waiting']
        observation = waiting['candidateDrain']['workerObservation']
        if (waiting['schemaVersion'] != 5 or waiting['update']['phase'] != 'draining'
                or waiting['candidateDrain']['phase'] != 'waiting_accepted'
                or observation['acceptedOperations'] != 2 or observation['queueMode'] != 'dispatch_paused'
                or waiting['candidateDrain']['confirmedCommands'] != 1):
            raise ValueError('Live paused accepted-work observation missing')
        final, ledger = drain['final'], drain['journal']
        commands = ledger['commands']
        if len(commands) != 3 or len(drain['tasks']) != 2:
            raise ValueError('Missing command/task evidence')
        states = [r['delivery']['state'] for r in commands]
        intent = ledger['intent']
        if (intent['binding']['updateId'] != waiting['update']['updateId']
                or intent['binding']['ownerId'] != health['intent']['ownerId']
                or intent['binding']['ownerEpoch'] != health['intent']['ownerEpoch']
                or len({r['command']['commandId'] for r in commands}) != 3):
            raise ValueError('Drain intent scope/owner/command identity differs')
        for n, row in enumerate(commands):
            command = intent['commands'][n]
            if row['command'] != command or row['ordinal'] != n:
                raise ValueError('Drain command intent differs')
            if row['delivery']['state'] == 'confirmed':
                receipt = row['delivery']['receipt']
                if (receipt['revision'] != command['expectedRevision']+1
                        or receipt['phase'] != ['draining', 'quiescing', 'quiesced'][n]
                        or receipt['reason'] != command['reason']
                        or receipt['spec'] != commands[0]['delivery']['receipt']['spec']):
                    raise ValueError('Exact immutable worker receipt differs')
        if case['case'] in {'verified', 'lost-drain-ack'}:
            if (states != ['confirmed']*3 or final['update']['phase'] != 'quiesced'
                    or ledger['quiescedRevision'] != final['update']['revision']
                    or abort['spec']['originalPhase'] != 'quiesced'):
                raise ValueError('Atomic complete quiescence missing')
        elif case['case'] == 'deny-drain-receipt':
            if (states != ['confirmed', 'confirmed', 'uncertain']
                    or ledger['quiescedRevision'] is not None
                    or final['update']['phase'] != 'draining' or not final['driverError']):
                raise ValueError('Ignored receipt did not retain unknown outcome')
        elif states != ['confirmed', 'prepared', 'prepared']:
            raise ValueError('Cancellation unexpectedly dispatched further drain commands')
        if case['case'] == 'lost-drain-ack' and (drain.get('lostAckReconciled') is not True or drain.get('beginDrainRequests') != 1):
            raise ValueError('Lost ACK replayed mutation or was not reconciled')
        expected = 'queued' if case['case'] == 'cancel-drain' else 'succeeded'
        if any(t['status'] != expected or expected == 'succeeded' and t['exitCode'] != 0 for t in drain['tasks']):
            raise ValueError('Updater cancelled or failed accepted tasks')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--target-archive', required=True, type=Path)
    parser.add_argument('--cache-run', required=True)
    parser.add_argument('--drain-gate', action='store_true')
    parser.add_argument('--previous-updater')
    parser.add_argument('--previous-updater-sha256')
    parser.add_argument('--controller-config', default='.winbridge/controller-snapshot.local.json', type=Path)
    args = parser.parse_args()
    source, target = archive_pin(args.archive), archive_pin(args.target_archive)
    drain_parameters = ''
    if args.drain_gate:
        if (not args.previous_updater or not re.fullmatch(r'[0-9a-f]{64}', args.previous_updater_sha256 or '')
                or PureWindowsPath(args.previous_updater).drive.casefold() != 'k:'
                or not PureWindowsPath(args.previous_updater).is_absolute()):
            raise ValueError('Pinned absolute local schema10 updater reference required')
        drain_parameters = (' -DrainGate -PreviousUpdaterExecutable '+native.ps_string(args.previous_updater)
                            +' -PreviousUpdaterSha256 '+native.ps_string(args.previous_updater_sha256))
    if source['sourceCommit'] == target['sourceCommit'] or not re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.cache_run):
        raise ValueError('Distinct source generations and original cache run required')
    cfg = json.loads(args.controller_config.read_text())
    endpoint = Path(cfg['stateDirectory']) / ('controller-'+cfg['instanceId']+'.sock')
    info = native.call(endpoint, {'method': 'info'})['info']
    if (info['activeTaskCount'] or info.get('maintenanceError') or info.get('journalRecoveryRequired')
            or info.get('recoveryCoverage', {}).get('complete') is not True):
        raise ValueError('Healthy idle installed worker required')
    project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture')
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime())+'-'+uuid.uuid4().hex[:8]
    # Use the same WinBridge namespace and registered local project as other native gates.
    report = Path('/mnt/hgfs/shared/WinBridge/workflow-validation') / run
    report.mkdir()
    local_report = ROOT / '.winbridge/workflow-validation' / run
    local_report.mkdir(parents=True)
    local = str(PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / run)
    windows_report = 'K:\\VMs\\shared\\WinBridge\\workflow-validation\\'+run
    cache = str(PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / args.cache_run)
    ps = native.ps_string
    script = f'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$archive={ps(str(PureWindowsPath('K:/VMs/shared') / args.archive.relative_to('/mnt/hgfs/shared')))}
$target={ps(str(PureWindowsPath('K:/VMs/shared') / args.target_archive.relative_to('/mnt/hgfs/shared')))}
$local={ps(local)}
$source=Join-Path $local 'winbridge'
if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(source['archiveSha256'])}){{throw 'Source archive changed'}}
if(Test-Path -LiteralPath $local){{throw 'Existing isolated run preserved'}}
$drive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($local))
if($drive.AvailableFreeSpace -lt 6GB){{throw 'Less than 6 GiB before extraction; no cleanup'}}
New-Item -ItemType Directory -Path $local | Out-Null
$copiedArchive=Join-Path $local 'source.zip'
[IO.File]::Copy($archive,$copiedArchive,$false)
if((Get-FileHash -LiteralPath $copiedArchive -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(source['archiveSha256'])}){{throw 'Copied source archive changed'}}
Expand-Archive -LiteralPath $copiedArchive -DestinationPath $local
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $source 'scripts/test-update-health-actor.ps1') -SourceDirectory $source -SourceArchiveSha256 {ps(source['archiveSha256'])} -TargetArchivePath $target -TargetArchiveSha256 {ps(target['archiveSha256'])} -ValidationCacheDirectory {ps(cache)} -ReportDirectory {ps(windows_report)}{drain_parameters}
exit $LASTEXITCODE
'''
    request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'snapshot-fixture',
               'idempotencyKey': 'native-health-'+run, 'timeoutMs': 1800000,
               'command': {'kind': 'direct', 'executable': 'pwsh.exe', 'arguments': ['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()], 'workingDirectory': '.'}}
    (local_report / 'request.json').write_text(json.dumps(request, indent=2))
    task = native.call(endpoint, {'method': 'submit', 'request': request})['task']
    print(json.dumps({'taskId': task['taskId'], 'runId': run, 'reportDirectory': str(report)}), flush=True)
    chunks, sequence = [], 0
    deadline = time.monotonic()+1850
    while True:
        task = native.call(endpoint, {'method': 'get_task', 'taskId': task['taskId']})['task']
        while True:
            page = native.call(endpoint, {'method': 'read_output', 'taskId': task['taskId'], 'afterSequence': sequence})['chunks']
            if not page:
                break
            chunks.extend(page); sequence = page[-1]['sequence']
        if task['status'] not in {'queued', 'running'}:
            break
        if time.monotonic() > deadline:
            raise TimeoutError('Observation deadline; inspect same task, no retry: '+task['taskId'])
        time.sleep(1)
    (local_report / 'task-report.json').write_text(json.dumps({'task': task, 'output': chunks}))
    if not (report/'report.ready.json').exists():
        diagnostic = ''.join(bytes(c['bytes']).decode('utf-8', errors='replace') for c in chunks)[-8000:]
        raise RuntimeError('Native bootstrap failed; inspect '+str(local_report)+': '+diagnostic)
    ready = json.loads((report / 'report.ready.json').read_text(encoding='utf-8-sig'))
    for entry in ready['files']:
        name = entry['path']
        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name):
            raise ValueError('Unsafe report path')
        b = (report/name).read_bytes()
        if len(b) != entry['bytes'] or hashlib.sha256(b).hexdigest() != entry['sha256']:
            raise ValueError('Report bytes differ: '+name)
    result = json.loads((report / 'report.json').read_text(encoding='utf-8-sig'))
    if task['status'] != 'succeeded' or task['exitCode'] != 0 or not result['passed']:
        raise RuntimeError({'taskId': task['taskId'], 'error': result['error'], 'reportDirectory': str(report)})
    verifier = verify_drain_proof if args.drain_gate else verify_proof
    verifier(json.loads((report/'candidate-health-actor-proof.json').read_text(encoding='utf-8-sig')), source, target)
    print(json.dumps({'passed': True, 'taskId': task['taskId'], 'reportDirectory': str(report), 'filesVerified': len(ready['files'])}), flush=True)


if __name__ == '__main__':
    main()
