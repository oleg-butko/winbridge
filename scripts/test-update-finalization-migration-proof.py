#!/usr/bin/env python3
"""Synthetic observer regressions; never evidence of a native Windows migration."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import unittest


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


finalization = load('finalization_evidence', 'update-activation-finalization-migration-proof.py')
previous = load('lifetime_fixtures', 'test-update-lifetime-migration-proof.py')
profiles = load('fixed_profiles', 'update-migration-profiles.py')
checks = previous.checks


def fixture():
    proof, source, reference = previous.fixture()
    physical, root = proof['retainedActivation'], proof['retainedRoot']
    physical['intent']['context']['updaterJournalSchema'] = 22
    root['updatedAtMs'] = 10
    saved = dict(revision=2, command=dict(commandId=previous.controller.service.old.uid(88),
        binding=dict(keeper=dict(birth='123'))), state=dict(phase='confirmed', receipt={}))
    proof.update(kind='activation_finalization_schema_migration_proof', retainedLifetime=saved,
        oldTwentyTwoInitRefused=True, oldTwentyTwoStatusRefused=True, noFreshFinalizationIntent=True,
        oldLifetimeReceiverRefused=True)
    for key in ('oldTwentyOneInitRefused', 'oldTwentyOneStatusRefused', 'noFreshLifetimeIntent', 'oldControllerReceiverRefused'):
        proof.pop(key)
    proof['before']['schemaVersion'] = proof['backupInfo']['schemaVersion'] = 22
    proof['after']['schemaVersion'] = proof['afterColdObservation']['schemaVersion'] = 23
    proof['coldResident'] = dict(schemaVersion=20, build=copy.deepcopy(proof['build']), update=copy.deepcopy(root),
        driverActive=False, driverError=None, observedAtMs=20, source=None, sourceAppliedFiles=0,
        activationResident=dict(phase='not_resident', lifetimeCommandId=saved['command']['commandId'],
            residentProcess=None, processAlive=None, originalCustodyHeld=None, observation=None, reason='No original owner'))
    for value, key in ((physical, 'activationSha256'), (saved, 'lifetimeSha256'), (root, 'rootSha256')):
        reference[key] = hashlib.sha256(checks.canonical(value)).hexdigest()
    return proof, source, reference


def reference_fixture():
    meta, report, proof, audit = previous.reference_fixture()
    p, _, _ = fixture()
    case, = proof['cases']
    physical, lifetime, root = (p[k] for k in ('retainedActivation', 'retainedLifetime', 'retainedRoot'))
    case['activationWriter']['record'] = physical
    case['activationController']['physicalRecord'] = physical
    case['activationLifetime'] = dict(physicalRecord=physical, record=lifetime)
    case['activationResident'] = dict(first=dict(update=root), second=dict(update=root), rootCommitted=False,
        maintenanceReleased=False, privateChannelRead=False)
    case['coldActivationResident'] = dict(view=dict(update=root))
    report['kind'] = 'activation_resident_main_native_gate'
    audit.update(kind='main29_independent_audit', profile='worker19-updater22', updaterJournalSchema=22, readyLossMarkersVerified=1)
    return meta, report, proof, audit


class FinalizationMigrationEvidenceTests(unittest.TestCase):
    def test_retained_history_cold_scope_and_all_33_negative_audits(self):
        proof, source, reference = fixture()
        self.assertEqual(finalization.audit(proof, source, reference, checks), 33)

    def test_selection_refuses_skipped_duplicate_old_profile_and_boolean_exit(self):
        name = finalization.TEST
        command = dict(executable='cargo', exitCode=0, arguments=['test', '-p', 'winbridge-update', '--features',
            'bundled-sqlite', '--test', 'activation_finalization_migration', '--locked', '--offline', '--',
            '--exact', name, '--ignored', '--test-threads=1', '--nocapture'])
        log = f'running 1 test\ntest {name} ... diagnostic\nok\ntest result: ok. 1 passed; 0 failed; 0 ignored; finished in 1s\n'
        finalization.verify_selection(command, log)
        for wrong in (log+log, log.replace(name, 'another'), log.replace('1 passed', '0 passed'), log.replace('0 ignored', '1 ignored')):
            with self.assertRaises(ValueError):
                finalization.verify_selection(command, wrong)
        with self.assertRaises(ValueError):
            finalization.verify_selection(dict(command, exitCode=False), log)
        with self.assertRaises(ValueError):
            finalization.verify_selection(dict(command, arguments=[]), log)

    def test_correlated_headers_cannot_rebind_installation_scope_or_history(self):
        for field, value in (('installationId', previous.controller.service.old.uid(99)), ('scopeSha256', '0'*64), ('lastSequence', 0)):
            proof, source, reference = fixture()
            for key in ('before', 'after', 'afterColdObservation', 'backupInfo'):
                proof[key][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                finalization.verify(proof, source, reference, checks)

    def test_cold_read_and_unknown_permissions_never_create_a_grant(self):
        for key in ('startupPermit', 'nonce', 'canFinalize', 'keeperResident', 'releaseMaintenance'):
            proof, source, reference = fixture()
            proof[key] = False
            with self.subTest(key=key), self.assertRaises(ValueError):
                finalization.verify(proof, source, reference, checks)
        for key, value in (('residentProcess', {}), ('observation', {}), ('lifetimeCommandId', 'another')):
            proof, source, reference = fixture()
            proof['coldResident']['activationResident'][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                finalization.verify(proof, source, reference, checks)

    def test_only_fully_audited_main29_schema22_archive_may_be_a_reference(self):
        meta, report, proof, audit = reference_fixture()
        ref, _ = finalization.derive_reference(meta, report, proof, audit, checks)
        self.assertEqual(ref['kind'], 'activation_schema22_reference')
        self.assertIn('lifetimeSha256', ref)
        for mutation in range(8):
            m, r, p, a = copy.deepcopy((meta, report, proof, audit))
            case = p['cases'][0]
            if mutation == 0:
                a['profile'] = 'worker19-updater21'
            elif mutation == 1:
                case['activationWriter']['record']['intent']['context']['updaterJournalSchema'] = 23
            elif mutation == 2:
                case['activationLifetime']['record']['state']['phase'] = 'issued'
            elif mutation == 3:
                case['activationResident']['rootCommitted'] = True
            elif mutation == 4:
                case['activationResident']['privateChannelRead'] = True
            elif mutation == 5:
                r['runtimeArchive']['directory'] = 'K:/outside/runtime-archive-' + previous.controller.service.old.uid(99)
            elif mutation == 6:
                p['cases'][0]['replacement']['actualManifest']['artifacts'][0]['sha256'] = '0'*64
            else:
                a['readyLossMarkersVerified'] = 0
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                finalization.derive_reference(m, r, p, a, checks)

    def test_fixed_profiles_json_and_windows_selection_are_explicit(self):
        self.assertEqual(profiles.get('Finalization22To23')['schemas'], (23, 22, 22))
        self.assertEqual(profiles.get('Lifetime21To22')['schemas'], (22, 21, 21))
        with self.assertRaises(ValueError):
            profiles.get('Finalization22To24')
        for raw in ('{"epoch":1,"epoch":2}', '{"epoch":1.0}', '{"epoch":NaN}'):
            with self.assertRaises(ValueError):
                finalization.parse_json(raw)
        ps = Path(__file__).with_name('test-update-journal-migration.ps1').read_text()
        self.assertIn("'Finalization22To23'", ps)
        self.assertIn("schema=23;referenceKind='activation_schema22_reference'", ps)
        self.assertIn(finalization.TEST, ps)
        self.assertIn("'--exact',$case.test,'--ignored','--test-threads=1'", ps)
        self.assertIn("-lt 6GB", ps)


if __name__ == '__main__':
    unittest.main()
