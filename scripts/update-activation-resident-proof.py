"""Independent Main29: actual authenticated current header, no release/commit."""
import copy
import importlib.util
from pathlib import Path
import re
import uuid

spec = importlib.util.spec_from_file_location('resident_ready_prefix', Path(__file__).with_name('update-activation-lifetime-ready-loss-proof.py'))
ready = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ready)
lifetime, controller, service = ready.lifetime, ready.controller, ready.service
need, digest, canonical = ready.need, ready.digest, ready.canonical
PROFILE = ready.PROFILE
KIND = 'activation_resident_main_native_proof'
TEST = 'actual_main_observes_resident_connection_after_partial_ready_without_replay'
profile_schemas = ready.profile_schemas
verify_worker_migration = ready.verify_worker_migration
verify_activity = ready.verify_activity if hasattr(ready, 'verify_activity') else controller.verify_activity
audit_auxiliary = ready.audit_auxiliary


def verify_main_selection(command, log):
    need(command['arguments'] == ['test','-p','winbridge-worker','--features','bundled-sqlite,update-validation',
         '--test','update_shutdown','--locked','--offline','--','--exact',TEST,'--ignored','--test-threads=1','--nocapture'], 'Main29 exact selection differs')
    log = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    summaries = re.findall(r'^test result: ([^\r\n]+)', log, re.MULTILINE)
    need(len(re.findall(r'^test '+re.escape(TEST)+r' \.\.\.', log, re.MULTILINE)) == 1
         and re.search(r'^running 1 test\s*$', log, re.MULTILINE) and len(summaries) == 1
         and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;'), 'Main29 did not execute exactly once')


def verify_boundaries(commands, files, *, resident_model_count=13):
    ready.verify_boundaries(commands, files)
    definitions = [
        ('resident-model-and-actor', 'winbridge-update', ['--features','bundled-sqlite,update-validation','--lib','resident'], resident_model_count,
         ['resident_completion_binds_the_original_graph_query_and_historical_lifetime',
          'a_cold_actor_cannot_observe_even_with_a_saved_lifetime_key',
          'pending_lifetime_reads_keep_service_started_without_enabling_a_second_start',
          'resident_cold_view20_is_separate_bounded_readonly_data_without_a_prefix_grant',
          'resident_mailbox_rejects_unbound_or_rebound_requests_without_a_worker_launch',
          'current_reconnected_session_outlives_entry_window_without_rewriting_the_receipt',
          'responses_cannot_rebind_scope_time_provider_or_selected_current_session']),
        ('resident-v28-mailbox', 'winbridge-worker', ['--features','bundled-sqlite,update-validation','--lib',
          'update_control::tests::resident_connection_v28_cannot_adopt_cold_history_or_change_legacy_views'], 1,
         ['resident_connection_v28_cannot_adopt_cold_history_or_change_legacy_views']),
        ('resident-os-peer', 'winbridge-transport', ['--lib','local_peer'], 1,
         ['accepted_named_pipe_authenticates_the_real_client_pid']),
        ('resident-fixture-reply', 'winbridge-worker', ['--features','bundled-sqlite,update-validation','--test','update_activation_fixture_reply'], 3,
         ['native_adapter_accepts_a_real_compact_v28_view_and_checks_its_block',
          'native_adapter_rejects_foreign_schema_scope_and_reported_errors',
          'native_adapter_never_treats_unknown_contracts_as_successful_busy_replies']),
    ]
    for name, package, args, count, required in definitions:
        selected = [c for c in commands if c['name'] == name]
        need(len(selected) == 1, 'Missing/repeated native resident boundary: '+name)
        command, = selected
        expected = ['test','-p',package]+args+['--locked','--offline','--']
        if name == 'resident-v28-mailbox': expected += ['--exact']
        expected += ['--test-threads=1','--nocapture']
        need(command['arguments'] == expected, 'Resident boundary selection differs: '+name)
        log = files[command['log']].decode('utf-8-sig')
        need(f'test result: ok. {count} passed; 0 failed; 0 ignored;' in log, 'Resident boundary count differs: '+name)
        for case in required: need(case+' ... ok' in log, 'Missing native resident case: '+case)


def _uuid(value):
    return type(value) is str and str(uuid.UUID(value)) == value


def verify_resident(case):
    p, cold = case['activationResident'], case['coldActivationResident']
    controller.typed(p); controller.typed(cold); service.strict(p); service.strict(cold)
    need(set(p) == {'first','second','wrongPeer','pidSpoof','unsupportedVersion','updaterBefore','updaterAfter',
         'workerBytesBefore','workerBytesAfter','lifetimeHistoryBefore','lifetimeHistoryAfter','sameLifetimeCommand',
         'rootCommitted','maintenanceReleased','privateChannelRead'}, 'Unknown resident proof fields')
    saved = case['activationLifetime']['record']; command = saved['command']; b = command['binding']; entry = saved['state']['receipt']
    root = case['activationLifetime']['afterExpiry']['update']
    info = p['updaterBefore'][0]
    need(set(info) == {'schemaVersion','installationId','scopeSha256','ownerEpoch','activeUpdateId','lastSequence'}
         and _uuid(info['installationId']) and re.fullmatch('[0-9a-f]{64}', info['scopeSha256'])
         and type(info['lastSequence']) is int and 0 < info['lastSequence'] <= 9007199254740991,
         'Resident updater snapshot identity/sequence malformed')
    need(p['updaterBefore'] == p['updaterAfter'] and len(p['updaterBefore']) == 2
         and p['updaterBefore'][1] == root and p['updaterBefore'][0]['schemaVersion'] == 22
         and p['updaterBefore'][0]['ownerEpoch'] == b['updaterEpoch']
         and p['updaterBefore'][0]['activeUpdateId'] == b['updateId'], 'Readonly resident query changed updater state/owner')
    need(p['workerBytesBefore'] == p['workerBytesAfter'] == case['activationLifetime']['workerBytesAfter']
         and p['lifetimeHistoryBefore'] == p['lifetimeHistoryAfter'] == case['activationLifetime']['history'], 'Resident query mutated worker/history')
    need(p['sameLifetimeCommand'] is True and all(p[k] is False for k in ('rootCommitted','maintenanceReleased','privateChannelRead')), 'Resident query overstates effects')
    ids, times = [], []
    old_session = case['activationController']['record']['state']['receipt']['session']
    for name in ('first', 'second'):
        view = p[name]; v = view['activationResident']; obs = v['observation']; r = obs['result']; f = r['frame']; link = r['link']; c = link['connection']
        need(view['schemaVersion'] == 20 and view['driverActive'] is False and view['driverError'] is None and view['update'] == root
             and all(k not in view for k in ('activationLifetime','activationController','activationService','activationWriter','runtimeReplacement')), 'Resident view is not compact/current-only')
        need(set(v) == {'phase','lifetimeCommandId','residentProcess','processAlive','originalCustodyHeld','observation','reason'}
             and v['phase'] == 'observed' and v['lifetimeCommandId'] == command['commandId']
             and v['residentProcess'] == entry['compatibility']['identity']['process']
             and v['processAlive'] is True and v['originalCustodyHeld'] is True, 'Resident view changed original child/key/custody')
        need(set(obs) == {'residentObservationVersion','requestId','result'} and type(obs['residentObservationVersion']) is int and obs['residentObservationVersion'] == 7
             and _uuid(obs['requestId']) and obs['requestId'] != command['commandId'], 'Resident correlation/version differs')
        need(set(r) == {'kind','entry','frame','link','observedAtMs'} and r['kind'] == 'observed' and r['entry'] == entry
             and type(r['observedAtMs']) is int and command['deadlineAtMs'] < r['observedAtMs'] <= 9007199254740991, 'Resident entry rewritten or observation not late')
        need(set(f) == {'schemaVersion','controllerBuild','controllerGeneration','sessionId','providerBuild','providerGeneration','context','sequence','projectionSha256'}
             and f['schemaVersion'] == 1 and _uuid(f['controllerGeneration']) and _uuid(f['sessionId'])
             and type(f['sequence']) is int and 0 < f['sequence'] <= 9007199254740991
             and re.fullmatch('[0-9a-f]{64}', f['projectionSha256'])
             and f['providerBuild'] == b['compatibility']['identity']['descriptor']['build']
             and f['providerGeneration'] == b['compatibility']['generation'] and f['context'] == b['compatibility']['context']
             and f['controllerBuild'] == old_session['controllerBuild'] and f['controllerGeneration'] == old_session['controllerGeneration'], 'Resident accepted header/provider differs')
        endpoint = case['activationController']['record']['command']['binding']['topology']
        endpoint, = [e for e in endpoint['controllers'] if e['id'] == endpoint['confirmationEndpointId']]
        attempt = int(link['attempts'])
        need(link['id'] == endpoint['id'] and c['url'] == endpoint['url'] and c['state'] == 'connected' and c['error'] is None
             and c['sessionId'] == f['sessionId'] and c['controllerGeneration'] == f['controllerGeneration']
             and c['controllerNodeId'] == old_session['controllerNodeId'] and attempt > 0 and link['attempts'] == str(attempt)
             and type(link['lastConnectedAtMs']) is int and type(c['lastConfirmedAtMs']) is int
             and link['lastConnectedAtMs'] <= c['lastConfirmedAtMs'] <= r['observedAtMs']
             and (link['lastDisconnectedAtMs'] is None or link['lastDisconnectedAtMs'] <= link['lastConnectedAtMs']), 'Resident current link/heartbeat differs')
        if f['sessionId'] == old_session['sessionId']:
            need(attempt == old_session['connectionAttempt'] and f['sequence'] >= old_session['projectionSequence'], 'Resident sequence/attempt regressed')
        else: need(attempt > old_session['connectionAttempt'], 'New resident session lacks a newer connection attempt')
        ids.append(obs['requestId']); times.append(r['observedAtMs'])
    need(ids[0] != ids[1] and times[1] >= times[0], 'Fresh read reused correlation or time regressed')
    wrong = p['wrongPeer']; query = wrong['query']; response = wrong['response']
    requester = wrong['requester']
    need(set(wrong) == {'requester','query','response'}
         and type(requester['processId']) is int and 0 < requester['processId'] <= 4294967295
         and requester['processId'] != b['keeper']['processId'] and requester['hostId'] == b['keeper']['hostId']
         and requester['sessionId'] == b['keeper']['sessionId']
         and set(query) == {'residentObservationVersion','requestId','command'}
         and query['residentObservationVersion'] == 7 and _uuid(query['requestId']) and query['command'] == command
         and set(response) == {'residentObservationVersion','requestId','result'}
         and response['residentObservationVersion'] == 7 and response['requestId'] == query['requestId']
         and set(response['result']) == {'kind','message'} and response['result']['kind'] == 'rejected'
         and 'actual original keeper peer' in response['result']['message'], 'Actual foreign peer was not specifically refused')
    need(p['pidSpoof']['query'] == dict(query, processId=b['keeper']['processId'])
         and p['pidSpoof']['response']['residentObservationVersion'] == 7 and p['pidSpoof']['response']['requestId'] is None
         and p['pidSpoof']['response']['result']['kind'] == 'rejected', 'JSON PID spoof became peer evidence')
    need(p['unsupportedVersion']['query'] == dict(query, residentObservationVersion=6)
         and p['unsupportedVersion']['response']['residentObservationVersion'] == 7
         and p['unsupportedVersion']['response']['result']['kind'] == 'rejected', 'V7 fell back to V6')
    v = cold['view']['activationResident']
    need(set(cold) == {'view','historyUnchanged','automaticColdLaunchCount','noColdResidentAdoption'}
         and cold['view']['schemaVersion'] == 20 and cold['view']['driverActive'] is False
         and cold['view']['driverError'] is None and cold['view']['update'] == root and v['phase'] == 'not_resident'
         and v['lifetimeCommandId'] == command['commandId']
         and all(v[k] is None for k in ('residentProcess','processAlive','originalCustodyHeld','observation'))
         and cold['historyUnchanged'] is True and type(cold['automaticColdLaunchCount']) is int and cold['automaticColdLaunchCount'] == 0
         and cold['noColdResidentAdoption'] is True, 'Cold resident data recreated a process/observation')
    return ids


def audit_resident(case):
    ids = verify_resident(case)
    changes = [('activationResident/first/schemaVersion',19), ('activationResident/first/activationResident/phase','unconfirmed'),
        ('activationResident/first/activationResident/processAlive',None), ('activationResident/first/activationResident/originalCustodyHeld',1),
        ('activationResident/first/activationResident/lifetimeCommandId',str(uuid.uuid4())),
        ('activationResident/first/activationResident/observation/residentObservationVersion',True),
        ('activationResident/first/activationResident/observation/requestId',case['activationLifetime']['record']['command']['commandId']),
        ('activationResident/first/activationResident/observation/result/observedAtMs',0),
        ('activationResident/first/activationResident/observation/result/entry/jobName','foreign'),
        ('activationResident/first/activationResident/observation/result/frame/providerGeneration',str(uuid.uuid4())),
        ('activationResident/first/activationResident/observation/result/frame/projectionSha256','bad'),
        ('activationResident/first/activationResident/observation/result/frame/sequence',0),
        ('activationResident/first/activationResident/observation/result/link/connection/error','Disconnected'),
        ('activationResident/first/activationResident/observation/result/link/connection/sessionId',str(uuid.uuid4())),
        ('activationResident/first/activationResident/observation/result/link/connection/lastConfirmedAtMs',9007199254740991),
        ('activationResident/second/activationResident/observation/requestId',ids[0]),
        ('activationResident/wrongPeer/requester/processId',case['activationLifetime']['record']['command']['binding']['keeper']['processId']),
        ('activationResident/wrongPeer/requester/processId',0),
        ('activationResident/wrongPeer/response/result/kind','observed'),
        ('activationResident/pidSpoof/response/requestId',str(uuid.uuid4())),
        ('activationResident/unsupportedVersion/query/residentObservationVersion',7),
        ('activationResident/workerBytesAfter',None), ('activationResident/lifetimeHistoryAfter',[]),
        ('activationResident/updaterAfter',None), ('activationResident/privateChannelRead',True),
        ('activationResident/rootCommitted',True), ('coldActivationResident/view/activationResident/originalCustodyHeld',True),
        ('coldActivationResident/automaticColdLaunchCount',1), ('coldActivationResident/view/driverActive',True)]
    for path, value in changes:
        bad = copy.deepcopy(case); node = bad; parts = path.split('/')
        for part in parts[:-1]: node = node[part]
        need(canonical(node[parts[-1]]) != canonical(value), 'No-op resident negative: '+path)
        node[parts[-1]] = value
        try: verify_resident(bad)
        except (ValueError, KeyError, TypeError, IndexError): continue
        raise ValueError('Resident observer accepted mutation: '+path)
    return ids, len(changes)


def verify_resident_activity(rows, proof):
    case, = proof['cases']; p = case['activationResident']
    for name in ('first','second'):
        frame = p[name]['activationResident']['observation']['result']['frame']
        need(any(r.get('event') == 'worker_connected' and r.get('sessionId') == frame['sessionId']
                 and r.get('generation') == frame['providerGeneration'] for r in rows), 'No actual Debian event for observed resident session')


def audit(proof, source, target, reservation, *, profile=PROFILE):
    profile_schemas(profile)
    result = service.audit_local_prefix(proof, source, target, reservation, profile=profile, expected_schemas=(19,22), proof_kind=KIND)
    case, = proof['cases']
    controller_key, controller_count = controller.audit_controller_case(case)
    lifetime_key, lifetime_count = lifetime.audit_lifetime_case(case, ready_loss=True)
    _, fault_count = ready.audit_fault(case, source)
    ids, count = audit_resident(case)
    return dict(result, kind='main29_independent_audit', controllerCommandId=controller_key, controllerNegativeAudits=controller_count,
                lifetimeCommandId=lifetime_key, lifetimeNegativeAudits=lifetime_count, readyLossNegativeAudits=fault_count,
                residentRequestIds=ids, residentNegativeAudits=count, fullProjectionBodyIndependentlyVerified=False)
