#!/usr/bin/env python3
"""Verify a completed profile pilot and compare samples; never execute Windows work."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import statistics

SPEC = Path(__file__).with_name('experiments') / 'update-profile-v1.json'


def need(condition, message):
    if not condition:
        raise ValueError(message)


def counter(value):
    return type(value) is int and value >= 0


def compare_samples(samples, warm_builds, setup_builds, spec):
    need([x['variant'] for x in samples] == spec['sampleOrder'], 'Sample order differs')
    need(all(counter(x['elapsedMs']) and x['elapsedMs'] > 0 for x in samples), 'Invalid sample duration')
    values = {v: [x['elapsedMs'] for x in samples if x['variant'] == v] for v in ('A', 'B')}
    median = {v: statistics.median(values[v]) for v in values}
    spread = {v: (max(values[v])-min(values[v]))/median[v] for v in values}
    pairs = [dict((x['variant'], x['elapsedMs']) for x in samples[i:i+2]) for i in range(0, len(samples), 2)]
    faster = sum(p['B'] < p['A'] for p in pairs)
    gain = 1-median['B']/median['A']
    noisy = any(x > spec['maximumRelativeSpread'] for x in spread.values())
    promising = not noisy and gain >= spec['minimumMedianGain'] and faster >= spec['minimumFasterPairs']
    need(all(counter(x) for costs in (warm_builds, setup_builds) for x in costs.values()), 'Invalid build duration')
    return dict(decision='inconclusive_noise' if noisy else 'promising_component_only' if promising else 'no_stable_gain',
                medianExecutionMs=median, medianGain=gain, relativeSpread=spread, fasterPairs=faster,
                warmBuildMs=warm_builds, warmBuildPlusMedianExecutionMs={v: warm_builds[v]+median[v] for v in median},
                observedSetupBuildMs=setup_builds,
                interpretation='Setup builds inherit cache history and are not a cold A/B comparison. '
                'Execution samples exclude Cargo. A pilot gain is not a full native-gate speedup or updater readiness.')


def verify(directory):
    loader = importlib.util.spec_from_file_location('timings', Path(__file__).with_name('summarize-update-timings.py'))
    timings = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(timings)
    files = timings.verified_files(directory)
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    spec_bytes = SPEC.read_bytes()
    spec = json.loads(spec_bytes)
    need(type(report['schemaVersion']) is int and report['schemaVersion'] == 1
         and report['kind'] == 'update_profile_pilot' and report['passed'] is True
         and report['platform'] == 'windows' and report['installedRuntimeChanged'] is False
         and report['sourceDirty'] is False and report['error'] is None, 'Pilot did not complete safely')
    need(report['specSha256'] == hashlib.sha256(spec_bytes).hexdigest(), 'Different experiment spec')
    need(re.fullmatch('[a-f0-9]{40,64}', report['sourceCommit']) and re.fullmatch('[a-f0-9]{64}', report['sourceManifestSha256']), 'Invalid source pin')
    names = report['testNames']
    need(names == sorted(set(names)) and len(names) >= spec['minimumTests']
         and all(n in names for n in spec['requiredTests']), 'Incomplete test set')
    commands = {}
    for command in report['commands']:
        name = command['name']
        need(name not in commands and command['log'] == name+'.log' and command['log'] in files, 'Duplicate/missing command log')
        need(type(command['exitCode']) is int and command['exitCode'] == 0 and counter(command['elapsedMs']), 'Failed/invalid command')
        commands[name] = command
    expected = [name for v in ('A', 'B') for name in (f'build-{v}-setup', f'build-{v}-warm', f'list-{v}', f'policy-{v}')]
    expected += [f'{stage}-{i}-{v}' for stage, order in [('warmup', spec['warmupOrder']), ('sample', spec['sampleOrder'])] for i, v in enumerate(order)]
    need(list(commands) == expected, 'Command order/set differs')
    build_rows = {(x['variant'], x['phase']): x for x in report['builds']}
    need(len(build_rows) == len(report['builds']) == 4, 'Build set differs')
    binary_rows = {x['variant']: x for x in report['binaries']}
    need(len(binary_rows) == len(report['binaries']) == 2 and set(binary_rows) == {'A', 'B'}, 'Binary set differs')
    for v in ('A', 'B'):
        binary = binary_rows[v]
        need(binary['file'] == f'tests-{v}.exe' and counter(binary['bytes']) and binary['bytes'] > 0, 'Invalid binary pin')
        data = files[binary['file']]
        need(len(data) == binary['bytes'] and hashlib.sha256(data).hexdigest() == binary['sha256'], 'Frozen binary differs')
        need(sorted(binary['features']) == sorted(spec['features']), 'Binary features differ')
        for phase in ('setup', 'warm'):
            row = build_rows[v, phase]
            need(row['command'] == f'build-{v}-{phase}', 'Build command mismatch')
            command = commands[row['command']]
            args = command['arguments']
            for profile in ('dev', 'test'):
                for setting in ('opt-level=0', 'debug=2', 'debug-assertions=true', 'overflow-checks=true', 'incremental=false'):
                    need(f'profile.{profile}.{setting}' in args, 'Missing explicit safety setting')
                for package in spec['packages']:
                    need(f'profile.{profile}.package.{package}.opt-level={spec["variants"][v]}' in args, 'Profile override differs')
            need(all(a in args for a in ('--locked', '--offline', '--no-run', '--message-format=json', '--timings')), 'Build invocation differs')
            messages = [json.loads(line) for line in files[command['log']].decode('utf-8-sig').splitlines() if line.startswith('{')]
            artifacts = [x for x in messages if x['reason'] == 'compiler-artifact']
            for package in spec['packages']:
                matches = [x for x in artifacts if x['target']['name'] == package.replace('-', '_') and 'lib' in x['target']['kind']]
                need(matches, 'Missing package profile evidence: '+package)
                for artifact in matches:
                    profile = artifact['profile']
                    need(profile['opt_level'] == str(spec['variants'][v]) and type(profile['debuginfo']) is int
                         and profile['debuginfo'] == 2 and profile['debug_assertions'] is True
                         and profile['overflow_checks'] is True, 'Compiled profile weakened or differs')
            roots = [x for x in artifacts if x['target']['name'] == 'winbridge_update' and x['profile']['test'] is True and x['executable']]
            need(len(roots) == 1 and roots[0] == row['artifact'] and roots[0]['profile'] == binary['profile']
                 and sorted(roots[0]['features']) == sorted(spec['features']), 'Libtest artifact differs')
            identity = dict(pair for m in messages if m['reason'] == 'build-script-executed' for pair in m['env'] if pair[0].startswith('WINBRIDGE_'))
            need(identity == row['sourceIdentity'] and identity['WINBRIDGE_SOURCE_COMMIT'] == report['sourceCommit']
                 and identity['WINBRIDGE_SOURCE_MANIFEST_SHA256'] == report['sourceManifestSha256']
                 and identity['WINBRIDGE_SOURCE_DIRTY'] == 'false', 'Compiled source identity differs')
            need(f'build-{v}-{phase}.html' in files, 'Cargo HTML timing missing')
            need(row['space'] and all(counter(s['freeBytes']) and s['freeBytes'] >= 6*1024**3 for s in row['space']), 'Build disk admission missing')
        listing = files[commands[f'list-{v}']['log']].decode('utf-8-sig')
        need(sorted(re.findall(r'^(.+): test$', listing, re.M)) == names, 'Listed tests differ')
        policy = files[commands[f'policy-{v}']['log']].decode('utf-8-sig')
        need(commands[f'policy-{v}']['arguments'] == [spec['policyTest'], '--exact', '--ignored', '--test-threads=1', '--nocapture', '--color=never']
             and re.search(r'test result: ok\. 1 passed; 0 failed; 0 ignored;', policy), 'Safety policy probe missing')
    for stage, order in [('warmup', spec['warmupOrder']), ('sample', spec['sampleOrder'])]:
        for i, v in enumerate(order):
            command = commands[f'{stage}-{i}-{v}']
            need(command['arguments'] == [spec['filter'], '--test-threads=1', '--nocapture', '--color=never']
                 and command['executable'].replace('\\', '/').endswith('/'+binary_rows[v]['file']), 'Measured invocation differs')
            log = files[command['log']].decode('utf-8-sig')
            results = re.findall(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;[^\r\n]*finished in ([0-9.]+)s', log)
            need(len(results) == 1 and int(results[0][0]) == len(names), 'Measured test count differs')
            need(float(results[0][1])*1000 == command['libtestMs'], 'Libtest time differs')
    samples = [dict(variant=v, elapsedMs=commands[f'sample-{i}-{v}']['elapsedMs']) for i, v in enumerate(spec['sampleOrder'])]
    costs = {phase: {v: commands[f'build-{v}-{phase}']['elapsedMs'] for v in ('A', 'B')} for phase in ('warm', 'setup')}
    result = compare_samples(samples, costs['warm'], costs['setup'], spec)
    return dict(sourceCommit=report['sourceCommit'], filesVerifiedTwice=len(files), testsPerSample=len(names), **result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report_directory', type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.report_directory), indent=2, ensure_ascii=False))
