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
    return subprocess.check_output(['git', '--no-replace-objects', '-C', str(ROOT), *args])


def capture_sources():
    if Path(git('rev-parse', '--show-toplevel').decode().strip()).resolve() != ROOT.resolve():
        raise ValueError('Source export requires its own Git checkout, not a parent repository')
    commit = git('rev-parse', 'HEAD').decode().strip()
    dirty = bool(git('status', '--porcelain'))
    # Status can hide assume-unchanged/skip-worktree edits, and the worktree may
    # change after status was read. Compare the bytes actually captured with
    # this pinned commit, independently of the index and Git status hints.
    tree = {}
    for entry in git('ls-tree', '-r', '-z', '--full-tree', commit).split(b'\0'):
        if entry:
            metadata, name = entry.split(b'\t', 1)
            mode, kind, oid = metadata.split()
            if kind != b'blob' or mode not in (b'100644', b'100755'):
                raise ValueError('Source export supports regular files only; no symlinks/submodules')
            tree[name.decode()] = oid.decode()
    algorithm = git('rev-parse', '--show-object-format').decode().strip()
    if algorithm not in ('sha1', 'sha256'):
        raise ValueError('Unsupported Git object format')
    names = sorted(set(git('ls-files', '--cached', '--others', '--exclude-standard', '-z').decode().split('\0')) - {''})
    files = []
    for name in names:
        path = PurePosixPath(name)
        if path.is_absolute() or '..' in path.parts or any(part in {'.git', '.winbridge', 'target', 'node_modules', 'dist'} for part in path.parts):
            raise ValueError(f'Unexpected runtime/generated path: {name}')
        if path.name.startswith('.env') or path.name.endswith('.local.json') or name == 'winbridge-source-manifest.json':
            raise ValueError(f'Local settings or an earlier export manifest cannot be exported: {name}')
        source = ROOT
        for part in path.parts:
            source /= part
            if source.is_symlink():
                raise ValueError(f'Source path contains a symlink: {name}')
        if not source.exists():  # tracked deletion in an explicitly dirty export
            continue
        if not source.is_file():
            raise ValueError(f'Source must be a regular file: {name}')
        data = source.read_bytes()
        blob = hashlib.new(algorithm, b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
        dirty |= tree.get(name) != blob
        files.append((name, data))
    dirty |= {name for name, _ in files} != set(tree)
    dirty |= bool(git('status', '--porcelain')) or git('rev-parse', 'HEAD').decode().strip() != commit
    return commit, dirty, files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-directory', type=Path, required=True)
    parser.add_argument('--include-built-ui', action='store_true', help='Include the locally built web-dist for isolated native desktop validation without Windows npm installation')
    args = parser.parse_args()
    commit, dirty, files = capture_sources()
    if args.include_built_ui:
        frontend = ROOT / 'web-dist'
        if not (frontend / 'index.html').is_file():
            raise ValueError('Build the UI with npm run build:ui before --include-built-ui')
        for source in sorted(frontend.rglob('*')):
            if source.is_symlink():
                raise ValueError('Built frontend contains a symlink')
            if source.is_file():
                data = source.read_bytes()
                if len(data) > 8 * 1024 * 1024:
                    raise ValueError('Built frontend file exceeds 8 MiB')
                files.append((str(source.relative_to(ROOT)), data))
    created = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
    manifest = {'schemaVersion': 1, 'sourceCommit': commit, 'dirty': dirty, 'createdAt': created,
                'includesBuiltUi': args.include_built_ui,
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
