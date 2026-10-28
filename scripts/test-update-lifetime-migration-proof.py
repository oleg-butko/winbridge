#!/usr/bin/env python3
"""Synthetic observer regressions, never evidence of a native Windows migration."""
import copy
import hashlib
import importlib.util
from pathlib import Path, PureWindowsPath
import unittest


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


lifetime = load('lifetime_evidence', 'update-activation-lifetime-migration-proof.py')
controller = load('controller_fixtures', 'test-update-controller-migration-proof.py')
profiles = load('fixed_profiles', 'update-migration-profiles.py')
checks = controller.checks


def fixture():
    proof, source, reference = controller.fixture()
    physical, service = proof['retainedActivation'], proof['retainedService']
    physical['intent']['context']['updaterJournalSchema'] = 21
    saved = dict(revision=1, command=dict(binding=dict(serviceCommand=service['command'],
                 serviceReceipt=service['state']['receipt'])), state=dict(phase='confirmed'))
    proof.update(kind='activation_lifetime_schema_migration_proof', retainedController=saved,
                 oldTwentyOneInitRefused=True, oldTwentyOneStatusRefused=True, noFreshLifetimeIntent=True, oldControllerReceiverRefused=True)
    for key in ('oldTwentyInitRefused', 'oldTwentyStatusRefused', 'noFreshControllerIntent'):
        proof.pop(key)
    proof['before']['schemaVersion'] = proof['backupInfo']['schemaVersion'] = 21
    proof['after']['schemaVersion'] = proof['afterColdObservation']['schemaVersion'] = 22
    for value, key in ((physical, 'activationSha256'), (saved, 'controllerSha256')):
        reference[key] = hashlib.sha256(checks.canonical(value)).hexdigest()
    return proof, source, reference


def reference_fixture():
    p, source, r = fixture()
    work = PureWindowsPath('K:/fixtures/tmp-main26-204-' + r['referenceRun'])
    archive = dict(directory=str(work / 'shutdown-main-cases' / ('runtime-archive-' + controller.service.old.uid(99))),
                   planSha256='7' * 64, entries=10)
    target = dict(source, archiveName='old21.zip', archiveSha256='8' * 64)
    producer = dict(sourceCommit='9' * 40, manifestSha256='1' * 64, archiveName='producer21.zip', archiveSha256='2' * 64)
    meta = dict(run=r['referenceRun'], taskId=controller.service.old.uid(70), source=producer, target=target,
                workingDirectory=str(work), testRuntimeDirectory='K:\\_proj-26\\winbridge\\runtime-test-204')
    physical, writer, service, saved, root = (p[key] for key in ('retainedActivation', 'retainedWriter', 'retainedService', 'retainedController', 'retainedRoot'))
    images = [{}, {}, {}, dict(r['image'], path=meta['testRuntimeDirectory'] + '\\winbridge-update.exe'), dict(sha256=r['manifestSha256'])]
    descriptor = dict(role='updater', build=dict(sourceCommit=target['sourceCommit'], sourceManifestSha256=target['manifestSha256']))
    case = dict(activationWriter=dict(record=physical, writerRecord=writer, serviceRecord=service, confirmed=dict(update=root)),
                activationController=dict(physicalRecord=physical, record=saved, confirmed=dict(update=root), rootCommitted=False, maintenanceReleased=False),
                coldActivationController=dict(view=dict(update=root)), replacement=dict(actualInstalledFiles=images,
                actualManifest=dict(artifacts=[dict(name='winbridge-update.exe', sha256=images[3]['sha256'], descriptor=descriptor)])))
    report = dict(kind='activation_controller_main_native_gate', passed=True, error=None, sourceCommit=producer['sourceCommit'],
                  sourceArchiveSha256=producer['archiveSha256'], target=target, runtimeArchive=archive)
    audit = dict(passed=True, kind='main26_independent_audit', profile='worker19-updater21', workerJournalSchema=19,
                 updaterJournalSchema=21, nativeMarkersVerified=3, installedWorkerUnchanged=True, run=meta['run'],
                 taskId=meta['taskId'], source=producer, target=target, runtimeArchive=archive,
                 proofSha256=r['proofSha256'], reportSha256=r['reportSha256'])
    return meta, report, dict(cases=[case]), audit


class LifetimeMigrationEvidenceTests(unittest.TestCase):
    def test_all_correlated_history_and_negative_mutations(self):
        proof, source, reference = fixture()
        self.assertEqual(lifetime.audit(proof, source, reference, checks), 24)

    def test_selection_refuses_skipped_duplicate_other_profile_and_boolean_exit(self):
        name = 'actual_twenty_one_controller_history_survives_twenty_two_and_old_twenty_one_refuses'
        command = dict(executable='cargo', exitCode=0, arguments=['test', '-p', 'winbridge-update',
            '--features', 'bundled-sqlite', '--test', 'activation_lifetime_migration', '--locked', '--offline',
            '--', '--exact', name, '--ignored', '--test-threads=1', '--nocapture'])
        log = f'running 1 test\ntest {name} ... diagnostic\nok\ntest result: ok. 1 passed; 0 failed; 0 ignored; finished in 1s\n'
        lifetime.verify_selection(command, log)
        for changed in (log + log, log.replace(name, 'another'), log.replace('1 passed', '0 passed'), log.replace('0 ignored', '1 ignored')):
            with self.assertRaises(ValueError):
                lifetime.verify_selection(command, changed)
        for changed in (dict(command, arguments=[]), dict(command, exitCode=False)):
            with self.assertRaises(ValueError):
                lifetime.verify_selection(changed, log)

    def test_coordinated_header_rewrite_cannot_change_the_original_scope(self):
        for field, value in (('installationId', controller.service.old.uid(99)), ('scopeSha256', '0' * 64), ('lastSequence', 0)):
            proof, source, reference = fixture()
            for key in ('before', 'after', 'afterColdObservation', 'backupInfo'):
                proof[key][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                lifetime.verify(proof, source, reference, checks)

    def test_private_permission_and_numeric_coercions_are_rejected(self):
        for key in ('lifetimeReady', 'keeperResident', 'canCommit', 'canLaunch', 'ownerHandle', 'releaseMaintenance', 'startupPermit', 'nonce'):
            proof, source, reference = fixture()
            proof[key] = False
            with self.subTest(key=key), self.assertRaises(ValueError):
                lifetime.verify(proof, source, reference, checks)
        for value in (True, 4.0):
            proof, source, reference = fixture()
            proof['before']['ownerEpoch'] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                lifetime.verify(proof, source, reference, checks)

    def test_controller_rewrite_cannot_override_the_verified_reference(self):
        proof, source, reference = fixture()
        proof['retainedController']['command']['binding']['serviceCommand'] = dict(forged=True)
        with self.assertRaises(ValueError):
            lifetime.verify(proof, source, reference, checks)

    def test_reference_binds_sdk21_controller_and_archive_outside_the_network_slot(self):
        args = reference_fixture()
        reference, location = lifetime.derive_reference(*args, checks)
        self.assertEqual(reference['kind'], 'activation_schema21_reference')
        self.assertIn('runtime-archive-', reference['image']['path'])
        self.assertNotEqual(reference['image']['path'], location['originalImage']['path'])
        for mutation in range(9):
            meta, report, proof, audit = copy.deepcopy(args)
            c = proof['cases'][0]['activationController']
            if mutation == 0:
                audit['updaterJournalSchema'] = 22
            elif mutation == 1:
                c['record']['state']['phase'] = 'issued'
            elif mutation == 2:
                c['physicalRecord']['intent']['context']['updaterJournalSchema'] = 22
            elif mutation == 3:
                c['rootCommitted'] = True
            elif mutation == 4:
                c['maintenanceReleased'] = True
            elif mutation == 5:
                report['runtimeArchive']['directory'] = 'K:/outside/runtime-archive-' + controller.service.old.uid(99)
            elif mutation == 6:
                meta['testRuntimeDirectory'] = 'K:/another-runtime'
            elif mutation == 7:
                proof['cases'][0]['replacement']['actualManifest']['artifacts'][0]['sha256'] = '0' * 64
            else:
                meta['taskId'] = controller.service.old.uid(99)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                lifetime.derive_reference(meta, report, proof, audit, checks)

    def test_duplicate_keys_floats_and_unreviewed_profiles_are_rejected(self):
        for raw in ('{"epoch":1,"epoch":2}', '{"epoch":1.0}', '{"epoch":NaN}', '{"epoch":Infinity}'):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                lifetime.parse_json(raw)
        self.assertEqual(profiles.get('Service19To20')['schemas'], (20, 19, 19))
        self.assertEqual(profiles.get('Controller20To21')['schemas'], (21, 20, 20))
        self.assertEqual(profiles.get('Lifetime21To22')['schemas'], (22, 21, 21))
        with self.assertRaises(ValueError):
            profiles.get('Lifetime21To23')


if __name__ == '__main__':
    unittest.main()
