#!/usr/bin/env python3
"""Synthetic desktop benchmark proof tests; no actual compiler or Windows work."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

loader = importlib.util.spec_from_file_location('desktop', Path(__file__).with_name('verify-desktop-release.py'))
desktop = importlib.util.module_from_spec(loader)
loader.loader.exec_module(desktop)


def fixture():
    files = {}
    report = dict(schemaVersion=1, kind='desktop_release_profile_pilot', passed=True, error=None, testsExecuted=False,
                  windowsOpened=False, installedRuntimeChanged=False, controlUsesDefault=True, validationOnlyCandidate=True,
                  candidateOptLevel=0, packages=list(desktop.PACKAGE_PATHS), archiveSha256='a'*64, sourceManifestSha256='b'*64,
                  verifierSha256='c'*64, trials=[])
    for i, (variant, warmup) in enumerate(desktop.ORDER):
        key = f'trial-{i}-{variant}'
        source = f'K:/probe/{key}/winbridge'
        level = 3 if variant == 'A' else 0
        profile = dict(opt_level=str(level), debuginfo=0, debug_assertions=False, overflow_checks=False, test=False)
        artifacts = [dict(reason='compiler-artifact', fresh=False, manifest_path=source+'/'+path,
                          target=dict(kind=['bin' if name == 'winbridge-desktop' else 'lib']), profile=copy.deepcopy(profile))
                     for name, path in desktop.PACKAGE_PATHS.items()]
        args = []
        if variant == 'B':
            for name in desktop.PACKAGE_PATHS:
                args += ['--config', f'profile.release.package.{name}.opt-level=0']
        args += ['build', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--features', 'custom-protocol', '--release',
                 '--locked', '--offline', '--message-format=json', '--timings', '--target-dir', 'K:/cache']
        raw = ('synthetic desktop '+key).encode()
        sha = hashlib.sha256(raw).hexdigest()
        query = dict(kind='artifact_query', readOnly=True, candidateVerified=False, cleanupVerified=True,
                     image=dict(querySupported=True, sha256=sha, bytes=len(raw)), descriptor=dict(role='desktop', targetOs='windows',
                     build=dict(sourceDirty=False, sourceCommit='d'*40, sourceManifestSha256='b'*64)))
        files[key+'.exe'] = raw
        files[key+'.log'] = '\n'.join(json.dumps(a) for a in artifacts).encode()
        files[key+'-identity.log'] = json.dumps(query).encode()
        files[key+'.html'] = b'synthetic compiler timings'
        report['trials'].append(dict(key=key, variant=variant, warmup=warmup, sourceDirectory=source, sourceCommit='d'*40,
              exitCode=0, freeBytes=10*1024**3, setupMs=50, elapsedMs=120000 if variant == 'A' else 60000,
              optLevel=level, arguments=args, log=key+'.log', executable=key+'.exe', identityLog=key+'-identity.log',
              compiledArtifacts=copy.deepcopy(artifacts), localArtifacts=copy.deepcopy(artifacts), bytes=len(raw), sha256=sha))
    return report, files


def check(report, files):
    with tempfile.TemporaryDirectory(prefix='winbridge-desktop-proof-') as tmp:
        root = Path(tmp)
        data = dict(files, **{'report.json': json.dumps(report).encode()})
        ready = dict(schemaVersion=1, files=[])
        for name, raw in data.items():
            (root/name).write_bytes(raw)
            ready['files'].append(dict(path=name, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest()))
        (root/'report.ready.json').write_text(json.dumps(ready))
        return desktop.verify(root)


class DesktopTests(unittest.TestCase):
    def test_valid_complete_report_is_only_build_pilot(self):
        result = check(*fixture())
        self.assertEqual(result['decision'], 'promising_build_pilot')
        self.assertEqual(result['relativeGain'], .5)
        self.assertIn('not production UI', result['interpretation'])

    def test_new_ready_hashes_do_not_make_bad_provenance_acceptable(self):
        def external_compile(r, f):
            row = r['trials'][2]
            extra = dict(reason='compiler-artifact', fresh=False, manifest_path='K:/registry/serde/Cargo.toml')
            row['compiledArtifacts'].append(extra)
            f[row['log']] += b'\n'+json.dumps(extra).encode()
        mutations = [
            lambda r, f: r.update(passed=1),
            lambda r, f: r.update(controlUsesDefault=False),
            lambda r, f: r['trials'][0]['arguments'].__setitem__(slice(0, 0), ['--config', 'profile.release.package.winbridge-desktop.opt-level=3']),
            lambda r, f: r['trials'][0].update(exitCode=False),
            lambda r, f: r['trials'][3].update(sourceDirectory=r['trials'][2]['sourceDirectory']),
            lambda r, f: r['trials'][4].update(warmup=True),
            lambda r, f: f.update({'trial-2-A.exe': b'changed executable'}),
            lambda r, f: r['trials'][3]['localArtifacts'][0]['profile'].update(overflow_checks=True),
            lambda r, f: r['trials'][3]['localArtifacts'][0].update(fresh=True),
            lambda r, f: r['trials'].pop(),
            external_compile,
        ]
        for i, mutate in enumerate(mutations):
            with self.subTest(mutation=i):
                report, files = fixture()
                mutate(report, files)
                with self.assertRaises(ValueError):
                    check(report, files)

    def test_noise_small_absolute_or_relative_gains_are_inconclusive(self):
        for a, b, decision in [(120000, 100000, 'insufficient_gain'), (60000, 40000, 'insufficient_gain')]:
            report, _ = fixture()
            for row in report['trials']:
                row['elapsedMs'] = a if row['variant'] == 'A' else b
            self.assertEqual(desktop.compare(report['trials'])['decision'], decision)
        report, _ = fixture()
        report['trials'][4]['elapsedMs'] = 95000
        self.assertEqual(desktop.compare(report['trials'])['decision'], 'inconclusive_noise')


if __name__ == '__main__':
    unittest.main()
