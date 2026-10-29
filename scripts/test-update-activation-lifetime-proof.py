#!/usr/bin/env python3
"""Synthetic lifetime observer boundaries, not Windows execution evidence."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest
import uuid


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(path))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


p = load('lifetime_proof', 'update-activation-lifetime-proof.py')
base = load('controller_fixture', 'test-update-activation-controller-proof.py')
uid = lambda n: str(uuid.UUID(int=n))


def fixture():
    case = json.loads(json.dumps(base.fixture()).replace('"updaterJournalSchema": 21', '"updaterJournalSchema": 22'))
    local = case['activationWriter']['record']
    prefix = case['activationController']['record']
    compat = prefix['state']['receipt']['compatibility']
    keeper = dict(processId=88, birth='90', hostId='c'*64, sessionId=1)
    job = 'Global\\WinBridge-Activation-'+'e'*64
    local['intent'] = dict(source=dict(updateId=uid(4), launchId=uid(11), ownerId=uid(12), ownerEpoch=2, parentRevision=15),
                           attemptId=uid(5), admittedAtMs=10, context=dict(supervisor=keeper, startupTimeoutMs=240000))
    local['created'] = dict(jobName=job)
    prefix['command']['deadlineAtMs'] = 240010
    binding = dict(updateId=uid(4), attemptId=uid(5), launchId=uid(11), updaterOwnerId=uid(12), updaterEpoch=2, rootRevision=16,
                   controllerCommandId=prefix['command']['commandId'], controllerRecordSha256=p.digest('winbridge.activation.lifetime-controller.v1', prefix),
                   localRecordSha256=p.digest('winbridge.activation.lifetime-local.v1', local), keeper=keeper, jobName=job,
                   compatibility=compat, authority=prefix['state']['receipt']['authority'])
    command = dict(schemaVersion=1, commandId=uid(20), mode='supervised_quarantine', binding=binding, issuedAtMs=105, deadlineAtMs=240010)
    receipt = dict(schemaVersion=1, commandId=uid(20), updateId=uid(4), attemptId=uid(5), commandSha256=p.digest('winbridge.activation.lifetime-command.v1', command),
                   keeper=keeper, jobName=job, compatibility=dict(compat, observedAtMs=110), authority=binding['authority'], mode=command['mode'])
    record = dict(schemaVersion=1, revision=1, command=command, state=dict(phase='confirmed', receipt=receipt), updatedAtMs=111)
    v = dict(phase='observed', attemptId=uid(5), commandId=uid(20), revision=1, durablePhase='confirmed', savedReceipt=receipt,
             residentProcess=compat['identity']['process'], processAlive=True, originalCustodyHeld=True, observedReceipt=receipt, observedAtMs=200, reason=None)
    entered = dict(schemaVersion=19, driverActive=False, driverError=None, update=case['activationController']['confirmed']['update'], activationLifetime=v)
    late = copy.deepcopy(entered)
    late['activationLifetime']['observedAtMs'] = 240020
    current = dict(record=record, history=[dict(schemaVersion=1, revision=0, command=command, state=dict(phase='issued'), updatedAtMs=105), record],
                   entered=entered, afterExpiry=late, duplicate=copy.deepcopy(late), physicalRecord=local, workerBytesAfter='unchanged',
                   normalObservation=dict(lifetimeObservationVersion=1, result=dict(kind='observed', receipt=receipt, observedAtMs=240030)),
                   normalInfo=case['activationController']['normalInfo'], lostLifetimeDispatchAckSentOnce=True, savedLifetimeCommandCount=1,
                   submitRefusedAfterExpiry=True, sameJobVerified=True, rootCommitted=False, maintenanceReleased=False, privateReadyLossTested=False)
    cold = copy.deepcopy(late)
    cold['activationLifetime'].update(phase='not_resident', residentProcess=None, processAlive=None, originalCustodyHeld=None, observedReceipt=None, observedAtMs=None)
    case.update(activationLifetime=current, coldActivationLifetime=dict(view=cold, historyUnchanged=True, automaticColdLaunchCount=0, noColdLifetimeReplay=True))
    return json.loads(json.dumps(case))


class LifetimeEvidenceTests(unittest.TestCase):
    def test_causal_binding_and_all_negative_audits(self):
        key, count = p.audit_lifetime_case(fixture())
        self.assertEqual(key, uid(20))
        self.assertEqual(count, 37)

    def test_coherent_key_aliases_remain_invalid_after_all_hashes_and_copies_are_rebound(self):
        case = fixture()
        for alias in p.key_aliases(case):
            with self.subTest(alias=alias):
                bad = p.coherent_key_alias(case, alias)
                command = bad['activationLifetime']['record']['command']
                receipt = bad['activationLifetime']['record']['state']['receipt']
                self.assertEqual(command['commandId'], alias)
                self.assertEqual(receipt['commandId'], alias)
                self.assertEqual(receipt['commandSha256'], p.digest('winbridge.activation.lifetime-command.v1', command))
                with self.assertRaisesRegex(ValueError, 'Lifetime key/mode differs'):
                    p.verify_lifetime(bad)

    def test_previous_profiles_remain_frozen(self):
        self.assertEqual(p.profile_schemas(p.PROFILE), (19, 22))
        for old in ('worker19-updater20', 'worker19-updater21'):
            with self.assertRaises(ValueError):
                p.profile_schemas(old)
        for reader in (p.service, p.controller):
            with self.assertRaises(ValueError):
                reader.profile_schemas(p.PROFILE)
        for schemas, kind in [((19, 22), p.controller.KIND), ((19, 21), p.KIND)]:
            with self.assertRaises(ValueError):
                p.service.verify_local_prefix({}, {}, {}, {}, profile='test', expected_schemas=schemas, proof_kind=kind)

    def test_observation_does_not_replace_or_extend_entry_window(self):
        case = fixture()
        p.verify_lifetime(case)
        case['activationLifetime']['record']['state']['receipt']['compatibility']['observedAtMs'] = 240010
        with self.assertRaises(ValueError):
            p.verify_lifetime(case)

    def test_exact_native_test_cannot_be_replaced_by_main26_or_a_zero_test_log(self):
        command = dict(arguments=['test', '-p', 'winbridge-worker', '--features', 'bundled-sqlite,update-validation', '--test', 'update_shutdown',
                                  '--locked', '--offline', '--', '--exact', p.TEST, '--ignored', '--test-threads=1', '--nocapture'])
        log = f'running 1 test\ntest {p.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
        p.verify_main_selection(command, log)
        with self.assertRaises(ValueError):
            p.verify_main_selection(command, log.replace('running 1 test', 'running 0 tests'))
        command['arguments'][12] = 'actual_main_connects_same_verified_worker_to_controller_without_commit_or_replay'
        with self.assertRaises(ValueError):
            p.verify_main_selection(command, log)


if __name__ == '__main__':
    unittest.main()
