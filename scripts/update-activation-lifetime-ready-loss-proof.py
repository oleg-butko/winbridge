"""Independent Main28 partial-Ready proof. Main27's normal profile stays frozen."""
import copy
import hashlib
import importlib.util
import json
import re
from pathlib import Path

spec = importlib.util.spec_from_file_location('main28_lifetime', Path(__file__).with_name('update-activation-lifetime-proof.py'))
lifetime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lifetime)
service, controller = lifetime.service, lifetime.controller
need, digest, canonical = lifetime.need, lifetime.digest, lifetime.canonical
PROFILE = lifetime.PROFILE
KIND = 'activation_lifetime_ready_loss_main_native_proof'
TEST = 'actual_main_reconciles_partial_lifetime_ready_after_original_deadline_without_replay'
def profile_schemas(profile):
    need(type(profile) is str and profile == PROFILE, 'Main28 requires explicit worker19/updater22')
    return (19, 22)
verify_worker_migration = lifetime.verify_worker_migration
verify_activity = lifetime.verify_activity


def verify_main_selection(command, log):
    need(command['arguments'] == ['test', '-p', 'winbridge-worker', '--features', 'bundled-sqlite,update-validation',
         '--test', 'update_shutdown', '--locked', '--offline', '--', '--exact', TEST, '--ignored', '--test-threads=1', '--nocapture'],
         'Main28 exact native selection differs')
    log = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', log)
    summaries = re.findall(r'^test result: ([^\r\n]+)', log, re.MULTILINE)
    need(len(re.findall(r'^test ' + re.escape(TEST) + r' \.\.\.', log, re.MULTILINE)) == 1
         and re.search(r'^running 1 test\s*$', log, re.MULTILINE) and len(summaries) == 1
         and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;'), 'Main28 did not execute exactly once')


def verify_boundaries(commands, files):
    selected = [c for c in commands if c['name'] == 'lifetime-ready-loss-native-boundaries']
    need(len(selected) == 1, 'Missing/repeated Ready-loss native boundary tests')
    c, = selected
    need(c['arguments'] == ['test','-p','winbridge-update','--features','bundled-sqlite,update-validation','--lib',
         'lifetime_ready_loss','--locked','--offline','--','--test-threads=1','--nocapture'], 'Ready-loss boundary selection differs')
    log = files[c['log']].decode('utf-8-sig')
    need('test result: ok. 5 passed; 0 failed; 0 ignored;' in log, 'Ready-loss boundary tests incomplete')
    for name in ('partial_ready_consumes_only_header_and_preserves_same_stream_body',
                 'expired_original_window_consumes_nothing_and_cannot_be_renewed',
                 'invalid_or_truncated_header_never_becomes_a_fault_success',
                 'lifetime_ready_loss_fixture_requires_exact_local7_record_and_command_scope',
                 'lifetime_ready_loss_input_rejects_unknown_grants_and_cannot_relabel_a_trace'):
        need(name+' ... ok' in log, 'Missing native boundary case: '+name)


def verify_marker(marker, case, source=None):
    p = case['activationLifetime']
    command, local = p['record']['command'], case['activationWriter']['record']
    receipt = p['record']['state']['receipt']
    controller.typed(marker)
    service.strict(marker)
    need(set(marker) == {'schemaVersion', 'kind', 'processId', 'build', 'input', 'inputEncoded', 'inputSha256',
         'record', 'recordSha256', 'command', 'commandSha256', 'readyFrameBytes', 'headerBytesRead', 'bodyBytesRead',
         'readyValidated', 'normalProbeBeforeFailure', 'observedAtMs'}, 'Unknown partial-Ready marker fields')
    record_hash = digest('winbridge.activation.barrier.record.v1', local)
    expected_input = dict(schemaVersion=1, kind='winbridge_lifetime_ready_body_loss_validation', recordSha256=record_hash)
    need(marker['schemaVersion'] == 1 and marker['kind'] == 'activation_lifetime_ready_body_loss'
         and marker['record'] == local and marker['recordSha256'] == record_hash
         and marker['input'] == expected_input and json.loads(marker['inputEncoded']) == expected_input
         and hashlib.sha256(marker['inputEncoded'].encode()).hexdigest() == marker['inputSha256']
         and marker['command'] == command and marker['commandSha256'] == receipt['commandSha256']
         and marker['processId'] == command['binding']['keeper']['processId'], 'Ready loss detached from original owner/command/input')
    need(type(marker['headerBytesRead']) is int and marker['headerBytesRead'] == 4
         and type(marker['bodyBytesRead']) is int and marker['bodyBytesRead'] == 0
         and marker['readyValidated'] is False and marker['normalProbeBeforeFailure'] is False
         and type(marker['readyFrameBytes']) is int and 1 <= marker['readyFrameBytes'] <= 32768
         and marker['readyFrameBytes'] == len(canonical(dict(contractVersion=6, receipt=receipt)))
         and receipt['compatibility']['observedAtMs'] <= marker['observedAtMs'] < command['deadlineAtMs'],
         'Ready loss did not leave the bounded original frame body unconsumed')
    if source is not None:
        need(marker['build']['sourceDirty'] is False and marker['build']['sourceCommit'] == source['sourceCommit']
             and marker['build']['sourceManifestSha256'] == source['manifestSha256'], 'Ready loss used another supervisor')


def verify_fault(case, source=None):
    key = lifetime.verify_lifetime(case, ready_loss=True)
    p = case['activationLifetime']
    command, saved = p['record']['command'], p['record']
    fault, b = p['readyLoss'], command['binding']
    need(set(fault) == {'unknownRecord', 'historyBeforeReconciliation', 'unknownView', 'marker',
         'reconciliationStartedAfterOriginalDeadline', 'privateCommandSentOnce'}, 'Unknown Ready-loss evidence')
    unknown = fault['unknownRecord']
    need(set(unknown) == set(saved) and unknown['schemaVersion'] == 1 and unknown['revision'] == 1
         and unknown['command'] == command and set(unknown['state']) == {'phase', 'reason'}
         and unknown['state']['phase'] == 'indeterminate' and type(unknown['state']['reason']) is str
         and 0 < len(unknown['state']['reason']) <= 1000
         and fault['historyBeforeReconciliation'] == p['history'][:2]
         and fault['marker']['observedAtMs'] <= unknown['updatedAtMs'] <= saved['updatedAtMs'], 'Missing original causal uncertainty before reconciliation')
    view, v = fault['unknownView'], fault['unknownView']['activationLifetime']
    need(view['schemaVersion'] == 19 and view['driverActive'] is False
         and type(view['driverError']) is str and 'Ready body intentionally unread' in view['driverError']
         and view['update'] == p['afterExpiry']['update'] and v['phase'] == 'indeterminate'
         and v['durablePhase'] == 'indeterminate' and v['revision'] == 1
         and v['commandId'] == key and v['attemptId'] == b['attemptId']
         and v['residentProcess'] == b['compatibility']['identity']['process'] and v['processAlive'] is True
         and all(v[k] is None for k in ('savedReceipt', 'observedReceipt', 'observedAtMs', 'originalCustodyHeld')),
         'Lost Ready was mistaken for a confirmed observation or another child')
    need(fault['reconciliationStartedAfterOriginalDeadline'] is True and fault['privateCommandSentOnce'] is True
         and p['entered']['activationLifetime']['observedAtMs'] > command['deadlineAtMs']
         and saved['updatedAtMs'] > command['deadlineAtMs'], 'Ready uncertainty received a new admission/window or early reconciliation')
    verify_marker(fault['marker'], case, source)
    return key


def audit_fault(case, source=None):
    key = verify_fault(case, source)
    changes = [('readyLoss/unknownRecord/revision', 0), ('readyLoss/unknownRecord/state/phase', 'confirmed'),
               ('readyLoss/unknownView/driverActive', True), ('readyLoss/unknownView/driverError', None),
               ('readyLoss/unknownView/activationLifetime/observedAtMs', 1),
               ('readyLoss/unknownView/activationLifetime/originalCustodyHeld', True),
               ('readyLoss/reconciliationStartedAfterOriginalDeadline', False), ('readyLoss/privateCommandSentOnce', False),
               ('readyLoss/marker/bodyBytesRead', 1), ('readyLoss/marker/headerBytesRead', True),
               ('readyLoss/marker/readyFrameBytes', 0), ('readyLoss/marker/readyValidated', True),
               ('readyLoss/marker/normalProbeBeforeFailure', True), ('readyLoss/marker/observedAtMs', 0),
               ('readyLoss/marker/recordSha256', '0'*64)]
    for path, value in changes:
        bad = copy.deepcopy(case)
        node = bad['activationLifetime']
        parts = path.split('/')
        for part in parts[:-1]:
            node = node[part]
        need(canonical(node[parts[-1]]) != canonical(value), 'No-op Ready-loss mutation: '+path)
        node[parts[-1]] = value
        try:
            verify_fault(bad, source)
        except (ValueError, KeyError, TypeError, IndexError):
            continue
        raise ValueError('Ready-loss observer accepted mutation: '+path)
    return key, len(changes)


def audit_auxiliary(files, proof, source):
    result = lifetime.audit_auxiliary(files, proof, source)
    case, = proof['cases']
    raw = files['None-activation-lifetime-ready-body-loss.json']
    marker = json.loads(raw.decode('utf-8-sig'))
    need(marker == case['activationLifetime']['readyLoss']['marker'], 'Separate Ready-loss marker differs from proof')
    need(files['None-lifetime-ready-loss-validation.json'].decode('utf-8') == marker['inputEncoded'], 'Ready-loss input bytes differ')
    verify_marker(marker, case, source)
    return dict(result, readyLossMarkersVerified=1)


def audit(proof, source, target, reservation, *, profile=PROFILE):
    profile_schemas(profile)
    result = service.audit_local_prefix(proof, source, target, reservation, profile=profile, expected_schemas=(19,22), proof_kind=KIND)
    case, = proof['cases']
    controller_key, controller_count = controller.audit_controller_case(case)
    key, count = lifetime.audit_lifetime_case(case, ready_loss=True)
    _, fault_count = audit_fault(case, source)
    return dict(result, kind='main28_independent_audit', controllerCommandId=controller_key,
                controllerNegativeAudits=controller_count, lifetimeCommandId=key, lifetimeNegativeAudits=count,
                readyLossNegativeAudits=fault_count)
