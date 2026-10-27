#!/usr/bin/env python3
"""Synthetic observer regressions; never evidence of an actual Windows migration."""
import hashlib
import importlib.util
from pathlib import Path
import unittest


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


service = load('service_migration', 'update-activation-service-migration-proof.py')
old = load('old_migration_fixtures', 'test-update-activation-writer-migration-native.py')
checks = old.checks


def fixture():
    proof, _, source, reference, _ = old.fixtures()
    ref = reference['updater']
    writer = dict(revision=1, state=dict(phase='confirmed'),
                  command=dict(binding=dict(updateId=ref['updateId'])))
    ref['writerSha256'] = hashlib.sha256(checks.canonical(writer)).hexdigest()
    proof.update(kind='activation_service_schema_migration_proof', retainedWriter=writer,
                 noFreshServiceIntent=True, oldNineteenInitRefused=True, oldNineteenStatusRefused=True)
    proof.pop('oldEighteenInitRefused')
    proof.pop('oldEighteenStatusRefused')
    proof['before']['schemaVersion'] = 19
    proof['backupInfo']['schemaVersion'] = 19
    proof['after']['schemaVersion'] = 20
    proof['afterColdObservation'] = dict(proof['after'], ownerEpoch=proof['after']['ownerEpoch'] + 1)
    return proof, source, ref


class ServiceMigrationEvidenceTests(unittest.TestCase):
    def test_exact_selection_accepts_diagnostics_and_refuses_skipped_or_other_test(self):
        name = 'actual_nineteen_writer_history_survives_twenty_and_old_nineteen_refuses'
        command = dict(executable='cargo', exitCode=0, arguments=['test', '-p', 'winbridge-update',
               '--features', 'bundled-sqlite', '--test', 'activation_service_migration', '--locked',
               '--offline', '--', '--exact', name, '--ignored', '--test-threads=1', '--nocapture'])
        log = f'running 1 test\ntest {name} ... diagnostic\nok\ntest result: ok. 1 passed; 0 failed; 0 ignored; finished in 1s\n'
        service.verify_selection(command, log)
        for changed in (log.replace(name, 'unrelated'), log.replace('1 passed', '0 passed'),
                        log.replace('0 ignored', '1 ignored'), log + log):
            with self.assertRaises(ValueError):
                service.verify_selection(command, changed)
        with self.assertRaises(ValueError):
            service.verify_selection(dict(command, arguments=[]), log)

    def test_correlated_history_and_all_negative_mutations(self):
        proof, source, reference = fixture()
        self.assertEqual(service.audit(proof, source, reference, checks), 21)

    def test_coordinated_writer_rewrite_cannot_replace_the_old_reference(self):
        proof, source, reference = fixture()
        proof['retainedWriter']['command']['binding']['updateId'] = old.uid(99)
        with self.assertRaises(ValueError):
            service.verify(proof, source, reference, checks)

    def test_typed_epochs_and_build_identity_are_not_inferred(self):
        for change in ('epoch', 'schema', 'build', 'grant'):
            proof, source, reference = fixture()
            if change == 'epoch':
                proof['after']['ownerEpoch'] = True
            elif change == 'schema':
                proof['before']['schemaVersion'] = True
            elif change == 'build':
                proof['build']['sourceManifestSha256'] = '0' * 64
            else:
                proof['startupPermit'] = True
            with self.subTest(change=change), self.assertRaises(ValueError):
                service.verify(proof, source, reference, checks)


if __name__ == '__main__':
    unittest.main()
