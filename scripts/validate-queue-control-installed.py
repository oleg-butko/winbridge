#!/usr/bin/env python3
"""Verify installed queue control with isolated state; no rebuild or working pause."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
CHECKS = ['installed-executable-no-rebuild', 'root-quota-existing-key',
          'rejected-key-not-reserved', 'separate-pause-controls',
          'running-owner-survives-pause', 'output-cancel-readable',
          'restart-pause-and-receipt', 'stale-revision-no-rebase',
          'zero-cleanup-slots', 'working-queue-unchanged']


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def check_info(info, provider, context):
    view = info['contractCatalog']['queueControl']
    if info['build'] != provider or info['fencedRequests']['context'] != context or view['provider'] != provider or view['context'] != context:
        raise RuntimeError('Foreign queue observation')
    if view['schemaVersion'] != 1 or view['readOnly']:
        raise RuntimeError('Unsupported/readonly queue state')
    return view


def same_queue(before, after):
    return all(before[k] == after[k] for k in ['provider', 'context', 'policy', 'mode', 'revision', 'lastCommandId', 'changedAtMs'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', required=True, type=Path)
    parser.add_argument('--finish', type=Path, help='Observe the original task/report; never submit')
    args = parser.parse_args()
    operations = module('operations', 'validate-operation-pages.py')
    config = json.loads(args.controller_config.read_text())
    endpoint = Path(config['stateDirectory']) / ('controller-' + config['instanceId'] + '.sock')
    info = operations.mcp(args.controller_config, 'winbridge_info', {})['info']
    context = info['fencedRequests']['context']
    view = check_info(info, info['build'], context)
    if info['platform'] != 'windows' or info.get('journalRecoveryRequired') or info.get('maintenanceError'):
        raise RuntimeError('Healthy Windows worker required; no submit')
    if args.finish:
        directory = args.finish
        plan = json.loads((directory / 'request.json').read_text())
        identifier = json.loads((directory / 'prepared.json').read_text())['taskId']
        if info['build'] != plan['provider'] or context != plan['context'] or info['generation'] != plan['workerGeneration']:
            raise RuntimeError('Provider changed; retain original evidence, no rebind')
    else:
        if info['activeTaskCount'] or view['pendingRoots'] or info['contractCatalog']['workScheduling']['occupiedSlots']:
            raise RuntimeError('Installed worker must be idle before this isolated gate')
        if view['mode'] != 'running':
            raise RuntimeError('Working queue is paused; do not bypass the user decision')
        schemas = json.loads(subprocess.check_output([str(ROOT / 'target/debug/winbridge-controller'), '--contract-schemas']))['schemas']
        expected = next(v for v in schemas if v['contract']['id'] == 'winbridge.queue-control')
        if expected not in info['contractSchemas']['schemas'] or expected['contract'] not in info['contractCatalog']['supported']:
            raise RuntimeError('Installed queue schema not ready; no submit')
        fixture = next(p for p in info['projects'] if p['id'] == 'fixture')
        command = next(r for r in fixture['recipes'] if r['id'] == 'fixture-success')['command']
        executable = PureWindowsPath(command['executable'])
        if executable.name.lower() != 'winbridge-fixture.exe' or command['arguments'] != ['success']:
            raise RuntimeError('Known WinBridge fixture recipe required')
        project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
        run = 'queue-installed-' + time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
        directory = ROOT / '.winbridge/architecture-transition' / run
        directory.mkdir(mode=0o700)
        report = Path(config['delivery']['root']) / config['delivery']['namespace'] / 'workflow-validation' / run
        settings = {'workerExecutable': str(executable.parent / 'winbridge-worker.exe'), 'fixtureExecutable': str(executable),
                    'provider': info['build'], 'context': context, 'workerGeneration': info['generation'],
                    'workDirectory': str(PureWindowsPath(project['workspaceRoot']) / 'queue-control-validation' / run),
                    'reportDirectory': str(PureWindowsPath(config['delivery']['peerRoot']) / config['delivery']['namespace'] / 'workflow-validation' / run)}
        encoded = json.dumps(settings, ensure_ascii=False).replace("'", "''")
        script = "$wbQueueSettings = '" + encoded + "' | ConvertFrom-Json -Depth 30\n" + (ROOT / 'scripts/verify-queue-control.ps1').read_text()
        envelope = {'requestContractVersion': 2, 'context': context, 'request': {'method': 'submit', 'request': {
            'protocolVersion': 1, 'requestId': run, 'idempotencyKey': run, 'projectId': project['id'], 'timeoutMs': 240000,
            'command': {'kind': 'direct', 'executable': 'pwsh.exe', 'arguments': ['-NoProfile', '-NonInteractive', '-Command', script], 'workingDirectory': '.'}}}}
        plan = {'provider': info['build'], 'context': context, 'workerGeneration': info['generation'], 'reportDirectory': str(report), 'envelope': envelope}
        operations.save(directory / 'request.json', plan)
        operations.save(directory / 'initial-working-info.json', info)
        task = module('provider', 'validate-installed-provider.py').call_fenced(endpoint, envelope)['task']
        identifier = task['taskId']
        operations.save(directory / 'prepared.json', {'taskId': identifier})
        print(json.dumps({'gateDirectory': str(directory), 'taskId': identifier, 'reportDirectory': str(report)}), flush=True)
        duplicate = module('provider', 'validate-installed-provider.py').call_fenced(endpoint, envelope)['task']
        if duplicate['taskId'] != identifier:
            raise RuntimeError('Parent duplicate changed task ID')
    deadline = time.monotonic() + (20 if args.finish else 270)
    while True:
        task = operations.fenced(args.controller_config, context, {'method': 'get_task', 'taskId': identifier})['task']
        if task['status'] not in ['queued', 'running']:
            break
        if time.monotonic() > deadline:
            raise RuntimeError('Original task still active; use --finish, no new submission')
        time.sleep(.5)
    report = Path(plan['reportDirectory'])
    data = (report / 'report.json').read_bytes()
    ready = json.loads((report / 'report.ready.json').read_text(encoding='utf-8-sig'))
    if ready['bytes'] != len(data) or ready['sha256'] != hashlib.sha256(data).hexdigest():
        raise RuntimeError('Installed report checksum mismatch')
    proof = json.loads(data)
    if task['status'] != 'succeeded' or task['exitCode'] != 0 or not ready['passed'] or not proof['passed']:
        raise RuntimeError({'taskId': identifier, 'taskStatus': task['status'], 'error': proof.get('error'), 'reportDirectory': str(report)})
    if proof['provider'] != plan['provider'] or not re.fullmatch('[0-9a-f]{64}', proof['binarySha256']) or proof['profile'] != 'installed-queue-control-v1' or proof['checks'] != CHECKS or not proof['isolatedStopped']:
        raise RuntimeError('Incomplete installed executable proof')
    observations = proof['observations']
    if len(observations) != 4:
        raise RuntimeError('Missing queue/restart observations')
    test_context = observations[0]['fencedRequests']['context']
    for observed in observations:
        check_info(observed, plan['provider'], test_context)
    modes = [v['contractCatalog']['queueControl']['mode'] for v in observations]
    if modes != ['dispatch_paused', 'paused', 'paused', 'running'] or observations[0]['generation'] == observations[2]['generation']:
        raise RuntimeError('Pause/restart proof incomplete')
    if proof['receipt'] != proof['replay'] or proof['receipt']['toRevision'] != '6' or proof['cancelled']['status'] != 'cancelled' or proof['succeeded']['status'] != 'succeeded' or proof['succeeded']['exitCode'] != 0:
        raise RuntimeError('Receipt/cancel/resume proof incomplete')
    if observations[-1]['contractCatalog']['queueControl']['pendingRoots'] or observations[-1]['contractCatalog']['workScheduling']['occupiedSlots'] or not same_queue(proof['before'], proof['after']):
        raise RuntimeError('Cleanup or working queue preservation unconfirmed')
    final = operations.mcp(args.controller_config, 'winbridge_info', {})['info']
    final_view = check_info(final, plan['provider'], context)
    initial = json.loads((directory / 'initial-working-info.json').read_text())
    if final['generation'] != plan['workerGeneration'] or final['activeTaskCount'] or final_view['pendingRoots'] or not same_queue(initial['contractCatalog']['queueControl'], final_view) or final.get('journalRecoveryRequired') or final.get('maintenanceError'):
        raise RuntimeError('Working worker/policy changed or failed cleanup')
    name = 'observation-' + uuid.uuid4().hex + '.json' if args.finish or (directory / 'completed.json').exists() else 'completed.json'
    operations.save(directory / name, {'passed': True, 'taskId': identifier, 'processId': task['processId'], 'provider': plan['provider'],
        'context': context, 'reportSha256': ready['sha256'], 'reportDirectory': str(report), 'finalWorkingInfo': final, 'checks': CHECKS})
    print(json.dumps({'passed': True, 'gateDirectory': str(directory), 'taskId': identifier, 'reportSha256': ready['sha256']}))


if __name__ == '__main__':
    main()
