"""Independent actual20 ->21 copy migration and archived Main25 references."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def need(ok, message):
    if not ok:
        raise ValueError(message)


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def reference_from_main25(run_info, source_archive, target_archive):
    """Full readonly Main25 audit first, then relocate only its archived image path."""
    result = subprocess.run([sys.executable, str(ROOT / 'scripts/verify-update-service-main-native.py'),
        '--profile', 'worker19-updater20', '--run-info', str(run_info), '--source-archive', str(source_archive),
        '--target-archive', str(target_archive)], check=True, capture_output=True, text=True)
    audit = json.loads(result.stdout)
    meta = json.loads(run_info.read_text())
    need(audit['passed'] is True and audit['run'] == meta['run'] and audit['taskId'] == meta['taskId']
         and audit['nativeMarkersVerified'] == 3, 'Successful exact Main25 reference required')
    reader = load('controller_reference_files', 'summarize-update-timings.py')
    checks = load('controller_reference_json', 'update-activation-preparation-proof.py')
    files = reader.verified_files(Path(meta['reportDirectory']))
    need(files == reader.verified_files(Path(meta['reportDirectory'])), 'Old reference changed')
    need(hashlib.sha256(files['report.json']).hexdigest() == audit['reportSha256']
         and hashlib.sha256(files['candidate-health-actor-proof.json']).hexdigest() == audit['proofSha256'], 'Old reference SHA changed')
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
    case, = proof['cases']
    p = case['activationWriter']
    physical, writer, service = (p[key] for key in ('record', 'writerRecord', 'serviceRecord'))
    root = p['confirmed']['update']
    need(physical['revision'] == 7 and physical['state'] == {'phase': 'local_verified'}
         and physical['intent']['context']['updaterJournalSchema'] == 20
         and writer['state']['phase'] == service['state']['phase'] == 'confirmed'
         and physical['local'] == service['state']['receipt']['compatibility']
         and root == case['coldActivationWriter']['view']['update'] and root['phase'] == 'awaiting_controller',
         'Actual20 LocalVerified/service/root reference required')
    windows = checks.windows_path
    archive = windows(report['runtimeArchive']['directory'])
    need(archive.is_absolute() and archive.drive.lower() == 'k:'
         and archive.parent == windows(meta['workingDirectory']) / 'shutdown-main-cases'
         and re.fullmatch(r'runtime-archive-[0-9a-f-]{36}', archive.name), 'Archive outside the exact old fixture')
    images, manifest = case['replacement']['actualInstalledFiles'], case['replacement']['actualManifest']
    need(len(images) == 5 and windows(images[3]['path']).parent == windows(meta['testRuntimeDirectory']), 'Old image/runtime differs')
    artifact, = (a for a in manifest['artifacts'] if a['name'] == 'winbridge-update.exe')
    need(artifact['sha256'] == images[3]['sha256'] and artifact['descriptor']['build']['sourceCommit'] == meta['target']['sourceCommit']
         and artifact['descriptor']['build']['sourceManifestSha256'] == meta['target']['manifestSha256'], 'Old20 artifact source differs')
    image = dict(images[3], path='\\\\?\\' + str(archive / 'winbridge-update.exe'))
    sha = lambda value: hashlib.sha256(checks.canonical(value)).hexdigest()
    reference = dict(schemaVersion=1, kind='activation_schema20_reference', referenceRun=meta['run'],
        reportSha256=audit['reportSha256'], proofSha256=audit['proofSha256'], source=meta['target'],
        image=image, descriptor=artifact['descriptor'], manifestSha256=images[4]['sha256'],
        updateId=physical['intent']['source']['updateId'], activationSha256=sha(physical),
        rootSha256=sha(root), writerSha256=sha(writer), serviceSha256=sha(service))
    location = dict(originalImage=images[3], archivedImage=image, archive=report['runtimeArchive'])
    return reference, location, audit, meta


def verify_selection(command, log):
    name = 'actual_twenty_local_service_history_survives_twenty_one_and_old_twenty_refuses'
    need(command['executable'] == 'cargo' and type(command['exitCode']) is int and command['exitCode'] == 0
         and command['arguments'] == ['test', '-p', 'winbridge-update', '--features', 'bundled-sqlite', '--test',
            'activation_controller_migration', '--locked', '--offline', '--', '--exact', name, '--ignored', '--test-threads=1', '--nocapture'],
         'Another migration command selected')
    text = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    summaries = re.findall(r'^test result: ([^\r\n]+)', text, re.MULTILINE)
    need(len(re.findall(r'^test ' + re.escape(name) + r' \.\.\.', text, re.MULTILINE)) == 1
         and len(summaries) == 1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;')
         and re.search(r'^running 1 test\s*$', text, re.MULTILINE), 'Actual20 migration did not run once')


def verify(proof, source, reference, checks):
    checks.strict(proof)
    load('controller_migration_strict', 'update-activation-service-proof.py').strict(proof)
    def no_grants(value):
        if isinstance(value, dict):
            for key, child in value.items():
                need(key not in ('serviceReady', 'canStartService', 'authority_id'), 'Invented controller/service permission: ' + key)
                no_grants(child)
        elif isinstance(value, list):
            for child in value:
                no_grants(child)
    no_grants(proof)
    need(type(proof['schemaVersion']) is int and proof['schemaVersion'] == 1
         and proof['kind'] == 'activation_controller_schema_migration_proof' and proof['passed'] is True
         and proof['platform'] == 'windows' and proof['build']['sourceCommit'] == source['sourceCommit']
         and proof['build']['sourceManifestSha256'] == source['manifestSha256'] and proof['build']['sourceDirty'] is False
         and all(proof[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened')), 'Wrong actual21 producer/isolation')
    need(proof['referenceProofSha256'] == reference['proofSha256'] and proof['referenceReportSha256'] == reference['reportSha256']
         and proof['referenceImage'] == reference['image'] and proof['referenceManifestSha256'] == reference['manifestSha256'], 'Another old20 archive/reference used')
    before, after = proof['before'], proof['after']
    need(type(before['schemaVersion']) is int and before['schemaVersion'] == 20
         and after == dict(before, schemaVersion=21, ownerEpoch=before['ownerEpoch'] + 1)
         and proof['afterColdObservation'] == dict(after, ownerEpoch=after['ownerEpoch'] + 1)
         and proof['backupInfo'] == before and before['activeUpdateId'] == reference['updateId'], 'Migration changed root/sequence/installation/fencing')
    physical, writer, service, root = (proof[k] for k in ('retainedActivation', 'retainedWriter', 'retainedService', 'retainedRoot'))
    sha = lambda value: hashlib.sha256(checks.canonical(value)).hexdigest()
    need(all(sha(value) == reference[key] for value, key in ((physical, 'activationSha256'), (writer, 'writerSha256'),
             (service, 'serviceSha256'), (root, 'rootSha256')))
         and physical['revision'] == 7 and physical['state'] == {'phase': 'local_verified'}
         and physical['intent']['source']['updateId'] == reference['updateId']
         and physical['intent']['source']['ownerEpoch'] == before['ownerEpoch']
         and physical['intent']['context']['updaterJournalSchema'] == 20
         and physical['intent']['context']['workerJournalSchema'] == 19
         and before['installationId'] == physical['intent']['source']['installationId'] == root['request']['expected']['installationId']
         and before['scopeSha256'] == physical['intent']['source']['scopeSha256'] == root['request']['expected']['scopeSha256']
         and before['lastSequence'] >= root['revision'] + 1
         and writer['revision'] == 1 and writer['state']['phase'] == 'confirmed'
         and service['state']['phase'] == 'confirmed' and service['command']['binding']['writerCommand'] == writer['command']
         and physical['local'] == service['state']['receipt']['compatibility']
         and root['updateId'] == reference['updateId'] and root['phase'] == 'awaiting_controller', 'Retained local/service/writer/root changed or relabelled')
    need(type(proof['gateRows']) is int and proof['gateRows'] == 0
         and proof['rowsBefore'] == proof['rowsAfter'] == proof['backupRows']
         and re.fullmatch(r'[0-9a-f]{64}', proof['rowsBefore'])
         and all(proof[k] is True for k in ('oldTwentyInitRefused', 'oldTwentyStatusRefused', 'sourceBytesUnchanged',
             'copyBytesUnchangedByOld', 'noFreshReplacementPermit', 'existingReplacementObserved', 'migrationBackupVerified', 'noFreshControllerIntent')),
         'Missing actual20 retention/backup/refusal/no-new-controller evidence')


def audit(proof, source, reference, checks):
    verify(proof, source, reference, checks)
    changes = [('before/schemaVersion', 19), ('after/schemaVersion', 20), ('after/ownerEpoch', 0),
        ('afterColdObservation/ownerEpoch', 0), ('afterColdObservation/lastSequence', 0), ('backupInfo/ownerEpoch', 0),
        ('gateRows', True), ('gateRows', 1), ('retainedActivation/revision', 6), ('retainedRoot/phase', 'committed'),
        ('retainedWriter/state/phase', 'issued'), ('retainedService/state/phase', 'issued'), ('rowsAfter', '0' * 64),
        ('backupRows', '0' * 64), ('oldTwentyInitRefused', False), ('oldTwentyStatusRefused', False),
        ('sourceBytesUnchanged', False), ('copyBytesUnchangedByOld', False), ('noFreshReplacementPermit', False),
        ('noFreshControllerIntent', False), ('installedRuntimeApplied', True)]
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
        raise ValueError('Controller migration observer accepted mutation: ' + path)
    return len(changes)
