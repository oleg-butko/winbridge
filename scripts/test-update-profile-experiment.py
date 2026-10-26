#!/usr/bin/env python3
"""Synthetic report/verifier checks. These are not Windows benchmark measurements."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

loader = importlib.util.spec_from_file_location('pilot', Path(__file__).with_name('verify-update-profile.py'))
pilot = importlib.util.module_from_spec(loader)
loader.loader.exec_module(pilot)
SPEC = json.loads(pilot.SPEC.read_bytes())


def fixture():
    names = sorted(SPEC['requiredTests'] + [f'activation::synthetic_{i}' for i in range(SPEC['minimumTests']-len(SPEC['requiredTests']))])
    files = {}
    report = dict(schemaVersion=1, kind='update_profile_pilot', passed=True, platform='windows', installedRuntimeChanged=False,
                  sourceDirty=False, error=None, sourceCommit='a'*40, sourceManifestSha256='b'*64,
                  specSha256=hashlib.sha256(pilot.SPEC.read_bytes()).hexdigest(), testNames=names, builds=[], binaries=[], commands=[])

    def command(name, args, text, ms=100):
        files[name+'.log'] = text.encode()
        row = dict(name=name, log=name+'.log', exitCode=0, elapsedMs=ms, arguments=args, executable='K:/pilot/tests-'+name[-1]+'.exe')
        report['commands'].append(row)
        return row

    for v in ('A', 'B'):
        profile = dict(opt_level=str(SPEC['variants'][v]), debuginfo=2, debug_assertions=True, overflow_checks=True, test=True)
        root = dict(reason='compiler-artifact', target=dict(name='winbridge_update', kind=['lib']), profile=profile,
                    executable='K:/cache/tests-'+v+'.exe', features=SPEC['features'])
        artifacts = [root] + [dict(reason='compiler-artifact', target=dict(name=p.replace('-', '_'), kind=['lib']), profile=profile)
                              for p in SPEC['packages'] if p != 'winbridge-update']
        identity = dict(WINBRIDGE_SOURCE_COMMIT=report['sourceCommit'], WINBRIDGE_SOURCE_MANIFEST_SHA256=report['sourceManifestSha256'], WINBRIDGE_SOURCE_DIRTY='false')
        messages = artifacts + [dict(reason='build-script-executed', env=list(identity.items()))]
        args = ['--locked', '--offline', '--no-run', '--message-format=json', '--timings']
        for prof in ('dev', 'test'):
            args += [f'profile.{prof}.{s}' for s in ('opt-level=0', 'debug=2', 'debug-assertions=true', 'overflow-checks=true', 'incremental=false')]
            args += [f'profile.{prof}.package.{p}.opt-level={SPEC["variants"][v]}' for p in SPEC['packages']]
        for phase in ('setup', 'warm'):
            name = f'build-{v}-{phase}'
            command(name, args, '\n'.join(json.dumps(m) for m in messages))
            report['builds'].append(dict(variant=v, phase=phase, command=name, artifact=copy.deepcopy(root), sourceIdentity=identity,
                                         space=[dict(freeBytes=8*1024**3)]))
            files[name+'.html'] = b'synthetic timing'
        data = ('synthetic executable '+v).encode()
        files[f'tests-{v}.exe'] = data
        report['binaries'].append(dict(variant=v, file=f'tests-{v}.exe', bytes=len(data), sha256=hashlib.sha256(data).hexdigest(), profile=copy.deepcopy(profile), features=SPEC['features']))
        command('list-'+v, [], '\n'.join(n+': test' for n in names)+'\n')
        command('policy-'+v, [SPEC['policyTest'], '--exact', '--ignored', '--test-threads=1', '--nocapture', '--color=never'], 'test result: ok. 1 passed; 0 failed; 0 ignored;')
    for stage, order in [('warmup', SPEC['warmupOrder']), ('sample', SPEC['sampleOrder'])]:
        for i, v in enumerate(order):
            row = command(f'{stage}-{i}-{v}', [SPEC['filter'], '--test-threads=1', '--nocapture', '--color=never'],
                          f'test result: ok. {len(names)} passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1.00s', 1000 if v == 'A' else 750)
            row['libtestMs'] = 1000
    return report, files


def check(report, files):
    with tempfile.TemporaryDirectory(prefix='winbridge-pilot-verify-') as tmp:
        root = Path(tmp)
        data = dict(files, **{'report.json': json.dumps(report).encode()})
        ready = dict(schemaVersion=1, files=[])
        for name, raw in data.items():
            (root/name).write_bytes(raw)
            ready['files'].append(dict(path=name, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest()))
        (root/'report.ready.json').write_text(json.dumps(ready))
        return pilot.verify(root)


class PilotTests(unittest.TestCase):
    def test_complete_synthetic_report_is_component_evidence_only(self):
        result = check(*fixture())
        self.assertEqual(result['decision'], 'promising_component_only')
        self.assertEqual(result['medianGain'], .25)
        self.assertEqual(result['testsPerSample'], 62)
        self.assertIn('not a cold A/B', result['interpretation'])

    def test_report_mutations_rejected_even_with_valid_ready_hashes(self):
        def mutate_cargo(files, edit):
            name = 'build-B-warm.log'
            messages = [json.loads(line) for line in files[name].decode().splitlines()]
            edit(messages)
            files[name] = '\n'.join(json.dumps(m) for m in messages).encode()
        mutations = [
            lambda r, f: r.update(passed=1),
            lambda r, f: r.update(installedRuntimeChanged=True),
            lambda r, f: r['commands'][0].update(exitCode=False),
            lambda r, f: r['commands'].reverse(),
            lambda r, f: r['commands'][-1].update(elapsedMs=True),
            lambda r, f: r['testNames'].pop(),
            lambda r, f: r['binaries'][0].update(features=['bundled-sqlite']),
            lambda r, f: f.update({'tests-A.exe': b'changed image'}),
            lambda r, f: f.update({'policy-B.log': b'test result: ok. 0 passed; 0 failed; 1 ignored;'}),
            lambda r, f: f.update({'sample-0-A.log': b'test result: ok. 1 passed; 0 failed; 0 ignored; finished in 1.00s'}),
            lambda r, f: mutate_cargo(f, lambda ms: ms[0]['profile'].update(debug_assertions=False)),
            lambda r, f: mutate_cargo(f, lambda ms: ms[0]['profile'].update(overflow_checks=1)),
            lambda r, f: mutate_cargo(f, lambda ms: ms[0]['profile'].update(opt_level='0')),
            lambda r, f: mutate_cargo(f, lambda ms: ms[-1]['env'].append(['WINBRIDGE_SOURCE_DIRTY', 'true'])),
            lambda r, f: r['builds'][0]['space'][0].update(freeBytes=1),
        ]
        for i, mutate in enumerate(mutations):
            with self.subTest(mutation=i):
                report, files = fixture()
                mutate(report, files)
                with self.assertRaises(ValueError):
                    check(report, files)

    def test_noisy_or_small_gains_do_not_promote_profile(self):
        for b_times, expected in [([750, 750, 750, 1200], 'inconclusive_noise'), ([950]*4, 'no_stable_gain')]:
            samples = [dict(variant=v, elapsedMs=1000) for v in SPEC['sampleOrder']]
            it = iter(b_times)
            for row in samples:
                if row['variant'] == 'B':
                    row['elapsedMs'] = next(it)
            result = pilot.compare_samples(samples, dict(A=10, B=10), dict(A=200, B=500), SPEC)
            self.assertEqual(result['decision'], expected)


if __name__ == '__main__':
    unittest.main()
