#!/usr/bin/env python3
"""Export reviewable project sources, never runtime state, with a verified manifest."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-directory', type=Path, required=True)
    args = parser.parse_args()
    commit = git('rev-parse', 'HEAD').decode().strip()
    dirty = bool(git('status', '--porcelain'))
    names = sorted(set(git('ls-files', '--cached', '--others', '--exclude-standard', '-z').decode().split('\0')) - {''})
    files = []
    for name in names:
        path = PurePosixPath(name)
        if path.is_absolute() or '..' in path.parts or any(part in {'.git', '.winbridge', 'target', 'node_modules', 'dist'} for part in path.parts):
            raise ValueError(f'Unexpected runtime/generated path: {name}')
        if path.name.startswith('.env') or path.name.endswith('.local.json'):
            raise ValueError(f'Local settings cannot be exported: {name}')
        source = ROOT / name
        if not source.exists():  # tracked deletion in an explicitly dirty export
            continue
        if source.is_symlink() or not source.is_file():
            raise ValueError(f'Source must be a regular file: {name}')
        data = source.read_bytes()
        files.append((name, data))
    created = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
    manifest = {'schemaVersion': 1, 'sourceCommit': commit, 'dirty': dirty, 'createdAt': created,
                'files': [{'path': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()} for name, data in files]}
    label = time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()) + '-' + commit[:7] + '-' + uuid.uuid4().hex[:8]
    args.output_directory.mkdir(parents=True, exist_ok=True)
    archive = args.output_directory / ('winbridge-' + label + '.zip')
    staging = archive.with_suffix('.zip.part')
    with zipfile.ZipFile(staging, 'x', compression=zipfile.ZIP_DEFLATED) as output:
        for name, data in files:
            output.writestr('winbridge/' + name, data)
        output.writestr('winbridge/winbridge-source-manifest.json', json.dumps(manifest, ensure_ascii=False, indent=2))
    data = staging.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    with zipfile.ZipFile(staging) as check:
        assert check.testzip() is None
        for name, expected in files:
            assert check.read('winbridge/' + name) == expected
    staging.rename(archive)
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == digest
    ready = {'schemaVersion': 1, 'archive': archive.name, 'sha256': digest, 'bytes': len(data),
             'sourceCommit': commit, 'dirty': dirty, 'createdAt': created, 'fileCount': len(files)}
    archive.with_suffix('.zip.sha256').write_text(digest + '  ' + archive.name + '\n')
    archive.with_suffix('.zip.ready.json').write_text(json.dumps(ready, indent=2) + '\n')
    print(json.dumps({'archivePath': str(archive), **ready}))


if __name__ == '__main__':
    main()
