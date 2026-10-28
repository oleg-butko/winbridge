"""Independent actual21 ->22 copy migration, bound to the archived Main26 proof."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]
NETWORK_SESSION_PATH = ('retainedController', 'state', 'receipt', 'session', 'sessionId')


def need(ok, message):
    if not ok:
        raise ValueError(message)


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def parse_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, 'Duplicate JSON key: ' + key)
            result[key] = value
        return result
    def invalid(value):
        raise ValueError('Non-integer JSON number: ' + value)
    return json.loads(raw, object_pairs_hook=pairs, parse_float=invalid, parse_constant=invalid)


def derive_reference(meta, report, proof, audit, checks):
    """Data derivation only; caller must run the complete Main26 reader first."""
    need(audit['passed'] is True and audit['kind'] == 'main26_independent_audit'
         and audit['profile'] == 'worker19-updater21' and audit['workerJournalSchema'] == 19
         and audit['updaterJournalSchema'] == 21 and audit['nativeMarkersVerified'] == 3
         and audit['installedWorkerUnchanged'] is True
         and audit['run'] == meta['run'] and audit['taskId'] == meta['taskId']
         and audit['source'] == meta['source'] and audit['target'] == meta['target']
         and audit['runtimeArchive'] == report['runtimeArchive'], 'Exact successful Main26 audit required')
    need(report['kind'] == 'activation_controller_main_native_gate' and report['passed'] is True
         and report['error'] is None and report['sourceCommit'] == meta['source']['sourceCommit']
         and report['sourceArchiveSha256'] == meta['source']['archiveSha256']
         and report['target'] == meta['target'], 'Another producer or Main26 target')
    case, = proof['cases']
    p, c = case['activationWriter'], case['activationController']
    physical, writer, service = (p[key] for key in ('record', 'writerRecord', 'serviceRecord'))
    controller, root = c['record'], c['confirmed']['update']
    need(physical == c['physicalRecord'] and physical['revision'] == 7
         and physical['state'] == {'phase': 'local_verified'}
         and physical['intent']['context']['updaterJournalSchema'] == 21
         and physical['intent']['context']['workerJournalSchema'] == 19
         and writer['state']['phase'] == service['state']['phase'] == controller['state']['phase'] == 'confirmed'
         and controller['command']['binding']['serviceCommand'] == service['command']
         and controller['command']['binding']['serviceReceipt'] == service['state']['receipt']
         and physical['local'] == service['state']['receipt']['compatibility']
         and root == p['confirmed']['update'] == case['coldActivationController']['view']['update']
         and root['phase'] == 'awaiting_controller' and c['rootCommitted'] is False
         and c['maintenanceReleased'] is False, 'Actual21 local/controller/root prefix required')
    windows = checks.windows_path
    archive = windows(report['runtimeArchive']['directory'])
    need(archive.is_absolute() and archive.drive.lower() == 'k:'
         and archive.parent == windows(meta['workingDirectory']) / 'shutdown-main-cases'
         and archive.name.startswith('runtime-archive-'), 'Archive outside exact Main26 fixture')
    suffix = archive.name.removeprefix('runtime-archive-')
    need(str(uuid.UUID(suffix)) == suffix, 'Canonical runtime archive ID required')
    images, manifest = case['replacement']['actualInstalledFiles'], case['replacement']['actualManifest']
    need(len(images) == 5 and windows(images[3]['path']).parent == windows(meta['testRuntimeDirectory']), 'Old21 image/runtime differs')
    artifact, = (a for a in manifest['artifacts'] if a['name'] == 'winbridge-update.exe')
    need(artifact['sha256'] == images[3]['sha256']
         and artifact['descriptor']['build']['sourceCommit'] == meta['target']['sourceCommit']
         and artifact['descriptor']['build']['sourceManifestSha256'] == meta['target']['manifestSha256'], 'Old21 artifact source differs')
    image = dict(images[3], path='\\\\?\\' + str(archive / 'winbridge-update.exe'))
    sha = lambda value: hashlib.sha256(checks.canonical(value)).hexdigest()
    reference = dict(schemaVersion=1, kind='activation_schema21_reference', referenceRun=meta['run'],
        reportSha256=audit['reportSha256'], proofSha256=audit['proofSha256'], source=meta['target'],
        image=image, descriptor=artifact['descriptor'], manifestSha256=images[4]['sha256'],
        updateId=physical['intent']['source']['updateId'], activationSha256=sha(physical),
        rootSha256=sha(root), writerSha256=sha(writer), serviceSha256=sha(service), controllerSha256=sha(controller))
    return reference, dict(originalImage=images[3], archivedImage=image, archive=report['runtimeArchive'])


def reference_from_main26(run_info, source_archive, target_archive):
    result = subprocess.run([sys.executable, str(ROOT / 'scripts/verify-update-controller-main-native.py'),
        '--profile', 'worker19-updater21', '--run-info', str(run_info), '--source-archive', str(source_archive),
        '--target-archive', str(target_archive)], check=True, capture_output=True, text=True)
    audit, meta = parse_json(result.stdout), parse_json(run_info.read_bytes())
    reader = load('lifetime_reference_files', 'summarize-update-timings.py')
    checks = load('lifetime_reference_json', 'update-activation-preparation-proof.py')
    files = reader.verified_files(Path(meta['reportDirectory']))
    need(files == reader.verified_files(Path(meta['reportDirectory'])), 'Main26 reference changed')
    need(hashlib.sha256(files['report.json']).hexdigest() == audit['reportSha256']
         and hashlib.sha256(files['candidate-health-actor-proof.json']).hexdigest() == audit['proofSha256'], 'Main26 report/proof SHA changed')
    report, proof = (parse_json(files[name].decode('utf-8-sig')) for name in ('report.json', 'candidate-health-actor-proof.json'))
    reference, location = derive_reference(meta, report, proof, audit, checks)
    return reference, location, audit, meta


def verify_selection(command, log):
    name = 'actual_twenty_one_controller_history_survives_twenty_two_and_old_twenty_one_refuses'
    need(command['executable'] == 'cargo' and type(command['exitCode']) is int and command['exitCode'] == 0
         and command['arguments'] == ['test', '-p', 'winbridge-update', '--features', 'bundled-sqlite', '--test',
            'activation_lifetime_migration', '--locked', '--offline', '--', '--exact', name, '--ignored', '--test-threads=1', '--nocapture'],
         'Another migration command selected')
    text = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    summaries = re.findall(r'^test result: ([^\r\n]+)', text, re.MULTILINE)
    need(len(re.findall(r'^test ' + re.escape(name) + r' \.\.\.', text, re.MULTILINE)) == 1
         and len(summaries) == 1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;')
         and re.search(r'^running 1 test\s*$', text, re.MULTILINE), 'Actual21 migration did not run once')


def verify(proof, source, reference, checks):
    # Only this exact controller DTO leaf is a UUID. All OS process sessionId
    # counters keep the original numeric validation; no records are rewritten.
    checks.strict(proof, uuid_session_paths=frozenset({NETWORK_SESSION_PATH}))
    load('lifetime_migration_strict', 'update-activation-service-proof.py').strict(proof)
    def no_grants(value):
        if isinstance(value, dict):
            for key, child in value.items():
                need(key not in ('serviceReady', 'canStartService', 'authority_id', 'lifetimeReady',
                     'canCommit', 'canLaunch', 'keeperResident', 'releaseMaintenance', 'ownerHandle'), 'Invented lifetime permission: ' + key)
                no_grants(child)
        elif isinstance(value, list):
            for child in value:
                no_grants(child)
    no_grants(proof)
    need(type(proof['schemaVersion']) is int and proof['schemaVersion'] == 1
         and proof['kind'] == 'activation_lifetime_schema_migration_proof' and proof['passed'] is True
         and proof['platform'] == 'windows' and proof['build']['sourceCommit'] == source['sourceCommit']
         and proof['build']['sourceManifestSha256'] == source['manifestSha256'] and proof['build']['sourceDirty'] is False
         and all(proof[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened')), 'Wrong actual22 producer/isolation')
    need(proof['referenceProofSha256'] == reference['proofSha256'] and proof['referenceReportSha256'] == reference['reportSha256']
         and proof['referenceImage'] == reference['image'] and proof['referenceManifestSha256'] == reference['manifestSha256'], 'Another old21 archive/reference used')
    before, after = proof['before'], proof['after']
    need(type(before['schemaVersion']) is int and before['schemaVersion'] == 21
         and after == dict(before, schemaVersion=22, ownerEpoch=before['ownerEpoch'] + 1)
         and proof['afterColdObservation'] == dict(after, ownerEpoch=after['ownerEpoch'] + 1)
         and proof['backupInfo'] == before and before['activeUpdateId'] == reference['updateId'], 'Migration changed root/sequence/installation/fencing')
    physical, writer, service, controller, root = (proof[k] for k in ('retainedActivation', 'retainedWriter', 'retainedService', 'retainedController', 'retainedRoot'))
    sha = lambda value: hashlib.sha256(checks.canonical(value)).hexdigest()
    need(all(sha(value) == reference[key] for value, key in ((physical, 'activationSha256'), (writer, 'writerSha256'),
             (service, 'serviceSha256'), (controller, 'controllerSha256'), (root, 'rootSha256')))
         and physical['revision'] == 7 and physical['state'] == {'phase': 'local_verified'}
         and physical['intent']['source']['updateId'] == reference['updateId']
         and physical['intent']['source']['ownerEpoch'] == before['ownerEpoch']
         and physical['intent']['context']['updaterJournalSchema'] == 21
         and physical['intent']['context']['workerJournalSchema'] == 19
         and before['installationId'] == physical['intent']['source']['installationId'] == root['request']['expected']['installationId']
         and before['scopeSha256'] == physical['intent']['source']['scopeSha256'] == root['request']['expected']['scopeSha256']
         and before['lastSequence'] >= root['revision'] + 1
         and writer['revision'] == 1 and writer['state']['phase'] == service['state']['phase'] == controller['state']['phase'] == 'confirmed'
         and service['command']['binding']['writerCommand'] == writer['command']
         and controller['command']['binding']['serviceCommand'] == service['command']
         and controller['command']['binding']['serviceReceipt'] == service['state']['receipt']
         and physical['local'] == service['state']['receipt']['compatibility']
         and root['updateId'] == reference['updateId'] and root['phase'] == 'awaiting_controller', 'Retained controller/local/service/writer/root changed or relabelled')
    need(type(proof['gateRows']) is int and proof['gateRows'] == 0
         and proof['rowsBefore'] == proof['rowsAfter'] == proof['backupRows']
         and re.fullmatch(r'[0-9a-f]{64}', proof['rowsBefore'])
         and all(proof[k] is True for k in ('oldTwentyOneInitRefused', 'oldTwentyOneStatusRefused', 'sourceBytesUnchanged',
             'copyBytesUnchangedByOld', 'noFreshReplacementPermit', 'existingReplacementObserved', 'migrationBackupVerified', 'noFreshLifetimeIntent', 'oldControllerReceiverRefused')),
         'Missing actual21 retention/backup/refusal/no-new-lifetime evidence')


def audit(proof, source, reference, checks):
    verify(proof, source, reference, checks)
    changes = [('before/schemaVersion', 20), ('after/schemaVersion', 21), ('after/ownerEpoch', 0),
        ('afterColdObservation/ownerEpoch', 0), ('afterColdObservation/lastSequence', 0), ('backupInfo/ownerEpoch', 0),
        ('gateRows', True), ('gateRows', 1), ('retainedActivation/revision', 6), ('retainedRoot/phase', 'committed'),
        ('retainedWriter/state/phase', 'issued'), ('retainedService/state/phase', 'issued'),
        ('retainedController/state/phase', 'issued'), ('retainedController/revision', 0),
        ('retainedController/state/receipt/session/sessionId', 1),
        ('retainedController/state/receipt/compatibility/identity/process/sessionId', '00000000-0000-0000-0000-000000000001'),
        ('rowsAfter', '0' * 64), ('backupRows', '0' * 64), ('oldTwentyOneInitRefused', False),
        ('oldTwentyOneStatusRefused', False), ('sourceBytesUnchanged', False), ('copyBytesUnchangedByOld', False),
        ('noFreshReplacementPermit', False), ('noFreshLifetimeIntent', False), ('oldControllerReceiverRefused', False), ('installedRuntimeApplied', True)]
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
        raise ValueError('Lifetime migration observer accepted mutation: ' + path)
    return len(changes)
