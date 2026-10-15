#!/usr/bin/env python3
"""Verify the installed Windows executable's pool, with immutable requests and no rebuild."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def check_info(info, provider, context):
    catalog = info['contractCatalog']
    view = catalog['workScheduling']
    if info['build'] != provider or info['fencedRequests']['context'] != context or view['provider'] != provider or view['context'] != context:
        raise RuntimeError('Foreign scheduling observation')
    if view['schemaVersion'] != 1 or view['readOnly'] or view['uncertainSlots']:
        raise RuntimeError('Unsupported/readonly/uncertain scheduling state')
    return view


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
        if info['activeTaskCount'] or view['occupiedSlots'] or view['queuedOperations']:
            raise RuntimeError('Installed worker must be idle before this isolated gate')
        local_schemas = json.loads(subprocess.check_output([str(ROOT / 'target/debug/winbridge-controller'), '--contract-schemas']))['schemas']
        expected = next(v for v in local_schemas if v['contract']['id'] == 'winbridge.work-scheduling')
        if expected not in info['contractSchemas']['schemas'] or expected['contract'] not in info['contractCatalog']['supported']:
            raise RuntimeError('Installed scheduling schema not ready; no submit')
        fixture = next(p for p in info['projects'] if p['id'] == 'fixture')
        command = next(r for r in fixture['recipes'] if r['id'] == 'fixture-success')['command']
        executable = PureWindowsPath(command['executable'])
        if executable.name.lower() != 'winbridge-fixture.exe' or command['arguments'] != ['success']:
            raise RuntimeError('Known WinBridge fixture recipe required')
        project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
        run = 'work-installed-' + time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
        directory = ROOT / '.winbridge/architecture-transition' / run
        directory.mkdir(mode=0o700)
        report = Path(config['delivery']['root']) / config['delivery']['namespace'] / 'workflow-validation' / run
        settings = {'workerExecutable': str(executable.parent / 'winbridge-worker.exe'), 'fixtureExecutable': str(executable),
                    'provider': info['build'], 'context': context, 'workerGeneration': info['generation'],
                    'workDirectory': str(PureWindowsPath(project['workspaceRoot']) / 'work-scheduling-validation' / run),
                    'reportDirectory': str(PureWindowsPath(config['delivery']['peerRoot']) / config['delivery']['namespace'] / 'workflow-validation' / run)}
        encoded = json.dumps(settings, ensure_ascii=False).replace("'", "''")
        script = "$wbWorkSettings = '" + encoded + "' | ConvertFrom-Json -Depth 30\n" + (ROOT / 'scripts/verify-work-scheduling.ps1').read_text()
        key = run
        envelope = {'requestContractVersion': 2, 'context': context, 'request': {'method': 'submit', 'request': {
            'protocolVersion': 1, 'requestId': key, 'idempotencyKey': key, 'projectId': project['id'], 'timeoutMs': 240000,
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
    if proof['provider'] != plan['provider'] or len(proof['binarySha256']) != 64 or proof['profile'] != 'installed-work-scheduling-v1':
        raise RuntimeError('Wrong installed executable proof')
    if [s['maxConcurrentWork'] for s in proof['scenarios']] != [1, 2, None]:
        raise RuntimeError('Missing installed pool scenarios')
    for case in proof['scenarios']:
        observed = case['initialInfo']
        v = check_info(observed, plan['provider'], observed['fencedRequests']['context'])
        if case['admissions'] != 45 or case['initialSlots'] != (case['maxConcurrentWork'] or 3) or v['queuedOperations'] != 45-case['initialSlots'] or case['cleanupSlots'] or not case['isolatedStopped'] or not 0 <= case['ipcInfoMs'] < 5000:
            raise RuntimeError('Incomplete installed pool/cleanup/latency evidence')
    final = operations.mcp(args.controller_config, 'winbridge_info', {})['info']
    final_view = check_info(final, plan['provider'], context)
    initial = json.loads((directory / 'initial-working-info.json').read_text())
    if final['generation'] != plan['workerGeneration'] or final['activeTaskCount'] or final_view['occupiedSlots'] or final_view['queuedOperations'] or final_view['maxConcurrentWork'] != initial['contractCatalog']['workScheduling']['maxConcurrentWork']:
        raise RuntimeError('Working worker/policy changed or failed cleanup')
    name = 'observation-' + uuid.uuid4().hex + '.json' if args.finish or (directory / 'completed.json').exists() else 'completed.json'
    operations.save(directory / name, {'passed': True, 'taskId': identifier, 'processId': task['processId'], 'provider': plan['provider'],
        'context': context, 'reportSha256': ready['sha256'], 'reportDirectory': str(report), 'finalWorkingInfo': final,
        'checks': ['installed-executable-three-policies', '135-admissions-three-projects', 'old-backlog-visible', 'local-ipc-fenced-cancel', 'fair-next-project', 'duplicate-same-process', 'zero-cleanup-slots', 'working-provider-policy-unchanged']})
    print(json.dumps({'passed': True, 'gateDirectory': str(directory), 'taskId': identifier, 'reportSha256': ready['sha256']}))


if __name__ == '__main__':
    main()
