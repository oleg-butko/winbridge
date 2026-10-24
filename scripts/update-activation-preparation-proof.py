"""Independent preparation20/ledger18 verifier; these facts grant no execution rights."""
import copy
import hashlib
import json
from pathlib import PureWindowsPath
import re
import uuid


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode()


def strict(value):
    if isinstance(value, dict):
        counters = {'schemaVersion', 'contractVersion', 'queryContract', 'revision', 'ownerEpoch',
                    'parentRevision', 'expectedRevision', 'processId', 'sessionId', 'bytes',
                    'workerConfigSchema', 'workerJournalSchema', 'updaterJournalSchema', 'protocolVersion',
                    'startupTimeoutMs', 'workerLaunchCount', 'stateHandoffCount', 'gateRows', 'lastSequence','heldFiles'}
        for key, item in value.items():
            need(key not in ('nonce', 'channelNonce', 'token', 'permit', 'canSpawn', 'canInstall'),
                 'Private material or fabricated permission: '+key)
            if key in ('originalStateHeld','physicalCustodyHeld','readOnly','driverActive','querySupported','sourceDirty'):
                need(type(item) is bool,'Invalid boolean observation: '+key)
            if key in counters or key.endswith('AtMs'):
                need(item is None or type(item) is int and 0 <= item <= 9007199254740991,
                     'Invalid counter: '+key)
            strict(item)
    elif isinstance(value, list):
        for item in value:
            strict(item)
    elif type(value) is float:
        raise ValueError('Floating evidence is not a typed contract counter')


def windows_path(value):
    return PureWindowsPath(value[4:] if value.startswith('\\\\?\\') else value)


def verify(proof, source, target, root, health):
    strict(proof)
    need(proof['kind'] == 'activation_preparation_main_native_proof', 'Wrong preparation kind')
    health.verify_runtime_replacement_proof(dict(proof, kind='runtime_replacement_main_native_proof'), source, target)
    case, = proof['cases']
    before, replaced = case['beforeReplacement'], case['replacement']
    saved, cold = case['activationPreparation'], case['coldActivationPreparation']
    r, sup, h = saved['record'], saved['supervisor'], saved['health']
    intent, ctx, binding = r['intent'], r['intent']['context'], before['shutdown']['intent']['binding']
    replacement = replaced['record']
    plan = replacement['intent']['preparation']['plan']
    need(windows_path(plan['runtimeDirectory']) == PureWindowsPath(root)/'shutdown-main-cases'/'None'/'runtime',
         'Preparation reused another isolated run')
    expected_source = dict(schemaVersion=1, updateId=plan['updateId'],
                           replacementAttemptId=replacement['intent']['attemptId'], replacementRevision=22,
                           replacementSha256=hashlib.sha256(b'winbridge.activation.replacement.v1\n'+canonical(replacement)).hexdigest(),
                           ownerId=plan['ownerId'], ownerEpoch=plan['ownerEpoch'], launchId=plan['launchId'],
                           parentRevision=plan['parentRevision']+1, installationId=plan['manifest']['installationId'],
                           scopeSha256=plan['manifest']['scopeSha256'], setId=plan['manifest']['setId'],
                           runtimeDirectory=plan['runtimeDirectory'], runtimeIdentity=plan['runtimeIdentity'],
                           updaterConfigSha256=plan['configSha256'], shutdownSha256=plan['proofs']['shutdownSha256'],
                           backupSha256=plan['proofs']['backupSha256'], manifest=replaced['actualInstalledFiles'][4],
                           worker=replaced['actualInstalledFiles'][0], workerDescriptor=plan['manifest']['artifacts'][0]['descriptor'])
    need(intent['source'] == expected_source, 'Source differs from exact completed replacement')
    need(json.loads(saved['healthEncoded']) == h
         and hashlib.sha256(saved['healthEncoded'].encode()).hexdigest() == plan['proofs']['healthSha256'],
         'Authoritative health digest differs')
    rehearsal = h['result']['rehearsal']
    need(h['result']['outcome'] == 'verified' and h['revision'] == 10
         and h['context']['workerConfig'] == before['finalStateBackup']['record']['intent']['binding']['workerConfig']
         and rehearsal['descriptor'] == expected_source['workerDescriptor']
         and sup == before['beforeMainExit']['supervisor']
         and sup['phase'] == 'active' and sup['ownerId'] == plan['ownerId']
         and sup['ownerEpoch'] == plan['ownerEpoch'] and sup['spec']['worker'] == binding['worker'],
         'Authoritative health/supervisor/old worker differs')
    expected_context = dict(supervisor=sup['process'], previousWorker=binding['worker']['process'],
                            previousGeneration=binding['worker']['generation'], workerContext=binding['worker']['context'],
                            workerState=binding['workerState'], workerConfig=h['context']['workerConfig'],
                            workerConfigSchema=rehearsal['workerConfigSchema'], workerJournalSchema=16,
                            updaterJournalSchema=18, protocolVersion=rehearsal['protocolVersion'], startupTimeoutMs=120000,
                            channelNonceSha256=ctx['channelNonceSha256'])
    need(ctx == expected_context and re.fullmatch(r'[0-9a-f]{64}', ctx['channelNonceSha256'])
         and str(uuid.UUID(intent['attemptId'])) == intent['attemptId']
         and intent['attemptId'] not in [plan[k] for k in ('updateId', 'attemptId', 'ownerId', 'launchId')]
         and set(intent) == {'schemaVersion', 'attemptId', 'admittedAtMs', 'source', 'context'}
         and intent['schemaVersion'] == 1, 'Frozen preparation context/attempt differs')
    expected_record = dict(schemaVersion=1, intent=intent, revision=0, updatedAtMs=intent['admittedAtMs'],
                           state={'phase': 'prepared'}, createIssuedAtMs=None, created=None, resumeGrantedAtMs=None,
                           ready=None, handoffIssuedAtMs=None, stateOwned=None, local=None)
    need(r == expected_record and saved['history'] == cold['history'] == [r] and cold['record'] == r,
         'Preparation history advanced/replayed or changed cold')
    view, duplicate, initial = saved['view'], saved['duplicate'], saved['before']
    edge = saved['rootEdge']
    need(edge['updateId'] == plan['updateId'] and edge['revision'] == plan['parentRevision']+1
         and edge['phase'] == 'verifying_local' and edge['previousPhase'] == 'replacing'
         and edge['command']['event'] == {'kind': 'files_verified'}
         and edge['command']['expectedRevision']+1 == edge['revision']
         and edge['ownerId'] == plan['ownerId'] and edge['ownerEpoch'] == plan['ownerEpoch']
         and edge['occurredAtMs'] == view['update']['updatedAtMs'] <= intent['admittedAtMs'], 'FilesVerified causal edge differs')
    projection = dict(phase='prepared', attemptId=intent['attemptId'], revision=0, setId=plan['manifest']['setId'],
                      replacementSha256=expected_source['replacementSha256'], worker=expected_source['worker'],
                      workerConfigSha256=ctx['workerConfig']['sha256'], preparedAtMs=intent['admittedAtMs'],
                      physicalCustodyHeld=True, reason=None)
    for v, custody in ((view, True), (duplicate, True), (cold['view'], False)):
        need(v['schemaVersion'] == 12 and v['driverActive'] is False and v['driverError'] is None
             and v['update'] == replaced['view']['update'] and v['update']['phase'] == 'verifying_local'
             and v['runtimeReplacement'] == dict(replaced['view']['runtimeReplacement'], physicalCustodyHeld=custody)
             and v['activationPreparation'] == dict(projection, physicalCustodyHeld=custody),
             'Live/duplicate/cold projection fabricated custody or changed root')
    need(initial['schemaVersion'] == 12 and initial['update'] == view['update']
         and initial['activationPreparation'] == dict.fromkeys(projection) | dict(phase='not_prepared', physicalCustodyHeld=True),
         'Observe admitted preparation before explicit command')
    need(saved['workerBytesBefore'] == saved['workerBytesAfter'] == replaced['workerBytesBefore']
         and all(saved[k] is True for k in ('lostAckSentOnce', 'durableBeforeAckConsumption', 'wrongVersionRefused',
                                           'oldViewFieldAbsent', 'instanceLockHeld', 'filesUnchanged'))
         and saved['publicActivationEnabled'] is False and cold['ownedJobEmpty'] is True
         and saved['workerLaunchCount'] == cold['workerLaunchCount'] == saved['stateHandoffCount'] == cold['stateHandoffCount'] == 0,
         'Preparation changed worker/state/files or lost effect/isolation evidence')


def audit(proof, source, target, root, health):
    verify(proof, source, target, root, health)
    edits = [('cases/0/activationPreparation/record/intent/source/replacementSha256', '0'*64),
             ('cases/0/activationPreparation/record/intent/context/channelNonceSha256', '0'*64),
             ('cases/0/activationPreparation/record/intent/context/workerJournalSchema', 17),
             ('cases/0/activationPreparation/record/intent/context/updaterJournalSchema', 17),
             ('cases/0/activationPreparation/workerLaunchCount', 1),
             ('cases/0/activationPreparation/stateHandoffCount', 1),
             ('cases/0/coldActivationPreparation/view/activationPreparation/physicalCustodyHeld', True),
             ('cases/0/activationPreparation/view/activationPreparation/canSpawn', True),
             ('cases/0/activationPreparation/record/revision', True),
             ('cases/0/activationPreparation/record/state/phase', 'creating'),
             ('cases/0/activationPreparation/supervisor/process/processId', 1),
             ('cases/0/activationPreparation/rootEdge/command/event', 'local_health_verified'),
             ('cases/0/activationPreparation/rootEdge/command/event', 'files_verified'),
             ('cases/0/activationPreparation/rootEdge/command/event', {'kind': 'files_verified', 'permit': True}),
             ('cases/0/activationPreparation/lostAckSentOnce', False),
             ('cases/0/activationPreparation/workerBytesAfter', []),
             ('cases/0/activationPreparation/history', []),
             ('cases/0/coldActivationPreparation/record/intent/attemptId', str(uuid.uuid4()))]
    for path, value in edits:
        changed = copy.deepcopy(proof)
        obj = changed
        parts = path.split('/')
        for part in parts[:-1]:
            obj = obj[int(part)] if isinstance(obj, list) else obj[part]
        obj[parts[-1]] = value
        try:
            verify(changed, source, target, root, health)
        except (ValueError, KeyError, TypeError, IndexError):
            continue
        raise ValueError('Accepted mutated preparation proof: '+path)
    # Change every retained copy together: consistency between copies alone is
    # insufficient; each context field must still bind its authoritative source.
    coherent = [('context/previousGeneration', str(uuid.uuid4())),
                ('context/workerConfigSchema', 9), ('context/protocolVersion', 9),
                ('context/supervisor/processId', 1), ('context/workerState/lockIdentity', 'foreign'),
                ('context/workerContext/journalIncarnation', str(uuid.uuid4())),
                ('context/workerConfig/fileIdentity', 'foreign'), ('context/startupTimeoutMs', 1),
                ('source/ownerEpoch', 0), ('source/runtimeIdentity', 'foreign')]
    for path, value in coherent:
        changed = copy.deepcopy(proof)
        case = changed['cases'][0]
        records = [case['activationPreparation']['record'], *case['activationPreparation']['history'],
                   case['coldActivationPreparation']['record'], *case['coldActivationPreparation']['history']]
        for record in records:
            obj = record['intent']
            parts = path.split('/')
            for part in parts[:-1]:
                obj = obj[part]
            obj[parts[-1]] = value
        try:
            verify(changed, source, target, root, health)
        except (ValueError, KeyError, TypeError, IndexError):
            continue
        raise ValueError('Accepted coherently changed preparation: '+path)
    return len(edits)+len(coherent)


def verify_migration(proof, source, reference):
    strict(proof)
    need(proof['schemaVersion'] == 1 and proof['kind'] == 'activation_schema_migration_proof'
         and proof['passed'] is True and proof['platform'] == 'windows'
         and proof['build']['sourceCommit'] == source['sourceCommit']
         and proof['build']['sourceManifestSha256'] == source['manifestSha256']
         and proof['build']['sourceDirty'] is False and proof['referenceProofSha256'] == reference['proofSha256']
         and proof['referenceImage'] == reference['image'] and proof['referenceManifestSha256'] == reference['manifestSha256'],
         'Migration scope/reference differs')
    before = proof['before']
    need(before['schemaVersion'] == 17 and proof['after'] == dict(before, schemaVersion=18, ownerEpoch=before['ownerEpoch']+1)
         and proof['backupInfo'] == before and proof['rowsBefore'] == proof['rowsAfter'] == proof['backupRows']
         and re.fullmatch(r'[0-9a-f]{64}', proof['rowsBefore']) and proof['gateRows'] == 0,
         'Migration lost rows/backup/epoch or admitted activation')
    for field, digest in (('retainedSupport', 'supportSha256'), ('retainedCompatibleDrain', 'compatibleSha256')):
        need(hashlib.sha256(canonical(proof[field])).hexdigest() == reference[digest], 'Retained history differs: '+field)
    need(proof['retainedSupport']['intent']['updateId'] == reference['updateId']
         and all(proof[k] is True for k in ('oldSeventeenInitRefused', 'oldSeventeenStatusRefused', 'sourceBytesUnchanged',
                                          'copyBytesUnchangedByOld', 'newAdmissionDenied', 'migrationBackupVerified'))
         and all(proof[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened')),
         'Migration/refusal changed source or manufactured admission')


def audit_migration(proof, source, reference):
    verify_migration(proof, source, reference)
    edits = [('after/schemaVersion', 17), ('after/ownerEpoch', 0), ('gateRows', 1), ('gateRows', False),
             ('rowsAfter', '0'*64), ('backupRows', '0'*64), ('oldSeventeenInitRefused', False),
             ('oldSeventeenStatusRefused', False), ('sourceBytesUnchanged', False), ('newAdmissionDenied', False),
             ('retainedCompatibleDrain/supportSha256', '0'*64), ('referenceProofSha256', '0'*64)]
    for path, value in edits:
        changed = copy.deepcopy(proof)
        obj = changed
        parts = path.split('/')
        for part in parts[:-1]:
            obj = obj[part]
        obj[parts[-1]] = value
        try:
            verify_migration(changed, source, reference)
        except (ValueError, KeyError, TypeError):
            continue
        raise ValueError('Accepted mutated migration: '+path)
    return len(edits)
