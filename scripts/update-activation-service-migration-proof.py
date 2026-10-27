"""Updater19 ->20 copy-only migration evidence. Retained history is never a grant."""
import copy
import hashlib
import re


def need(ok, message):
    if not ok:
        raise ValueError(message)


def verify_selection(command, log):
    name = 'actual_nineteen_writer_history_survives_twenty_and_old_nineteen_refuses'
    need(command['executable'] == 'cargo' and type(command['exitCode']) is int and command['exitCode'] == 0
         and command['arguments'] == ['test', '-p', 'winbridge-update', '--features', 'bundled-sqlite',
               '--test', 'activation_service_migration', '--locked', '--offline', '--', '--exact', name,
               '--ignored', '--test-threads=1', '--nocapture'], 'Another migration command selected')
    text = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    summaries = re.findall(r'^test result: ([^\r\n]+)', text, re.MULTILINE)
    need(len(re.findall(r'^test ' + re.escape(name) + r' \.\.\.', text, re.MULTILINE)) == 1
         and len(summaries) == 1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;')
         and re.search(r'^running 1 test\s*$', text, re.MULTILINE), 'Migration did not run exactly once')


def derive_reference(pin, proof, canonical):
    """Only after the complete old Main24 proof and terminal task were verified."""
    case, = proof['cases']
    old = case['activationWriter']
    physical, writer = old['record'], old['writerRecord']
    need(physical['intent']['context']['workerJournalSchema'] == 17
         and physical['intent']['context']['updaterJournalSchema'] == 19
         and physical['revision'] == 6 and physical['state'] == {'phase': 'state_owned'}
         and writer['revision'] == 1 and writer['state']['phase'] == 'confirmed'
         and case['coldActivationWriter']['writerRecord'] == writer,
         'Actual confirmed19/17 writer reference required')
    replacement = case['replacement']
    images, manifest = replacement['actualInstalledFiles'], replacement['actualManifest']
    need(len(images) == 5 and len(manifest['artifacts']) == 4, 'Complete retained image set required')
    updater, = (a for a in manifest['artifacts'] if a['name'] == 'winbridge-update.exe')
    need(updater['descriptor']['role'] == 'updater'
         and updater['descriptor']['build']['sourceCommit'] == pin['target']['sourceCommit']
         and updater['descriptor']['build']['sourceManifestSha256'] == pin['target']['manifestSha256']
         and updater['descriptor']['build']['sourceDirty'] is False
         and updater['sha256'] == images[3]['sha256'], 'Retained19 image/source differs')
    sha = lambda value: hashlib.sha256(canonical(value)).hexdigest()
    return dict(schemaVersion=1, kind='activation_schema19_reference', referenceRun=pin['run'],
                reportSha256=pin['reportSha256'], proofSha256=pin['proofSha256'], source=pin['target'],
                image=images[3], descriptor=updater['descriptor'], manifestSha256=images[4]['sha256'],
                updateId=physical['intent']['source']['updateId'], activationSha256=sha(physical),
                rootSha256=sha(replacement['view']['update']), writerSha256=sha(writer))


def verify(proof, source, reference, checks):
    checks.strict(proof)
    def no_grants(value):
        if isinstance(value, dict):
            for key, child in value.items():
                need(key not in ('authorityId', 'authority_id', 'startupPermit', 'writeGrant',
                                 'serviceReady', 'canStartService'), 'Invented permission/private identity: ' + key)
                no_grants(child)
        elif isinstance(value, list):
            for child in value:
                no_grants(child)
    no_grants(proof)
    need(type(proof['schemaVersion']) is int and proof['schemaVersion'] == 1
         and proof['kind'] == 'activation_service_schema_migration_proof'
         and proof['passed'] is True and proof['platform'] == 'windows'
         and proof['build']['sourceCommit'] == source['sourceCommit']
         and proof['build']['sourceManifestSha256'] == source['manifestSha256']
         and proof['build']['sourceDirty'] is False
         and all(proof[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened')),
         'Wrong migration helper source/kind/isolation')
    need(proof['referenceProofSha256'] == reference['proofSha256']
         and proof['referenceReportSha256'] == reference['reportSha256']
         and proof['referenceImage'] == reference['image']
         and proof['referenceManifestSha256'] == reference['manifestSha256'], 'Another retained19 reference used')
    before, after = proof['before'], proof['after']
    need(type(before['schemaVersion']) is int and before['schemaVersion'] == 19
         and after == dict(before, schemaVersion=20, ownerEpoch=before['ownerEpoch'] + 1)
         and proof['afterColdObservation'] == dict(after, ownerEpoch=after['ownerEpoch'] + 1)
         and proof['backupInfo'] == before and before['activeUpdateId'] == reference['updateId'],
         'Migration lost root/dedup/sequence/installation or owner fencing')
    sha = lambda value: hashlib.sha256(checks.canonical(value)).hexdigest()
    physical, writer, root = (proof[k] for k in ('retainedActivation', 'retainedWriter', 'retainedRoot'))
    need(sha(physical) == reference['activationSha256'] and sha(writer) == reference['writerSha256']
         and sha(root) == reference['rootSha256']
         and physical['revision'] == 6 and physical['state'] == {'phase': 'state_owned'}
         and physical['intent']['source']['updateId'] == reference['updateId']
         and physical['intent']['source']['ownerEpoch'] == before['ownerEpoch']
         and writer['revision'] == 1 and writer['state']['phase'] == 'confirmed'
         and writer['command']['binding']['updateId'] == reference['updateId']
         and root['updateId'] == reference['updateId'] and root['phase'] == 'verifying_local',
         'Migration changed or relabelled the retained physical/writer/root prefix')
    need(type(proof['gateRows']) is int and proof['gateRows'] == 0
         and proof['rowsBefore'] == proof['rowsAfter'] == proof['backupRows']
         and re.fullmatch(r'[0-9a-f]{64}', proof['rowsBefore'])
         and all(proof[k] is True for k in ('oldNineteenInitRefused', 'oldNineteenStatusRefused',
                  'sourceBytesUnchanged', 'copyBytesUnchangedByOld', 'noFreshReplacementPermit',
                  'existingReplacementObserved', 'migrationBackupVerified', 'noFreshServiceIntent')),
         'Missing source/backup/old19 refusal or no-new-service evidence')


def audit(proof, source, reference, checks):
    verify(proof, source, reference, checks)
    changes = [
        ('before/schemaVersion', 18), ('after/schemaVersion', 19), ('after/ownerEpoch', 0),
        ('afterColdObservation/ownerEpoch', 0), ('afterColdObservation/lastSequence', 0),
        ('after/lastSequence', 0), ('backupInfo/ownerEpoch', 0), ('gateRows', True),
        ('gateRows', 1), ('retainedActivation/revision', 7), ('retainedRoot/phase', 'committed'),
        ('retainedWriter/state/phase', 'issued'), ('rowsAfter', '0' * 64),
        ('backupRows', '0' * 64), ('oldNineteenInitRefused', False),
        ('oldNineteenStatusRefused', False), ('sourceBytesUnchanged', False),
        ('copyBytesUnchangedByOld', False), ('noFreshReplacementPermit', False),
        ('noFreshServiceIntent', False), ('installedRuntimeApplied', True),
    ]
    for path, replacement in changes:
        changed = copy.deepcopy(proof)
        parts = path.split('/')
        node = changed
        for key in parts[:-1]:
            node = node[key]
        need(checks.canonical(node[parts[-1]]) != checks.canonical(replacement), 'No-op mutation: ' + path)
        node[parts[-1]] = replacement
        try:
            verify(changed, source, reference, checks)
        except (ValueError, KeyError, TypeError):
            continue
        raise ValueError('Migration observer accepted mutation: ' + path)
    return len(changes)
