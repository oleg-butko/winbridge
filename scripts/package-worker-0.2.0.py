#!/usr/bin/env python3
"""Package the retained, tested Windows worker; never contact or change a runtime."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import zipfile


ROOT = Path(__file__).resolve().parents[1]
SOURCE = '48c834c00c658f11feec066c9ae4650a4e6cff7a'
REVIEW = '591b471f0aba266d53f2e773a5610b8cbeba3450'
MANIFEST = '310e6f7fef74b6f8e56b693c9cec028acf68afb7679c9176d349a6c15bcac3dd'
IMAGE = '9dac4060f3abf9a2167ee432b536d9f9ffc9a787730e5d4096ed4a5eda997104'
BASELINE = '536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712'
ARCHIVE = 'd11deff1be30d9b376c64417a0b700498eb85f24d154c7533edcad0b143f0bdd'
READY = 'fdc52ef2000c26f59e979c6b8666632417e31887309b11115c4e928322c2e42d'
RUN = '20261027-183307-dda61a12'
NAME = 'winbridge-worker-0.2.0-48c834c-windows-x64.zip'


def need(value, message):
    if not value:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode()


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def source_equivalence():
    subprocess.run(['git', 'merge-base', '--is-ancestor', SOURCE, REVIEW], cwd=ROOT, check=True)
    changed = git('diff', '--name-only', SOURCE, REVIEW, '--', 'Cargo.toml', 'Cargo.lock',
                  '.cargo', 'crates', 'modules', 'apps', 'ui', 'package.json', 'package-lock.json').splitlines()
    validation = 'apps/update/src/runtime_replacement/activation_process_validation.rs'
    need([p for p in changed if '/tests/' not in p] == [validation],
         'Production sources differ from the tested image; build and validate a new candidate')
    # This changed module is excluded from the normal bundled-sqlite worker.
    for ref in (SOURCE, REVIEW):
        text = git('show', ref + ':apps/update/src/runtime_replacement/activation_process.rs')
        need('#[cfg(feature = "update-validation")]\n#[path = "activation_process_validation.rs"]\nmod validation;'
             in text, 'Validation module is no longer feature-gated')
    return {'reviewedMainCommit': REVIEW, 'binarySourceCommit': SOURCE,
            'normalWorkerCodeEquivalent': True, 'changedCodePaths': changed,
            'excludedFeature': 'update-validation',
            'note': 'The EXE retains its original build identity; it is not relabelled as a build of main.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reports', type=Path, required=True)
    parser.add_argument('--source-archive', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='New directory; existing files are never overwritten')
    args = parser.parse_args()
    need(not args.output.exists(), 'Output directory already exists; release will not be overwritten')
    equivalence = source_equivalence()
    need(sha(args.source_archive.read_bytes()) == ARCHIVE, 'Retained source archive changed')
    need(sha((args.reports / 'report.ready.json').read_bytes()) == READY, 'Retained ready manifest changed')
    reader = module('release_files', 'summarize-update-timings.py')
    files = reader.verified_files(args.reports)
    proof = json.loads(files['network-complete.json'].decode('utf-8-sig'))
    source = proof['source']
    need(source['sourceCommit'] == SOURCE and source['manifestSha256'] == MANIFEST,
         'Native gate used a different source')
    need(sha(files['winbridge-worker.exe']) == IMAGE, 'Tested EXE changed')
    audit = module('release_network', 'multi-controller-native-proof.py').audit(
        files, source, proof['controllerBuild'], r'K:\_proj-26\winbridge\runtime-test-204',
        proof['legacyController'])
    audit.update(module('release_migration', 'update-activation-service-proof.py').verify_worker_migration(
        json.loads(files['worker-journal-attribution-migration-proof.json'].decode('utf-8-sig')),
        BASELINE, source))
    need(reader.verified_files(args.reports) == files, 'Native report changed during publication')
    metadata = {'schemaVersion': 1, 'kind': 'manual_worker_release', 'version': '0.2.0',
                'platform': 'windows-x86_64', 'sourceCommit': SOURCE, 'manifestSha256': MANIFEST,
                'sourceArchiveSha256': ARCHIVE, 'workerSha256': IMAGE, 'baselineWorkerSha256': BASELINE,
                'nativeRun': RUN, 'filesVerifiedTwice': len(files), 'audit': audit,
                'sourceEquivalence': equivalence,
                'profile': 'dev; bundled-sqlite; no update-validation; native gate image unchanged',
                'installationPerformed': False, 'twoDifferentDebianVMsDeploymentVerified': False,
                'scope': 'Manual worker-only update of an inactive runtime without an installed-set manifest; updater unfinished.'}
    # Keep all original ready-bound native files as an independently auditable bundle.
    # The test-only worker.json lives under evidence, never beside the installed EXE.
    payload = {'winbridge-worker.exe': files['winbridge-worker.exe'],
               'build-identity.json': files['worker-build-identity.json'],
               'README.md': (ROOT / 'docs/releases/worker-0.2.0.md').read_bytes(),
               'controller-topology.example.json': (ROOT / 'examples/windows-multi-controller.topology.json').read_bytes(),
               'release.json': encoded(metadata)}
    for name, data in files.items():
        if name != 'winbridge-worker.exe':
            payload['evidence/' + name] = data
    marker = (args.reports / 'report.ready.json').read_bytes()
    need(sha(marker) == READY, 'Original ready manifest changed during publication')
    # The original manifest binds the EXE stored at the ZIP root as well as evidence.
    ready = json.loads(marker.decode('utf-8-sig'))
    for item in ready['files']:
        need(item['path'] in files and item['sha256'] == sha(files[item['path']])
             and item['bytes'] == len(files[item['path']]), 'Original ready manifest changed')
    payload['evidence/report.ready.json'] = marker
    payload['SHA256SUMS.txt'] = ''.join(sha(data) + '  ' + name + '\n'
                                       for name, data in sorted(payload.items())).encode()
    args.output.mkdir(parents=True, exist_ok=False)
    path = args.output / NAME
    with zipfile.ZipFile(path, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in payload.items():
            archive.writestr(name, data)
    with zipfile.ZipFile(path) as archive:
        need(archive.testzip() is None and len(archive.namelist()) == len(payload)
             and set(archive.namelist()) == set(payload), 'ZIP structure or CRC differs')
        need(all(archive.read(name) == data for name, data in payload.items()), 'ZIP readback differs')
        for line in archive.read('SHA256SUMS.txt').decode().splitlines():
            digest, name = line.split('  ', 1)
            need(sha(archive.read(name)) == digest, 'ZIP checksum mismatch')
    zip_sha = sha(path.read_bytes())
    (args.output / (NAME + '.sha256')).write_text(zip_sha + '  ' + NAME + '\n', encoding='utf-8')
    (args.output / 'README.md').write_bytes(payload['README.md'])
    # Last marker means archive, CRC, exact payload bytes and all checksums passed.
    receipt = {'schemaVersion': 1, 'passed': True, 'archive': NAME, 'sha256': zip_sha,
               'bytes': path.stat().st_size, 'entries': len(payload), 'workerSha256': IMAGE,
               'nativeAudit': audit, 'installedRuntimeChanged': False}
    (args.output / 'release.ready.json').write_bytes(encoded(receipt))
    print(json.dumps({'directory': str(args.output), **receipt}, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
