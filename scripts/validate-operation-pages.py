#!/usr/bin/env python3
"""Actual installed read-model gate. --finish/--query are strictly readonly."""
import argparse
import copy
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
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def save(path, value):
    module('paired', 'validate-pair-readiness.py').save_new(path, value)


def mcp(config, tool, arguments, expect_error=False):
    messages = [
        {'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {
            'protocolVersion': '2025-11-25', 'capabilities': {},
            'clientInfo': {'name': 'installed-operation-pages-gate', 'version': '1'}}},
        {'jsonrpc': '2.0', 'method': 'notifications/initialized'},
        {'jsonrpc': '2.0', 'id': 2, 'method': 'tools/call',
         'params': {'name': tool, 'arguments': arguments}},
    ]
    process = subprocess.run([str(ROOT / 'target/debug/winbridge-controller'), '--config', str(config), 'mcp'],
                             input=''.join(json.dumps(v, ensure_ascii=False) + '\n' for v in messages),
                             capture_output=True, text=True, timeout=25, check=True)
    reply = next(json.loads(line) for line in process.stdout.splitlines() if json.loads(line).get('id') == 2)
    if 'error' in reply:
        raise RuntimeError(reply['error'])
    result = reply['result']
    value = result.get('structuredContent')
    if value is None:
        value = json.loads(next(v['text'] for v in result['content'] if v['type'] == 'text'))
    if bool(result.get('isError')) != expect_error:
        raise RuntimeError({'tool': tool, 'unexpectedResult': value.get('result', {}).get('message', value.get('message'))})
    return value


def page(config, request):
    result = mcp(config, 'winbridge_operations', request)['result']
    if result['kind'] != 'operation_page' or result['page']['context'] != request['context']:
        raise RuntimeError('Wrong operation page contract/scope')
    return result['page']


def fenced(config, context, request):
    response = mcp(config, 'winbridge_invoke_fenced', {'requestContractVersion': 2, 'context': context, 'request': request})
    if response.get('outcome') != 'responded' or response.get('context') != context:
        raise RuntimeError('Unconfirmed response; retain original scope/key, never retry with a new key')
    return response['response']


def wait_file(path, seconds):
    deadline = time.monotonic() + seconds
    while not path.is_file():
        if time.monotonic() > deadline:
            raise RuntimeError(f'Probe marker deadline: {path}; inspect original task, no resubmit')
        time.sleep(.2)
    return json.loads(path.read_text(encoding='utf-8-sig'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', required=True, type=Path)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--finish', type=Path, help='Observe original accepted probe/report; never submit')
    modes.add_argument('--query', type=Path, help='Print one real MCP operation page; never submit')
    args = parser.parse_args()
    if args.query:
        print(json.dumps(page(args.controller_config, json.loads(args.query.read_text())), ensure_ascii=False))
        return
    native = module('native', 'validate-workflow-native.py')
    config = json.loads(args.controller_config.read_text())
    endpoint = Path(config['stateDirectory']) / ('controller-' + config['instanceId'] + '.sock')
    info = native.call(endpoint, {'method': 'info'})['info']
    context = info['fencedRequests']['context']
    if args.finish:
        directory = args.finish
        plan = json.loads((directory / 'request.json').read_text())
        prepared = json.loads((directory / 'prepared.json').read_text())
        identifier = prepared['taskId']
        if context != plan['context'] or info['build'] != plan['provider'] or info['generation'] != plan['workerGeneration']:
            raise RuntimeError('Installed provider changed; retained evidence is historical')
        deadline = time.monotonic() + 15
        while True:
            task = fenced(args.controller_config, context, {'method': 'get_task', 'taskId': identifier})['task']
            if task['status'] not in ['running', 'queued']:
                break
            if time.monotonic() > deadline:
                raise RuntimeError('Original task not terminal; inspect it without resubmission')
            time.sleep(.2)
        if task['status'] != 'succeeded' or task['exitCode'] != 0:
            raise RuntimeError({'taskId': identifier, 'status': task['status'], 'reason': task.get('failureReason')})
        report = Path(plan['report'])
        ready = wait_file(report / 'report.ready.json', 5)
        data = (report / 'report.json').read_bytes()
        proof = json.loads(data)
        checks = ['actual-local-pipe-running-state-and-resource', 'unknown-contract-and-foreign-scope-refused',
                  'local-page-remains-readable-during-disconnect', 'new-session-same-task-process-and-journal']
        if not ready['passed'] or ready['bytes'] != len(data) or ready['sha256'] != hashlib.sha256(data).hexdigest() or not proof['passed'] or proof['checks'] != checks or proof['taskId'] != identifier or proof['processId'] != task['processId']:
            raise RuntimeError('Incomplete or mismatched actual Windows IPC evidence')
        chunks = fenced(args.controller_config, context, {'method': 'read_output', 'taskId': identifier, 'afterSequence': 0})['chunks']
        stdout = b''.join(bytes(v['bytes']) for v in chunks if v['stream'] == 'stdout').decode('utf-8')
        if 'operation page probe Привет ✓' not in stdout:
            raise RuntimeError('Actual UTF-8 output was not preserved')
        request = copy.deepcopy(prepared['request'])
        request['query']['search'] = identifier
        request['query']['cursor'] = None
        final = page(args.controller_config, request)
        if len(final['records']) != 1 or final['records'][0]['state'] != {'family': 'execution', 'status': 'succeeded'} or final['records'][0]['resources']:
            raise RuntimeError('Terminal operation/resource observation differs')
        # The same retained cursor remains usable across controller generation/session change.
        page(args.controller_config, prepared['retainedCursorRequest'])
        save(directory / 'completed.json', {'passed': True, 'taskId': identifier, 'provider': info['build'],
             'context': context, 'reportSha256': ready['sha256'], 'proof': proof, 'prepared': prepared, 'finalPage': final,
             'checks': checks + ['mcp-cli-pages', 'admission-fence', 'idempotent-parent', 'cursor-scope-filter-rejection', 'retained-cursor-after-reconnect', 'output-exit-and-terminal-resource-release']})
        print(json.dumps({'passed': True, 'gateDirectory': str(directory), 'taskId': identifier, 'reportSha256': ready['sha256']}))
        return
    if info['platform'] != 'windows' or info['activeTaskCount'] or info.get('journalRecoveryRequired') or info.get('maintenanceError'):
        raise RuntimeError('Healthy idle installed Windows worker required; no submit performed')
    supported = {(v['id'], v['version']) for v in info['contractCatalog']['supported']}
    schemas = json.loads(subprocess.check_output([str(ROOT / 'target/debug/winbridge-controller'), '--contract-schemas']))['schemas']
    expected = next(v for v in schemas if v['contract']['id'] == 'winbridge.operation-read-model')
    if ('winbridge.extensions', 1) not in supported or ('winbridge.operation-read-model', 1) not in supported or expected not in info['contractSchemas']['schemas']:
        raise RuntimeError('Installed operation contract/schema not ready')
    run = 'operations-' + time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
    directory = ROOT / '.winbridge/architecture-transition' / run
    directory.mkdir(mode=0o700)
    request = {'context': context, 'query': {'projectId': None, 'kind': None, 'search': '', 'limit': 7, 'cursor': None}}
    first = page(args.controller_config, request)
    if not first['hasMore']:
        raise RuntimeError('Existing history needs at least eight operations for this installed pagination gate')
    save(directory / 'first-page.json', first)
    report = Path(config['delivery']['root']) / config['delivery']['namespace'] / 'workflow-validation' / run
    settings = {'context': context, 'provider': info['build'], 'workerGeneration': info['generation'],
                'reportDirectory': str(PureWindowsPath(config['delivery']['peerRoot']) / config['delivery']['namespace'] / 'workflow-validation' / run)}
    header = json.dumps(settings, ensure_ascii=False).replace("'", "''")
    script = "$wbOperationSettings = '" + header + "' | ConvertFrom-Json -Depth 30\n" + (ROOT / 'scripts/verify-operation-pages.ps1').read_text()
    submit = {'method': 'submit', 'request': {'protocolVersion': 1, 'requestId': run, 'idempotencyKey': run,
              'projectId': 'snapshot-fixture', 'timeoutMs': 300000, 'command': {'kind': 'direct', 'executable': 'pwsh.exe',
              'arguments': ['-NoProfile', '-NonInteractive', '-Command', script], 'workingDirectory': '.'}}}
    save(directory / 'request.json', {**settings, 'report': str(report), 'submission': submit})
    task = fenced(args.controller_config, context, submit)['task']
    save(directory / 'task.json', {'taskId': task['taskId']})
    print(json.dumps({'gateDirectory': str(directory), 'taskId': task['taskId'], 'reportDirectory': str(report)}), flush=True)
    repeated = fenced(args.controller_config, context, submit)['task']
    if repeated['taskId'] != task['taskId']:
        raise RuntimeError('Duplicate task effect detected; stop installed worker gate')
    marker = wait_file(report / 'before.ready.json', 15)
    if marker['taskId'] != task['taskId']:
        raise RuntimeError('Wrong probe parent identity')
    retained = copy.deepcopy(request)
    retained['query']['cursor'] = first['nextCursor']
    current = copy.deepcopy(retained)
    current['query']['limit'] = 50
    records = list(first['records'])
    for _ in range(100):
        observed = page(args.controller_config, current)
        records.extend(observed['records'])
        if not observed['hasMore']:
            break
        current['query']['cursor'] = observed['nextCursor']
    else:
        raise RuntimeError('Installed history exceeds bounded 5,007-row test; no further queries')
    refs = [(r['reference']['kind'], r['reference']['operationId']) for r in records]
    if len(set(refs)) != len(refs) or any(v[1] == task['taskId'] for v in refs):
        raise RuntimeError('Cursor duplicate/new admission leaked into frozen selection')
    for kind in ['task', 'import', 'workflow', 'application']:
        query = copy.deepcopy(request)
        query['query']['kind'] = kind
        observed = page(args.controller_config, query)
        if not observed['records'] or any(r['reference']['kind'] != kind for r in observed['records']):
            raise RuntimeError('Installed history is missing expected domain ' + kind)
    own_request = copy.deepcopy(request)
    own_request['query']['search'] = task['taskId']
    own = page(args.controller_config, own_request)
    if len(own['records']) != 1 or own['records'][0]['state']['status'] != 'running':
        raise RuntimeError('New admission missing from a refreshed selection')
    save(directory / 'own-query.json', own_request)
    cli = subprocess.run([str(ROOT / 'target/debug/winbridge-controller'), '--config', str(args.controller_config),
                          'operations', str(directory / 'own-query.json')], capture_output=True, text=True, check=True, timeout=20)
    cli_page = json.loads(cli.stdout)['result']['page']
    if cli_page['records'][0]['reference'] != own['records'][0]['reference'] or cli_page['records'][0]['resources'] != own['records'][0]['resources']:
        raise RuntimeError('CLI/MCP observation mismatch')
    foreign = copy.deepcopy(request)
    foreign['context']['journalIncarnation'] = 'foreign'
    mcp(args.controller_config, 'winbridge_operations', foreign, expect_error=True)
    changed = copy.deepcopy(retained)
    changed['query']['search'] = 'changed-filter'
    mcp(args.controller_config, 'winbridge_operations', changed, expect_error=True)
    save(directory / 'prepared.json', {'taskId': task['taskId'], 'processId': marker['processId'], 'request': request,
         'retainedCursorRequest': retained, 'historyRows': len(records), 'historyKinds': sorted({v[0] for v in refs}), 'ownPage': own})
    save(directory / 'ui-info.json', {k: info[k] for k in ['instanceId', 'nodeId', 'build', 'generation', 'contractCatalog', 'fencedRequests']})
    print(json.dumps({'readyForControllerRestart': True, 'gateDirectory': str(directory), 'historyRows': len(records),
                      'processId': marker['processId'], 'taskId': task['taskId']}), flush=True)


if __name__ == '__main__':
    main()
