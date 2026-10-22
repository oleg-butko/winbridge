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
import unicodedata
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


def verify_shutdown_main_proof(proof, source, target):
    build = proof.get('build', {})
    if (proof.get('kind') != 'shutdown_main_native_proof' or proof.get('passed') is not True
            or proof.get('platform') != 'windows' or proof.get('target') != target
            or build.get('sourceCommit') != source['sourceCommit'] or build.get('sourceDirty') is not False
            or build.get('sourceManifestSha256') != source['manifestSha256']
            or proof.get('runtimeApplied') is not False or proof.get('windowsOpened') is not False
            or any(proof.get(k) is not True for k in ('normalMain', 'actualOldRoles', 'actualCandidateBuild', 'validationFeature'))):
        raise ValueError('Actual Main source/target/isolation proof differs')
    cases = proof.get('cases', [])
    if [c.get('fault') for c in cases] != ['none', 'lost_ack', 'delayed_reply', 'receipt_write_denied']:
        raise ValueError('Missing ordered Main shutdown cases')
    for case in cases:
        _verify_shutdown_main_case(case, build, target)


def _verify_shutdown_main_case(case, build, target):
    before, shutdown, root, cold = (case[k] for k in ('beforeMainExit', 'shutdown', 'root', 'coldAfterMain'))
    intent, delivery = shutdown['intent'], shutdown['delivery']
    binding = intent['binding']
    denied = case['fault'] == 'receipt_write_denied'
    if (before.get('requestShutdownCount') != 1 or before.get('normalMain') is not True
            or before.get('normalServe') is not True or before.get('passed') is not True
            or before.get('build') != build or before.get('worker') != binding['worker']
            or before.get('runtimeApplied') is not False or before.get('windowsOpened') is not False
            or root['updateId'] != binding['updateId'] or root['request']['source'] != target
            or root['phase'] != ('stopping' if denied else 'replacing')
            or delivery['state'] != ('uncertain' if denied else 'exited')
            or case.get('physicalCustodyAfterExit') is not (not denied)
            or any(case.get(k) is not True for k in ('supervisorOutlivedMain', 'startupStillBlocked', 'oldRuntimeUnchanged', 'ownedSupervisorCrashRetainsHistoryWithoutCustody'))):
        raise ValueError('Shutdown effect count, scope, exit or retained barrier differs')
    if (cold.get('kind') != 'worker_shutdown_inspection' or cold.get('journalOpenedReadOnly') is not True
            or any(cold.get(k) is not False for k in ('workerConstructed', 'schedulerStarted', 'recoveryMarkerChanged', 'runtimeApplied'))
            or cold['state'] != binding['workerState'] or cold['helperImage'] != binding['loadedWorker']['image']
            or cold['descriptor'] != binding['loadedWorker']['descriptor']
            or cold['workerConfig']['sha256'] != binding['worker']['configSha256']
            or cold['workerJournalSchema'] != 16):
        raise ValueError('Actual Main cold reader changed state or belongs to another scope')
    if denied:
        if cold['receipt'] is not None or before.get('receiptDeniedFixtureStop') is not True:
            raise ValueError('Receipt write failure unexpectedly accepted a stop')
    else:
        receipt, physical = delivery['receipt'], delivery['physical']
        if (cold['receipt'] != receipt or receipt['command'] != intent['command']
                or receipt['process'] != binding['worker']['process'] or receipt['state'] != binding['workerState']
                or physical['process'] != receipt['process'] or physical['state'] != receipt['state']
                or physical['exactProcessExited'] is not True or physical['instanceLockAcquired'] is not True
                or receipt['lease'] != binding['lease']):
            raise ValueError('Accepted receipt and exact process/lock evidence differ')
        if case['fault'] == 'lost_ack' and shutdown['revision'] != 4:
            raise ValueError('Lost ACK did not retain uncertainty before reconciliation')
    duplicate = before['duplicate']['candidateShutdown']
    if duplicate['attemptId'] != intent['attemptId'] or duplicate['exitedAtMs'] is not None or duplicate['physicalCustodyHeld'] is not False:
        raise ValueError('Live Main duplicate fabricated exit/custody')


def verify_state_backup_proof(proof, source, target):
    build = proof.get('build', {})
    if (proof.get('kind') != 'state_backup_main_native_proof' or proof.get('passed') is not True
            or proof.get('platform') != 'windows' or proof.get('target') != target
            or build.get('sourceCommit') != source['sourceCommit'] or build.get('sourceDirty') is not False
            or build.get('sourceManifestSha256') != source['manifestSha256']
            or proof.get('runtimeApplied') is not False or proof.get('windowsOpened') is not False
            or any(proof.get(k) is not True for k in ('normalMain', 'actualOldRoles', 'actualCandidateBuild', 'validationFeature'))
            or len(proof.get('cases', [])) != 1 or proof['cases'][0].get('fault') != 'none'):
        raise ValueError('Final backup Main/source/target/isolation proof differs')
    case = proof['cases'][0]
    _verify_shutdown_main_case(case, build, target)
    backup = case['finalStateBackup']
    r = backup['record']
    intent, report, cleanup = r['intent'], r['report'], r['cleanup']
    binding = intent['binding']
    old = case['shutdown']['intent']['binding']
    canonical = json.dumps([binding, intent['attemptId']], sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()
    if (intent.get('schemaVersion') != 1
            or hashlib.sha256(b'winbridge.final-state-backup.intent.v1\n'+canonical).hexdigest() != intent['bindingSha256']
            or binding['shutdown'] != case['shutdown'] or binding['workerJournalSchema'] != 16
            or PureWindowsPath(binding['parentDirectory']) != PureWindowsPath(old['loadedWorker']['image']['path']).parent/'update'/old['updateId']
            or case['root']['revision'] != old['parentRevision']+2
            or r.get('phase') != 'verified' or r.get('revision') != 5
            or r.get('resumeGranted') is not True or r.get('resumeAcknowledged') is not True
            or r.get('reason') is not None or report.get('kind') != 'worker_state_backup'
            or report.get('schemaVersion') != 1 or report.get('attemptId') != intent['attemptId']
            or report.get('updateId') != old['updateId'] or report.get('helperProcess') != r['process']
            or report.get('helperDescriptor') != old['loadedWorker']['descriptor']
            or report.get('helperImage') != old['loadedWorker']['image']
            or report.get('workerConfig') != binding['workerConfig']
            or report.get('state') != old['workerState'] or report.get('workerJournalSchema') != 16
            or report.get('shutdownReceipt') != case['shutdown']['delivery']['receipt']
            or report.get('inputSha256') != r['inputFile']['sha256']
            or report['sourceAuthority'] != report['backupAuthority']
            or report['sourceAuthority']['incarnation'] != old['worker']['context']['journalIncarnation']
            or report['sourceAuthority']['epoch'] <= 0
            or not re.fullmatch(r'[0-9a-f]{64}', report['sourceAuthority']['fingerprintSha256'])
            or report['backup']['bytes'] != report['pageSize']*report['pageCount']
            or report['pageCount'] <= 0 or not re.fullmatch(r'[0-9a-f]{64}', report['backup']['sha256'])
            or PureWindowsPath(report['backup']['path']) != PureWindowsPath(binding['parentDirectory'])/('state-backup-'+intent['attemptId']+'.sqlite')
            or PureWindowsPath(r['inputFile']['path']) != PureWindowsPath(binding['parentDirectory'])/('state-backup-'+intent['attemptId']+'.json')
            or report['backup']['fileIdentity'] in (old['workerState']['journalIdentity'], r['inputFile']['fileIdentity'], binding['parentIdentity'])
            or report['helperProcess'] == old['worker']['process']
            or any(report.get(k) is not True for k in ('integrityVerified', 'backupCreatedNew', 'journalOpenedReadOnly'))
            or any(report.get(k) is not False for k in ('workerConstructed', 'schedulerStarted', 'recoveryMarkerChanged', 'runtimeApplied'))
            or report['completedAtMs'] < binding['admittedAtMs'] or report['completedAtMs'] > r['updatedAtMs']):
        raise ValueError('Final backup intent/helper/authority/file/header differs')
    if (cleanup.get('process') != r['process'] or cleanup.get('exitCode') != 0
            or cleanup.get('failure') is not None or cleanup.get('noProcessCreated') is not False
            or cleanup.get('outputExceeded') is not False
            or any(cleanup.get(k) is not True for k in ('creationConfirmed', 'resumeRequested', 'treeEmpty', 'captureComplete'))
            or cleanup.get('jobName') != 'Global\\WinBridge-StateBackup-'+intent['bindingSha256']):
        raise ValueError('Final backup owned cleanup/PID differs')
    view = backup['view']
    v = view['candidateStateBackup']
    if (view.get('schemaVersion') != 7 or view.get('driverActive') is not False
            or view.get('driverError') is not None or view['update'] != case['root']
            or view['candidateShutdown'].get('physicalCustodyHeld') is not True
            or v.get('phase') != 'verified' or v.get('attemptId') != intent['attemptId']
            or v.get('revision') != r['revision'] or v.get('helperProcess') != r['process']
            or v.get('backupBytes') != report['backup']['bytes'] or v.get('backupSha256') != report['backup']['sha256']
            or v.get('completedAtMs') != report['completedAtMs'] or v.get('reason') is not None
            or backup['duplicate']['candidateStateBackup'] != v
            or backup.get('sourceRowsSha256') != backup.get('backupRowsSha256')
            or not re.fullmatch(r'[0-9a-f]{64}', backup.get('sourceRowsSha256', ''))
            or backup.get('sourceTaskCount', 0) < 1 or backup.get('sourceOutputCount', 0) < 2
            or any(backup.get(k) is not True for k in ('sourceBytesUnchanged', 'allLogicalRowsPreserved', 'duplicateNoNewAttempt', 'privateAuthorityNotExported'))):
        raise ValueError('Final backup scope/history/source preservation or duplicate differs')


def backup_failure_reason(text):
    # Match Rust char::is_control and chars().take(900), including non-BMP text.
    return ''.join(ch for ch in text if unicodedata.category(ch) != 'Cc')[:900]


def verify_state_backup_faults_proof(proof, source, target):
    build = proof.get('build', {})
    if (proof.get('kind') != 'state_backup_faults_native_proof' or proof.get('passed') is not True
            or proof.get('platform') != 'windows' or proof.get('target') != target
            or build.get('sourceCommit') != source['sourceCommit'] or build.get('sourceDirty') is not False
            or build.get('sourceManifestSha256') != source['manifestSha256']
            or proof.get('runtimeApplied') is not False or proof.get('windowsOpened') is not False
            or any(proof.get(k) is not True for k in ('normalMain', 'actualOldRoles', 'actualCandidateBuild', 'validationFeature'))):
        raise ValueError('Backup faults source/target/Main isolation differs')
    faults = ['existing_destination', 'exit_after_copy', 'wrong_input_report', 'hold_before_copy']
    cases = proof.get('cases', [])
    if [c.get('finalStateBackup', {}).get('expectedFault') for c in cases] != faults:
        raise ValueError('Four ordered backup fault cases required')
    attempts, processes = set(), set()
    for case, fault in zip(cases, faults):
        if case.get('fault') != 'none':
            raise ValueError('Backup fault case requires successful exact stop')
        _verify_shutdown_main_case(case, build, target)
        b = case['finalStateBackup']
        r, view, after = b['record'], b['view'], b['afterOwnedCrash']
        intent, binding = r['intent'], r['intent']['binding']
        old = case['shutdown']['intent']['binding']
        canonical = json.dumps([binding, intent['attemptId']], sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()
        proc = r['process']
        stamp = (proc['hostId'], proc['sessionId'], proc['processId'], proc['birth'])
        if (intent['attemptId'] in attempts or stamp in processes
                or proc['processId'] <= 0 or not str(proc['birth']).isdigit()
                or proc == old['worker']['process'] or proc['hostId'] != old['worker']['process']['hostId']
                or proc['sessionId'] != old['worker']['process']['sessionId']):
            raise ValueError('Backup helper identity/attempt repeated or differs')
        attempts.add(intent['attemptId']); processes.add(stamp)
        held = fault == 'hold_before_copy'
        if (r.get('schemaVersion') != 1 or intent.get('schemaVersion') != 1
                or hashlib.sha256(b'winbridge.final-state-backup.intent.v1\n'+canonical).hexdigest() != intent['bindingSha256']
                or binding['shutdown'] != case['shutdown'] or binding['workerJournalSchema'] != 16
                or binding['workerConfig'] != case['coldAfterMain']['workerConfig']
                or PureWindowsPath(binding['parentDirectory']) != PureWindowsPath(old['loadedWorker']['image']['path']).parent/'update'/old['updateId']
                or case['root']['revision'] != old['parentRevision']+2
                or r.get('phase') != ('running' if held else 'indeterminate')
                or r.get('revision') != (3 if held else 5)
                or r.get('resumeGranted') is not True or r.get('resumeAcknowledged') is not True
                or r.get('report') is not None or (r.get('reason') is None) is not held
                or r['updatedAtMs'] < binding['admittedAtMs']
                or b['faultMarker'] != {'fault': fault, 'attemptId': intent['attemptId'], 'process': proc}
                or PureWindowsPath(r['inputFile']['path']) != PureWindowsPath(binding['parentDirectory'])/('state-backup-'+intent['attemptId']+'.json')
                or not re.fullmatch(r'[0-9a-f]{64}', r['inputFile']['sha256'])
                or not 0 < r['inputFile']['bytes'] <= 65536
                or not re.fullmatch(r'[0-9a-f]{64}', b.get('sourceRowsSha256', ''))
                or b.get('sourceTaskCount', 0) < 1 or b.get('sourceOutputCount', 0) < 2
                or b.get('sourceBytesUnchanged') is not True
                or b.get('sameHistoryWithoutCustody') is not True):
            raise ValueError('Backup fault intent/record/marker/history differs')
        before = b['beforeAdmission']
        if (before.get('schemaVersion') != 7 or before['update'] != case['root']
                or before.get('driverActive') is not False or before.get('driverError') is not None
                or before['candidateShutdown'].get('physicalCustodyHeld') is not True
                or before['candidateStateBackup'].get('phase') != 'not_admitted'
                or before['candidateStateBackup'].get('attemptId') is not None):
            raise ValueError('Backup was requested before original shutdown driver retired')
        v = view['candidateStateBackup']
        if (view.get('schemaVersion') != 7 or view['update'] != case['root']
                or view.get('driverActive') is not held
                or (view.get('driverError') is None) is not held
                or view['candidateShutdown'].get('physicalCustodyHeld') is not True
                or v.get('phase') != ('copying_state' if held else 'indeterminate')
                or v.get('attemptId') != intent['attemptId'] or v.get('revision') != r['revision']
                or v.get('helperProcess') != proc or v.get('reason') != r.get('reason')
                or any(v.get(k) is not None for k in ('backupBytes', 'backupSha256', 'completedAtMs'))
                or after.get('schemaVersion') != 7 or after.get('driverActive') is not False
                or after['update'] != case['root'] or after['candidateShutdown'].get('physicalCustodyHeld') is not False
                or after['candidateStateBackup'] != dict(v, phase='indeterminate')):
            raise ValueError('Backup fault view invented success/custody or lost history')
        if held:
            if (r.get('cleanup') is not None or b.get('helperHeldBeforeCopy') is not True
                    or b.get('destinationAbsent') is not True):
                raise ValueError('Held helper fabricated cleanup or backup')
            continue
        c = r['cleanup']
        expected_exit = {'existing_destination': 1, 'exit_after_copy': 79, 'wrong_input_report': 0}[fault]
        if (c.get('process') != proc or c.get('exitCode') != expected_exit
                or c.get('noProcessCreated') is not False or c.get('outputExceeded') is not False
                or any(c.get(k) is not True for k in ('creationConfirmed', 'resumeRequested', 'treeEmpty', 'captureComplete'))
                or c.get('jobName') != 'Global\\WinBridge-StateBackup-'+intent['bindingSha256']
                or b['duplicate']['candidateStateBackup'] != v or b['duplicate'].get('driverActive') is not False
                or b.get('duplicateNoNewAttempt') is not True or b.get('destinationPreserved') is not True):
            raise ValueError('Backup fault cleanup/duplicate differs')
        if fault == 'wrong_input_report':
            if (c.get('failure') is not None or r['reason'] != 'Backup report differs from saved helper/input/exit scope or successful cleanup'):
                raise ValueError('Wrong input report was not rejected by binding')
        elif (not isinstance(c.get('failure'), str) or not c['failure'].startswith('Artifact query failed; stderr=')
              or r['reason'] != backup_failure_reason(c['failure'])):
            raise ValueError('Fault helper exit was not preserved')
        evidence = b['destinationEvidence']
        if (PureWindowsPath(evidence['path']) != PureWindowsPath(binding['parentDirectory'])/('state-backup-'+intent['attemptId']+'.sqlite')
                or not re.fullmatch(r'[0-9a-f]{64}', evidence['sha256'])
                or evidence['fileIdentity'] in (binding['parentIdentity'], old['workerState']['journalIdentity'], r['inputFile']['fileIdentity'])):
            raise ValueError('Preserved fault destination identity differs')
        if fault == 'existing_destination':
            sentinel = b'preserve existing validation backup destination'
            if evidence['sha256'] != hashlib.sha256(sentinel).hexdigest() or evidence['bytes'] != len(sentinel):
                raise ValueError('Existing destination was overwritten')
        elif (evidence['bytes'] <= 0 or evidence['bytes'] % 512 or b.get('backupRowsSha256') != b['sourceRowsSha256']):
            raise ValueError('Unaccepted copied database lost rows')


def verify_shutdown_supplemental_proof(proof, source, target):
    build = proof.get('build', {})
    if (proof.get('kind') != 'shutdown_main_supplemental_proof' or proof.get('passed') is not True
            or proof.get('platform') != 'windows' or proof.get('target') != target
            or build.get('sourceCommit') != source['sourceCommit'] or build.get('sourceDirty') is not False
            or build.get('sourceManifestSha256') != source['manifestSha256']
            or proof.get('runtimeApplied') is not False or proof.get('windowsOpened') is not False
            or any(proof.get(k) is not True for k in ('normalMain', 'actualOldRoles', 'actualCandidateBuild', 'validationFeature'))):
        raise ValueError('Supplemental Main source/target/isolation differs')
    cases = proof.get('cases', [])
    if [c.get('fault') for c in cases] != ['delayed_instance_lock', 'supervisor_crash_before_exit']:
        raise ValueError('Both ordered supplemental cases required')
    delayed, crash = cases
    _verify_shutdown_main_case(delayed, build, target)
    held = delayed.get('delayedLockHolder', {})
    if (delayed.get('delayedOriginalLockVerified') is not True
            or held.get('state') != delayed['shutdown']['intent']['binding']['workerState']
            or held.get('attemptId') != delayed['shutdown']['intent']['attemptId']):
        raise ValueError('Original instance lock was not held after Main exit')
    before, saved, root, cold = (crash[k] for k in ('beforeMainExit', 'shutdown', 'root', 'coldAfterMain'))
    binding, delivery = saved['intent']['binding'], saved['delivery']
    if (before.get('requestShutdownCount') != 1 or before.get('normalMain') is not True
            or before.get('normalServe') is not True or before.get('build') != build
            or before.get('worker') != binding['worker']
            or before.get('runtimeApplied') is not False or before.get('windowsOpened') is not False
            or root['request']['source'] != target or root['updateId'] != binding['updateId']
            or root['phase'] != 'stopping' or delivery['state'] != 'acknowledged'
            or saved['revision'] != 2 or crash.get('physicalCustodyAfterExit') is not False
            or any(crash.get(k) is not True for k in ('mainAndSupervisorAliveBeforeCrash', 'startupStillBlocked', 'oldRuntimeUnchanged', 'ownedSupervisorCrashRetainsHistoryWithoutCustody'))):
        raise ValueError('Pre-exit crash fabricated exit/authority or lost its barrier')
    receipt = delivery['receipt']
    if (cold['receipt'] != receipt or receipt['command'] != saved['intent']['command']
            or receipt['lease'] != binding['lease'] or receipt['state'] != binding['workerState']
            or receipt['process'] != binding['worker']['process'] or cold['state'] != receipt['state']
            or cold['descriptor'] != binding['loadedWorker']['descriptor']
            or cold['helperImage'] != binding['loadedWorker']['image']
            or cold['journalOpenedReadOnly'] is not True or cold['workerJournalSchema'] != 16
            or any(cold[k] is not False for k in ('workerConstructed', 'schedulerStarted', 'recoveryMarkerChanged', 'runtimeApplied'))):
        raise ValueError('Cold receipt after owned pre-exit crash differs')
    duplicate = before['duplicate']['candidateShutdown']
    if duplicate['exitedAtMs'] is not None or duplicate['physicalCustodyHeld'] is not False or duplicate['attemptId'] != saved['intent']['attemptId']:
        raise ValueError('Live Main before crash fabricated exit/custody')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--target-archive', required=True, type=Path)
    parser.add_argument('--cache-run', required=True)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--drain-gate', action='store_true')
    modes.add_argument('--shutdown-main-gate', action='store_true')
    modes.add_argument('--shutdown-main-supplemental', action='store_true')
    modes.add_argument('--final-state-backup-gate', action='store_true')
    modes.add_argument('--final-state-backup-faults', action='store_true')
    parser.add_argument('--previous-updater')
    parser.add_argument('--previous-updater-sha256')
    parser.add_argument('--controller-config', default='.winbridge/controller-snapshot.local.json', type=Path)
    args = parser.parse_args()
    source, target = archive_pin(args.archive), archive_pin(args.target_archive)
    drain_parameters = ' -FinalStateBackupFaults' if args.final_state_backup_faults else ' -FinalStateBackupGate' if args.final_state_backup_gate else ' -ShutdownMainSupplemental' if args.shutdown_main_supplemental else ' -ShutdownMainGate' if args.shutdown_main_gate else ''
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
    if (info.get('platform') != 'windows' or info['activeTaskCount'] or info.get('maintenanceError') or info.get('journalRecoveryRequired')
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
    timeout_ms = 3600000 if args.final_state_backup_faults else 1800000
    request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'snapshot-fixture',
               'idempotencyKey': 'native-health-'+run, 'timeoutMs': timeout_ms,
               'command': {'kind': 'direct', 'executable': 'pwsh.exe', 'arguments': ['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()], 'workingDirectory': '.'}}
    (local_report / 'request.json').write_text(json.dumps(request, indent=2))
    task = native.call(endpoint, {'method': 'submit', 'request': request})['task']
    print(json.dumps({'taskId': task['taskId'], 'runId': run, 'reportDirectory': str(report)}), flush=True)
    chunks, sequence = [], 0
    deadline = time.monotonic()+timeout_ms/1000+50
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
    verified_names = set()
    for entry in ready['files']:
        name = entry['path']
        if name in verified_names or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name):
            raise ValueError('Unsafe report path')
        verified_names.add(name)
        b = (report/name).read_bytes()
        if len(b) != entry['bytes'] or hashlib.sha256(b).hexdigest() != entry['sha256']:
            raise ValueError('Report bytes differ: '+name)
    if 'report.json' not in verified_names:
        raise ValueError('Ready manifest must cover the report')
    result = json.loads((report / 'report.json').read_text(encoding='utf-8-sig'))
    if task['status'] != 'succeeded' or task['exitCode'] != 0 or not result['passed']:
        raise RuntimeError({'taskId': task['taskId'], 'error': result['error'], 'reportDirectory': str(report)})
    if 'candidate-health-actor-proof.json' not in verified_names:
        raise ValueError('Ready manifest must cover the actual process proof')
    expected_kind = 'state_backup_faults_native_gate' if args.final_state_backup_faults else 'state_backup_main_native_gate' if args.final_state_backup_gate else 'shutdown_main_supplemental_native_gate' if args.shutdown_main_supplemental else 'shutdown_main_native_gate' if args.shutdown_main_gate else 'candidate_drain_native_gate' if args.drain_gate else 'candidate_health_native_gate'
    if result.get('kind') != expected_kind or result.get('sourceCommit') != source['sourceCommit'] or result.get('sourceArchiveSha256') != source['archiveSha256'] or result.get('target') != target:
        raise ValueError('Report kind or exact source/target binding differs')
    verifier = verify_state_backup_faults_proof if args.final_state_backup_faults else verify_state_backup_proof if args.final_state_backup_gate else verify_shutdown_supplemental_proof if args.shutdown_main_supplemental else verify_shutdown_main_proof if args.shutdown_main_gate else verify_drain_proof if args.drain_gate else verify_proof
    verifier(json.loads((report/'candidate-health-actor-proof.json').read_text(encoding='utf-8-sig')), source, target)
    print(json.dumps({'passed': True, 'taskId': task['taskId'], 'reportDirectory': str(report), 'filesVerified': len(ready['files'])}), flush=True)


if __name__ == '__main__':
    main()
