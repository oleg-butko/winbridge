#!/usr/bin/env python3
"""Bounded MCP/Windows RPC-2 smoke probe; evidence only, no contract enablement."""
import argparse
import json
import os
from pathlib import Path
import select
import subprocess
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller', default='./target/debug/winbridge-controller')
    parser.add_argument('--controller-config', required=True)
    parser.add_argument('--report', required=True)
    args = parser.parse_args()
    descriptor = json.loads(subprocess.run([args.controller, "--describe"], capture_output=True, text=True, check=True, timeout=20).stdout)
    process = subprocess.Popen([args.controller, '--config', args.controller_config, 'mcp'],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
    try:
        def message(value):
            process.stdin.write(json.dumps(value) + '\n')
            process.stdin.flush()
            if not select.select([process.stdout], [], [], 20)[0]:
                raise RuntimeError('MCP response timeout; no automatic task resubmission')
            line = process.stdout.readline(3 * 1024 * 1024 + 1)
            if len(line) > 3 * 1024 * 1024 or not line.endswith('\n'):
                raise RuntimeError('Invalid/oversized MCP response; acceptance may be unknown')
            return json.loads(line)

        def call(identifier, name, arguments):
            response = message({'jsonrpc': '2.0', 'id': identifier, 'method': 'tools/call',
                                'params': {'name': name, 'arguments': arguments}})
            if response.get('error'):
                raise RuntimeError(response['error'])
            return response['result']

        message({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {
            'protocolVersion': '2025-11-25', 'capabilities': {},
            'clientInfo': {'name': 'winbridge-installed-pair-smoke', 'version': '1'}}})
        process.stdin.write(json.dumps({'jsonrpc': '2.0', 'method': 'notifications/initialized'}) + '\n')
        process.stdin.flush()
        info = call(2, 'winbridge_info', {})['structuredContent']['info']
        project = next(p for p in info['projects'] if p['id'] == 'fixture')
        assert any(r['id'] == 'fixture-success' for r in project['recipes'])
        assert info['platform'] == 'windows' and not info['build']['sourceDirty']
        assert info['activeTaskCount'] == 0 and not info.get('journalRecoveryRequired')
        scope = info['fencedRequests']['context']
        assert info['contractCatalog']['provider'] == info['build']
        assert {'id': 'winbridge.worker-rpc', 'version': 2} in info['contractCatalog']['supported']
        key = 'paired-smoke-' + uuid.uuid4().hex
        request = {'requestContractVersion': 2, 'context': scope, 'request': {
            'method': 'submit', 'request': {'protocolVersion': 1, 'requestId': key,
            'projectId': 'fixture', 'idempotencyKey': key, 'timeoutMs': 10000,
            'command': {'kind': 'recipe', 'recipeId': 'fixture-success', 'arguments': []}}}}
        # Persist the original scope/key BEFORE transmission. A partial report is not proof.
        report = Path(args.report)
        report.parent.mkdir(parents=True, exist_ok=True)
        with report.open('x', encoding='utf-8') as output:
            json.dump({'schemaVersion': 1, 'passed': False, 'request': request}, output)
            output.flush()
            os.fsync(output.fileno())
        directory_fd = os.open(report.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
        response = call(3, 'winbridge_invoke_fenced', request)
        assert not response['isError'], response
        fenced = response['structuredContent']
        assert fenced['outcome'] == 'responded' and fenced['context'] == scope
        task_id = fenced['response']['task']['taskId']
        repeat = call(4, 'winbridge_invoke_fenced', request)
        assert not repeat['isError'] and repeat['structuredContent']['response']['task']['taskId'] == task_id
        stale = json.loads(json.dumps(request))
        stale['context']['journalIncarnation'] = 'stale-' + uuid.uuid4().hex
        refused = call(5, 'winbridge_invoke_fenced', stale)
        assert refused['isError'] and 'original request was not transmitted' in refused['structuredContent']['response']['message']
        deadline = time.monotonic() + 20
        task = None
        identifier = 6
        while time.monotonic() < deadline:
            result = call(identifier, 'winbridge_get_task', {'taskId': task_id})
            assert not result['isError'], result
            task = result['structuredContent']['task']
            identifier += 1
            if task['status'] not in ['queued', 'running']:
                break
            time.sleep(.25)
        assert task and task['status'] == 'succeeded' and task['exitCode'] == 0
        output = call(identifier, 'winbridge_read_output', {'taskId': task_id, 'afterSequence': 0})
        assert not output['isError'], output
        final = call(identifier + 1, 'winbridge_info', {})['structuredContent']['info']
        assert final['generation'] == info['generation'] and final['build'] == info['build']
        assert final['fencedRequests']['context'] == scope
        assert final['controllerConnection']['sessionId'] == info['controllerConnection']['sessionId']
        proof = {'schemaVersion': 1, 'passed': True, 'profile': 'installed-mcp-fenced-task-smoke-v1',
                 'consumer': descriptor['build'], 'provider': info['build'], 'context': scope, 'generation': info['generation'],
                 'connection': info['controllerConnection'], 'request': request, 'taskId': task_id,
                 'taskStatus': task['status'], 'exitCode': task['exitCode'], 'sameKeySameTask': True,
                 'staleContextRefused': True, 'catalogPreserved': True, 'output': output['structuredContent'],
                 'enablesContracts': False}
        # Keep the pre-transmission record unchanged; successful evidence is create-only too.
        completed = report.with_name(report.stem + '.completed.json')
        with completed.open('x', encoding='utf-8') as saved:
            json.dump(proof, saved, ensure_ascii=False, indent=2)
            saved.write('\n')
            saved.flush()
            os.fsync(saved.fileno())
        print(json.dumps({'report': str(completed), 'taskId': task_id, 'passed': True}))
    finally:
        process.stdin.close()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.terminate()
            process.wait(timeout=5)


if __name__ == '__main__':
    main()
