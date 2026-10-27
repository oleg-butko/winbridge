"""Report boundary tests, not native execution evidence."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('service_proof', Path(__file__).with_name('update-activation-service-proof.py'))
proof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof)


class ServiceEvidenceTests(unittest.TestCase):
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
