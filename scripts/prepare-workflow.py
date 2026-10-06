#!/usr/bin/env python3
"""Publish a fresh committed workflow fixture and freeze a ready-to-submit profile."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path, required=True)
    parser.add_argument('--project', default='snapshot-fixture')
    parser.add_argument('--submit', action='store_true', help='Submit the frozen profile to an updated, connected worker')
    args = parser.parse_args()
    module = importlib.util.spec_from_file_location('native_rpc', ROOT / 'scripts/validate-workflow-native.py')
    rpc = importlib.util.module_from_spec(module)
    module.loader.exec_module(rpc)
    settings = json.loads(args.controller_config.read_text())
    endpoint = Path(settings['stateDirectory']) / ('controller-' + settings['instanceId'] + '.sock')
    info = rpc.call(endpoint, {'method': 'info'})['info']
    project = next(p for p in info['projects'] if p['id'] == args.project)
    if not project['allowDirectCommands']:
        raise RuntimeError('The fixture profile requires direct commands in its registered project')
    run = ROOT / '.winbridge' / 'workflow-fixture' / (time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8])
    source = run / 'source'
    shutil.copytree(ROOT / 'fixtures/snapshot-project', source, ignore=shutil.ignore_patterns('target', '.git'))
    for argv in [['init', '-q'], ['add', '.'], ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@local', 'commit', '-qm', 'workflow fixture']]:
        subprocess.run(['git', '-C', str(source), *argv], check=True)
    # A separate generated config pins this source. The running controller's
    # configuration is preserved; publishing is a CLI capture operation.
    config = {**settings, 'projects': [{'id': args.project, 'sourceRoot': str(source), 'sourceMode': 'commit'}]}
    config_path = run / 'controller.local.json'
    config_path.write_text(json.dumps(config, indent=2))
    ready = json.loads(subprocess.check_output([str(ROOT / 'target/debug/winbridge-controller'), '--config', str(config_path), 'publish', args.project, 'HEAD']))
    sep = '\\' if info['platform'] == 'windows' else '/'
    root = project['workspaceRoot'].rstrip('\\/')
    path = lambda *parts: sep.join([root, *parts])
    recipe = next((r for r in project['recipes'] if r['id'] == 'snapshot-build'), None)
    build = {'kind': 'recipe', 'recipeId': 'snapshot-build', 'arguments': []} if recipe else {
        'kind': 'direct', 'executable': 'cargo.exe' if info['platform'] == 'windows' else 'cargo',
        'arguments': ['build', '--locked', '--offline', '--target-dir', path('build-cache')], 'workingDirectory': '.'}
    request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'idempotencyKey': str(uuid.uuid4()),
               'name': 'snapshot-fixture · development cycle', 'projectId': args.project,
               'snapshotId': ready['snapshotId'], 'manifestSha256': ready['manifestSha256'], 'timeoutMs': 180000,
               'build': build, 'start': {'kind': 'direct', 'executable': path('build-cache', 'debug', 'snapshot-fixture' + ('.exe' if info['platform'] == 'windows' else '')),
                                         'arguments': ['serve', path('logs', 'app.log'), 'ready'], 'workingDirectory': '.'},
               'readiness': {'stdoutContains': 'READY Привет ✓', 'timeoutMs': 10000}, 'logPath': 'logs/app.log'}
    request_path = run / 'workflow.request.json'
    request_path.write_text(json.dumps(request, ensure_ascii=False, indent=2))
    result = {'requestPath': str(request_path), 'snapshotId': ready['snapshotId'], 'manifestSha256': ready['manifestSha256'], 'workerWorkflowAvailable': info.get('workflowAvailable', False)}
    if args.submit:
        if not info.get('workflowAvailable', False):
            raise RuntimeError(f'Profile prepared at {request_path}; update the fixed worker runtime before submission')
        result['response'] = rpc.call(endpoint, {'method': 'submit_workflow', 'request': request})
    print(json.dumps(result, ensure_ascii=False))


if __name__ == '__main__':
    main()
