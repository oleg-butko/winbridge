#!/usr/bin/env python3
"""Verify the actual installed Windows executable through an isolated state, not a rebuild."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import socket
import struct
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def call_fenced(endpoint, envelope):
    encoded = json.dumps(envelope).encode()
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(20)
        stream.connect(str(endpoint))
        stream.sendall(struct.pack('<I', len(encoded)) + encoded)
        def exact(size):
            output = b''
            while len(output) < size:
                part = stream.recv(size - len(output))
                if not part:
                    raise RuntimeError('Unconfirmed fenced reply; retain original request')
                output += part
            return output
        size = struct.unpack('<I', exact(4))[0]
        if size > 1024 * 1024:
            raise RuntimeError('Oversized fenced reply; retain original request')
        response = json.loads(exact(size))
    if response.get('requestContractVersion') != 2:
        raise RuntimeError('Wrong reply contract; no fallback')
    if response.get('outcome') == 'responded' and response.get('context') != envelope['context']:
        raise RuntimeError('Foreign response scope; no retry')
    if response.get('outcome') != 'responded' or response['response']['kind'] == 'error':
        raise RuntimeError(response)
    return response['response']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('native', ROOT / 'scripts/validate-workflow-native.py')
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    config = json.loads(args.controller_config.read_text())
    endpoint = Path(config['stateDirectory']) / ('controller-' + config['instanceId'] + '.sock')
    info = native.call(endpoint, {'method': 'info'})['info']
    if info['platform'] != 'windows' or info['activeTaskCount'] != 0 or info.get('journalRecoveryRequired') or info.get('maintenanceError'):
        raise RuntimeError('Healthy idle installed Windows worker required')
    fixture = next(p for p in info['projects'] if p['id'] == 'fixture')
    recipe = next(r for r in fixture['recipes'] if r['id'] == 'fixture-success')['command']
    fixture_exe = PureWindowsPath(recipe['executable'])
    if fixture_exe.name.lower() != 'winbridge-fixture.exe' or recipe['arguments'] != ['success']:
        raise RuntimeError('Known WinBridge fixture recipe required; no arbitrary recipe executed')
    project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
    run = time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
    windows_root = PureWindowsPath(project['workspaceRoot']) / 'provider-verification' / run
    shared = Path(config['delivery']['root']) / config['delivery']['namespace'] / 'workflow-validation' / run
    windows_report = PureWindowsPath(config['delivery']['peerRoot']) / config['delivery']['namespace'] / 'workflow-validation' / run
    request = {'workerExecutable': str(fixture_exe.parent / 'winbridge-worker.exe'), 'fixtureExecutable': str(fixture_exe),
               'workDirectory': str(windows_root), 'reportDirectory': str(windows_report), 'expectedBuild': info['build'],
               'expectedSchemaSha256': info['contractSchemas']['schemas'][0]['sha256']}
    settings = json.dumps(request, ensure_ascii=False).replace("'", "''")
    script = "$wbVerification = '" + settings + "' | ConvertFrom-Json -Depth 30\n" + (ROOT / 'scripts/verify-installed-provider.ps1').read_text()
    key = 'provider-build-' + run
    envelope = {'requestContractVersion': 2, 'context': info['fencedRequests']['context'], 'request': {'method': 'submit', 'request': {
        'protocolVersion': 1, 'requestId': key, 'projectId': project['id'], 'idempotencyKey': key, 'timeoutMs': 180000,
        'command': {'kind': 'direct', 'executable': 'pwsh.exe', 'arguments': ['-NoProfile', '-NonInteractive', '-Command', script], 'workingDirectory': '.'}}}}
    local = ROOT / '.winbridge/architecture-transition' / ('provider-' + run)
    local.mkdir(parents=True)
    with (local / 'request.json').open('x') as output:
        json.dump({'envelope': envelope, 'provider': info['build']}, output, indent=2)
        output.flush()
        import os
        os.fsync(output.fileno())
    task = call_fenced(endpoint, envelope)['task']
    identifier = task['taskId']
    print(json.dumps({'taskId': identifier, 'runId': run, 'report': str(shared), 'request': str(local / 'request.json')}), flush=True)
    deadline = time.monotonic() + 210
    while time.monotonic() < deadline:
        observed = {'requestContractVersion': 2, 'context': envelope['context'], 'request': {'method': 'get_task', 'taskId': identifier}}
        task = call_fenced(endpoint, observed)['task']
        if task['status'] not in ['queued', 'running']:
            break
        time.sleep(.5)
    else:
        raise RuntimeError('Observation deadline; reconcile the same task, never restart it')
    (local / 'task.json').write_text(json.dumps(task, indent=2))
    if task['status'] != 'succeeded' or task['exitCode'] != 0:
        raise RuntimeError({'taskId': identifier, 'status': task['status'], 'report': str(shared)})
    data = (shared / 'provider-report.json').read_bytes()
    ready = json.loads((shared / 'provider-report.ready.json').read_text(encoding='utf-8-sig'))
    if ready['bytes'] != len(data) or ready['sha256'] != hashlib.sha256(data).hexdigest():
        raise RuntimeError('Provider report checksum/readback mismatch')
    report = json.loads(data)
    expected_checks = ['binary-description', 'ipc-info-schema-policy', 'task-success-dedup', 'output-exit', 'stale-scope-rejected', 'managed-tree-cancel', 'isolated-provider-stopped']
    if not ready['passed'] or not report['passed'] or report['build'] != info['build'] or report['checks'] != expected_checks:
        raise RuntimeError('Incomplete actual installed-provider evidence')
    final = native.call(endpoint, {'method': 'info'})['info']
    if final['build'] != info['build'] or final['generation'] != info['generation'] or final['fencedRequests']['context'] != envelope['context']:
        raise RuntimeError('Installed provider changed; report remains historical, no pair enabled')
    (local / 'completed.json').write_text(json.dumps({'report': report, 'reportSha256': ready['sha256'], 'workingProviderUnchanged': True, 'enablesContracts': False}, indent=2))
    print(json.dumps({'passed': True, 'taskId': identifier, 'reportSha256': ready['sha256'], 'build': report['build'], 'localEvidence': str(local / 'completed.json')}), flush=True)


if __name__ == '__main__':
    main()
