#!/usr/bin/env python3
"""Synthetic publication regressions; these are not native release evidence."""
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('release', Path(__file__).with_name('worker-release-proof.py'))
proof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof)


def fixture():
    source = dict(sourceCommit='a' * 40, sha256='b' * 64)
    manifest = 'c' * 64
    image = b'MZ synthetic fixture, not a Windows executable'
    sha = hashlib.sha256(image).hexdigest()
    build = dict(version='0.1.1', sourceCommit=source['sourceCommit'], sourceManifestSha256=manifest,
                 sourceDirty=False, buildId='fixture')
    descriptor = dict(role='worker', targetOs='windows', targetArch='x86_64', build=build)
    network = dict(passed=True, build=build, duplicateSameTask=True, staleScopeRejected=True,
                   retainedTaskCount=1, output='success Привет ✓')
    common = ['test', '-p', 'winbridge-worker', '--locked', '--offline', '--features',
              'bundled-sqlite', '--target-dir', 'K:/cache']
    selected = ['--lib'] + [a for suite in proof.SUITES for a in ['--test', suite]] + ['--', '--test-threads=1']
    commands = [dict(name=name, log=name + '.log', exitCode=0, elapsedMs=1000, freeBytesBefore=10 * 1024**3,
                     arguments=common + args) for name, args in [
        ('worker-regressions', selected), ('idle-cpu', ['--test', 'idle_scheduler',
         'native_idle_cpu_baseline_and_candidate', '--', '--ignored', '--nocapture'])]]
    commands.append(dict(name='test-runtime-policy', log='test-runtime-policy.log', exitCode=0,
                    elapsedMs=1, freeBytesBefore=10 * 1024**3, arguments=['-NoProfile', '-NonInteractive', '-File',
                    'K:/src/scripts/test-worker-runtime.ps1', '-WorkDirectory', 'K:/case/policy', '-Fixture', 'K:/fixture.exe']))
    cpu = [dict(variant=v, rows=256, argumentBytesPerRecord=32768, wallMs=33000,
                cpuMs=n, percentOfOneCore=100 * n / 33000) for v, n in [('baseline', 32000), ('candidate', 500)]]
    report = dict(schemaVersion=1, kind='worker_patch_release', version='0.1.1', passed=True, error=None,
                  installedRuntimeChanged=False, windowsOpened=False, isolatedWorkerExitConfirmed=True, syntaxVerified=True,
                  sourceCommit=source['sourceCommit'], manifestSha256=manifest, archiveSha256=source['sha256'],
                  commands=commands, cpu=cpu, network=network, baselineWorkerSha256='d' * 64,
                  testRuntimeDirectory='K:/winbridge/runtime-test', testExecutable='K:/winbridge/runtime-test/winbridge-worker.exe',
                  testExecutableSha256=sha, firewallWaitSeconds=900,
                  artifactQuery=dict(kind='artifact_query', cleanupVerified=True, readOnly=True,
                    candidateVerified=False, image=dict(querySupported=True, sha256=sha, bytes=len(image)), descriptor=descriptor),
                  artifacts=[dict(name='winbridge-worker.exe', bytes=len(image), sha256=sha)])
    files = {'winbridge-worker.exe': image, 'build-identity.json': json.dumps(descriptor).encode(),
             'source-final-validation.log': json.dumps(dict(kind='source_verified', sourceCommit=source['sourceCommit'], manifestSha256=manifest)).encode(),
             'worker-regressions.log': ('test result: ok. 1 passed; 0 failed;\n' * 9).encode(),
             'test-runtime-policy.log': b'TEST_RUNTIME_POLICY {"passed":true,"stablePath":true,"checksPassed":6}',
             'idle-cpu.log': ('\n'.join('IDLE_CPU ' + json.dumps(s) for s in cpu) + '\ntest result: ok. 1 passed; 0 failed;').encode()}
    # Report and independent network observation must not alias.
    return json.loads(json.dumps(report)), files, source, manifest, json.loads(json.dumps(network))


def verify(report, files, source, manifest, network):
    return proof.verify(dict(files, **{'report.json': json.dumps(report).encode()}), source, manifest, network,
                        'd' * 64, 'K:/winbridge/runtime-test', 900)


class ReleaseProofTests(unittest.TestCase):
    def test_stable_directory_is_independent_of_run_and_cannot_overlap_production(self):
        spec = importlib.util.spec_from_file_location('builder', Path(__file__).with_name('build-worker-release-native.py'))
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        installed = 'K:/_proj-26/winbridge/runtime/winbridge-worker.exe'
        self.assertEqual(builder.stable_runtime_directory(installed), 'K:\\_proj-26\\winbridge\\runtime-test')
        for path in ('relative', 'K:relative', '//host/share/test', 'K:/_proj-26/winbridge/runtime',
                     'K:/_proj-26/winbridge', 'K:/_proj-26/winbridge/runtime/test', 'K:/foo/../runtime-test'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                builder.stable_runtime_directory(installed, path)

    def test_complete_components_accept_only_the_exact_frozen_image(self):
        result = verify(*fixture())
        self.assertEqual(result['nativeTestsPassed'], 10)
        self.assertEqual(result['testRuntimeChecksPassed'], 6)

    def test_publication_rejects_partial_changed_or_numeric_success(self):
        mutations = [lambda r, f, n: r.update(passed=1),
                     lambda r, f, n: r.update(isolatedWorkerExitConfirmed=False),
                     lambda r, f, n: r['commands'][0].update(exitCode=False),
                     lambda r, f, n: r['commands'][0]['arguments'].remove('--lib'),
                     lambda r, f, n: r['commands'][1]['arguments'].remove('--ignored'),
                     lambda r, f, n: f.update({'worker-regressions.log': b'test result: ok. 1 passed; 0 failed;'}),
                     lambda r, f, n: f.update({'winbridge-worker.exe': b'changed'}),
                     lambda r, f, n: r['artifactQuery']['descriptor']['build'].update(sourceDirty=True),
                     lambda r, f, n: r['network']['build'].update(buildId='another frozen executable'),
                     lambda r, f, n: r['network'].update(retainedTaskCount=True),
                     lambda r, f, n: r.update(installedRuntimeChanged=True),
                     lambda r, f, n: r.update(baselineWorkerSha256='e' * 64),
                     lambda r, f, n: r.update(testExecutable='K:/random-run/winbridge-worker.exe'),
                     lambda r, f, n: r.update(firewallWaitSeconds=True),
                     lambda r, f, n: r.update(testExecutableSha256='f' * 64),
                     lambda r, f, n: f.update({'test-runtime-policy.log': b'TEST_RUNTIME_POLICY {"passed":true,"stablePath":true,"checksPassed":true}'}),
                     lambda r, f, n: f.update({'source-final-validation.log': b'{"kind":"source_verified","sourceCommit":"wrong","manifestSha256":"wrong"}'})]
        for i, mutate in enumerate(mutations):
            with self.subTest(case=i):
                r, f, s, m, n = fixture()
                mutate(r, f, n)
                with self.assertRaises(ValueError):
                    verify(r, f, s, m, n)

    def test_cpu_measurement_must_agree_with_log_and_elapsed_time(self):
        for change in [dict(wallMs=True), dict(cpuMs=float('nan')), dict(percentOfOneCore=0), dict(wallMs=10)]:
            with self.subTest(change=change):
                r, f, s, m, n = fixture()
                r['cpu'][1].update(change)
                f['idle-cpu.log'] = ('\n'.join('IDLE_CPU ' + json.dumps(x) for x in r['cpu'])
                                     + '\ntest result: ok. 1 passed; 0 failed;').encode()
                with self.assertRaises(ValueError):
                    verify(r, f, s, m, n)


if __name__ == '__main__':
    unittest.main()
