"""Independent Main23 physical-state evidence. No serialized writer permission."""
import copy
import hashlib
import json
import uuid


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def strict(value, preparation):
    preparation.strict(value)

    def visit(v):
        if isinstance(v, dict):
            for key, item in v.items():
                need(key not in ('stateGrant', 'canHandoff', 'writerOwned', 'canWriteState'),
                     'Fabricated state permission: ' + key)
                if key == 'creationTime':
                    need(type(item) is int and 0 <= item <= 2**64 - 1, 'Invalid actual FILETIME')
                if key in ('originalLockHeld', 'processAlive', 'processCustodyHeld', 'outputIncomplete', 'captureComplete'):
                    need(item is None or type(item) is bool, 'Invalid nullable observation: ' + key)
                if key in ('lockAvailable', 'stateTraceOnly', 'actualJobOccupied', 'actualProcessInJob',
                           'childInstanceLockOccupied', 'lostHandoffAckSentOnce', 'writableStoreStarted',
                           'publicActivationEnabled', 'actualRetainedProcessExited', 'ownedJobEmpty',
                           'activationJobEmpty', 'sameInstanceLockAvailable', 'workerBytesUnchanged',
                           'writeDenied', 'deleteDenied'):
                    need(type(item) is bool, 'Invalid physical observation: ' + key)
                visit(item)
        elif isinstance(v, list):
            for item in v:
                visit(item)
    visit(value)


def verify_owned_prefix(p, cold, before, replaced, root, preparation, *, source, worker_schema=16, updater_schema=18):
    prepared, record, history = p['prepared'], p['record'], p['history']
    intent = prepared['intent']
    pin, ctx = preparation.verify_scope(prepared, p, before, replaced, root, worker_schema=worker_schema, updater_schema=updater_schema)
    root_record = replaced['view']['update']
    need(root_record['updatedAtMs'] <= intent['admittedAtMs'], 'State preparation predates FilesVerified')
    empty = dict(schemaVersion=1, intent=intent, revision=0, updatedAtMs=intent['admittedAtMs'],
                 state={'phase': 'prepared'}, createIssuedAtMs=None, created=None,
                 resumeGrantedAtMs=None, ready=None, handoffIssuedAtMs=None, stateOwned=None, local=None)
    need(prepared == empty and len(history) == 7 and history[0] == prepared and history[-1] == record
         and history == cold['history'] and record == cold['record'], 'State ledger skipped/replayed or changed cold')
    need(record['revision'] == 6 and record['state'] == {'phase': 'state_owned'} and record['local'] is None,
         'Physical receipt pretends to be local/writable activation')
    created, ready, owned = record['created'], record['ready'], record['stateOwned']
    process, identity = created['process'], ready['identity']
    need(process == p['actualProcess'] and process['processId'] > 0
         and process['processId'] not in (ctx['supervisor']['processId'], ctx['previousWorker']['processId'])
         and str(int(process['birth'])) == process['birth'] and 0 < int(process['birth']) <= 2**64 - 1
         and process['hostId'] == ctx['supervisor']['hostId'] and process['sessionId'] == ctx['supervisor']['sessionId'],
         'State owner is not the actual distinct same-host/session child')
    job = 'Global\\WinBridge-Activation-' + hashlib.sha256(b'winbridge.activation.job.v1\n' + preparation.canonical(intent)).hexdigest()
    need(created['jobName'] == job and p['actualJob'] == dict(jobName=job, hostId=process['hostId'],
         sessionId=process['sessionId'], processId=process['processId'], creationTime=int(process['birth']))
         and p['actualJobOccupied'] is True and p['actualProcessInJob'] is True, 'Actual state-owner Job differs')
    expected_identity = dict(schemaVersion=1, kind='loaded_worker_identity', process=process,
                             runtimeIdentity=pin['runtimeIdentity'], setId=pin['setId'],
                             manifestSha256=pin['manifest']['sha256'], descriptor=pin['workerDescriptor'], image=pin['worker'])
    need(ready == dict(attemptId=intent['attemptId'], identity=expected_identity,
                       workerConfigSha256=ctx['workerConfig']['sha256'], observedAtMs=ready['observedAtMs']),
         'Ready does not bind the same loaded child/config')
    need(owned == dict(attemptId=intent['attemptId'], process=process, context=ctx['workerContext'],
                       generation=owned['generation'], state=ctx['workerState'], observedAtMs=owned['observedAtMs'])
         and str(uuid.UUID(owned['generation'])) == owned['generation']
         and owned['generation'] != ctx['previousGeneration'], 'State receipt rebound the child/context/files/generation')
    phases = ['prepared', 'creating', 'created', 'resume_granted', 'ready_for_state', 'handoff_issued', 'state_owned']
    previous = intent['admittedAtMs']
    deadline = previous + ctx['startupTimeoutMs']
    for index, entry in enumerate(history):
        expected = dict(empty, revision=index, updatedAtMs=entry['updatedAtMs'], state={'phase': phases[index]})
        for from_index, field in ((1, 'createIssuedAtMs'), (2, 'created'), (3, 'resumeGrantedAtMs'),
                                  (4, 'ready'), (5, 'handoffIssuedAtMs'), (6, 'stateOwned')):
            if index >= from_index:
                expected[field] = record[field]
        need(entry == expected and previous <= entry['updatedAtMs'], 'State history rewrote a frozen causal prefix')
        previous = entry['updatedAtMs']
    need(history[1]['updatedAtMs'] == record['createIssuedAtMs'] <= deadline
         and history[1]['updatedAtMs'] <= created['observedAtMs'] <= history[2]['updatedAtMs']
         and history[2]['updatedAtMs'] <= record['resumeGrantedAtMs'] == history[3]['updatedAtMs'] <= deadline
         and history[3]['updatedAtMs'] <= ready['observedAtMs'] <= history[4]['updatedAtMs']
         and history[4]['updatedAtMs'] <= record['handoffIssuedAtMs'] == history[5]['updatedAtMs'] <= deadline
         and history[5]['updatedAtMs'] <= owned['observedAtMs'] <= history[6]['updatedAtMs'],
         'Create/resume/handoff grants expired or receipt chronology differs')
    expected_input = dict(schemaVersion=1, kind='winbridge_activation_process_validation',
                          updateId=pin['updateId'], launchId=pin['launchId'], ownerId=pin['ownerId'],
                          ownerEpoch=pin['ownerEpoch'], attemptId=intent['attemptId'], runtimeIdentity=pin['runtimeIdentity'],
                          updaterConfigSha256=pin['updaterConfigSha256'], stateTraceOnly=True)
    for name, phase, available in (('beforeRelease', 'before_release', False), ('afterRelease', 'after_release', True)):
        marker = p[name]
        need(marker['schemaVersion'] == 1 and marker['kind'] == 'activation_state_lock_boundary'
             and marker['phase'] == phase and marker['processId'] == ctx['supervisor']['processId']
             and marker['record'] == history[5] and marker['input'] == json.loads(marker['inputEncoded']) == expected_input
             and marker['inputSha256'] == hashlib.sha256(marker['inputEncoded'].encode()).hexdigest()
             and marker['recordSha256'] == hashlib.sha256(b'winbridge.activation.barrier.record.v1\n' + preparation.canonical(history[5])).hexdigest()
             and marker['handoffRecordSha256'] == hashlib.sha256(b'winbridge.activation.state-handoff.v1\n' + preparation.canonical(history[5])).hexdigest()
             and record['handoffIssuedAtMs'] <= marker['observedAtMs'] <= deadline
             and marker['observedState'] == ctx['workerState'] and marker['lockAvailable'] is available
             and marker['build']['sourceCommit'] == source['sourceCommit'] and marker['build']['sourceDirty'] is False
             and marker['build']['sourceManifestSha256'] == source['manifestSha256'], 'Physical lock boundary has foreign scope/build/state')
    need(p['beforeRelease']['observedAtMs'] <= p['afterRelease']['observedAtMs'] <= owned['observedAtMs'],
         'Release observations do not precede the successor ownership receipt')
    expected_set = dict(schemaVersion=1, kind='installed_set_observation', readOnly=True,
                        runtimeDirectory=pin['runtimeDirectory'], runtimeIdentity=pin['runtimeIdentity'],
                        manifestFile=replaced['actualInstalledFiles'][4], manifest=replaced['actualManifest'],
                        images=replaced['actualInstalledFiles'][:4])
    need(p['actualSet'] == expected_set and p['sharing'] == [dict(image=image, writeDenied=True, deleteDenied=True)
         for image in expected_set['images'] + [expected_set['manifestFile']]], 'Five-file identities/sharing evidence differs')
    return pin, ctx, intent, record, history, process, owned, identity


def verify(proof, source, target, root, preparation, health):
    strict(proof, preparation)
    need(proof['kind'] == 'activation_state_main_native_proof', 'Wrong physical state proof kind')
    health.verify_runtime_replacement_proof(dict(proof, kind='runtime_replacement_main_native_proof'), source, target, True)
    case, = proof['cases']
    before, replaced = case['beforeReplacement'], case['replacement']
    p, cold = case['activationState'], case['coldActivationState']
    need('activationProcess' not in case and 'coldActivationProcess' not in case,
         'StateOwned may not masquerade as an old Ready-only process proof')
    prepared, record, history = p['prepared'], p['record'], p['history']
    pin, ctx, intent, record, history, process, owned, identity = verify_owned_prefix(
        p, cold, before, replaced, root, preparation, source=source)
    root_record = replaced['view']['update']
    files = dict(phase='bound', attemptId=intent['attemptId'], setId=pin['setId'], heldFiles=5, worker=pin['worker'],
                 manifestSha256=pin['manifest']['sha256'], originalStateHeld=False, reason=None)
    physical = dict(phase='state_owned', attemptId=intent['attemptId'], revision=6, durablePhase='state_owned',
                    savedOwnership=owned, originalLockHeld=False, residentProcess=process, processAlive=True, reason=None)
    for name in ('owned', 'duplicate'):
        view = p[name]
        need(view['schemaVersion'] == 15 and view['update'] == root_record and view['driverActive'] is False
             and view['driverError'] is None and view['activationState'] == physical and view['activationFiles'] == files,
             'Warm/duplicate state view invents ownership or changes scope')
    for name, schema in (('before', 12), ('preparing', 12), ('preparedView', 12),
                         ('binding', 13), ('bound', 13), ('launching', 14)):
        view = p[name]
        need(view['schemaVersion'] == schema and view['update'] == root_record and view['driverError'] is None
             and 'activationState' not in view, 'Earlier view changed frozen scope/shape: ' + name)
    ready_view = p['ready']
    rp = ready_view['activationProcess']
    need(ready_view['schemaVersion'] == 14 and ready_view['update'] == root_record and ready_view['driverActive'] is False
         and ready_view['driverError'] is None and rp['phase'] == 'ready_for_state' and rp['revision'] == 4
         and rp['savedProcess'] == rp['residentProcess'] == process and rp['savedReady'] == identity
         and rp['originalStateHeld'] is True and rp['processCustodyHeld'] is True and rp['processAlive'] is True
         and 'activationState' not in ready_view, 'Ready view did not precede handoff')
    legacy = p['legacyProcess']
    lp = legacy['activationProcess']
    need(legacy['schemaVersion'] == 14 and legacy['update'] == root_record and 'activationState' not in legacy
         and legacy['driverActive'] is False and legacy['driverError'] is None
         and lp['phase'] == 'indeterminate' and lp['revision'] == 6 and lp['savedProcess'] == lp['residentProcess'] == process
         and lp['originalStateHeld'] is False and lp['processAlive'] is True and 'internal23' in lp['reason']
         and legacy['activationFiles'] == files, 'Legacy14 claims old state custody or changed frozen shape')
    need(p['legacyFiles']['schemaVersion'] == 13 and 'activationState' not in p['legacyFiles']
         and 'activationProcess' not in p['legacyFiles'] and p['legacyFiles']['activationFiles'] == files
         and p['legacyFiles']['update'] == root_record, 'Legacy13 lost the retained files or gained new fields')
    cv = cold['view']
    need(cv['schemaVersion'] == 15 and cv['update'] == root_record and cv['driverActive'] is False and cv['driverError'] is None
         and cv['activationState'] == dict(physical, phase='not_resident', originalLockHeld=False, residentProcess=None, processAlive=None)
         and cv['activationFiles'] == dict(files, phase='not_bound', heldFiles=0, worker=None, manifestSha256=None)
         and cv['activationProcess']['processCustodyHeld'] is False and cv['activationProcess']['residentProcess'] is None
         and cv['activationProcess']['processAlive'] is None and cv['activationProcess']['originalStateHeld'] is False,
         'Cold receipt recreated live resources or forgot ownership history')
    need(p['workerBytesBefore'] == p['workerBytesAfter'] == replaced['workerBytesBefore']
         and p['childInstanceLockOccupied'] is True and p['lostHandoffAckSentOnce'] is True
         and p['workerLaunchCount'] == cold['workerLaunchCount'] == 1
         and p['stateHandoffCount'] == cold['stateHandoffCount'] == 1 and cold['automaticColdLaunchCount'] == 0
         and p['writableStoreStarted'] is False and cold['writableStoreStarted'] is False and p['publicActivationEnabled'] is False
         and all(cold[k] is True for k in ('actualRetainedProcessExited', 'ownedJobEmpty', 'activationJobEmpty',
                                         'sameInstanceLockAvailable', 'workerBytesUnchanged'))
         and cold['observedState'] == ctx['workerState'], 'Physical one-shot/readonly/actual exit/reacquisition evidence missing')


def audit(proof, source, target, root, preparation, health):
    verify(proof, source, target, root, preparation, health)
    p = 'cases/0/activationState/'
    c = 'cases/0/coldActivationState/'
    edits = [(p+'record/revision', 5), (p+'history', []), (p+'actualProcess/processId', 1),
             (p+'actualJob/jobName', 'foreign'), (p+'actualJob/creationTime', True), (p+'actualJobOccupied', False),
             (p+'actualProcessInJob', False), (p+'record/stateOwned/generation', 'invalid'),
             (p+'record/stateOwned/state/lockIdentity', 'foreign'), (p+'record/stateOwned/context/instanceId', 'foreign'),
             (p+'beforeRelease/lockAvailable', True), (p+'afterRelease/lockAvailable', False),
             (p+'afterRelease/recordSha256', '0'*64), (p+'beforeRelease/inputSha256', '0'*64),
             (p+'afterRelease/handoffRecordSha256', '0'*64), (p+'afterRelease/observedAtMs', 0),
             (p+'afterRelease/observedState/journalIdentity', 'foreign'), (p+'afterRelease/processId', 1),
             (p+'afterRelease/input/stateTraceOnly', False), (p+'afterRelease/build/sourceDirty', True),
             (p+'owned/activationState/originalLockHeld', True), (p+'duplicate/activationState/revision', 7),
             (p+'duplicate/activationState/processAlive', 1), (p+'ready/activationProcess/originalStateHeld', False),
             (p+'legacyProcess/activationProcess/originalStateHeld', True), (p+'legacyFiles/activationFiles/heldFiles', 0),
             (p+'actualSet/images/0/fileIdentity', 'foreign'), (p+'sharing/0/writeDenied', False),
             (p+'workerBytesAfter', []), (p+'workerLaunchCount', 2), (p+'stateHandoffCount', 2),
             (p+'writableStoreStarted', True), (p+'lostHandoffAckSentOnce', False),
             (c+'view/activationState/originalLockHeld', True), (c+'view/activationState/processAlive', True),
             (c+'actualRetainedProcessExited', False), (c+'sameInstanceLockAvailable', False),
             (c+'observedState/lockIdentity', 'foreign'), (c+'automaticColdLaunchCount', True)]
    for path, value in edits:
        bad = copy.deepcopy(proof)
        at = bad
        parts = path.split('/')
        for part in parts[:-1]:
            at = at[int(part)] if isinstance(at, list) else at[part]
        at[parts[-1]] = value
        try:
            verify(bad, source, target, root, preparation, health)
        except (ValueError, KeyError, TypeError, IndexError):
            continue
        raise ValueError('Accepted mutated state proof: ' + path)
    # Preserve every redundant receipt copy while violating the intrinsic new
    # generation rule; this cannot be rejected merely by a stale sibling copy.
    bad = copy.deepcopy(proof)
    old_generation = proof['cases'][0]['activationState']['record']['intent']['context']['previousGeneration']
    new_generation = proof['cases'][0]['activationState']['record']['stateOwned']['generation']

    def replace_generation(value):
        if isinstance(value, dict):
            for key, item in value.items():
                if key == 'generation' and item == new_generation:
                    value[key] = old_generation
                else:
                    replace_generation(item)
        elif isinstance(value, list):
            for item in value:
                replace_generation(item)
    replace_generation(bad)
    try:
        verify(bad, source, target, root, preparation, health)
    except (ValueError, KeyError, TypeError, IndexError):
        return len(edits) + 1
    raise ValueError('Accepted coordinated reuse of the previous worker generation')
