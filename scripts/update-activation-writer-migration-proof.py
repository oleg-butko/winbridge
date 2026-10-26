"""Independent updater18/worker16 migration observations. Never a startup grant."""
import copy
import hashlib
import re
import uuid
from pathlib import PureWindowsPath


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def build(proof, source, kind):
    b = proof['build']
    need(proof['schemaVersion'] == 1 and proof['kind'] == kind and proof['passed'] is True
         and proof['platform'] == 'windows' and b['sourceCommit'] == source['sourceCommit']
         and b['sourceManifestSha256'] == source['manifestSha256'] and b['sourceDirty'] is False
         and all(proof[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened')),
         'Actual migration helper source/kind/isolation differs')


def verify_updater(proof, source, reference, checks, physical):
    physical.strict(proof, checks)
    build(proof, source, 'activation_writer_schema_migration_proof')
    r = reference['updater']
    before, after = proof['before'], proof['after']
    need(proof['referenceProofSha256'] == r['proofSha256'] and proof['referenceReportSha256'] == r['reportSha256']
         and proof['referenceImage'] == r['image'] and proof['referenceManifestSha256'] == r['manifestSha256'],
         'Updater18 migration used another retained image/history')
    need(before['schemaVersion'] == 18 and after == dict(before, schemaVersion=19, ownerEpoch=before['ownerEpoch']+1)
         and proof['backupInfo'] == before and before['activeUpdateId'] == r['updateId'],
         'Updater migration lost installation/root/sequence or owner fencing')
    activation = proof['retainedActivation']
    root = proof['retainedRoot']
    sha = lambda value: hashlib.sha256(checks.canonical(value)).hexdigest()
    need(sha(activation) == r['activationSha256'] and sha(root) == r['rootSha256']
         and activation['revision'] == 6 and activation['state'] == {'phase': 'state_owned'}
         and activation['intent']['source']['updateId'] == r['updateId']
         and activation['intent']['source']['ownerEpoch'] == before['ownerEpoch']
         and root['updateId'] == r['updateId'] and root['phase'] == 'verifying_local',
         'Retained physical18 prefix/root was rewritten or relabelled as activated')
    need(type(proof['gateRows']) is int and proof['gateRows'] == 0
         and proof['rowsBefore'] == proof['rowsAfter'] == proof['backupRows']
         and re.fullmatch(r'[0-9a-f]{64}', proof['rowsBefore'])
         and all(proof[k] is True for k in ('oldEighteenInitRefused', 'oldEighteenStatusRefused',
                  'sourceBytesUnchanged', 'copyBytesUnchangedByOld', 'noFreshReplacementPermit', 'existingReplacementObserved', 'migrationBackupVerified')),
         'Updater18 data, backup, refusal or no-new-writer evidence missing')


def verify_worker(proof, source, reference, checks, physical, root):
    # OutputChunk.bytes is a byte vector; artifact.bytes elsewhere is a counter.
    # Validate the vector separately without allowing unknown output fields.
    chunks = proof['outputChunks']
    physical.strict({k: v for k, v in proof.items() if k != 'outputChunks'}, checks)
    build(proof, source, 'worker_journal_activation_migration_proof')
    r = reference['worker']
    need(proof['referenceRun'] == r['referenceRun'] and proof['referenceProofSha256'] == r['proofSha256']
         and proof['referenceReportSha256'] == r['reportSha256'] and proof['referenceImage'] == r['image']
         and proof['referenceDescriptor'] == r['descriptor'], 'Worker16 retained image/history differs')
    need(type(proof['fromSchema']) is int and proof['fromSchema'] == 16
         and type(proof['toSchema']) is int and proof['toSchema'] == 17
         and proof['authorityBefore'] == proof['authorityAfter']
         and proof['rowsBefore'] == proof['rowsAfter'] == proof['backupRows']
         and re.fullmatch(r'[0-9a-f]{64}', proof['rowsBefore']), 'Inactive worker migration changed old data/authority')
    need(type(proof['activeSchemaAfter']) is int and proof['activeSchemaAfter'] == 16
         and proof['activeRowsBefore'] == proof['activeRowsAfter']
         and proof['activeAuthority'] == r['authority'] and proof['activeAcceptedShutdown'] == r['acceptedShutdown']
         and re.fullmatch(r'[0-9a-f]{64}', proof['activeRowsBefore'])
         and isinstance(proof['activeRefusal'], str) and 'unfinished' in proof['activeRefusal'].lower()
         and all(proof[k] is True for k in ('oldSixteenRefusedSeventeen', 'activeSixteenRefusedBeforeWrites',
                  'migrationBackupVerified', 'oldWorkerJobEmpty', 'sourceBytesUnchanged', 'noActivationInvented')),
         'Active worker16 migrated/wrote or old16 accepted17')
    for stamp in (proof['authorityBefore'], proof['activeAuthority']):
        need(set(stamp) == {'incarnation', 'epoch', 'fingerprintSha256'}
             and str(uuid.UUID(stamp['incarnation'])) == stamp['incarnation']
             and type(stamp['epoch']) is int and 0 < stamp['epoch'] <= 9007199254740991
             and re.fullmatch(r'[0-9a-f]{64}', stamp['fingerprintSha256']), 'Invalid public worker authority stamp')
    for name, old, file in (('copiedOldImage', r['image'], 'winbridge-worker.exe'),
                             ('copiedFixtureImage', r['fixtureImage'], 'winbridge-fixture.exe')):
        image = proof[name]
        need(image == dict(old, path=image['path'], fileIdentity=image['fileIdentity'])
             and image['fileIdentity'] != old['fileIdentity']
             and checks.windows_path(image['path']) == PureWindowsPath(root)/'worker-journal-migration-case'/'old16-bin'/file,
             'Copied old16 image generation/identity/path differs')
    task = proof['task']
    need(task['status'] == 'succeeded' and type(task['exitCode']) is int and task['exitCode'] == 0
         and str(uuid.UUID(task['taskId'])) == task['taskId'] == proof['duplicateTaskId']
         and task['submission']['projectId'] == 'source' and task['submission']['idempotencyKey'] == 'old16-history'
         and task['submission']['command']['kind'] == 'direct'
         and task['submission']['command']['arguments'] == ['success']
         and checks.windows_path(task['submission']['command']['executable']) == checks.windows_path(proof['copiedFixtureImage']['path']),
         'Actual old16 task/dedup or executable differs')
    process = proof['oldProcess']
    need(process['processId'] > 0 and 0 < process['creationTime'] <= 2**64-1
         and isinstance(process['jobName'], str) and 'WinBridge-worker-migration-' in process['jobName'],
         'Old16 native process/Job observation missing')
    need(isinstance(chunks, list) and 0 < len(chunks) <= 256, 'Bounded actual output chunks required')
    output = {'stdout': bytearray(), 'stderr': bytearray()}
    sequence = 0
    for c in chunks:
        need(set(c) == {'sequence', 'stream', 'offset', 'bytes'} and type(c['sequence']) is int and c['sequence'] > sequence
             and isinstance(c['stream'], str) and c['stream'] in output and type(c['offset']) is int and c['offset'] == len(output[c['stream']])
             and isinstance(c['bytes'], list) and len(c['bytes']) <= 65536
             and all(type(b) is int and 0 <= b <= 255 for b in c['bytes']), 'Invalid actual output sequence/offset/bytes')
        sequence = c['sequence']
        output[c['stream']].extend(c['bytes'])
    need('success' in output['stdout'].decode('utf-8') and output['stderr'], 'Old16 successful fixture stdout/stderr missing')


def audit(updater, worker, source, reference, checks, physical, root):
    def verify(a, b):
        verify_updater(a, source, reference, checks, physical)
        verify_worker(b, source, reference, checks, physical, root)
    verify(updater, worker)
    changes = [
        ('updater/after/ownerEpoch', 0), ('updater/before/schemaVersion', 17),
        ('updater/retainedActivation/revision', 7), ('updater/retainedRoot/phase', 'committed'),
        ('updater/gateRows', 1), ('updater/backupRows', '0'*64),
        ('updater/oldEighteenInitRefused', False), ('updater/sourceBytesUnchanged', False),
        ('worker/authorityAfter/epoch', 0), ('worker/activeSchemaAfter', 17),
        ('worker/activeRowsAfter', '0'*64), ('worker/oldSixteenRefusedSeventeen', False),
        ('worker/noActivationInvented', False), ('worker/oldWorkerJobEmpty', False),
        ('worker/duplicateTaskId', str(uuid.uuid4())), ('worker/outputChunks/0/bytes', [True]),
        ('worker/copiedOldImage/sha256', '0'*64), ('worker/fromSchema', True),
    ]
    for path, value in changes:
        objects = {'updater': copy.deepcopy(updater), 'worker': copy.deepcopy(worker)}
        node = objects
        parts = path.split('/')
        for key in parts[:-1]:
            node = node[int(key)] if isinstance(node, list) else node[key]
        key = int(parts[-1]) if isinstance(node, list) else parts[-1]
        need(checks.canonical(node[key]) != checks.canonical(value), 'Migration mutation is a no-op: '+path)
        node[key] = value
        try:
            verify(objects['updater'], objects['worker'])
        except (ValueError, KeyError, TypeError, IndexError, OverflowError):
            continue
        raise ValueError('Migration observer accepted mutation: '+path)
    return len(changes)
