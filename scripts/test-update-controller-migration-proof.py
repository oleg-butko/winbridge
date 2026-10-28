#!/usr/bin/env python3
"""Synthetic observer regressions, never evidence of an actual Windows migration."""
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


controller = load('controller_evidence', 'update-activation-controller-migration-proof.py')
service = load('service_fixtures', 'test-update-service-migration-proof.py')
checks = service.checks


def fixture():
    proof, source, reference = service.fixture()
    before = proof['before']
    physical, writer, root = (proof[key] for key in ('retainedActivation', 'retainedWriter', 'retainedRoot'))
    physical.update(revision=7, state=dict(phase='local_verified'), local=dict(verified=True))
    physical['intent']['context'] = dict(updaterJournalSchema=20, workerJournalSchema=19)
    physical['intent']['source'].update(installationId=before['installationId'], scopeSha256=before['scopeSha256'])
    root.update(revision=10, phase='awaiting_controller', request=dict(expected=dict(
        installationId=before['installationId'], scopeSha256=before['scopeSha256'])))
    saved = dict(revision=1, command=dict(binding=dict(writerCommand=writer['command'])),
                 state=dict(phase='confirmed', receipt=dict(compatibility=physical['local'])))
    proof.update(kind='activation_controller_schema_migration_proof', retainedService=saved,
                 oldTwentyInitRefused=True, oldTwentyStatusRefused=True, noFreshControllerIntent=True)
    for key in ('oldNineteenInitRefused', 'oldNineteenStatusRefused', 'noFreshServiceIntent'):
        proof.pop(key)
    before['schemaVersion'] = proof['backupInfo']['schemaVersion'] = 20
    proof['after']['schemaVersion'] = proof['afterColdObservation']['schemaVersion'] = 21
    for value, key in ((physical, 'activationSha256'), (writer, 'writerSha256'), (saved, 'serviceSha256'), (root, 'rootSha256')):
        reference[key] = hashlib.sha256(checks.canonical(value)).hexdigest()
    return proof, source, reference


class ControllerMigrationEvidenceTests(unittest.TestCase):
    def test_all_correlated_history_and_negative_mutations(self):
        proof, source, reference = fixture()
        self.assertEqual(controller.audit(proof, source, reference, checks), 21)

    def test_exact_selection_not_skipped_duplicated_or_another_profile(self):
        name = 'actual_twenty_local_service_history_survives_twenty_one_and_old_twenty_refuses'
        command = dict(executable='cargo', exitCode=0, arguments=['test', '-p', 'winbridge-update',
            '--features', 'bundled-sqlite', '--test', 'activation_controller_migration', '--locked', '--offline',
            '--', '--exact', name, '--ignored', '--test-threads=1', '--nocapture'])
        log = f'running 1 test\ntest {name} ... diagnostic\nok\ntest result: ok. 1 passed; 0 failed; 0 ignored; finished in 1s\n'
        controller.verify_selection(command, log)
        for changed in (log + log, log.replace(name, 'another'), log.replace('1 passed', '0 passed'), log.replace('0 ignored', '1 ignored')):
            with self.assertRaises(ValueError):
                controller.verify_selection(command, changed)
        with self.assertRaises(ValueError):
            controller.verify_selection(dict(command, arguments=[]), log)

    def test_coordinated_info_rewrite_does_not_override_retained_installation_or_scope(self):
        for field, replacement in (('installationId', service.old.uid(99)), ('scopeSha256', '0' * 64), ('lastSequence', 0)):
            proof, source, reference = fixture()
            for key in ('before', 'after', 'afterColdObservation', 'backupInfo'):
                proof[key][field] = replacement
            with self.subTest(field=field), self.assertRaises(ValueError):
                controller.verify(proof, source, reference, checks)

    def test_private_grants_and_numeric_type_coercions_refused(self):
        for change in ('epoch-bool', 'epoch-float', 'gateRows', 'serviceReady', 'startupPermit', 'authorityId', 'nonce'):
            proof, source, reference = fixture()
            if change.startswith('epoch'):
                proof['before']['ownerEpoch'] = True if change == 'epoch-bool' else 4.0
            elif change == 'gateRows':
                proof['gateRows'] = False
            else:
                proof[change] = False
            with self.subTest(change=change), self.assertRaises(ValueError):
                controller.verify(proof, source, reference, checks)

    def test_reference_cannot_be_changed_by_coordinated_service_or_local_rewrite(self):
        for change in ('service', 'writer', 'local'):
            proof, source, reference = fixture()
            proof = copy.deepcopy(proof)
            if change == 'service':
                proof['retainedService']['state']['phase'] = 'issued'
            elif change == 'writer':
                proof['retainedWriter']['command']['binding']['updateId'] = service.old.uid(99)
                proof['retainedService']['command']['binding']['writerCommand'] = proof['retainedWriter']['command']
            else:
                proof['retainedActivation']['local'] = proof['retainedService']['state']['receipt']['compatibility'] = dict(verified=False)
            with self.subTest(change=change), self.assertRaises(ValueError):
                controller.verify(proof, source, reference, checks)


if __name__ == '__main__':
    unittest.main()
