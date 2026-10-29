"""Synthetic Main29 reader regressions, never Windows execution evidence."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest
import uuid


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(path))
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module


p = load('resident_proof', 'update-activation-resident-proof.py')
base = load('resident_case_prefix', 'test-update-activation-lifetime-ready-loss-proof.py')


def fixture():
    case = base.fixture(); life = case['activationLifetime']; command = life['record']['command']; b = command['binding']
    entry = life['record']['state']['receipt']; old = case['activationController']['record']['state']['receipt']['session']
    root = life['afterExpiry']['update']; at = command['deadlineAtMs'] + 100
    frame = dict(schemaVersion=1, controllerBuild=old['controllerBuild'], controllerGeneration=old['controllerGeneration'],
        sessionId=old['sessionId'], providerBuild=b['compatibility']['identity']['descriptor']['build'],
        providerGeneration=b['compatibility']['generation'], context=b['compatibility']['context'],
        sequence=old['projectionSequence']+2, projectionSha256='a'*64)
    link = dict(id=old['controllerId'], attempts=str(old['connectionAttempt']),
        connection=dict(url=old['controllerUrl'], state='connected', sessionId=old['sessionId'], controllerNodeId=old['controllerNodeId'],
            controllerGeneration=old['controllerGeneration'], lastConfirmedAtMs=at-1, error=None),
        lastConnectedAtMs=old['connectedAtMs'], lastDisconnectedAtMs=None, lastError=None,
        rpc=dict(inFlight=0, limit=16, admitted='0', completed='0', rejected='0'))
    def view(t):
        observed = dict(residentObservationVersion=7, requestId=str(uuid.uuid4()),
            result=dict(kind='observed', entry=copy.deepcopy(entry), frame=copy.deepcopy(frame), link=copy.deepcopy(link), observedAtMs=t))
        return dict(schemaVersion=20, update=copy.deepcopy(root), driverActive=False, driverError=None,
            activationResident=dict(phase='observed', lifetimeCommandId=command['commandId'], residentProcess=entry['compatibility']['identity']['process'],
                processAlive=True, originalCustodyHeld=True, observation=observed, reason='Observed at this timestamp'))
    q = dict(residentObservationVersion=7, requestId=str(uuid.uuid4()), command=copy.deepcopy(command))
    reply = dict(residentObservationVersion=7, requestId=q['requestId'], result=dict(kind='rejected', message='Requires actual original keeper peer and command'))
    info = dict(schemaVersion=22, installationId=str(uuid.uuid4()), scopeSha256='c'*64, ownerEpoch=b['updaterEpoch'], activeUpdateId=b['updateId'], lastSequence=1)
    query6 = dict(q, residentObservationVersion=6)
    case['activationResident'] = dict(first=view(at), second=view(at+1),
        wrongPeer=dict(requester=dict(b['keeper'],processId=b['keeper']['processId']+1), query=q, response=reply),
        pidSpoof=dict(query=dict(q, processId=b['keeper']['processId']), response=dict(reply,requestId=None)),
        unsupportedVersion=dict(query=query6, response=copy.deepcopy(reply)),
        updaterBefore=[info,root], updaterAfter=[info,root], workerBytesBefore=life['workerBytesAfter'], workerBytesAfter=life['workerBytesAfter'],
        lifetimeHistoryBefore=life['history'], lifetimeHistoryAfter=life['history'], sameLifetimeCommand=True,
        rootCommitted=False, maintenanceReleased=False, privateChannelRead=False)
    cold = dict(schemaVersion=20, update=copy.deepcopy(root), driverActive=False, driverError=None, activationResident=dict(phase='not_resident', lifetimeCommandId=command['commandId'],
        residentProcess=None, processAlive=None, originalCustodyHeld=None, observation=None, reason='No actual owner'))
    case['coldActivationResident'] = dict(view=cold, historyUnchanged=True, automaticColdLaunchCount=0, noColdResidentAdoption=True)
    return json.loads(json.dumps(case))


class ResidentProofTests(unittest.TestCase):
    def test_original_scope_late_current_header_foreign_peer_and_all_negative_audits(self):
        case = fixture()
        ids, count = p.audit_resident(case)
        self.assertEqual(count, 29); self.assertNotEqual(*ids)
        self.assertEqual(p.lifetime.audit_lifetime_case(case, ready_loss=True)[1], 37)
        self.assertEqual(p.ready.audit_fault(case)[1], 15)

    def test_current_sessions_require_actual_debian_events(self):
        case = fixture(); frame = case['activationResident']['first']['activationResident']['observation']['result']['frame']
        rows = [dict(event='worker_connected',sessionId=frame['sessionId'],generation=frame['providerGeneration'])]
        p.verify_resident_activity(rows,dict(cases=[case]))
        with self.assertRaises(ValueError): p.verify_resident_activity([],dict(cases=[case]))

    def test_exact_main29_cannot_be_a_relabelled_main28_log(self):
        args = ['test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--test','update_shutdown',
            '--locked','--offline','--','--exact',p.TEST,'--ignored','--test-threads=1','--nocapture']
        log = f'running 1 test\ntest {p.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
        p.verify_main_selection(dict(arguments=args),log)
        with self.assertRaises(ValueError): p.verify_main_selection(dict(arguments=args),log.replace(p.TEST,p.ready.TEST))
        with self.assertRaises(ValueError): p.ready.verify_main_selection(dict(arguments=args),log)

    def test_coherent_key_reissue_and_new_window_still_fail_the_original_prefix(self):
        case = fixture()
        for alias in p.lifetime.key_aliases(case):
            with self.assertRaises(ValueError): p.ready.verify_fault(p.lifetime.coherent_key_alias(case,alias))
        case['activationLifetime']['record']['command']['deadlineAtMs'] += 1
        with self.assertRaises(ValueError): p.ready.verify_fault(case)

    def test_explicit_new_mode_does_not_change_historical_runner_defaults(self):
        runner = load('resident_runner','run-update-service-main-native.py')
        verifier = load('resident_verifier','verify-update-service-main-native.py')
        import inspect
        self.assertIs(inspect.signature(runner.main).parameters['resident'].default,False)
        self.assertIs(inspect.signature(verifier.main).parameters['resident'].default,False)
        ps = Path(__file__).with_name('test-update-health-actor.ps1').read_text()
        self.assertIn("$shutdownArguments.residentConnection=$true",ps)
        self.assertIn("$mainTest='"+p.TEST+"'",ps)

    def test_native_boundaries_cannot_replace_windows_peer_proof_with_unix_or_empty_tests(self):
        old_names = ['partial_ready_consumes_only_header_and_preserves_same_stream_body',
            'expired_original_window_consumes_nothing_and_cannot_be_renewed',
            'invalid_or_truncated_header_never_becomes_a_fault_success',
            'lifetime_ready_loss_fixture_requires_exact_local7_record_and_command_scope',
            'lifetime_ready_loss_input_rejects_unknown_grants_and_cannot_relabel_a_trace']
        names = ['resident_completion_binds_the_original_graph_query_and_historical_lifetime',
            'a_cold_actor_cannot_observe_even_with_a_saved_lifetime_key',
            'pending_lifetime_reads_keep_service_started_without_enabling_a_second_start',
            'resident_cold_view20_is_separate_bounded_readonly_data_without_a_prefix_grant',
            'resident_mailbox_rejects_unbound_or_rebound_requests_without_a_worker_launch',
            'current_reconnected_session_outlives_entry_window_without_rewriting_the_receipt',
            'responses_cannot_rebind_scope_time_provider_or_selected_current_session']
        definitions = [('lifetime-ready-loss-native-boundaries','winbridge-update',
            ['--features','bundled-sqlite,update-validation','--lib','lifetime_ready_loss'],5,old_names),
            ('resident-model-and-actor','winbridge-update',['--features','bundled-sqlite,update-validation','--lib','resident'],13,names),
            ('resident-v28-mailbox','winbridge-worker',['--features','bundled-sqlite,update-validation','--lib',
                'update_control::tests::resident_connection_v28_cannot_adopt_cold_history_or_change_legacy_views'],1,
                ['resident_connection_v28_cannot_adopt_cold_history_or_change_legacy_views']),
            ('resident-os-peer','winbridge-transport',['--lib','local_peer'],1,['accepted_named_pipe_authenticates_the_real_client_pid']),
            ('resident-fixture-reply','winbridge-worker',['--features','bundled-sqlite,update-validation','--test','update_activation_fixture_reply'],3,
                ['native_adapter_accepts_a_real_compact_v28_view_and_checks_its_block',
                 'native_adapter_rejects_foreign_schema_scope_and_reported_errors',
                 'native_adapter_never_treats_unknown_contracts_as_successful_busy_replies'])]
        commands, files = [], {}
        for name, package, args, count, required in definitions:
            arguments = ['test','-p',package]+args+['--locked','--offline','--']
            if name == 'resident-v28-mailbox': arguments += ['--exact']
            arguments += ['--test-threads=1','--nocapture']
            commands.append(dict(name=name,arguments=arguments,log=name+'.log'))
            files[name+'.log'] = ('\n'.join(n+' ... ok' for n in required)+f'\ntest result: ok. {count} passed; 0 failed; 0 ignored;').encode()
        p.verify_boundaries(commands,files)
        bad = dict(files);bad['resident-os-peer.log']=bad['resident-os-peer.log'].replace(
            b'accepted_named_pipe_authenticates_the_real_client_pid', b'peer_pid_comes_from_the_connected_socket_not_its_payload')
        with self.assertRaises(ValueError): p.verify_boundaries(commands,bad)
        with self.assertRaises(ValueError): p.verify_boundaries(commands[:-1],files)
        with self.assertRaises(ValueError): p.verify_boundaries([c for c in commands if c['name'] != 'resident-os-peer'],files)
        bad = dict(files); bad['resident-model-and-actor.log']=bad['resident-model-and-actor.log'].replace(b'13 passed',b'0 passed')
        with self.assertRaises(ValueError): p.verify_boundaries(commands,bad)


if __name__ == '__main__': unittest.main()
