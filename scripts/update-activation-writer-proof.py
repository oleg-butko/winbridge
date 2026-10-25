"""Independent Main24 writer evidence, not a grant or proof of normal service."""
import copy
import hashlib
import re
import uuid


def need(ok, message):
    if not ok:
        raise ValueError(message)


def digest(domain, value, preparation):
    return hashlib.sha256(domain + preparation.canonical(value)).hexdigest()


def strict(proof, preparation, physical):
    physical.strict(proof, preparation)
    booleans = {'lostWriterAckSentOnce', 'oldCachedAuthorityRefused', 'normalServiceStarted'}
    counters = {'epoch', 'busyObservations', 'savedWorkerCommandCount', 'tasks', 'output'}
    forbidden = {'authorityId', 'authority_id', 'startupPermit', 'writeGrant', 'serviceReady'}

    def visit(value):
        if isinstance(value, dict):
            for key, item in value.items():
                need(key not in forbidden, 'Private identity or invented permission: ' + key)
                if key in booleans:
                    need(type(item) is bool, 'Invalid writer observation: ' + key)
                if key in counters:
                    need(type(item) is int and 0 <= item <= 9007199254740991, 'Invalid writer counter: ' + key)
                visit(item)
        elif isinstance(value, list):
            for item in value:
                visit(item)
    visit(proof)


def authority(value):
    need(set(value) == {'incarnation', 'epoch', 'fingerprintSha256'}
         and str(uuid.UUID(value['incarnation'])) == value['incarnation']
         and type(value['epoch']) is int and 0 < value['epoch'] <= 9007199254740991
         and re.fullmatch(r'[0-9a-f]{64}', value['fingerprintSha256']), 'Invalid public authority stamp')


def verify_delivery(p, cold, before, pin, ctx, record, source_build, preparation):
    """Link the observed SQL facts/views to an already verified physical prefix."""
    intent, physical = record['intent'], record['stateOwned']
    saved = p['writerRecord']
    command = saved['command']
    key = command['commandId']
    old, new = p['workerAuthorityBefore'], p['workerAuthorityAfter']
    authority(old)
    authority(new)
    backup = before['finalStateBackup']['record']
    need(old == backup['report']['sourceAuthority'] and old['epoch'] < 9007199254740991
         and old['incarnation'] == ctx['workerContext']['journalIncarnation'],
         'Writer did not use the exact final-backup old authority')
    need(str(uuid.UUID(key)) == key and key not in
         [pin[k] for k in ('updateId', 'launchId', 'ownerId')] + [intent['attemptId'], physical['generation']],
         'Writer command key is noncanonical or aliases another operation')
    deadline = intent['admittedAtMs'] + ctx['startupTimeoutMs']
    expected_binding = dict(updateId=pin['updateId'], attemptId=intent['attemptId'], launchId=pin['launchId'],
                            updaterOwnerId=pin['ownerId'], updaterEpoch=pin['ownerEpoch'], activationRevision=6,
                            activationRecordSha256=digest(b'winbridge.activation.writer-binding.v1\n', record, preparation),
                            backupRecordSha256=pin['backupSha256'], physical=physical, expectedAuthority=old,
                            workerJournalSchema=17, updaterJournalSchema=19, workerConfigSha256=ctx['workerConfig']['sha256'])
    need(command == dict(schemaVersion=1, commandId=key, binding=expected_binding,
                         issuedAtMs=command['issuedAtMs'], deadlineAtMs=deadline)
         and record['updatedAtMs'] <= command['issuedAtMs'] < deadline,
         'Writer command changed physical/backup/owner/schema scope or renewed the window')
    receipt = p['actualWorkerReceipt']
    command_sha = digest(b'winbridge.activation.writer-command.v1\n', command, preparation)
    need(receipt == dict(schemaVersion=1, commandId=key, updateId=pin['updateId'], attemptId=intent['attemptId'],
                         commandSha256=command_sha, authority=new, committedAtMs=receipt['committedAtMs'])
         and command['issuedAtMs'] <= receipt['committedAtMs'] < deadline
         and new['incarnation'] == old['incarnation'] and new['epoch'] == old['epoch'] + 1
         and new['fingerprintSha256'] != old['fingerprintSha256'], 'Worker atomic receipt/next authority differs')
    issued = dict(schemaVersion=1, revision=0, command=command, state={'phase': 'issued'}, updatedAtMs=command['issuedAtMs'])
    confirmed = dict(schemaVersion=1, revision=1, command=command,
                     state={'phase': 'confirmed', 'receipt': receipt}, updatedAtMs=saved['updatedAtMs'])
    need(saved == confirmed and saved['updatedAtMs'] >= receipt['committedAtMs']
         and p['writerHistory'] == cold['writerHistory'] == [issued, confirmed]
         and cold['writerRecord'] == saved and cold['actualWorkerReceipt'] == receipt and cold['workerAuthority'] == new,
         'Updater/worker SQL observations disagree or writer history skipped/replayed')
    marker = p['beforeSend']
    boundary = p['afterRelease']
    need(marker == dict(schemaVersion=1, kind='activation_writer_before_send', processId=ctx['supervisor']['processId'],
                        build=boundary['build'], input=boundary['input'], inputEncoded=boundary['inputEncoded'],
                        inputSha256=boundary['inputSha256'], record=record,
                        recordSha256=digest(b'winbridge.activation.barrier.record.v1\n', record, preparation),
                        commandId=key, commandSha256=command_sha, observedAtMs=marker['observedAtMs'])
         and command['issuedAtMs'] <= marker['observedAtMs'] <= receipt['committedAtMs']
         and all(marker['build'][k] == source_build[k] for k in ('sourceCommit', 'sourceManifestSha256', 'sourceDirty')),
         'Original create-new send marker has another key/child/prefix/window/build')
    lease = p['workerLeaseBefore']
    shutdown = p['acceptedShutdownBefore']
    need(lease == p['workerLeaseAfter'] == shutdown['lease'] and lease['phase'] == 'quiesced'
         and shutdown == p['acceptedShutdownAfter'] == before['shutdown']['delivery']['receipt']
         and p['oldCachedAuthorityRefused'] is True, 'Writer released maintenance or accepted stop, or old owner survived')
    rows = p['preservedRowsBefore']
    need(set(rows) == {'sha256', 'tasks', 'output'} and re.fullmatch(r'[0-9a-f]{64}', rows['sha256'])
         and type(rows['tasks']) is int and rows['tasks'] > 0 and type(rows['output']) is int and rows['output'] > 0
         and rows == p['preservedRowsAfter'] == cold['preservedRowsAfter'],
         'Worker tables outside authority/activation changed, or history fixture is empty')
    projection = dict(phase='confirmed', attemptId=intent['attemptId'], commandId=key, revision=1,
                      durablePhase='confirmed', savedReceipt=receipt, residentProcess=physical['process'], processAlive=True, reason=None)
    root = p['owned']['update']
    for name in ('confirmed', 'duplicate'):
        v = p[name]
        need(v['schemaVersion'] == 16 and v['update'] == root and root['phase'] == 'verifying_local'
             and v['driverActive'] is False and v['driverError'] is None and v['activationWriter'] == projection
             and v['activationState'] == p['owned']['activationState']
             and v['activationFiles'] == p['owned']['activationFiles'],
             'Warm/duplicate view changed physical scope or confuses receipt with whole update success')
    cv = cold['view']
    need(cv['schemaVersion'] == 16 and cv['update'] == root and cv['driverActive'] is False and cv['driverError'] is None
         and cv['activationWriter'] == dict(projection, phase='not_resident', residentProcess=None, processAlive=None)
         and cv['activationState']['phase'] == 'not_resident'
         and cv['activationState']['savedOwnership'] == physical
         and cv['activationState']['residentProcess'] is None and cv['activationState']['processAlive'] is None
         and cv['activationFiles'] == dict(p['owned']['activationFiles'], phase='not_bound', heldFiles=0, worker=None, manifestSha256=None),
         'Cold writer receipt invented live process/files/service custody')
    for name, version, field in (('legacyState', 15, 'activationState'), ('legacyProcess', 14, 'activationProcess')):
        v = p[name]
        need(v['schemaVersion'] == version and 'activationWriter' not in v and v['update'] == root
             and v['driverActive'] is False and v['driverError'] is None and v[field]['phase'] == 'indeterminate'
             and v[field]['residentProcess'] == physical['process'] and v[field]['processAlive'] is True
             and '24' in v[field]['reason'], 'Frozen legacy view gained a writer or claims old ownership')
    need(p['workerLaunchCount'] == cold['workerLaunchCount'] == p['stateHandoffCount'] == cold['stateHandoffCount'] == 1
         and p['savedWorkerCommandCount'] == cold['savedWorkerCommandCount'] == 1
         and cold['automaticColdLaunchCount'] == 0
         and p['lostWriterAckSentOnce'] is True and p['lostHandoffAckSentOnce'] is True
         and p['normalServiceStarted'] is False and cold['normalServiceStarted'] is False
         and p['publicActivationEnabled'] is False and p['childInstanceLockOccupied'] is True
         and all(cold[k] is True for k in ('actualRetainedProcessExited', 'ownedJobEmpty', 'activationJobEmpty', 'sameInstanceLockAvailable'))
         and cold['observedState'] == physical['state'], 'One-shot effect, actual exit/lock or isolation evidence differs')


def verify(proof, source, target, root, preparation, physical, health, *, startup_timeout_ms=120000):
    strict(proof, preparation, physical)
    need(proof['kind'] == 'activation_writer_main_native_proof', 'Wrong writer proof kind')
    health.verify_runtime_replacement_proof(dict(proof, kind='runtime_replacement_main_native_proof'),
                                          source, target, True, worker_schema=17)
    case, = proof['cases']
    need(all(k not in case for k in ('activationState', 'coldActivationState', 'activationProcess', 'coldActivationProcess')),
         'Writer proof may not masquerade as an earlier physical-only result')
    before, replaced = case['beforeReplacement'], case['replacement']
    p, cold = case['activationWriter'], case['coldActivationWriter']
    pin, ctx, intent, record, history, process, owned, identity = physical.verify_owned_prefix(
        p, cold, before, replaced, root, preparation, source=source, worker_schema=17, updater_schema=19, startup_timeout_ms=startup_timeout_ms)
    root_record = replaced['view']['update']
    expected_files = dict(phase='bound', attemptId=intent['attemptId'], setId=pin['setId'], heldFiles=5, worker=pin['worker'],
                          manifestSha256=pin['manifest']['sha256'], originalStateHeld=False, reason=None)
    expected_physical = dict(phase='state_owned', attemptId=intent['attemptId'], revision=6, durablePhase='state_owned',
                             savedOwnership=owned, originalLockHeld=False, residentProcess=process, processAlive=True, reason=None)
    need(p['owned']['schemaVersion'] == 15 and p['owned']['update'] == root_record
         and p['owned']['activationState'] == expected_physical and p['owned']['activationFiles'] == expected_files
         and p['owned']['driverActive'] is False and p['owned']['driverError'] is None,
         'StateOwned observation did not precede same-child writer start')
    need(p['workerBytesBefore'] == replaced['workerBytesBefore'] and p['workerBytesAfter'] != p['workerBytesBefore']
         and p['workerBytesAfter'] == cold['workerBytesAfter'] == case['afterOwnedExit']['workerBytesBeforeStartup']
         == case['afterOwnedExit']['workerBytesAfterStartup'], 'Worker activation bytes or refused-start preservation differs')
    verify_delivery(p, cold, before, pin, ctx, record, proof['build'], preparation)


def audit(proof, source, target, root, preparation, physical, health, *, startup_timeout_ms=120000):
    verify(proof, source, target, root, preparation, physical, health, startup_timeout_ms=startup_timeout_ms)
    paths = [
        ('cases/0/activationWriter/writerRecord/command/deadlineAtMs', 1),
        ('cases/0/activationWriter/workerAuthorityAfter/epoch', 1),
        ('cases/0/activationWriter/actualWorkerReceipt/commandSha256', '0'*64),
        ('cases/0/activationWriter/beforeSend/commandId', str(uuid.uuid4())),
        ('cases/0/activationWriter/workerLeaseAfter/phase', 'released'),
        ('cases/0/activationWriter/preservedRowsAfter/sha256', '0'*64),
        ('cases/0/activationWriter/oldCachedAuthorityRefused', False),
        ('cases/0/activationWriter/savedWorkerCommandCount', True),
        ('cases/0/activationWriter/writerHistory/0/command/binding/activationRecordSha256', '0'*64),
        ('cases/0/activationWriter/writerHistory/1/state/receipt/authority/epoch', 1),
        ('cases/0/coldActivationWriter/view/activationWriter/processAlive', True),
        ('cases/0/coldActivationWriter/view/activationWriter/phase', 'confirmed'),
        ('cases/0/coldActivationWriter/view/activationWriter/startupPermit', False),
        ('cases/0/coldActivationWriter/actualRetainedProcessExited', False),
        ('cases/0/coldActivationWriter/actualWorkerReceipt/committedAtMs', 1),
        ('cases/0/coldActivationWriter/writerHistory', []),
    ]
    for path, replacement in paths:
        bad = copy.deepcopy(proof)
        node = bad
        pieces = path.split('/')
        for key in pieces[:-1]:
            node = node[int(key)] if isinstance(node, list) else node[key]
        key = int(pieces[-1]) if isinstance(node, list) else pieces[-1]
        need(node.get(key) != replacement if isinstance(node, dict) else node[key] != replacement,
             'Writer mutation is a no-op: ' + path)
        node[key] = replacement
        try:
            verify(bad, source, target, root, preparation, physical, health, startup_timeout_ms=startup_timeout_ms)
        except (ValueError, KeyError, TypeError, IndexError, OverflowError):
            continue
        raise ValueError('Writer proof mutation accepted: ' + path)
    return len(paths)
