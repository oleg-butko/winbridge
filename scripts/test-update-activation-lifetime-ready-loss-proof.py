"""Synthetic Main28 reader checks; these do not prove native Ready loss."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(path))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


p = load('ready_loss_proof', 'update-activation-lifetime-ready-loss-proof.py')
base = load('lifetime_fixture', 'test-update-activation-lifetime-proof.py')


def fixture():
    case = base.fixture()
    v = case['activationLifetime']
    saved = v['record']
    command, receipt = saved['command'], saved['state']['receipt']
    issued = v['history'][0]
    unknown = dict(schemaVersion=1, revision=1, command=command,
                   state=dict(phase='indeterminate', reason='Original lifetime preparation unconfirmed; no replay'), updatedAtMs=116)
    saved.update(revision=2, updatedAtMs=240040)
    v['history'] = [issued, unknown, saved]
    for name in ('entered', 'afterExpiry', 'duplicate'):
        v[name]['activationLifetime'].update(revision=2, observedAtMs=240020)
    unknown_view = copy.deepcopy(v['entered'])
    unknown_view['driverError'] = 'Validation: private lifetime Ready body intentionally unread; observe same command without replay'
    unknown_view['activationLifetime'].update(phase='indeterminate', durablePhase='indeterminate', revision=1,
        savedReceipt=None, observedReceipt=None, observedAtMs=None, originalCustodyHeld=None)
    record = case['activationWriter']['record']
    record_hash = p.digest('winbridge.activation.barrier.record.v1', record)
    spec = dict(schemaVersion=1, kind='winbridge_lifetime_ready_body_loss_validation', recordSha256=record_hash)
    encoded = p.canonical(spec).decode()
    marker = dict(schemaVersion=1, kind='activation_lifetime_ready_body_loss', processId=command['binding']['keeper']['processId'],
        build=receipt['compatibility']['identity']['descriptor']['build'], input=spec, inputEncoded=encoded,
        inputSha256=hashlib.sha256(encoded.encode()).hexdigest(), record=record, recordSha256=record_hash,
        command=command, commandSha256=receipt['commandSha256'], readyFrameBytes=len(p.canonical(dict(contractVersion=6, receipt=receipt))),
        headerBytesRead=4, bodyBytesRead=0, readyValidated=False, normalProbeBeforeFailure=False, observedAtMs=115)
    v.update(privateReadyLossTested=True, readyLoss=dict(unknownRecord=unknown, historyBeforeReconciliation=[issued,unknown],
        unknownView=unknown_view, marker=marker, reconciliationStartedAfterOriginalDeadline=True, privateCommandSentOnce=True))
    return json.loads(json.dumps(case))


class ReadyLossEvidenceTests(unittest.TestCase):
    def test_unknown_receipt_and_late_same_key_reconciliation_with_all_negative_audits(self):
        case = fixture()
        _, count = p.audit_fault(case)
        self.assertEqual(count, 15)
        _, count = p.lifetime.audit_lifetime_case(case, ready_loss=True)
        self.assertEqual(count, 37)

    def test_main27_and_main28_cannot_claim_each_others_coverage(self):
        with self.assertRaises(ValueError):
            p.verify_fault(base.fixture())
        with self.assertRaises(ValueError):
            p.lifetime.verify_lifetime(fixture())

    def test_ready_loss_does_not_accept_a_consistent_reissued_command_or_new_budget(self):
        case = fixture()
        case['activationLifetime']['record']['command']['deadlineAtMs'] += 1000
        with self.assertRaises(ValueError):
            p.verify_fault(case)
        for alias in p.lifetime.key_aliases(fixture()):
            with self.assertRaises(ValueError):
                p.verify_fault(p.lifetime.coherent_key_alias(fixture(), alias))

    def test_main27_native_test_with_a_main28_diagnostic_does_not_prove_main28(self):
        arguments=['test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--test','update_shutdown',
                   '--locked','--offline','--','--exact',p.TEST,'--ignored','--test-threads=1','--nocapture']
        log=f'running 1 test\ntest {p.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
        p.verify_main_selection(dict(arguments=arguments),log)
        bad=log.replace('test '+p.TEST+' ...','test '+p.lifetime.TEST+' ...')+'diagnostic '+p.TEST+'\n'
        with self.assertRaises(ValueError):
            p.verify_main_selection(dict(arguments=arguments),bad)
        with self.assertRaises(ValueError):
            p.verify_main_selection(dict(arguments=arguments),log.replace('running 1 test','running 0 tests'))

    def test_native_boundary_selection_requires_all_five_cases(self):
        names=('partial_ready_consumes_only_header_and_preserves_same_stream_body',
               'expired_original_window_consumes_nothing_and_cannot_be_renewed',
               'invalid_or_truncated_header_never_becomes_a_fault_success',
               'lifetime_ready_loss_fixture_requires_exact_local7_record_and_command_scope',
               'lifetime_ready_loss_input_rejects_unknown_grants_and_cannot_relabel_a_trace')
        command=dict(name='lifetime-ready-loss-native-boundaries', log='boundaries.log',
            arguments=['test','-p','winbridge-update','--features','bundled-sqlite,update-validation','--lib',
                        'lifetime_ready_loss','--locked','--offline','--','--test-threads=1','--nocapture'])
        log='\n'.join(name+' ... ok' for name in names)+'\ntest result: ok. 5 passed; 0 failed; 0 ignored;'
        p.verify_boundaries([command],{'boundaries.log':log.encode()})
        with self.assertRaises(ValueError):
            p.verify_boundaries([],{'boundaries.log':log.encode()})
        with self.assertRaises(ValueError):
            p.verify_boundaries([command],{'boundaries.log':log.replace(names[-1], 'foreign_case').encode()})

    def test_partial_header_marker_requires_exact_frame_size_and_original_owner(self):
        case = fixture()
        for field,value in [('readyFrameBytes',32768),('bodyBytesRead',False),('headerBytesRead',True),('processId',1)]:
            marker=copy.deepcopy(case['activationLifetime']['readyLoss']['marker'])
            marker[field]=value
            with self.assertRaises(ValueError):
                p.verify_marker(marker,case)


if __name__ == '__main__':
    unittest.main()
