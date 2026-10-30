"""Independent Main30: original V8, interrupted ACK, sealed Active and one effect.

This gate has its own exact19/23 profile. Main25–29 remain frozen. Neither
retained JSON nor a successful SQL row constitutes physical custody here.
"""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import uuid

spec = importlib.util.spec_from_file_location('finalization_resident_prefix', Path(__file__).with_name('update-activation-resident-proof.py'))
resident = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resident)
service, controller = resident.service, resident.controller
need, canonical, digest = service.need, service.canonical, service.digest
verify_worker_migration = service.verify_worker_migration
audit_auxiliary = service.audit_auxiliary
verify_activity = controller.verify_activity
verify_resident_activity = resident.verify_resident_activity
PROFILE = 'worker19-updater23'
KIND = 'activation_finalization_main_native_proof'
TEST = 'actual_main_finalizes_same_worker_after_lost_v8_reply_without_replay'
REASON = 'Обновление WinBridge завершено; maintenance освобождён для этого обновления'


def profile_schemas(profile):
    need(type(profile) is str and profile == PROFILE, 'Main30 requires explicit worker19/updater23')
    return 19, 23


def verify_main_selection(command, log):
    need(command['arguments'] == ['test','-p','winbridge-worker','--features','bundled-sqlite,update-validation',
         '--test','update_shutdown','--locked','--offline','--','--exact',TEST,'--ignored','--test-threads=1','--nocapture'], 'Main30 exact selection differs')
    log = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    summaries = re.findall(r'^test result: ([^\r\n]+)', log, re.MULTILINE)
    need(len(re.findall(r'^test '+re.escape(TEST)+r' \.\.\.', log, re.MULTILINE)) == 1
         and re.search(r'^running 1 test\s*$', log, re.MULTILINE) and len(summaries) == 1
         and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;'), 'Main30 did not execute exactly once')


def verify_boundaries(commands, files):
    resident.verify_boundaries(commands, files)
    definitions = [
        ('finalization-model-ledger', 'winbridge-update', 'finalization', 24, False),
        ('finalization-v29-mailbox', 'winbridge-worker', 'update_control::tests::finalization_v29_cannot_adopt_saved_data_and_old_views_keep_their_shape', 1, True),
        ('finalization-known-seal-bounds', 'winbridge-worker', 'local_service::lifetime::finalization_mailbox::continuation_tests::', 1, False),
    ]
    for name, package, selected, count, exact in definitions:
        matches = [c for c in commands if c['name'] == name]
        need(len(matches) == 1, 'Missing/repeated Main30 boundary: '+name)
        c, = matches
        expected = ['test','-p',package,'--features','bundled-sqlite,update-validation','--lib',selected,'--locked','--offline','--']
        expected += (['--exact'] if exact else [])+['--test-threads=1','--nocapture']
        need(c['arguments'] == expected, 'Main30 boundary selection differs: '+name)
        log = files[c['log']].decode('utf-8-sig')
        need(f'test result: ok. {count} passed; 0 failed; 0 ignored;' in log, 'Main30 boundary count differs: '+name)


def integer(value, expected=None):
    return type(value) is int and 0 <= value <= 9007199254740991 and (expected is None or value == expected)


def uuid_string(value):
    return type(value) is str and str(uuid.UUID(value)) == value


def windows_path(value):
    return PureWindowsPath(value[4:] if value.startswith('\\\\?\\') else value)


def verify(proof, source, target, reservation):
    service.strict(proof)
    controller.typed(proof)
    need(proof['schemaVersion'] == 1 and proof['kind'] == KIND and proof['platform'] == 'windows' and proof['passed'] is True, 'Not a completed Main30 proof')
    need(all(proof[k] is True for k in ('normalMain','actualOldRoles','actualCandidateBuild','validationFeature','runtimeApplied'))
         and proof['installedRuntimeApplied'] is False and proof['windowsOpened'] is False, 'Wrong isolated runtime scope')
    need(proof['target'] == target and proof['build']['sourceDirty'] is False
         and proof['build']['sourceCommit'] == source['sourceCommit']
         and proof['build']['sourceManifestSha256'] == source['manifestSha256']
         and source['sourceCommit'] != target['sourceCommit'], 'Producer/target pin differs')
    case, = proof['cases']
    p, cold = case['activationFinalization'], case['afterOwnedExit']
    base = case['activationLifetime']['record']
    original, entry = base['command'], base['state']['receipt']
    writer = case['activationWriter']; local = writer['record']; context = local['intent']['context']
    need(context['workerJournalSchema'] == 19 and context['updaterJournalSchema'] == 23
         and context['startupTimeoutMs'] == 240000, 'Main30 source contract/startup window differs')
    runtime = PureWindowsPath(reservation['runtimeDirectory'])
    need(runtime == PureWindowsPath(r'K:\_proj-26\winbridge\runtime-test-204')
         and windows_path(local['ready']['identity']['image']['path']) == runtime/'winbridge-worker.exe', 'Worker escaped approved stable network slot')
    physical = local['stateOwned']
    need(writer['actualProcess'] == physical['process'] == local['created']['process'] == entry['compatibility']['identity']['process']
         and writer['actualJob']['jobName'] == local['created']['jobName'] == original['binding']['jobName']
         and writer['actualJob']['processId'] == physical['process']['processId']
         and writer['actualJob']['creationTime'] == int(physical['process']['birth'])
         and all(writer[k] is True for k in ('actualProcessInJob','actualJobOccupied','childInstanceLockOccupied')),
         'Original OS process/Job/instance custody differs')
    need(all(integer(writer[k], 1) for k in ('workerLaunchCount','stateHandoffCount','savedWorkerCommandCount','serviceDispatchCount')), 'Prefix launched/delivered more than once')
    build = entry['compatibility']['identity']['descriptor']['build']
    need(build['sourceDirty'] is False and build['sourceCommit'] == target['sourceCommit']
         and build['sourceManifestSha256'] == target['manifestSha256'], 'Actual target image differs')
    need(base['state']['phase'] == 'confirmed' and integer(base['revision'], 2), 'Original lifetime is not confirmed')
    lb = original['binding']; controller_record = case['activationController']['record']
    need(lb['localRecordSha256'] == digest('winbridge.activation.lifetime-local.v1',local)
         and lb['controllerRecordSha256'] == digest('winbridge.activation.lifetime-controller.v1',controller_record)
         and lb['controllerCommandId'] == controller_record['command']['commandId']
         and lb['keeper'] == context['supervisor'] and lb['keeper']['processId'] != physical['process']['processId']
         and lb['compatibility'] == controller_record['state']['receipt']['compatibility']
         and lb['authority'] == controller_record['state']['receipt']['authority']
         and local['intent']['source']['replacementSha256'] == digest('winbridge.activation.replacement.v1',case['replacement']['record']), 'Original causal controller/local/keeper graph differs')
    expected_entry = dict(schemaVersion=1,commandId=original['commandId'],updateId=lb['updateId'],attemptId=lb['attemptId'],
        commandSha256=digest('winbridge.activation.lifetime-command.v1',original),keeper=lb['keeper'],jobName=lb['jobName'],
        compatibility=dict(lb['compatibility'],observedAtMs=entry['compatibility']['observedAtMs']),authority=lb['authority'],mode=original['mode'])
    need(entry == expected_entry and original['mode'] == 'supervised_quarantine'
         and original['deadlineAtMs'] == local['intent']['admittedAtMs']+240000
         and original['issuedAtMs'] <= entry['compatibility']['observedAtMs'] < original['deadlineAtMs']
         and case['activationLifetime']['privateReadyLossTested'] is True, 'Original V6 entry/deadline or lost-Ready boundary differs')
    command, intent, saved = p['record']['command'], p['intent'], p['record']
    binding = command['binding']; receipt = saved['state']['receipt']
    need(integer(command['schemaVersion'], 1) and uuid_string(command['commandId'])
         and command['commandId'] not in (original['commandId'], original['binding']['updateId'], original['binding']['launchId'], original['binding']['attemptId'])
         and binding['lifetimeCommand'] == original and intent['command'] == command
         and binding['lifetimeRecordSha256'] == digest('winbridge.activation.finalization-lifetime.v1', base), 'Finalization detached from original lifetime')
    query, observation = intent['query'], intent['observation']
    need(integer(query['residentObservationVersion'],7) and integer(observation['residentObservationVersion'],7)
         and uuid_string(query['requestId']) and query['requestId'] == observation['requestId'] == binding['residentRequestId']
         and query['command'] == original and observation['result']['kind'] == 'observed'
         and observation['result']['entry'] == entry
         and binding['residentObservationSha256'] == digest('winbridge.activation.finalization-observation.v1', observation), 'Missing fresh causal V7')
    at = observation['result']['observedAtMs']
    need(integer(at) and integer(command['issuedAtMs']) and 0 <= command['issuedAtMs']-at <= 10000
         and command['issuedAtMs'] > original['deadlineAtMs'], 'Original fresh observation expired or startup window renewed')
    frame = observation['result']['frame']; link = observation['result']['link']
    need(frame['providerGeneration'] == entry['compatibility']['generation'] and frame['providerBuild'] == build
         and frame['context'] == entry['compatibility']['context'] and link['connection']['state'] == 'connected'
         and link['connection']['sessionId'] == frame['sessionId'] and link['connection']['error'] is None,
         'Decision lacks the original current controller link')
    shutdown = json.loads(p['shutdownEncoded'])
    need(shutdown == case['beforeReplacement']['shutdown']
         and hashlib.sha256(p['shutdownEncoded'].encode()).hexdigest() == binding['shutdownRecordSha256'], 'Retained shutdown bytes differ')
    old_root, new_root = p['updaterBefore'][1], p['updaterAfter'][1]
    need(intent['rootBefore'] == old_root and old_root['phase'] == 'awaiting_controller'
         and old_root['revision'] == original['binding']['rootRevision']
         and new_root == dict(old_root, phase='committed', revision=old_root['revision']+1, updatedAtMs=command['issuedAtMs'], reason=new_root['reason'])
         and binding['committedRootRevision'] == new_root['revision'], 'Root not committed exactly once from original revision')
    before_info, after_info = p['updaterBefore'][0], p['updaterAfter'][0]
    need(before_info['schemaVersion'] == after_info['schemaVersion'] == 23
         and all(before_info[k] == after_info[k] for k in ('installationId','scopeSha256','ownerEpoch','activeUpdateId'))
         and binding['installationId'] == before_info['installationId'] and binding['scopeSha256'] == before_info['scopeSha256']
         and original['binding']['updaterEpoch'] == before_info['ownerEpoch']
         and original['binding']['updateId'] == before_info['activeUpdateId']
         and binding['sourceManifestSha256'] == target['manifestSha256'], 'Updater owner/scope changed')
    history = p['history']
    need(len(history) == 3 and history[-1] == saved and saved['state']['phase'] == 'confirmed'
         and all(r['schemaVersion'] == 1 and integer(r['revision'], i) and r['command'] == command
                 and r['state']['phase'] == phase for i, (r, phase) in enumerate(zip(history, ('release_pending','indeterminate','confirmed'))))
         and history[0]['updatedAtMs'] == command['issuedAtMs']
         and all(a['updatedAtMs'] <= b['updatedAtMs'] for a,b in zip(history,history[1:])), 'Lost ACK did not preserve one causal ledger')
    compatibility = receipt['compatibility']
    need(compatibility == dict(entry['compatibility'], observedAtMs=compatibility['observedAtMs'])
         and compatibility['observedAtMs'] >= command['issuedAtMs'] and receipt['schemaVersion'] == 1
         and receipt['commandId'] == command['commandId'] and receipt['mode'] == 'active'
         and receipt['authority'] == entry['authority']
         and receipt['commandSha256'] == digest('winbridge.activation.finalization-command.v1', command), 'Active receipt is another image/epoch/command')
    lease = receipt['releasedLease']; old_lease = shutdown['delivery']['receipt']['lease']
    need(binding['expectedLeaseRevision'] == old_lease['revision']
         and lease == dict(old_lease, phase='released', revision=old_lease['revision']+1, updatedAtMs=lease['updatedAtMs'], reason=REASON)
         and lease['updatedAtMs'] == receipt['releasedAtMs'] >= command['issuedAtMs'], 'Released lease differs from exact shutdown lease')
    need(p['releasePayload'] == dict(commandId=command['commandId'], expectedRevision=old_lease['revision'], action='release', sourceManifestSha256=target['manifestSha256'], reason=REASON)
         and p['releaseReceipt'] == lease and integer(p['releaseCommandCount'], 1), 'Release repeated or rebound in worker SQL')
    request_ids = []
    for key in ('first','second'):
        view = p[key]; v = view['activationFinalization']; obs = v['observation']
        need(view['schemaVersion'] == 21 and view['update'] == new_root and view['driverActive'] is False and view['driverError'] is None
             and not any(k in view for k in ('activationLifetime','activationResident','activationController','activationService','activationWriter','runtimeReplacement')), 'Finalization view not compact/current')
        need(v['phase'] == v['durablePhase'] == 'confirmed' and v['commandId'] == command['commandId']
             and v['residentProcess'] == compatibility['identity']['process']
             and all(v[k] is True for k in ('processAlive','originalCustodyHeld','verificationPinsRetired','deliveryAttempted')), 'View lacks original live custody/retirement/dispatch')
        header = v['interruptedReplyPrefix']
        need(type(header) is list and len(header) == 4 and all(type(b) is int and 0 <= b <= 255 for b in header)
             and 0 < int.from_bytes(bytes(header),'little') <= 1048576, 'No actual interrupted normal frame header')
        need(integer(obs['finalizationObservationVersion'],1) and uuid_string(obs['requestId'])
             and obs['requestId'] not in (command['commandId'],original['commandId'],binding['residentRequestId'])
             and obs['result']['kind'] == 'observed' and obs['result']['receipt'] == receipt
             and obs['result']['observedAtMs'] >= compatibility['observedAtMs'], 'No independent same-receipt Active observation')
        request_ids.append(obs['requestId'])
    need(request_ids[0] != request_ids[1]
         and p['second']['activationFinalization']['observation']['result']['observedAtMs'] >= p['first']['activationFinalization']['observation']['result']['observedAtMs']
         and saved['updatedAtMs'] >= compatibility['observedAtMs'], 'Readonly reconciliation reused correlation or time regressed')
    for key, version in (('wrongPeerMutation','finalizationVersion'),('wrongPeerObservation','finalizationObservationVersion'),('pidSpoof','finalizationObservationVersion')):
        need(integer(p[key][version],8 if key == 'wrongPeerMutation' else 1) and p[key]['result']['kind'] == 'rejected', 'Foreign OS peer/JSON PID acquired access')
    need(p['configBefore']['fileIdentity'] != p['configAfter']['fileIdentity'] and p['configBefore']['sha256'] != p['configAfter']['sha256']
         and p['configWriteSucceeded'] is True and p['atomicConfigReplaceSucceeded'] is True, 'Verification pins still block actual config changes')
    info = p['infoBefore']
    need(info['maintenanceAvailable'] is True and info['generation'] == compatibility['generation']
         and info['fencedRequests']['context'] == compatibility['context'] and info['build'] == build, 'Actual normal Info is not Active')
    accepted, duplicate, task = p['accepted'], p['duplicate'], p['task']
    need(all(r['requestContractVersion'] == 2 and r['outcome'] == 'responded' and r['context'] == compatibility['context'] and r['response']['kind'] == 'task'
             and r['response']['task']['taskId'] == task['taskId'] for r in (accepted,duplicate))
         and task['status'] == 'succeeded' and integer(task['exitCode'],0)
         and task['submission'] == accepted['response']['task']['submission'] == duplicate['response']['task']['submission']
         and task['submission']['command']['executable'] == 'cmd.exe'
         and task['submission']['command']['arguments'] == ['/d','/c','echo ACTIVE>>main30-active-count.txt']
         and p['taskEffect'].splitlines() == ['ACTIVE'] and integer(p['taskEffectLines'],1), 'Fenced task failed or duplicate repeated external effect')
    cold_view = cold['view']; cf = cold_view['activationFinalization']
    need(case['ownedJobEmpty'] is True and cold['originalWorkerExited'] is True and cold['record'] == saved
         and cold['coldObservationOnly'] is True and integer(cold['automaticColdLaunchCount'],0)
         and cold_view['schemaVersion'] == 21 and cold_view['update'] == new_root and cold_view['driverActive'] is False and cold_view['driverError'] is None
         and cf['phase'] == 'no_original_custody' and cf['durablePhase'] == 'confirmed' and cf['commandId'] == command['commandId']
         and all(cf[k] is None for k in ('residentProcess','processAlive','originalCustodyHeld','verificationPinsRetired','deliveryAttempted','interruptedReplyPrefix','observation')), 'Cold data recreated live custody/observation')
    need(p['physicalV8Proved'] is True and p['authenticatedActiveProved'] is True and p['permanentKeeperProved'] is False, 'Finalization evidence overstates keeper completion')
    return command['commandId']


def audit(proof, source, target, reservation, *, profile=PROFILE):
    profile_schemas(profile)
    command_id = verify(proof, source, target, reservation)
    changes = [('activationFinalization/record/command/binding/lifetimeRecordSha256','0'*64),
        ('activationFinalization/intent/observation/result/kind','pending'),
        ('activationFinalization/releaseCommandCount', True), ('activationFinalization/taskEffectLines',2),
        ('activationFinalization/taskEffect','ACTIVE\nACTIVE\n'), ('activationFinalization/first/activationFinalization/originalCustodyHeld',False),
        ('activationFinalization/first/activationFinalization/verificationPinsRetired',False),
        ('activationFinalization/first/activationFinalization/interruptedReplyPrefix',[0,0,0,0]),
        ('activationFinalization/first/activationFinalization/observation/result/receipt/mode','pending'),
        ('activationFinalization/wrongPeerMutation/result/kind','active'),
        ('activationFinalization/pidSpoof/result/kind','observed'),
        ('activationFinalization/configWriteSucceeded',False), ('activationFinalization/atomicConfigReplaceSucceeded',False),
        ('activationFinalization/record/revision',1), ('activationFinalization/updaterAfter/1/phase','awaiting_controller'),
        ('activationFinalization/infoBefore/maintenanceAvailable',False),
        ('activationFinalization/task/exitCode',True), ('activationFinalization/permanentKeeperProved',True),
        ('afterOwnedExit/automaticColdLaunchCount',1), ('afterOwnedExit/originalWorkerExited',False),
        ('afterOwnedExit/view/activationFinalization/originalCustodyHeld',True)]
    for path, value in changes:
        bad = copy.deepcopy(proof); node = bad['cases'][0]; parts = path.split('/')
        for part in parts[:-1]: node = node[int(part)] if isinstance(node,list) else node[part]
        need(canonical(node[parts[-1]]) != canonical(value), 'No-op Main30 negative: '+path)
        node[parts[-1]] = value
        try: verify(bad,source,target,reservation)
        except (ValueError,KeyError,TypeError,IndexError): continue
        raise ValueError('Main30 observer accepted mutation: '+path)
    return dict(kind='main30_independent_audit', finalizationCommandId=command_id, finalizationNegativeAudits=len(changes),
                physicalV8Proved=True, authenticatedActiveProved=True, interruptedV8ReplyProved=True,
                permanentKeeperProved=False, installedRuntimeApplied=False)
