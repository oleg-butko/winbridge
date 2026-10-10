#!/usr/bin/env python3
"""Prepare two committed service projects for the automated stage-7 gate.

Preparation only: separate clones, explicit validation commits/diffs, pinned
shared packages and recipe/workflow configuration. Never submits Windows work.
"""
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def prepare(config, windows_run, run_id):
    local = ROOT / '.winbridge' / 'projects-validation' / run_id
    local.mkdir(parents=True)
    spec = importlib.util.spec_from_file_location('netwatch', ROOT / 'scripts/prepare-netwatch.py')
    netwatch = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(netwatch)
    sources = []
    for project_id in ('netwatch-agent', 'winbridge-worker'):
        source = local / project_id
        if project_id == 'netwatch-agent':
            original, _ = netwatch.isolated_clone(Path('/home/user1/netwatch'), source)
            shutil.copyfile(ROOT / 'fixtures/netwatch/validation-build.ps1', source / 'winbridge-service-build.ps1')
        else:
            original = command('git', '-C', str(ROOT), 'rev-parse', 'HEAD')
            subprocess.run(['git', 'clone', '--quiet', '--no-hardlinks', str(ROOT), str(source)], check=True)
            subprocess.run(['git', '-C', str(source), 'checkout', '--quiet', original], check=True)
            shutil.copyfile(ROOT / 'fixtures/worker-project/build.ps1', source / 'winbridge-service-build.ps1')
            shutil.copyfile(ROOT / 'fixtures/worker-project/start.ps1', source / 'winbridge-service-start.ps1')
            # The runtime identifies the base code plus explicit validation edits.
            # Do not pretend this additional commit is the unmodified release.
            names = command('git', '-C', str(source), 'ls-files').splitlines()
            names += ['winbridge-service-build.ps1', 'winbridge-service-start.ps1']
            identity = {'schemaVersion': 1, 'sourceCommit': original, 'dirty': True,
                        'files': [{'path': name, 'bytes': (source / name).stat().st_size,
                                   'sha256': hashlib.sha256((source / name).read_bytes()).hexdigest()}
                                  for name in sorted(names)]}
            (source / 'winbridge-source-manifest.json').write_text(json.dumps(identity, indent=2))
        subprocess.run(['git', '-C', str(source), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(source), '-c', 'user.name=WinBridge validation',
                        '-c', 'user.email=validation@local', 'commit', '--quiet', '-m',
                        'test(service): add explicit shared-profile validation adapter'], check=True)
        commit = command('git', '-C', str(source), 'rev-parse', 'HEAD')
        patch = command('git', '-C', str(source), 'diff', original, commit) + '\n'
        patch_path = local / (project_id + '.patch')
        patch_path.write_text(patch)
        item = {'projectId': project_id, 'sourceRoot': str(source), 'baseCommit': original,
                'validationCommit': commit, 'patchSha256': hashlib.sha256(patch.encode()).hexdigest()}
        if project_id == 'winbridge-worker':
            item['buildIdentityManifestSha256'] = hashlib.sha256((source / 'winbridge-source-manifest.json').read_bytes()).hexdigest()
        sources.append(item)
    shared = Path(config['delivery']['root'])
    delivery_root = shared / config['delivery']['namespace'] / 'projects-validation' / run_id
    delivery_root.mkdir(parents=True)
    publisher = {**config, 'stateDirectory': str(local / 'capture'),
                 'delivery': {**config['delivery'], 'root': str(delivery_root), 'namespace': 'packages'},
                 'projects': [{'id': s['projectId'], 'sourceRoot': s['sourceRoot'], 'sourceMode': 'commit'} for s in sources]}
    publisher_path = local / 'publisher.local.json'
    publisher_path.write_text(json.dumps(publisher, indent=2))
    windows_delivery = str(PureWindowsPath(config['delivery']['peerRoot']) / PureWindowsPath(delivery_root.relative_to(shared)))
    windows_root = PureWindowsPath(windows_run) / 'projects'
    profile = json.loads((ROOT / 'examples/service-profile.json').read_text())
    projects, workflows = [], []
    for item in sources:
        project_id = item['projectId']
        ready = json.loads(command(str(ROOT / 'target/debug/winbridge-controller'), '--config', str(publisher_path), 'publish', project_id, 'HEAD'))
        item['ready'] = ready
        root = windows_root / project_id
        runtime, logs, cache = root / 'application/runtime', root / 'application/logs', root / 'build-cache'
        if project_id == 'netwatch-agent':
            instance = 'winbridge-' + run_id
            start = {'id': 'start-service', 'executable': str(cache / 'debug/netwatch-agent.exe'),
                     'arguments': ['--managed', '--instance-id', instance, '--runtime-directory', str(runtime), '--log-directory', str(logs)],
                     'workingDirectory': '.', 'timeoutMs': 900000}
            readiness = {'netwatch': {'instanceId': instance, 'runtimeDirectory': 'application/runtime'}, 'timeoutMs': 15000}
            log_path = 'application/logs/netwatch-agent.log'
            artifacts = [{'name': 'Netwatch agent', 'path': 'build-cache/debug/netwatch-agent.exe'}]
        else:
            start = {'id': 'start-service', 'executable': 'pwsh.exe',
                     'arguments': ['-NoProfile', '-File', './winbridge-service-start.ps1', '-RuntimeDirectory', str(runtime),
                                   '-LogDirectory', str(logs), '-TargetDirectory', str(cache)],
                     'workingDirectory': '.', 'timeoutMs': 900000}
            readiness = {'stdoutContains': 'WinBridge worker ready:', 'timeoutMs': 15000}
            log_path = 'application/logs/worker.log'
            artifacts = [{'name': 'WinBridge worker', 'path': 'build-cache/debug/winbridge-worker.exe'},
                         {'name': 'Managed fixture', 'path': 'build-cache/debug/winbridge-fixture.exe'}]
        projects.append({'id': project_id, 'workspaceRoot': str(root), 'allowDirectCommands': False,
                         'maxConcurrentMutations': 1, 'recipeProfile': profile['id'], 'recipes': [start]})
        workflows.append({'protocolVersion': 1, 'requestId': run_id + '-' + project_id,
                          'idempotencyKey': run_id + '-' + project_id, 'name': project_id + ' · общий профиль сборки',
                          'projectId': project_id, 'snapshotId': ready['snapshotId'], 'manifestSha256': ready['manifestSha256'],
                          'timeoutMs': 900000, 'build': {'kind': 'recipe', 'recipeId': 'build-service', 'arguments': []},
                          'start': {'kind': 'recipe', 'recipeId': 'start-service', 'arguments': []},
                          'readiness': readiness, 'logPath': log_path, 'artifacts': artifacts})
    prepared = {'schemaVersion': 1, 'runId': run_id, 'windowsRoot': str(windows_root),
                'delivery': {'adapter': 'shared-folder', 'root': windows_delivery, 'namespace': 'packages'},
                'recipeProfiles': [profile], 'projects': projects, 'workflows': workflows, 'sources': sources}
    (local / 'prepared.json').write_text(json.dumps(prepared, ensure_ascii=False, indent=2))
    # Retain readable preparation evidence beside the native result; no rewrites.
    evidence = delivery_root / 'preparation'
    evidence.mkdir(parents=True)
    for path in (local / 'prepared.json', *local.glob('*.patch')):
        shutil.copyfile(path, evidence / path.name)
    return prepared
