"""Independent Main27: fixed19/22, original Job/child, no final commit/replay."""
import copy
import importlib.util
from pathlib import Path
import re
import uuid

spec = importlib.util.spec_from_file_location('main27_controller_prefix', Path(__file__).with_name('update-activation-controller-proof.py'))
controller = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controller)
service = controller.service
need, digest, canonical = service.need, service.digest, service.canonical
audit_auxiliary = service.audit_auxiliary
verify_worker_migration = service.verify_worker_migration
verify_activity = controller.verify_activity
PROFILE = 'worker19-updater22'
KIND = 'activation_lifetime_main_native_proof'
TEST = 'actual_main_enters_same_child_lifetime_and_observes_after_original_deadline_without_replay'


def profile_schemas(profile):
    need(type(profile) is str and profile == PROFILE, 'Main27 requires explicit worker19/updater22')
    return (19, 22)


def verify_main_selection(command, log):
    need(command['arguments'] == ['test', '-p', 'winbridge-worker', '--features', 'bundled-sqlite,update-validation',
         '--test', 'update_shutdown', '--locked', '--offline', '--', '--exact', TEST, '--ignored', '--test-threads=1', '--nocapture'],
         'Main27 exact native selection differs')
    log = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    summaries = re.findall(r'^test result: ([^\r\n]+)', log, re.MULTILINE)
    need(len(re.findall(r'^test ' + re.escape(TEST) + r' \.\.\.', log, re.MULTILINE)) == 1
         and re.search(r'^running 1 test\s*$', log, re.MULTILINE) and len(summaries) == 1
         and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;'), 'Main27 did not execute exactly once')


def verify_lifetime(case):
    p, cold = case['activationLifetime'], case['coldActivationLifetime']
    local, prefix = case['activationWriter']['record'], case['activationController']['record']
    controller.typed(p)
    controller.typed(cold)
    service.strict(p)
    service.strict(cold)
    saved = p['record']
    command, receipt = saved['command'], saved['state']['receipt']
    b = command['binding']
    need(set(saved) == {'schemaVersion', 'revision', 'command', 'state', 'updatedAtMs'} and saved['schemaVersion'] == 1
         and saved['revision'] == 1 and saved['state']['phase'] == 'confirmed', 'Lifetime causal record differs')
    need(set(command) == {'schemaVersion', 'commandId', 'mode', 'binding', 'issuedAtMs', 'deadlineAtMs'}
         and command['schemaVersion'] == 1 and command['mode'] == 'supervised_quarantine'
         and str(uuid.UUID(command['commandId'])) == command['commandId']
         and command['commandId'] not in (prefix['command']['commandId'], receipt['updateId'], receipt['attemptId'],
             b['launchId'], b['updaterOwnerId'], b['compatibility']['generation'], b['compatibility']['context']['journalIncarnation'],
             case['activationWriter']['writerRecord']['command']['commandId'],
             case['activationWriter']['serviceRecord']['command']['commandId']), 'Lifetime key/mode differs')
    need(set(b) == {'updateId', 'attemptId', 'launchId', 'updaterOwnerId', 'updaterEpoch', 'rootRevision',
         'controllerCommandId', 'controllerRecordSha256', 'localRecordSha256', 'keeper', 'jobName', 'compatibility', 'authority'}, 'Unknown lifetime binding')
    source = local['intent']['source']
    need(b['updateId'] == source['updateId'] and b['attemptId'] == local['intent']['attemptId']
         and b['launchId'] == source['launchId'] and b['updaterOwnerId'] == source['ownerId'] and b['updaterEpoch'] == source['ownerEpoch']
         and b['rootRevision'] == source['parentRevision'] + 1 and b['keeper'] == local['intent']['context']['supervisor']
         and b['jobName'] == local['created']['jobName']
         and b['controllerCommandId'] == prefix['command']['commandId']
         and b['controllerRecordSha256'] == digest('winbridge.activation.lifetime-controller.v1', prefix)
         and b['localRecordSha256'] == digest('winbridge.activation.lifetime-local.v1', local)
         and b['compatibility'] == prefix['state']['receipt']['compatibility']
         and b['authority'] == prefix['state']['receipt']['authority'], 'Lifetime detached from original causal graph/Job/keeper')
    need(command['issuedAtMs'] >= prefix['updatedAtMs'] and command['deadlineAtMs'] == prefix['command']['deadlineAtMs']
         and command['deadlineAtMs'] == local['intent']['admittedAtMs'] + local['intent']['context']['startupTimeoutMs']
         and local['intent']['context']['startupTimeoutMs'] == 240000, 'Original lifetime window renewed')
    expected = dict(schemaVersion=1, commandId=command['commandId'], updateId=b['updateId'], attemptId=b['attemptId'],
                    commandSha256=digest('winbridge.activation.lifetime-command.v1', command), keeper=b['keeper'], jobName=b['jobName'],
                    compatibility=dict(b['compatibility'], observedAtMs=receipt['compatibility']['observedAtMs']), authority=b['authority'], mode=command['mode'])
    need(receipt == expected and command['issuedAtMs'] <= receipt['compatibility']['observedAtMs'] < command['deadlineAtMs'], 'Lifetime entry receipt differs/outside original window')
    need(p['history'] == [dict(schemaVersion=1, revision=0, command=command, state=dict(phase='issued'), updatedAtMs=command['issuedAtMs']), saved]
         and saved['updatedAtMs'] >= receipt['compatibility']['observedAtMs'], 'Lifetime skipped/reissued causal history')
    for name in ('entered', 'afterExpiry', 'duplicate'):
        view = p[name]
        v = view['activationLifetime']
        need(view['schemaVersion'] == 19 and view['driverActive'] is False and view['driverError'] is None
             and view['update'] == case['activationController']['confirmed']['update'] and view['update']['phase'] == 'awaiting_controller'
             and v['phase'] == 'observed' and v['durablePhase'] == 'confirmed' and v['commandId'] == command['commandId']
             and v['attemptId'] == b['attemptId'] and v['revision'] == 1
             and v['savedReceipt'] == receipt and v['observedReceipt'] == receipt
             and v['residentProcess'] == b['compatibility']['identity']['process']
             and v['processAlive'] is True and v['originalCustodyHeld'] is True
             and type(v['observedAtMs']) is int and v['observedAtMs'] >= receipt['compatibility']['observedAtMs'], 'Lifetime view claims unsupported readiness/custody')
    need(p['afterExpiry']['activationLifetime']['observedAtMs'] > command['deadlineAtMs']
         and p['duplicate']['activationLifetime'] == p['afterExpiry']['activationLifetime'], 'Post-expiry observation missing or duplicate changed mode')
    observation = p['normalObservation']
    need(type(observation['lifetimeObservationVersion']) is int and observation['lifetimeObservationVersion'] == 1 and observation['result']['kind'] == 'observed'
         and observation['result']['receipt'] == receipt and observation['result']['observedAtMs'] > command['deadlineAtMs'], 'Actual normal IPC did not observe the same late quarantine')
    info = p['normalInfo']
    need(info['generation'] == b['compatibility']['generation'] and info['build'] == b['compatibility']['identity']['descriptor']['build']
         and info['activeTaskCount'] == 0 and info['maintenanceError'] is None and info['journalRecoveryRequired'] is None
         and info['fencedRequests']['context'] == b['compatibility']['context'] and info['fencedRequests']['legacyMutationsAllowed'] is False
         and info['updateCoordination']['readOnly'] is True and info['updateCoordination']['lease']['phase'] == 'quiesced'
         and all(info[k] is False for k in ('maintenanceAvailable', 'workflowAvailable', 'snapshotImportAvailable', 'snapshotDeltaAvailable', 'snapshotResumeAvailable')), 'Lifetime opened dispatch or changed loaded identity')
    need(type(p['savedLifetimeCommandCount']) is int and p['savedLifetimeCommandCount'] == 1
         and all(p[k] is True for k in ('lostLifetimeDispatchAckSentOnce', 'submitRefusedAfterExpiry', 'sameJobVerified'))
         and all(p[k] is False for k in ('rootCommitted', 'maintenanceReleased', 'privateReadyLossTested'))
         and p['physicalRecord'] == local and p['workerBytesAfter'] == case['activationWriter']['workerBytesAfter'], 'Lifetime replayed/mutated worker state or overstated fault coverage')
    v = cold['view']['activationLifetime']
    need(cold['view']['schemaVersion'] == 19 and cold['view']['update'] == p['afterExpiry']['update']
         and v['phase'] == 'not_resident' and v['savedReceipt'] == receipt
         and all(v[k] is None for k in ('residentProcess', 'processAlive', 'originalCustodyHeld', 'observedReceipt', 'observedAtMs'))
         and cold['historyUnchanged'] is True and type(cold['automaticColdLaunchCount']) is int
         and cold['automaticColdLaunchCount'] == 0 and cold['noColdLifetimeReplay'] is True, 'Cold lifetime history recreated custody/replayed')
    return command['commandId']


def coherent_key_alias(case, alias):
    """Rebind all lifetime copies and hashes; local hash mismatch is no defence."""
    command = case['activationLifetime']['record']['command']
    old_key = command['commandId']
    old_hash = digest('winbridge.activation.lifetime-command.v1', command)

    def replace(value, old, new):
        if isinstance(value, dict):
            return {k: replace(v, old, new) for k, v in value.items()}
        if isinstance(value, list):
            return [replace(v, old, new) for v in value]
        return new if value == old else value

    bad = replace(case, old_key, alias)
    new_hash = digest('winbridge.activation.lifetime-command.v1', bad['activationLifetime']['record']['command'])
    return replace(bad, old_hash, new_hash)


def key_aliases(case):
    b = case['activationLifetime']['record']['command']['binding']
    return (b['launchId'], b['updaterOwnerId'], b['compatibility']['generation'],
            b['compatibility']['context']['journalIncarnation'],
            case['activationWriter']['writerRecord']['command']['commandId'],
            case['activationWriter']['serviceRecord']['command']['commandId'])


def audit_lifetime_case(case):
    key = verify_lifetime(case)
    changes = [('record/command/mode', 'normal'), ('record/command/deadlineAtMs', 0), ('record/revision', True),
               ('record/command/binding/updaterEpoch', 0), ('record/command/binding/rootRevision', 0),
               ('record/command/binding/controllerRecordSha256', '0'*64), ('record/command/binding/localRecordSha256', '0'*64),
               ('record/command/binding/keeper/birth', '1'), ('record/state/receipt/jobName', 'foreign'),
               ('record/state/receipt/commandSha256', '0'*64), ('record/state/receipt/compatibility/observedAtMs', 0),
               ('afterExpiry/schemaVersion', 18), ('afterExpiry/update/phase', 'committed'),
               ('afterExpiry/activationLifetime/processAlive', None), ('afterExpiry/activationLifetime/originalCustodyHeld', None),
               ('afterExpiry/activationLifetime/observedAtMs', 0), ('afterExpiry/activationLifetime/observedReceipt/mode', 'normal'),
               ('normalObservation/result/observedAtMs', 0), ('normalObservation/lifetimeObservationVersion', True), ('normalInfo/maintenanceAvailable', True),
               ('normalInfo/fencedRequests/legacyMutationsAllowed', True), ('savedLifetimeCommandCount', 2),
               ('savedLifetimeCommandCount', True), ('submitRefusedAfterExpiry', False), ('sameJobVerified', False),
               ('rootCommitted', True), ('maintenanceReleased', True), ('privateReadyLossTested', True)]
    changes = [('activationLifetime/'+p, v) for p, v in changes] + [
        ('coldActivationLifetime/view/activationLifetime/originalCustodyHeld', True),
        ('coldActivationLifetime/automaticColdLaunchCount', 1), ('coldActivationLifetime/noColdLifetimeReplay', False)]
    for path, value in changes:
        bad = copy.deepcopy(case)
        node = bad
        parts = path.split('/')
        for part in parts[:-1]:
            node = node[part]
        need(canonical(node[parts[-1]]) != canonical(value), 'No-op lifetime mutation: '+path)
        node[parts[-1]] = value
        try:
            verify_lifetime(bad)
        except (ValueError, KeyError, TypeError, IndexError):
            continue
        raise ValueError('Lifetime observer accepted mutation: '+path)
    aliases = key_aliases(case)
    for alias in aliases:
        try:
            verify_lifetime(coherent_key_alias(case, alias))
        except (ValueError, KeyError, TypeError, IndexError):
            continue
        raise ValueError('Lifetime observer accepted coherent command-key alias: '+alias)
    return key, len(changes) + len(aliases)


def audit(proof, source, target, reservation, *, profile=PROFILE):
    profile_schemas(profile)
    result = service.audit_local_prefix(proof, source, target, reservation, profile=profile, expected_schemas=(19, 22), proof_kind=KIND)
    case, = proof['cases']
    controller_key, controller_count = controller.audit_controller_case(case)
    key, count = audit_lifetime_case(case)
    return dict(result, kind='main27_independent_audit', controllerCommandId=controller_key,
                controllerNegativeAudits=controller_count, lifetimeCommandId=key, lifetimeNegativeAudits=count)
