#!/usr/bin/env python3
"""Start/finish a pinned readonly Windows IPC probe across a controller restart."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', required=True, type=Path)
    parser.add_argument('--pair-gate', type=Path, help='Successful original paired gate directory')
    parser.add_argument('--finish', type=Path, help='Read original probe/report without submitting')
    args = parser.parse_args()
    if bool(args.pair_gate) == bool(args.finish):
        parser.error('Choose --pair-gate to start or --finish to observe; never automatically retry')
    native = module('native', ROOT / 'scripts/validate-workflow-native.py')
    paired = module('paired', ROOT / 'scripts/validate-pair-readiness.py')
    fenced = module('fenced', ROOT / 'scripts/validate-installed-provider.py')
    config = json.loads(args.controller_config.read_text())
    endpoint = Path(config['stateDirectory']) / ('controller-' + config['instanceId'] + '.sock')
    info = native.call(endpoint, {'method': 'info'})['info']
    if args.pair_gate:
        completed = json.loads((args.pair_gate / 'completed.json').read_text())
        verification = completed['view']
        if not completed['passed'] or verification['status'] != 'succeeded' or info['activeTaskCount']:
            raise RuntimeError('Healthy idle Windows provider and successful pair gate required')
        projection = info.get('contractCatalog', {}).get('controllerProjection', {})
        if not any(v['receiptId'] == verification['receiptId'] and v['readiness']['enabled'] for v in projection.get('views', [])):
            raise RuntimeError('Fresh enabled pair projection required before the probe')
        run = 'projection-' + time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
        report = Path(config['delivery']['root']) / config['delivery']['namespace'] / 'workflow-validation' / run
        windows_report = PureWindowsPath(config['delivery']['peerRoot']) / config['delivery']['namespace'] / 'workflow-validation' / run
        settings = {'context': info['fencedRequests']['context'], 'provider': info['build'], 'workerGeneration': info['generation'],
                    'receiptId': verification['receiptId'], 'reportDirectory': str(windows_report)}
        header = json.dumps(settings, ensure_ascii=False).replace("'", "''")
        script = "$wbProjectionSettings = '" + header + "' | ConvertFrom-Json -Depth 30\n" + (ROOT / 'scripts/verify-projection-session.ps1').read_text()
        envelope = {'requestContractVersion': 2, 'context': settings['context'], 'request': {'method': 'submit', 'request': {
            'protocolVersion': 1, 'requestId': run, 'idempotencyKey': run, 'projectId': 'snapshot-fixture', 'timeoutMs': 150000,
            'command': {'kind': 'direct', 'executable': 'pwsh.exe', 'arguments': ['-NoProfile', '-NonInteractive', '-Command', script], 'workingDirectory': '.'}}}}
        local = ROOT / '.winbridge/architecture-transition' / run
        local.mkdir(mode=0o700)
        paired.save_new(local / 'request.json', {'envelope': envelope, 'settings': settings, 'report': str(report)})
        task = fenced.call_fenced(endpoint, envelope)['task']
        paired.save_new(local / 'task.json', {'taskId': task['taskId']})
        print(json.dumps({'gateDirectory': str(local), 'taskId': task['taskId'], 'reportDirectory': str(report)}), flush=True)
        return
    local = args.finish
    plan = json.loads((local / 'request.json').read_text())
    identifier = json.loads((local / 'task.json').read_text())['taskId']
    context = plan['envelope']['context']
    task = fenced.call_fenced(endpoint, {'requestContractVersion': 2, 'context': context, 'request': {'method': 'get_task', 'taskId': identifier}})['task']
    if task['status'] != 'succeeded' or task['exitCode'] != 0:
        raise RuntimeError({'taskId': identifier, 'status': task['status'], 'detail': 'No submit performed; inspect the original task'})
    report = Path(plan['report'])
    data = (report / 'report.json').read_bytes()
    proof = json.loads(data)
    ready = json.loads((report / 'report.ready.json').read_text())
    if ready['schemaVersion'] != 1 or ready['sha256'] != hashlib.sha256(data).hexdigest() or ready['bytes'] != len(data) or not ready['passed']:
        raise RuntimeError('Local IPC probe report checksum/schema mismatch')
    checks = ['actual-local-pipe-ready', 'disconnect-clears-local-cache', 'new-session-same-receipt-ready', 'same-process-worker-and-journal']
    if not proof['passed'] or proof['platform'] != 'windows' or proof['checks'] != checks or proof['processId'] != task['processId']:
        raise RuntimeError('Local IPC probe evidence incomplete')
    if info['build'] != plan['settings']['provider'] or info['generation'] != plan['settings']['workerGeneration'] or info['fencedRequests']['context'] != context:
        raise RuntimeError('Working provider identity changed during probe')
    paired.save_new(local / 'completed.json', {'passed': True, 'taskId': identifier, 'reportSha256': ready['sha256'], 'proof': proof})
    print(json.dumps({'passed': True, 'taskId': identifier, 'reportSha256': ready['sha256'], 'checks': checks}), flush=True)


if __name__ == '__main__':
    main()
