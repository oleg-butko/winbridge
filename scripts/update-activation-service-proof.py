"""Independent Main25 ledger/IPC evidence audit. No custody or activation API."""
import copy
import hashlib
import json
from pathlib import PureWindowsPath
import re
import uuid


def need(ok, message):
    if not ok:
        raise ValueError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()


def digest(domain, value):
    return hashlib.sha256(domain.encode() + b'\n' + canonical(value)).hexdigest()


def verify_main_selection(command, log):
    """libtest --nocapture may print diagnostics before the final standalone ok."""
    name = 'actual_main_activates_one_local_service_without_controller_or_replay'
    arguments = command['arguments']
    need(arguments == ['test', '-p', 'winbridge-worker', '--features',
                       'bundled-sqlite,update-validation', '--test', 'update_shutdown',
                       '--locked', '--offline', '--', '--exact', name, '--ignored',
                       '--test-threads=1', '--nocapture'], 'Main25 exact test selection differs')
    text = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    selected = re.findall(r'^test ' + re.escape(name) + r' \.\.\.', text, re.MULTILINE)
    summaries = re.findall(r'^test result: ([^\r\n]+)', text, re.MULTILINE)
    need(len(selected) == 1 and len(summaries) == 1
         and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;')
         and re.search(r'^running 1 test\s*$', text, re.MULTILINE),
         'Main25 did not run exactly once to successful completion')


def strict(value):
    flags = {'passed', 'normalMain', 'actualOldRoles', 'actualCandidateBuild',
             'validationFeature', 'runtimeApplied', 'installedRuntimeApplied', 'windowsOpened',
             'oldCachedAuthorityRefused', 'actualJobOccupied', 'actualProcessInJob',
             'childInstanceLockOccupied', 'lostServiceAckSentOnce', 'normalServiceStarted',
             'controllerStarted', 'publicActivationEnabled', 'userResumeRefused',
             'actualRetainedProcessExited', 'activationJobEmpty', 'sameInstanceLockAvailable',
             'savedHistoryUnchanged', 'normalServiceResident', 'writeDenied', 'deleteDenied',
             'mutationRefused', 'beforeSupervisorAdmission', 'activationDeadlineRenewed'}
    counters = {'workerLaunchCount', 'stateHandoffCount', 'savedWorkerCommandCount',
                'serviceDispatchCount', 'automaticColdLaunchCount', 'busyObservations', 'epoch'}
    if isinstance(value, dict):
        for key, item in value.items():
            need(key not in {'nonce', 'token', 'permit', 'startupPermit', 'writeGrant', 'authorityId'},
                 'Private permission leaked into evidence: ' + key)
            if key in flags:
                need(type(item) is bool, 'Observation must be a JSON boolean: ' + key)
            if key in counters:
                need(type(item) is int and 0 <= item <= 9007199254740991, 'Invalid counter: ' + key)
            if key in {'schemaVersion', 'workerJournalSchema', 'updaterJournalSchema', 'workerConfigSchema', 'protocolVersion'} or key.endswith('AtMs'):
                need(item is None or type(item) is int and 0 <= item <= 9007199254740991, 'Invalid typed contract value: ' + key)
            if key in {'processAlive', 'processCustodyHeld', 'originalLockHeld'}:
                need(item is None or type(item) is bool, 'Invalid nullable liveness: ' + key)
            strict(item)
    elif isinstance(value, list):
        for item in value:
            strict(item)


PROFILES = {'worker17-updater20': (17, 20), 'worker19-updater20': (19, 20)}
DEFAULT_PROFILE = 'worker17-updater20'


def profile_schemas(profile):
    need(type(profile) is str and profile in PROFILES, 'Unknown explicit Main25 schema profile')
    return PROFILES[profile]


def require_schema_binding(value, profile):
    worker, updater = profile_schemas(profile)
    need(type(value.get('workerJournalSchema')) is int and value['workerJournalSchema'] == worker
         and type(value.get('updaterJournalSchema')) is int and value['updaterJournalSchema'] == updater,
         'Journal schema differs from the selected Main25 profile')


def require_resource_policy_reconciliation(value, before, after):
    need(type(value) is dict and value.get('schemaVersion') == 1
         and type(value.get('schemaVersion')) is int
         and value.get('normalizedColumn') == 'resource_policy.epoch'
         and value.get('policyUnchanged') is True
         and type(value.get('beforeEpoch')) is int and value['beforeEpoch'] == before
         and type(value.get('afterEpoch')) is int and value['afterEpoch'] == after
         and type(before) is int and type(after) is int and after == before + 1,
         'Resource policy reconciliation must change only the exact next owner epoch')


def verify(proof, source, target, reservation, *, profile=DEFAULT_PROFILE):
    result = verify_local_prefix(proof, source, target, reservation, profile=profile,
                  expected_schemas=profile_schemas(profile), proof_kind='activation_service_main_native_proof')
    return dict(result, kind='main25_independent_audit')


def verify_local_prefix(proof, source, target, reservation, *, profile, expected_schemas, proof_kind):
    """Shared factual prefix, with a fixed kind/schema pair supplied by each gate.

    Main25's public verifier still accepts only its original20 SDK profiles.
    Main26 may reuse the actual local-service prefix at21, never rewrite a
    reported schema or turn its later controller phase into a Main25 run.
    """
    need((expected_schemas, proof_kind) in (((17,20),'activation_service_main_native_proof'),
         ((19,20),'activation_service_main_native_proof'),((19,21),'activation_controller_main_native_proof'),
         ((19,22),'activation_lifetime_main_native_proof'),
         ((19,22),'activation_lifetime_ready_loss_main_native_proof'),
         ((19,22),'activation_resident_main_native_proof')),
         'Unsupported explicit local-prefix kind/schema pair')
    worker_schema, updater_schema = expected_schemas
    def require_scope(value):
        need(type(value.get('workerJournalSchema')) is int and value['workerJournalSchema'] == worker_schema
             and type(value.get('updaterJournalSchema')) is int and value['updaterJournalSchema'] == updater_schema,
             'Journal schema differs from the fixed local-prefix scope')
    strict(proof)
    need(proof['schemaVersion'] == 1 and proof['kind'] == proof_kind
         and proof['passed'] is True and proof['platform'] == 'windows', 'Not a completed Main25 proof')
    for key in ('normalMain', 'actualOldRoles', 'actualCandidateBuild', 'validationFeature', 'runtimeApplied'):
        need(proof[key] is True, 'Missing real pipeline evidence: ' + key)
    need(proof['installedRuntimeApplied'] is False and proof['windowsOpened'] is False, 'Installed/UI scope changed')
    need(proof['target'] == target and proof['build']['sourceDirty'] is False
         and all(proof['build'][k] == source[k] for k in ('sourceCommit', 'sourceManifestSha256') if k in source),
         'Producer/target source differs')
    need(proof['build']['sourceManifestSha256'] == source['manifestSha256'], 'Producer manifest differs')
    need(len(proof['cases']) == 1, 'Expected one isolated Main25 case')
    case = proof['cases'][0]
    p, cold = case['activationWriter'], case['coldActivationWriter']
    record, history = p['record'], p['history']
    phases = ['prepared', 'creating', 'created', 'resume_granted', 'ready_for_state', 'handoff_issued', 'state_owned', 'local_verified']
    need(len(history) == 8 and history[0] == p['prepared'] and history[-1] == record, 'Incomplete activation history')
    intent, context = record['intent'], record['intent']['context']
    deadline = intent['admittedAtMs'] + context['startupTimeoutMs']
    require_scope(context)
    need(context['startupTimeoutMs'] == 240000, 'Unreviewed startup window')
    previous = intent['admittedAtMs']
    for index, entry in enumerate(history):
        need(type(entry['revision']) is int and entry['revision'] == index and entry['intent'] == intent
             and entry['state'] == {'phase': phases[index]} and entry['updatedAtMs'] >= previous,
             'Activation skipped/reordered a causal edge')
        previous = entry['updatedAtMs']
        for start, field in ((1, 'createIssuedAtMs'), (2, 'created'), (3, 'resumeGrantedAtMs'),
                             (4, 'ready'), (5, 'handoffIssuedAtMs'), (6, 'stateOwned'), (7, 'local')):
            need(entry[field] == (record[field] if index >= start else None), 'Changed immutable activation prefix: ' + field)
    physical, process = record['stateOwned'], record['created']['process']
    runtime = PureWindowsPath(reservation['runtimeDirectory'])
    def windows_path(value):
        return PureWindowsPath(value[4:] if value.startswith('\\\\?\\') else value)
    need(windows_path(intent['source']['runtimeDirectory']) == runtime
         and windows_path(record['ready']['identity']['image']['path']) == runtime/'winbridge-worker.exe',
         'Activation changed the predeclared stable executable path')
    need(type(process['processId']) is int and process['processId'] > 0
         and process['processId'] not in (context['previousWorker']['processId'], context['supervisor']['processId'])
         and str(int(process['birth'])) == process['birth'] and 0 < int(process['birth']) <= 2**64-1,
         'Invalid distinct native process identity')
    need(physical['process'] == p['actualProcess'] == process
         and physical['context'] == context['workerContext'] and physical['state'] == context['workerState']
         and physical['generation'] != context['previousGeneration'], 'Same-child state binding differs')
    need(record['ready']['identity']['process'] == process
         and record['ready']['workerConfigSha256'] == context['workerConfig']['sha256'], 'Loaded worker/config differs')
    need(intent['source']['replacementSha256'] == digest('winbridge.activation.replacement.v1', case['replacement']['record'])
         and intent['source']['worker'] == record['ready']['identity']['image'], 'Activation detached from actual file exchange')
    job = 'Global\\WinBridge-Activation-' + digest('winbridge.activation.job.v1', intent)
    need(record['created']['jobName'] == job and p['actualJob']['jobName'] == job
         and p['actualJob']['processId'] == process['processId']
         and p['actualJob']['creationTime'] == int(process['birth']), 'Actual Job/process differs')
    for key in ('actualJobOccupied', 'actualProcessInJob', 'childInstanceLockOccupied', 'oldCachedAuthorityRefused', 'lostServiceAckSentOnce', 'userResumeRefused'):
        need(p[key] is True, 'Missing native observation: ' + key)
    for key in ('workerLaunchCount', 'stateHandoffCount', 'savedWorkerCommandCount', 'serviceDispatchCount'):
        need(p[key] == 1, 'Repeated effect: ' + key)
    need(p['normalServiceStarted'] is True and p['controllerStarted'] is False
         and p['publicActivationEnabled'] is False, 'Local verification claims a later phase')
    old, new = p['workerAuthorityBefore'], p['workerAuthorityAfter']
    for authority in (old, new):
        need(set(authority) == {'incarnation', 'epoch', 'fingerprintSha256'}
             and authority['epoch'] > 0 and re.fullmatch('[0-9a-f]{64}', authority['fingerprintSha256']),
             'Invalid public journal authority')
    need(old == case['beforeReplacement']['finalStateBackup']['record']['report']['sourceAuthority']
         and str(uuid.UUID(new['incarnation'])) == new['incarnation'] == old['incarnation']
         and new['epoch'] == old['epoch'] + 1 and new['fingerprintSha256'] != old['fingerprintSha256'],
         'Writer changed incarnation or advanced epoch more than once')
    w, s = p['writerRecord'], p['serviceRecord']
    wc, sc = w['command'], s['command']
    require_scope(wc['binding'])
    need(wc['binding']['physical'] == physical and wc['binding']['expectedAuthority'] == old
         and wc['binding']['activationRecordSha256'] == digest('winbridge.activation.writer-binding.v1', history[6])
         and wc['binding']['workerConfigSha256'] == context['workerConfig']['sha256'], 'Writer binding differs')
    receipt = p['actualWorkerReceipt']
    need(w['state'] == {'phase': 'confirmed', 'receipt': receipt} and w['revision'] == 1
         and receipt['authority'] == new and receipt['commandSha256'] == digest('winbridge.activation.writer-command.v1', wc),
         'Writer receipt/command differs')
    for saved, rows in ((w, p['writerHistory']), (s, p['serviceHistory'])):
        need(len(rows) == 2 and rows[0] == dict(schemaVersion=1, revision=0, command=saved['command'],
             state={'phase': 'issued'}, updatedAtMs=saved['command']['issuedAtMs']) and rows[1] == saved
             and saved['revision'] == 1 and saved['state']['phase'] == 'confirmed', 'Skipped/replayed delivery')
    need(sc['binding'] == dict(writerCommand=wc, writerReceipt=receipt,
         writerRecordSha256=digest('winbridge.activation.service-writer.v1', w),
         expectedWorker=record['ready']['identity'], workerConfigSchema=context['workerConfigSchema'],
         protocolVersion=context['protocolVersion']), 'Service changed confirmed writer scope')
    sr = s['state']['receipt']
    need(sr['commandId'] == sc['commandId'] and sr['updateId'] == wc['binding']['updateId']
         and sr['attemptId'] == intent['attemptId'] and sr['authority'] == new
         and sr['commandSha256'] == digest('winbridge.activation.service-command.v1', sc), 'Service receipt differs')
    need(wc['deadlineAtMs'] == sc['deadlineAtMs'] == deadline
         and history[6]['updatedAtMs'] <= wc['issuedAtMs'] <= receipt['committedAtMs'] < deadline
         and receipt['committedAtMs'] <= sc['issuedAtMs'] < deadline, 'Startup grant renewed or reordered')
    need(record['local'] == sr['compatibility'], 'LocalVerified/root do not use the confirmed normal IPC evidence')
    require_scope(sr['compatibility'])
    need(sr['compatibility']['identity'] == record['ready']['identity']
         and sr['compatibility']['context'] == physical['context']
         and sr['compatibility']['generation'] == physical['generation']
         and sr['compatibility']['workerConfigSha256'] == context['workerConfig']['sha256'],
         'Local compatibility changed child/epoch/config/schema')
    need(p['writerConfirmed']['update']['phase'] == 'verifying_local'
         and p['confirmed']['schemaVersion'] == 17 and p['confirmed']['update']['phase'] == 'awaiting_controller'
         and p['confirmed']['update']['revision'] == p['writerConfirmed']['update']['revision'] + 1
         and p['duplicate']['update'] == p['confirmed']['update'], 'Atomic local root or duplicate changed state')
    need(p['confirmed']['activationService']['savedReceipt'] == sr
         and p['confirmed']['activationService'] == p['duplicate']['activationService'], 'Duplicate dispatched another service')
    preflight = p['controllerPreflight']
    need(preflight['beforeSupervisorAdmission'] is True and preflight['activationDeadlineRenewed'] is False
         and preflight['instanceId'] == context['workerContext']['instanceId']
         and preflight['workerGeneration'] == context['previousGeneration']
         and preflight['connection']['state'] == 'connected' and preflight['connection']['sessionId'], 'No actual pre-admission connection')
    for info in (p['initialInfo'], p['finalInfo']):
        connection = info['controllerConnection']
        need(connection['url'] == preflight['connection']['url'] and connection['state'] == 'disconnected'
             and all(connection[k] is None for k in ('sessionId', 'controllerGeneration', 'controllerNodeId', 'lastConfirmedAtMs')),
             'Local service connected early or removed its configured URL')
        need(info['generation'] == physical['generation'] and info['activeTaskCount'] == 0
             and info['build']['sourceCommit'] == target['sourceCommit'] and info['build']['sourceManifestSha256'] == target['manifestSha256']
             and info['updateCoordination']['readOnly'] is True and info['updateCoordination']['lease']['phase'] == 'quiesced',
             'Normal IPC identity/maintenance differs')
    if worker_schema == 19 or 'resourcePolicyReconciliation' in p:
        require_resource_policy_reconciliation(p.get('resourcePolicyReconciliation'),
                                              p['workerAuthorityBefore']['epoch'], p['workerAuthorityAfter']['epoch'])
    need(p['preservedRowsBefore'] == p['preservedRowsAfter'] and p['preservedRowsBefore']['tasks'] > 0
         and p['preservedRowsBefore']['output'] > 0 and len(p['normalHistory']) == p['preservedRowsBefore']['tasks']
         and all(t['mutationRefused'] is True and t['taskId'] == t['task']['taskId'] for t in p['normalHistory'])
         and any(t['output'] for t in p['normalHistory']), 'Preserved history/output differs')
    need(len(p['sharing']) == 5 and all(e['writeDenied'] is True and e['deleteDenied'] is True for e in p['sharing'])
         and p['actualSet']['runtimeDirectory'] == intent['source']['runtimeDirectory'], 'Installed image pins/sharing changed')
    need(p['userPause']['result']['kind'] == 'observed'
         and p['userPause']['result']['observation']['mode'] == 'paused'
         and p['userPause']['result']['observation']['readOnly'] is True, 'User pause lost')
    need(cold['view']['activationService']['phase'] == 'not_resident'
         and cold['view']['activationService']['savedReceipt'] == sr
         and cold['view']['activationService']['residentProcess'] is None
         and cold['view']['activationService']['processAlive'] is None
         and cold['view']['update']['phase'] == 'awaiting_controller', 'Cold history inferred residency/readiness')
    for key in ('actualRetainedProcessExited', 'activationJobEmpty', 'sameInstanceLockAvailable', 'savedHistoryUnchanged'):
        need(cold[key] is True, 'Exit not confirmed: ' + key)
    need(cold['automaticColdLaunchCount'] == 0 and cold['normalServiceResident'] is False
         and cold['workerBytesAfter'] == p['workerBytesAfter'] == case['afterOwnedExit']['workerBytesBeforeStartup']
         == case['afterOwnedExit']['workerBytesAfterStartup'], 'Cold path changed bytes/launched another worker')
    need(reservation['sourceCommit'] == source['sourceCommit']
         and reservation['sourceManifestSha256'] == source['manifestSha256'], 'Original reservation source differs')
    return {'passed': True, 'schemaVersion': 1, 'kind': 'local_service_prefix_independent_audit', 'updateId': wc['binding']['updateId'],
            'serviceCommandId': sc['commandId'], 'source': source, 'target': target, 'migrationChecked': False,
            'profile': profile, 'workerJournalSchema': worker_schema, 'updaterJournalSchema': updater_schema}


def audit(proof, source, target, reservation, *, profile=DEFAULT_PROFILE):
    result = audit_local_prefix(proof, source, target, reservation, profile=profile,
                 expected_schemas=profile_schemas(profile), proof_kind='activation_service_main_native_proof')
    return dict(result, kind='main25_independent_audit')


def audit_local_prefix(proof, source, target, reservation, *, profile, expected_schemas, proof_kind):
    result = verify_local_prefix(proof, source, target, reservation, profile=profile, expected_schemas=expected_schemas, proof_kind=proof_kind)
    base = 'cases/0/activationWriter/'
    mutations = [
        ('kind', 'activation_writer_main_native_proof'),
        (base+'record/intent/context/workerJournalSchema', 18),
        (base+'writerRecord/command/binding/workerJournalSchema', 18),
        (base+'serviceRecord/state/receipt/compatibility/workerJournalSchema', 18),
        (base+'serviceDispatchCount', True), (base+'workerLaunchCount', 2),
        (base+'workerAuthorityAfter/epoch', 0), (base+'preservedRowsAfter/sha256', '0'*64),
        (base+'serviceRecord/command/deadlineAtMs', 1),
        (base+'serviceRecord/command/binding/writerRecordSha256', '0'*64),
        (base+'serviceRecord/state/receipt/commandSha256', '0'*64),
        (base+'initialInfo/controllerConnection/sessionId', str(uuid.uuid4())),
        (base+'finalInfo/controllerConnection/url', 'ws://127.0.0.1:1143'),
        (base+'userPause/result/observation/mode', 'running'),
        (base+'controllerPreflight/beforeSupervisorAdmission', False),
        (base+'oldCachedAuthorityRefused', False),
        (base+'record/revision', True), (base+'history/6/state/phase', 'local_verified'),
        ('cases/0/coldActivationWriter/view/activationService/processAlive', True),
        ('cases/0/coldActivationWriter/automaticColdLaunchCount', 1),
    ]
    if 'resourcePolicyReconciliation' in proof['cases'][0]['activationWriter']:
        mutations += [(base+'resourcePolicyReconciliation/beforeEpoch', True),
                      (base+'resourcePolicyReconciliation/afterEpoch', 0),
                      (base+'resourcePolicyReconciliation/policyUnchanged', False),
                      (base+'resourcePolicyReconciliation/normalizedColumn', 'resource_policy')]
    for path, replacement in mutations:
        bad = copy.deepcopy(proof)
        node = bad
        parts = path.split('/')
        for part in parts[:-1]:
            node = node[int(part)] if isinstance(node, list) else node[part]
        key = int(parts[-1]) if isinstance(node, list) else parts[-1]
        need(canonical(node[key]) != canonical(replacement), 'No-op negative audit')
        node[key] = replacement
        try:
            verify_local_prefix(bad, source, target, reservation, profile=profile, expected_schemas=expected_schemas, proof_kind=proof_kind)
        except (ValueError, KeyError, IndexError, TypeError):
            continue
        raise ValueError('Accepted negative audit: ' + path)
    return dict(result, negativeAudits=len(mutations))


def verify_auxiliary(files, proof, source):
    """Audit the original native barriers, separately exported by the runner."""
    p = proof['cases'][0]['activationWriter']
    history, context = p['history'], p['record']['intent']['context']
    deadline = p['record']['intent']['admittedAtMs'] + context['startupTimeoutMs']
    saved = history[5]['intent']['source']
    expected_input = dict(schemaVersion=1, kind='winbridge_activation_process_validation',
                          updateId=saved['updateId'], launchId=saved['launchId'], ownerId=saved['ownerId'],
                          ownerEpoch=saved['ownerEpoch'], attemptId=history[5]['intent']['attemptId'],
                          runtimeIdentity=saved['runtimeIdentity'], updaterConfigSha256=saved['updaterConfigSha256'],
                          stateTraceOnly=True)
    markers = []
    for phase, available in (('before_release', False), ('after_release', True)):
        marker = json.loads(files['None-activation-state-'+phase+'.json'].decode('utf-8-sig'))
        strict(marker)
        need(type(marker['schemaVersion']) is int and marker['schemaVersion'] == 1
             and marker['kind'] == 'activation_state_lock_boundary' and marker['phase'] == phase
             and marker['record'] == history[5] and marker['lockAvailable'] is available
             and marker['observedState'] == context['workerState']
             and marker['processId'] == context['supervisor']['processId']
             and marker['recordSha256'] == digest('winbridge.activation.barrier.record.v1', history[5])
             and marker['handoffRecordSha256'] == digest('winbridge.activation.state-handoff.v1', history[5]),
             'Original lock boundary differs')
        need(canonical(marker['input']) == canonical(json.loads(marker['inputEncoded'])) == canonical(expected_input)
             and hashlib.sha256(marker['inputEncoded'].encode()).hexdigest() == marker['inputSha256']
             and marker['input']['stateTraceOnly'] is True, 'Changed native barrier input')
        need(marker['build']['sourceCommit'] == source['sourceCommit']
             and marker['build']['sourceManifestSha256'] == source['manifestSha256']
             and marker['build']['sourceDirty'] is False, 'Barrier used another supervisor')
        markers.append(marker)
    need(history[5]['updatedAtMs'] <= markers[0]['observedAtMs'] <= markers[1]['observedAtMs']
         <= history[6]['stateOwned']['observedAtMs'] <= deadline, 'Physical handoff chronology differs')
    marker = json.loads(files['None-activation-writer-before-send.json'].decode('utf-8-sig'))
    strict(marker)
    command = p['writerRecord']['command']
    need(type(marker['schemaVersion']) is int and marker['schemaVersion'] == 1
         and marker['kind'] == 'activation_writer_before_send' and marker['record'] == history[6]
         and marker['processId'] == context['supervisor']['processId']
         and all(marker[key] == markers[0][key] == markers[1][key]
                 for key in ('build', 'input', 'inputEncoded', 'inputSha256'))
         and marker['recordSha256'] == digest('winbridge.activation.barrier.record.v1', history[6])
         and marker['commandId'] == command['commandId']
         and marker['commandSha256'] == digest('winbridge.activation.writer-command.v1', command)
         and command['issuedAtMs'] <= marker['observedAtMs'] <= p['actualWorkerReceipt']['committedAtMs'],
         'Lost writer ACK is not bound to its original single send')
    return {'nativeMarkersVerified': 3}


def audit_auxiliary(files, proof, source):
    result = verify_auxiliary(files, proof, source)
    mutations = []
    for phase in ('before_release', 'after_release'):
        name = 'None-activation-state-' + phase + '.json'
        mutations.extend([(name, 'processId', 0), (name, 'schemaVersion', True),
                          (name, 'input/ownerId', 'foreign-owner')])
    name = 'None-activation-writer-before-send.json'
    mutations.extend([(name, 'processId', 0), (name, 'schemaVersion', True),
                      (name, 'build/sourceCommit', '0' * 40), (name, 'input/ownerEpoch', 0)])
    for name, path, replacement in mutations:
        changed = json.loads(files[name].decode('utf-8-sig'))
        node = changed
        parts = path.split('/')
        for key in parts[:-1]:
            node = node[key]
        need(canonical(node[parts[-1]]) != canonical(replacement), 'No-op native marker mutation')
        node[parts[-1]] = replacement
        if path.startswith('input/'):
            changed['inputEncoded'] = canonical(changed['input']).decode()
            changed['inputSha256'] = hashlib.sha256(changed['inputEncoded'].encode()).hexdigest()
        altered = dict(files, **{name: canonical(changed)})
        try:
            verify_auxiliary(altered, proof, source)
        except (ValueError, KeyError, TypeError):
            continue
        raise ValueError('Native barrier observer accepted mutation: ' + path)
    return dict(result, nativeMarkerNegativeAudits=len(mutations))


def verify_worker_migration(proof, reference_sha256, source):
    """Separate cold worker17->19 gate, never active updater migration authority."""
    strict(proof)
    need(type(proof.get('schemaVersion')) is int and proof['schemaVersion'] == 1,
         'Unsupported native worker migration proof version')
    need(proof.get('kind') == 'worker_attribution_migration_native_proof'
         and proof.get('passed') is True and proof.get('platform') == 'windows', 'Missing native worker migration proof')
    need(type(proof.get('fromSchema')) is int and proof['fromSchema'] == 17
         and type(proof.get('toSchema')) is int and proof['toSchema'] == 19,
         'Worker migration must be exactly17->19')
    need(re.fullmatch('[0-9a-f]{64}', reference_sha256 or '')
         and proof['referenceSha256'] == reference_sha256, 'Worker17 reference pin differs')
    need(proof['build']['sourceDirty'] is False and proof['build']['sourceCommit'] == source['sourceCommit']
         and proof['build']['sourceManifestSha256'] == source['manifestSha256'], 'Migration used another worker19 build')
    for flag in ('oldHistoryPreserved', 'verifiedMigrationBackup', 'outputPreserved', 'dedupPreserved',
                 'legacyCreatorUnknown', 'creatorSurvivesRestart', 'cancellationSurvivesRestart', 'oldWorkerRefusedWithoutJournalWrite'):
        need(proof.get(flag) is True, 'Missing worker migration evidence: '+flag)
    need(proof.get('installedStateOpened') is False and proof.get('activeUpdaterMigrationChecked') is False,
         'Cold fixture must not claim installed/active-updater migration')
    for field, expected in (('completedExternalExecutions', 2), ('startedAndCancelledProcesses', 1)):
        need(type(proof.get(field)) is int and proof[field] == expected, 'Unexpected migration execution count')
    return {'workerJournalMigrationChecked': True, 'workerMigrationScope': 'isolated cold17-to19; no active updater migration'}
