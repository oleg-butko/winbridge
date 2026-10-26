#!/usr/bin/env python3
"""Independent worker patch checks. Input bytes must come from a verified ready set."""
import hashlib
import json
import math
from pathlib import PureWindowsPath
import re

SUITES = ('idle_scheduler', 'runtime', 'leases', 'queue_control',
          'resource_scheduling', 'task_fsm', 'work_scheduling', 'workflows')


def need(value, message):
    if not value:
        raise ValueError(message)


def counter(value):
    return type(value) is int and value >= 0


def same_json(left, right):
    return json.dumps(left, sort_keys=True, allow_nan=False) == json.dumps(right, sort_keys=True, allow_nan=False)


def verify(files, source, manifest_sha, network, baseline_sha, test_runtime, firewall_wait_seconds):
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    need(type(report['schemaVersion']) is int and report['schemaVersion'] == 1
         and report['kind'] == 'worker_patch_release' and report['version'] == '0.1.1'
         and report['passed'] is True and report['error'] is None
         and report['installedRuntimeChanged'] is False and report['windowsOpened'] is False
         and report['syntaxVerified'] is True
         and report['isolatedWorkerExitConfirmed'] is True, 'Incomplete or unsafe patch report')
    need(report['sourceCommit'] == source['sourceCommit']
         and report['manifestSha256'] == manifest_sha
         and report['archiveSha256'] == source['sha256']
         and report['baselineWorkerSha256'] == baseline_sha, 'Patch source or CPU baseline differs')
    commands = report['commands']
    need([c['name'] for c in commands] == ['worker-regressions', 'idle-cpu', 'test-runtime-policy'], 'Release checks omitted or changed')
    for c in commands:
        need(type(c['exitCode']) is int and c['exitCode'] == 0 and counter(c['elapsedMs'])
             and counter(c['freeBytesBefore']) and c['freeBytesBefore'] >= 6 * 1024**3
             and c['log'] == c['name'] + '.log', 'Invalid release command/disk evidence')
        args = c['arguments']
        if c['name'] == 'test-runtime-policy':
            need(len(args) == 8 and args[:3] == ['-NoProfile', '-NonInteractive', '-File']
                 and PureWindowsPath(args[3]).name == 'test-worker-runtime.ps1'
                 and args[4] == '-WorkDirectory' and args[6] == '-Fixture', 'Policy test invocation differs')
            lines = files[c['log']].decode('utf-8-sig').splitlines()
            policies = [json.loads(x[len('TEST_RUNTIME_POLICY '):]) for x in lines if x.startswith('TEST_RUNTIME_POLICY ')]
            need(len(policies) == 1 and policies[0]['passed'] is True and policies[0]['stablePath'] is True
                 and type(policies[0]['checksPassed']) is int and policies[0]['checksPassed'] == 6,
                 'Test runtime safety checks incomplete')
            continue
        need(len(args) > 9 and args[:8] == ['test', '-p', 'winbridge-worker', '--locked', '--offline',
             '--features', 'bundled-sqlite', '--target-dir']
             and PureWindowsPath(args[8]).is_absolute(), 'Unexpected build/features/cache')
        expected = ['--lib'] + [a for suite in SUITES for a in ['--test', suite]] + ['--', '--test-threads=1']
        if c['name'] == 'idle-cpu':
            expected = ['--test', 'idle_scheduler', 'native_idle_cpu_baseline_and_candidate',
                        '--', '--ignored', '--nocapture']
        need(args[9:] == expected, 'Release test selection changed')
        log = files[c['log']].decode('utf-8-sig')
        counts = re.findall(r'test result: ok\. (\d+) passed; 0 failed;', log)
        need(len(counts) == (len(SUITES) + 1 if c['name'] == 'worker-regressions' else 1)
             and all(int(n) > 0 for n in counts), 'Missing actual successful test suites')
    cpu_log = files['idle-cpu.log'].decode('utf-8-sig')
    samples = [json.loads(line[9:]) for line in cpu_log.splitlines() if line.startswith('IDLE_CPU ')]
    need(same_json(samples, report['cpu']) and [s['variant'] for s in samples] == ['baseline', 'candidate'], 'CPU samples differ')
    for s in samples:
        need(type(s['rows']) is int and s['rows'] == 256
             and type(s['argumentBytesPerRecord']) is int and s['argumentBytesPerRecord'] == 32768,
             'CPU history differs')
        need(all(type(s[k]) in (int, float) and math.isfinite(s[k]) and s[k] >= 0
                 for k in ('wallMs', 'cpuMs', 'percentOfOneCore'))
             and s['wallMs'] >= 33000
             and math.isclose(s['percentOfOneCore'], 100 * s['cpuMs'] / s['wallMs'], rel_tol=1e-8, abs_tol=1e-8),
             'Invalid CPU measurement')
    need(samples[1]['percentOfOneCore'] < samples[0]['percentOfOneCore'] * .5, 'Idle CPU regression')
    image = files['winbridge-worker.exe']
    sha = hashlib.sha256(image).hexdigest()
    need(isinstance(report['testRuntimeDirectory'], str) and isinstance(report['testExecutable'], str)
         and PureWindowsPath(report['testRuntimeDirectory']) == PureWindowsPath(test_runtime)
         and PureWindowsPath(report['testExecutable']) == PureWindowsPath(test_runtime) / 'winbridge-worker.exe'
         and report['testExecutableSha256'] == sha
         and type(report['firewallWaitSeconds']) is int and report['firewallWaitSeconds'] == firewall_wait_seconds,
         'Stable test runtime/connection budget differs')
    q = report['artifactQuery']
    descriptor = q['descriptor']
    build = descriptor['build']
    need(q['kind'] == 'artifact_query' and q['cleanupVerified'] is True
         and q['readOnly'] is True and q['candidateVerified'] is False
         and q['image']['querySupported'] is True and q['image']['sha256'] == sha
         and type(q['image']['bytes']) is int and q['image']['bytes'] == len(image)
         and descriptor['role'] == 'worker' and descriptor['targetOs'] == 'windows'
         and descriptor['targetArch'] == 'x86_64' and build['version'] == '0.1.1'
         and build['sourceCommit'] == source['sourceCommit'] and build['sourceManifestSha256'] == manifest_sha
         and build['sourceDirty'] is False, 'Frozen worker identity differs')
    need(same_json(json.loads(files['build-identity.json'].decode('utf-8-sig')), descriptor)
         and same_json(report['artifacts'], [dict(name='winbridge-worker.exe', bytes=len(image), sha256=sha)]), 'Payload pins differ')
    need(same_json(report['network'], network) and network['passed'] is True
         and same_json(network['build'], build) and network['duplicateSameTask'] is True
         and network['staleScopeRejected'] is True and type(network['retainedTaskCount']) is int
         and network['retainedTaskCount'] == 1 and 'Привет' in network['output'], 'Cross-version RPC evidence differs')
    return dict(workerSha256=sha, nativeTestsPassed=sum(int(n) for c in commands for n in
                re.findall(r'test result: ok\. (\d+) passed; 0 failed;', files[c['log']].decode('utf-8-sig'))),
                testRuntimeChecksPassed=6,
                cpuPercentOfOneCore={s['variant']: s['percentOfOneCore'] for s in samples})
