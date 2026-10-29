"""Independent actual22 ->23 copy migration bound to audited Main29; no RPC."""
import copy
import hashlib
import re
import subprocess
import sys
from pathlib import Path
import importlib.util

ROOT = Path(__file__).resolve().parents[1]


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


legacy = load('finalization_migration_json', 'update-activation-lifetime-migration-proof.py')
need, parse_json = legacy.need, legacy.parse_json
NETWORK_SESSION_PATH = legacy.NETWORK_SESSION_PATH


def derive_reference(meta, report, proof, audit, checks):
    """Caller MUST run the full frozen Main29 reader before deriving anything."""
    need(audit['passed'] is True and audit['kind'] == 'main29_independent_audit'
         and audit['profile'] == 'worker19-updater22' and audit['workerJournalSchema'] == 19
         and audit['updaterJournalSchema'] == 22 and audit['nativeMarkersVerified'] == 3
         and audit['readyLossMarkersVerified'] == 1 and audit['installedWorkerUnchanged'] is True
         and audit['run'] == meta['run'] and audit['taskId'] == meta['taskId']
         and audit['source'] == meta['source'] and audit['target'] == meta['target']
         and audit['runtimeArchive'] == report['runtimeArchive'], 'Exact successful Main29 audit required')
    need(report['kind'] == 'activation_resident_main_native_gate' and report['passed'] is True
         and report['error'] is None and report['sourceCommit'] == meta['source']['sourceCommit']
         and report['sourceArchiveSha256'] == meta['source']['archiveSha256']
         and report['target'] == meta['target'], 'Another Main29 producer/target')
    case, = proof['cases']
    p, c, l, resident = (case[k] for k in ('activationWriter', 'activationController', 'activationLifetime', 'activationResident'))
    physical, writer, service = (p[k] for k in ('record', 'writerRecord', 'serviceRecord'))
    controller, lifetime, root = c['record'], l['record'], resident['second']['update']
    need(physical == c['physicalRecord'] == l['physicalRecord'] and physical['revision'] == 7
         and physical['state'] == {'phase': 'local_verified'}
         and physical['intent']['context']['updaterJournalSchema'] == 22
         and physical['intent']['context']['workerJournalSchema'] == 19
         and all(v['state']['phase'] == 'confirmed' for v in (writer, service, controller, lifetime))
         and root == resident['first']['update'] == case['coldActivationResident']['view']['update']
         and root['phase'] == 'awaiting_controller' and resident['rootCommitted'] is False
         and resident['maintenanceReleased'] is False and resident['privateChannelRead'] is False,
         'Actual22 retained lifetime/local7/root prefix required')
    windows = checks.windows_path
    archive = windows(report['runtimeArchive']['directory'])
    need(archive.is_absolute() and archive.drive.lower() == 'k:'
         and archive.parent == windows(meta['workingDirectory']) / 'shutdown-main-cases'
         and archive.name.startswith('runtime-archive-'), 'Archive outside exact Main29 fixture')
    import uuid
    need(str(uuid.UUID(archive.name.removeprefix('runtime-archive-'))) == archive.name.removeprefix('runtime-archive-'), 'Canonical runtime archive ID required')
    images, manifest = case['replacement']['actualInstalledFiles'], case['replacement']['actualManifest']
    need(len(images) == 5 and windows(images[3]['path']).parent == windows(meta['testRuntimeDirectory']), 'Old22 image/runtime differs')
    artifact, = (a for a in manifest['artifacts'] if a['name'] == 'winbridge-update.exe')
    need(artifact['sha256'] == images[3]['sha256']
         and artifact['descriptor']['build']['sourceCommit'] == meta['target']['sourceCommit']
         and artifact['descriptor']['build']['sourceManifestSha256'] == meta['target']['manifestSha256'], 'Old22 artifact source differs')
    image = dict(images[3], path='\\\\?\\' + str(archive / 'winbridge-update.exe'))
    sha = lambda value: hashlib.sha256(checks.canonical(value)).hexdigest()
    reference = dict(schemaVersion=1, kind='activation_schema22_reference', referenceRun=meta['run'],
        reportSha256=audit['reportSha256'], proofSha256=audit['proofSha256'], source=meta['target'], image=image,
        descriptor=artifact['descriptor'], manifestSha256=images[4]['sha256'], updateId=physical['intent']['source']['updateId'],
        activationSha256=sha(physical), rootSha256=sha(root), writerSha256=sha(writer), serviceSha256=sha(service),
        controllerSha256=sha(controller), lifetimeSha256=sha(lifetime))
    return reference, dict(originalImage=images[3], archivedImage=image, archive=report['runtimeArchive'])


def reference_from_main29(run_info, source_archive, target_archive):
    result = subprocess.run([sys.executable, str(ROOT / 'scripts/verify-update-resident-main-native.py'),
        '--profile', 'worker19-updater22', '--run-info', str(run_info), '--source-archive', str(source_archive),
        '--target-archive', str(target_archive)], check=True, capture_output=True, text=True)
    audit, meta = parse_json(result.stdout), parse_json(run_info.read_bytes())
    reader = load('finalization_migration_files', 'summarize-update-timings.py')
    checks = load('finalization_migration_canonical', 'update-activation-preparation-proof.py')
    files = reader.verified_files(Path(meta['reportDirectory']))
    need(files == reader.verified_files(Path(meta['reportDirectory'])), 'Main29 reference changed')
    need(hashlib.sha256(files['report.json']).hexdigest() == audit['reportSha256']
         and hashlib.sha256(files['candidate-health-actor-proof.json']).hexdigest() == audit['proofSha256'], 'Main29 report/proof SHA changed')
    report, proof = (parse_json(files[name].decode('utf-8-sig')) for name in ('report.json', 'candidate-health-actor-proof.json'))
    reference, location = derive_reference(meta, report, proof, audit, checks)
    return reference, location, audit, meta


TEST = 'actual_twenty_two_lifetime_history_survives_twenty_three_and_old_twenty_two_refuses'


def verify_selection(command, log):
    need(command['executable'] == 'cargo' and type(command['exitCode']) is int and command['exitCode'] == 0
         and command['arguments'] == ['test', '-p', 'winbridge-update', '--features', 'bundled-sqlite', '--test',
             'activation_finalization_migration', '--locked', '--offline', '--', '--exact', TEST, '--ignored', '--test-threads=1', '--nocapture'],
         'Another migration command selected')
    text = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    summaries = re.findall(r'^test result: ([^\r\n]+)', text, re.MULTILINE)
    need(len(re.findall(r'^test ' + re.escape(TEST) + r' \.\.\.', text, re.MULTILINE)) == 1
         and len(summaries) == 1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;')
         and re.search(r'^running 1 test\s*$', text, re.MULTILINE), 'Actual22 migration did not run once')


def verify(proof, source, reference, checks):
    checks.strict(proof, uuid_session_paths=frozenset({NETWORK_SESSION_PATH}))
    load('finalization_migration_frame_strict', 'update-activation-service-proof.py').strict(proof)
    common = {'schemaVersion', 'kind', 'passed', 'platform', 'build', 'referenceProofSha256', 'referenceReportSha256',
        'referenceImage', 'referenceManifestSha256', 'before', 'after', 'afterColdObservation', 'backupInfo',
        'retainedActivation', 'retainedRoot', 'retainedWriter', 'rowsBefore', 'rowsAfter', 'backupRows', 'gateRows',
        'sourceBytesUnchanged', 'copyBytesUnchangedByOld', 'noFreshReplacementPermit', 'existingReplacementObserved',
        'migrationBackupVerified', 'runtimeApplied', 'installedRuntimeApplied', 'windowsOpened',
        'oldTwentyTwoInitRefused', 'oldTwentyTwoStatusRefused', 'noFreshFinalizationIntent', 'oldLifetimeReceiverRefused',
        'retainedService', 'retainedController', 'retainedLifetime', 'coldResident'}
    need(set(proof) == common, 'Unknown/missing finalization migration fields')
    def no_grants(value):
        if isinstance(value, dict):
            for key, child in value.items():
                need(key not in ('serviceReady', 'canStartService', 'authority_id', 'lifetimeReady', 'canCommit',
                    'canLaunch', 'keeperResident', 'releaseMaintenance', 'ownerHandle', 'canFinalize'), 'Invented permission: ' + key)
                no_grants(child)
        elif isinstance(value, list):
            for child in value:
                no_grants(child)
    no_grants(proof)
    need(type(proof['schemaVersion']) is int and proof['schemaVersion'] == 1
         and proof['kind'] == 'activation_finalization_schema_migration_proof' and proof['passed'] is True
         and proof['platform'] == 'windows' and proof['build']['sourceCommit'] == source['sourceCommit']
         and proof['build']['sourceManifestSha256'] == source['manifestSha256'] and proof['build']['sourceDirty'] is False
         and all(proof[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened')), 'Wrong actual23 producer/isolation')
    need(proof['referenceProofSha256'] == reference['proofSha256'] and proof['referenceReportSha256'] == reference['reportSha256']
         and proof['referenceImage'] == reference['image'] and proof['referenceManifestSha256'] == reference['manifestSha256'], 'Another old22 reference')
    before, after = proof['before'], proof['after']
    need(type(before['schemaVersion']) is int and before['schemaVersion'] == 22
         and after == dict(before, schemaVersion=23, ownerEpoch=before['ownerEpoch'] + 1)
         and proof['afterColdObservation'] == dict(after, ownerEpoch=after['ownerEpoch'] + 1)
         and proof['backupInfo'] == before and before['activeUpdateId'] == reference['updateId'], 'Migration changed root/sequence/scope/fencing')
    physical, writer, service, controller, lifetime, root = (proof[k] for k in
        ('retainedActivation', 'retainedWriter', 'retainedService', 'retainedController', 'retainedLifetime', 'retainedRoot'))
    sha = lambda value: hashlib.sha256(checks.canonical(value)).hexdigest()
    need(all(sha(v) == reference[k] for v, k in ((physical, 'activationSha256'), (writer, 'writerSha256'),
        (service, 'serviceSha256'), (controller, 'controllerSha256'), (lifetime, 'lifetimeSha256'), (root, 'rootSha256')))
         and physical['revision'] == 7 and physical['state'] == {'phase': 'local_verified'}
         and physical['intent']['source']['updateId'] == reference['updateId']
         and physical['intent']['source']['ownerEpoch'] == before['ownerEpoch']
         and physical['intent']['context']['updaterJournalSchema'] == 22
         and physical['intent']['context']['workerJournalSchema'] == 19
         and before['installationId'] == physical['intent']['source']['installationId'] == root['request']['expected']['installationId']
         and before['scopeSha256'] == physical['intent']['source']['scopeSha256'] == root['request']['expected']['scopeSha256']
         and before['lastSequence'] >= root['revision'] + 1
         and writer['revision'] == 1 and all(v['state']['phase'] == 'confirmed' for v in (writer, service, controller, lifetime))
         and service['command']['binding']['writerCommand'] == writer['command']
         and controller['command']['binding']['serviceCommand'] == service['command']
         and controller['command']['binding']['serviceReceipt'] == service['state']['receipt']
         and physical['local'] == service['state']['receipt']['compatibility']
         and root['updateId'] == reference['updateId'] and root['phase'] == 'awaiting_controller', 'Retained prefix changed/relabelled')
    need(type(proof['gateRows']) is int and proof['gateRows'] == 0
         and proof['rowsBefore'] == proof['rowsAfter'] == proof['backupRows'] and re.fullmatch(r'[0-9a-f]{64}', proof['rowsBefore'])
         and all(proof[k] is True for k in ('oldTwentyTwoInitRefused', 'oldTwentyTwoStatusRefused', 'sourceBytesUnchanged',
            'copyBytesUnchangedByOld', 'noFreshReplacementPermit', 'existingReplacementObserved', 'migrationBackupVerified',
            'noFreshFinalizationIntent', 'oldLifetimeReceiverRefused')), 'Missing22 retention/backup/refusal/no-finalization evidence')
    view = proof['coldResident']
    need(set(view) == {'activationResident', 'build', 'driverActive', 'driverError', 'observedAtMs', 'schemaVersion', 'source', 'sourceAppliedFiles', 'update'}
         and view['schemaVersion'] == 20 and type(view['schemaVersion']) is int and view['update'] == root
         and view['build'] == proof['build'] and view['driverActive'] is False and view['driverError'] is None
         and view['observedAtMs'] >= root['updatedAtMs'], 'Cold resident changed contract/build/root')
    r = view['activationResident']
    need(set(r) == {'phase', 'lifetimeCommandId', 'residentProcess', 'processAlive', 'originalCustodyHeld', 'observation', 'reason'}
         and r['phase'] == 'not_resident' and r['lifetimeCommandId'] == lifetime['command']['commandId']
         and all(r[k] is None for k in ('residentProcess', 'processAlive', 'originalCustodyHeld', 'observation')),
         'Cold read invented custody/liveness/current connection')


def audit(proof, source, reference, checks):
    verify(proof, source, reference, checks)
    changes = [('before/schemaVersion', 21), ('after/schemaVersion', 22), ('after/ownerEpoch', 0),
        ('afterColdObservation/ownerEpoch', 0), ('afterColdObservation/lastSequence', 0), ('backupInfo/ownerEpoch', 0),
        ('gateRows', True), ('gateRows', 1), ('retainedActivation/revision', 6), ('retainedRoot/phase', 'committed'),
        ('retainedWriter/state/phase', 'issued'), ('retainedService/state/phase', 'issued'), ('retainedController/state/phase', 'issued'),
        ('retainedLifetime/state/phase', 'issued'), ('retainedLifetime/command/binding/keeper/birth', '0'),
        ('retainedController/state/receipt/session/sessionId', 1),
        ('retainedController/state/receipt/compatibility/identity/process/sessionId', '00000000-0000-0000-0000-000000000001'),
        ('rowsAfter', '0' * 64), ('backupRows', '0' * 64), ('oldTwentyTwoInitRefused', False), ('oldTwentyTwoStatusRefused', False),
        ('sourceBytesUnchanged', False), ('copyBytesUnchangedByOld', False), ('noFreshReplacementPermit', False),
        ('noFreshFinalizationIntent', False), ('oldLifetimeReceiverRefused', False), ('installedRuntimeApplied', True),
        ('coldResident/activationResident/phase', 'observed'), ('coldResident/activationResident/originalCustodyHeld', True),
        ('coldResident/activationResident/processAlive', True), ('coldResident/schemaVersion', 19), ('coldResident/driverActive', True),
        ('coldResident/update/phase', 'committed')]
    for path, replacement in changes:
        changed = copy.deepcopy(proof)
        node = changed
        parts = path.split('/')
        for key in parts[:-1]:
            node = node[key]
        need(checks.canonical(node[parts[-1]]) != checks.canonical(replacement), 'No-op migration mutation: ' + path)
        node[parts[-1]] = replacement
        try:
            verify(changed, source, reference, checks)
        except (ValueError, KeyError, TypeError):
            continue
        raise ValueError('Finalization migration observer accepted mutation: ' + path)
    return len(changes)
