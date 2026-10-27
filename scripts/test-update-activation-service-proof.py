"""Report boundary tests, not native execution evidence."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('service_proof', Path(__file__).with_name('update-activation-service-proof.py'))
proof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof)


class ServiceEvidenceTests(unittest.TestCase):
    def test_profiles_are_explicit_and_historical_default_stays_worker_seventeen(self):
        self.assertEqual(proof.profile_schemas(proof.DEFAULT_PROFILE), (17, 20))
        self.assertEqual(proof.profile_schemas('worker19-updater20'), (19, 20))
        for profile in (None, 19, True, 'worker18-updater20', 'worker19-updater21', 'auto'):
            with self.subTest(profile=profile), self.assertRaises(ValueError):
                proof.profile_schemas(profile)

    def test_each_schema_binding_must_match_profile_without_numeric_coercion(self):
        for profile, (worker, updater) in proof.PROFILES.items():
            proof.require_schema_binding({'workerJournalSchema': worker, 'updaterJournalSchema': updater}, profile)
            for field in ('workerJournalSchema', 'updaterJournalSchema'):
                for replacement in (None, True, float(worker), str(worker), 16, 18, 21):
                    value = {'workerJournalSchema': worker, 'updaterJournalSchema': updater}
                    value[field] = replacement
                    with self.subTest(profile=profile, field=field, value=replacement), self.assertRaises(ValueError):
                        proof.require_schema_binding(value, profile)
            other = 19 if worker == 17 else 17
            with self.assertRaises(ValueError):
                proof.require_schema_binding({'workerJournalSchema': other, 'updaterJournalSchema': updater}, profile)

    def test_cold_worker_migration_proof_requires_each_fact_and_bound_source(self):
        import copy
        source = {'sourceCommit': 'a'*40, 'manifestSha256': 'b'*64}
        reference = 'c'*64
        flags = ('oldHistoryPreserved', 'verifiedMigrationBackup', 'outputPreserved', 'dedupPreserved',
                 'legacyCreatorUnknown', 'creatorSurvivesRestart', 'cancellationSurvivesRestart', 'oldWorkerRefusedWithoutJournalWrite')
        value = dict(schemaVersion=1, kind='worker_attribution_migration_native_proof', passed=True, platform='windows',
                     fromSchema=17, toSchema=19, referenceSha256=reference,
                     build={'sourceCommit': source['sourceCommit'], 'sourceManifestSha256': source['manifestSha256'], 'sourceDirty': False},
                     installedStateOpened=False, activeUpdaterMigrationChecked=False, completedExternalExecutions=2, startedAndCancelledProcesses=1)
        value.update({flag: True for flag in flags})
        self.assertTrue(proof.verify_worker_migration(value, reference, source)['workerJournalMigrationChecked'])
        for key, replacement in [(flag, 1) for flag in flags] + [('schemaVersion', 2), ('schemaVersion', None),
                ('schemaVersion', True), ('toSchema', 17), ('fromSchema', True),
                ('installedStateOpened', True), ('activeUpdaterMigrationChecked', True), ('referenceSha256', 'd'*64),
                ('completedExternalExecutions', True), ('startedAndCancelledProcesses', 2)]:
            bad = copy.deepcopy(value); bad[key] = replacement
            with self.subTest(key=key), self.assertRaises(ValueError):
                proof.verify_worker_migration(bad, reference, source)
        bad = copy.deepcopy(value); bad['build']['sourceDirty'] = True
        with self.assertRaises(ValueError):
            proof.verify_worker_migration(bad, reference, source)

    def test_resource_policy_metadata_allows_only_the_bound_next_epoch(self):
        good = {'schemaVersion': 1, 'normalizedColumn': 'resource_policy.epoch',
                'policyUnchanged': True, 'beforeEpoch': 1, 'afterEpoch': 2}
        proof.require_resource_policy_reconciliation(good, 1, 2)
        for key, replacement in [('beforeEpoch', True), ('afterEpoch', 3), ('schemaVersion', True),
                                 ('policyUnchanged', 1), ('normalizedColumn', 'resource_policy')]:
            bad = dict(good); bad[key] = replacement
            with self.subTest(key=key), self.assertRaises(ValueError):
                proof.require_resource_policy_reconciliation(bad, 1, 2)
        for absent in (None, {}, []):
            with self.assertRaises(ValueError):
                proof.require_resource_policy_reconciliation(absent, 1, 2)

    def test_boolean_observations_reject_json_number_coercion(self):
        for key in ('lostServiceAckSentOnce', 'actualRetainedProcessExited', 'controllerStarted', 'userResumeRefused'):
            for value in (True, False):
                proof.strict({'nested': [{key: value}]})
            for value in (0, 1, None, 'true', {}, []):
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    proof.strict({'nested': [{key: value}]})

    def test_effect_counts_reject_booleans_and_fractional_values(self):
        for key in ('serviceDispatchCount', 'workerLaunchCount', 'automaticColdLaunchCount', 'epoch'):
            proof.strict({key: 1})
            for value in (True, False, 1.0, -1, 2**53, '1', None):
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    proof.strict({'history': [{key: value}]})

    def test_private_grants_cannot_hide_in_retained_history(self):
        for key in ('nonce', 'token', 'permit', 'startupPermit', 'writeGrant', 'authorityId'):
            with self.subTest(key=key), self.assertRaises(ValueError):
                proof.strict({'history': [{'nested': {key: False}}]})


if __name__ == '__main__':
    unittest.main()
