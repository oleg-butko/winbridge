#!/usr/bin/env python3
"""Verify completed fresh-source desktop compiler trials; never build, run or install."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import statistics

PACKAGE_PATHS = {'winbridge-desktop': 'apps/desktop/src-tauri/Cargo.toml', 'winbridge-core': 'crates/core/Cargo.toml',
                 'winbridge-protocol': 'crates/protocol/Cargo.toml', 'winbridge-platform': 'crates/platform/Cargo.toml',
                 'winbridge-transport': 'crates/transport/Cargo.toml'}
ORDER = [('A', True), ('B', True), ('A', False), ('B', False), ('B', False), ('A', False)]


def need(condition, message):
    if not condition:
        raise ValueError(message)


def integer(value):
    return type(value) is int and value >= 0


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'))


def compare(trials):
    need(len(trials) == len(ORDER), 'Incomplete compiler series')
    need(all((t['variant'], t['warmup']) == pair and type(t['warmup']) is bool for t, pair in zip(trials, ORDER)), 'Compiler order changed')
    need(all(integer(t['elapsedMs']) and t['elapsedMs'] > 0 for t in trials), 'Invalid build duration')
    samples = {v: [t['elapsedMs'] for t in trials if t['variant'] == v and not t['warmup']] for v in ('A', 'B')}
    medians = {v: statistics.median(values) for v, values in samples.items()}
    spread = {v: (max(values)-min(values))/medians[v] for v, values in samples.items()}
    pairs = [dict((t['variant'], t['elapsedMs']) for t in trials[i:i+2]) for i in (2, 4)]
    faster = sum(p['B'] < p['A'] for p in pairs)
    saved = medians['A']-medians['B']
    gain = saved/medians['A']
    noisy = any(s > .2 for s in spread.values())
    promising = not noisy and gain >= .2 and saved >= 30000 and faster == 2
    return dict(decision='inconclusive_noise' if noisy else 'promising_build_pilot' if promising else 'insufficient_gain',
                medianCargoMs=medians, savedMs=saved, relativeGain=gain, relativeSpread=spread, fasterPairs=faster,
                measuredSamplesMs=samples,
                interpretation='Two fresh-source build samples per variant after warmups. Registry cache shared; local crates freshly compiled. '
                'Cargo wall excludes source extraction/hash verification and identity queries. '
                'This is a validation-only desktop build pilot, not production UI performance or full updater readiness.')


def verify(directory, metadata=None):
    loader = importlib.util.spec_from_file_location('timings', Path(__file__).with_name('summarize-update-timings.py'))
    timings = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(timings)
    files = timings.verified_files(directory)
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    need(type(report['schemaVersion']) is int and report['schemaVersion'] == 1 and report['kind'] == 'desktop_release_profile_pilot'
         and report['passed'] is True and report['error'] is None and report['testsExecuted'] is False
         and report['windowsOpened'] is False and report['installedRuntimeChanged'] is False
         and report['controlUsesDefault'] is True and report['validationOnlyCandidate'] is True
         and type(report['candidateOptLevel']) is int and report['candidateOptLevel'] == 0, 'Pilot did not complete in expected scope')
    need(report['packages'] == list(PACKAGE_PATHS), 'Package scope changed')
    need(re.fullmatch('[a-f0-9]{64}', report['archiveSha256']) and re.fullmatch('[a-f0-9]{64}', report['sourceManifestSha256'])
         and re.fullmatch('[a-f0-9]{64}', report['verifierSha256']), 'Invalid source/helper pins')
    if metadata:
        meta = json.loads(metadata.read_text())
        need(report['archiveSha256'] == meta['source']['archiveSha256'] and report['sourceManifestSha256'] == meta['source']['manifestSha256'], 'Reference source changed')
    result = compare(report['trials'])
    policies, directories, commits = {}, set(), set()
    for i, row in enumerate(report['trials']):
        key = f'trial-{i}-{row["variant"]}'
        source = PureWindowsPath(row['sourceDirectory'])
        need(source not in directories and source.name == 'winbridge' and source.parent.name == key, 'Reused/non-isolated source directory')
        directories.add(source)
        commits.add(row['sourceCommit'])
        need(re.fullmatch('[a-f0-9]{40,64}', row['sourceCommit']) and type(row['exitCode']) is int and row['exitCode'] == 0
             and integer(row['freeBytes']) and row['freeBytes'] >= 6*1024**3 and integer(row['setupMs']), 'Invalid exit/disk/setup data')
        level = 3 if row['variant'] == 'A' else 0
        need(type(row['optLevel']) is int and row['optLevel'] == level and row['key'] == key
             and row['log'] == key+'.log' and row['executable'] == key+'.exe' and row['identityLog'] == key+'-identity.log', 'Changed trial flags/files')
        args = row['arguments']
        configs = [v for j, v in enumerate(args) if j > 0 and args[j-1] == '--config']
        expected = [] if row['variant'] == 'A' else [f'profile.release.package.{p}.opt-level=0' for p in PACKAGE_PATHS]
        need(configs == expected, 'Control not ordinary release, or candidate overrides changed')
        need(args[2*len(configs):-1] == ['build', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--features',
             'custom-protocol', '--release', '--locked', '--offline', '--message-format=json', '--timings', '--target-dir'], 'Compiler invocation changed')
        messages = [json.loads(line) for line in files[row['log']].decode('utf-8-sig').splitlines() if line.startswith('{')]
        artifacts = [m for m in messages if m['reason'] == 'compiler-artifact']
        compiled = [a for a in artifacts if a['fresh'] is False]
        need(canonical(compiled) == canonical(row['compiledArtifacts']), 'Freshness metadata differs')
        for package, relative in PACKAGE_PATHS.items():
            matches = [a for a in artifacts if PureWindowsPath(a['manifest_path']) == source/PureWindowsPath(relative)
                       and any(k in a['target']['kind'] for k in ('lib', 'bin'))]
            need(len(matches) == 1, 'Local package artifact absent/ambiguous: '+package)
            artifact = matches[0]
            need(artifact['fresh'] is False and artifact['profile']['opt_level'] == str(level), 'Local build was cached or has wrong optimization')
            recorded = [a for a in row['localArtifacts'] if a['manifest_path'] == artifact['manifest_path']]
            need(len(recorded) == 1 and canonical(recorded[0]) == canonical(artifact), 'Local evidence differs from Cargo JSON')
            profile = dict(artifact['profile'])
            need(type(profile['debug_assertions']) is bool and type(profile['overflow_checks']) is bool and profile['test'] is False, 'Invalid release safety profile')
            profile.pop('opt_level')
            if i == 0:
                policies[package] = profile
            need(canonical(profile) == canonical(policies[package]), 'A/B changed non-opt compiler checks/settings')
        if not row['warmup']:
            need(not [a for a in compiled if not str(PureWindowsPath(a['manifest_path'])).lower().startswith(str(source).lower()+'\\')], 'External dependencies recompiled inside measured sample')
        image = files[row['executable']]
        need(integer(row['bytes']) and row['bytes'] == len(image) and hashlib.sha256(image).hexdigest() == row['sha256'], 'Frozen executable differs')
        query = json.loads(files[row['identityLog']].decode('utf-8-sig'))
        descriptor = query['descriptor']
        need(query['kind'] == 'artifact_query' and query['readOnly'] is True and query['candidateVerified'] is False
             and query['cleanupVerified'] is True and query['image']['querySupported'] is True
             and query['image']['sha256'] == row['sha256'] and query['image']['bytes'] == row['bytes']
             and descriptor['role'] == 'desktop' and descriptor['targetOs'] == 'windows'
             and descriptor['build']['sourceDirty'] is False and descriptor['build']['sourceCommit'] == row['sourceCommit']
             and descriptor['build']['sourceManifestSha256'] == report['sourceManifestSha256'], 'Managed descriptor/source/cleanup failed')
        need(key+'.html' in files, 'Missing compiler timings')
    need(len(commits) == 1, 'Sources differ between trials')
    if metadata:
        need(commits == {meta['source']['sourceCommit']}, 'Source differs from reference')
    return dict(sourceCommit=next(iter(commits)), filesVerifiedTwice=len(files), **result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report_directory', type=Path)
    parser.add_argument('--metadata', type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.report_directory, args.metadata), indent=2, ensure_ascii=False))
