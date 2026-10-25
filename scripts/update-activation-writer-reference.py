"""Retained Main23 evidence -> separate updater18 and worker16 data references."""
import hashlib
import json
from pathlib import PureWindowsPath
import re


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def derive(pin, proof, checks):
    """Call only after independently validating the complete old physical proof."""
    case, = proof['cases']
    p, replaced, before = case['activationState'], case['replacement'], case['beforeReplacement']
    record = p['record']
    manifest = replaced['actualManifest']
    images = replaced['actualInstalledFiles']
    need(record['intent']['context']['workerJournalSchema'] == 16
         and record['intent']['context']['updaterJournalSchema'] == 18
         and record['revision'] == 6 and record['state'] == {'phase': 'state_owned'}
         and len(images) == 5, 'Actual old18/16 StateOwned reference required')
    descriptors = [a['descriptor'] for a in manifest['artifacts']]
    for index, role in ((0, 'worker'), (1, 'fixture'), (3, 'updater')):
        d, image = descriptors[index], images[index]
        need(d['role'] == role and d['build']['sourceCommit'] == pin['target']['sourceCommit']
             and d['build']['sourceManifestSha256'] == pin['target']['manifestSha256']
             and d['build']['sourceDirty'] is False and image['sha256'] == manifest['artifacts'][index]['sha256'],
             'Retained installed target image/role/source differs')
    common = dict(schemaVersion=1, referenceRun=pin['run'], reportSha256=pin['reportSha256'],
                  proofSha256=pin['proofSha256'], source=pin['target'])
    sha = lambda value: hashlib.sha256(checks.canonical(value)).hexdigest()
    updater = dict(common, kind='activation_schema18_reference', image=images[3], descriptor=descriptors[3],
                   manifestSha256=images[4]['sha256'], updateId=record['intent']['source']['updateId'],
                   activationSha256=sha(record), rootSha256=sha(replaced['view']['update']))
    worker = dict(common, kind='journal_schema16_reference', image=images[0], descriptor=descriptors[0],
                  fixtureImage=images[1], fixtureDescriptor=descriptors[1], manifestFile=images[4],
                  state=record['intent']['context']['workerState'],
                  authority=before['finalStateBackup']['record']['report']['sourceAuthority'],
                  acceptedShutdown=before['shutdown']['delivery']['receipt'],
                  workerBytes=p['workerBytesAfter'])
    need(worker['acceptedShutdown']['state'] == worker['state']
         and worker['acceptedShutdown']['lease']['phase'] == 'quiesced'
         and p['workerBytesBefore'] == p['workerBytesAfter'], 'Old active16 history/state/bytes differ')
    return dict(schemaVersion=1, kind='journal_writer_migration_reference', referenceRun=pin['run'],
                reportSha256=pin['reportSha256'], proofSha256=pin['proofSha256'], updater=updater, worker=worker)


def from_pin(pin, reports, workspace, legacy):
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', pin['run']) and pin['passed'] is True
         and pin.get('activationState') is True and pin['nativeTestsPassed'] >= 132,
         'Successful terminal Main23 pin required')
    files = legacy.support.ready_files(reports / pin['run'])
    need(files == legacy.support.ready_files(reports / pin['run']), 'Old ready files changed')
    for file, field in (('report.json', 'reportSha256'), ('candidate-health-actor-proof.json', 'proofSha256')):
        need(hashlib.sha256(files[file]).hexdigest() == pin[field], 'Old reference SHA differs: ' + file)
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    need(report['kind'] == 'activation_state_main_native_gate' and report['passed'] is True
         and report['error'] is None and report['sourceCommit'] == pin['source']['sourceCommit']
         and report['sourceArchiveSha256'] == pin['source']['archiveSha256'] and report['target'] == pin['target'],
         'Reference is not the pinned actual18 physical report')
    proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
    root = str(PureWindowsPath(workspace) / 'workflow-validation' / pin['run'])
    legacy.state_checks.verify(proof, pin['source'], pin['target'], root, legacy.checks, legacy.health)
    return derive(pin, proof, legacy.checks)
