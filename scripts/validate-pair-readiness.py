#!/usr/bin/env python3
"""Exercise the real stdio MCP consumer and pinned Windows verification producer."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def mcp(config, name, arguments):
    messages = [
        {'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {
            'protocolVersion': '2025-11-25', 'capabilities': {},
            'clientInfo': {'name': 'paired-readiness-gate', 'version': '1'}}},
        {'jsonrpc': '2.0', 'method': 'notifications/initialized'},
        {'jsonrpc': '2.0', 'id': 2, 'method': 'tools/call',
         'params': {'name': name, 'arguments': arguments}},
    ]
    process = subprocess.run(
        [str(ROOT / 'target/debug/winbridge-controller'), '--config', str(config), 'mcp'],
        input=''.join(json.dumps(message, ensure_ascii=False) + '\n' for message in messages),
        capture_output=True, text=True, timeout=30, check=True)
    reply = next(json.loads(line) for line in process.stdout.splitlines()
                 if json.loads(line).get('id') == 2)
    if 'error' in reply:
        raise RuntimeError(reply['error']['message'])
    result = reply['result']
    value = result.get('structuredContent')
    if value is None:
        value = json.loads(next(item['text'] for item in result['content'] if item['type'] == 'text'))
    if result.get('isError'):
        raise RuntimeError(value.get('message') or value.get('result', {}).get('message'))
    return value['result']['view']


def save_new(path, value):
    data = (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'wb') as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())
    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path, required=True)
    parser.add_argument('--resume', type=Path, help='Original saved gate directory; identity/key never replaced')
    parser.add_argument('--observe-only', action='store_true', help='Read saved successful receipt after reconnect/restart; never submit')
    args = parser.parse_args()
    if args.observe_only and args.resume is None:
        parser.error('--observe-only requires --resume')
    query = {'projectId': 'fixture', 'contract': {'id': 'winbridge.worker-rpc', 'version': 2}}
    if args.resume:
        directory = args.resume
        plan = json.loads((directory / 'request.json').read_text())
    else:
        observation = mcp(args.controller_config, 'winbridge_contract_readiness', query)
        if observation['identity'] is None or not observation['readiness']['supported']:
            raise RuntimeError('Known fresh provider metadata required; no verification submitted')
        run = time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
        directory = ROOT / '.winbridge/architecture-transition' / ('paired-' + run)
        directory.mkdir(mode=0o700)
        plan = {'verificationKey': 'paired-' + run, 'identity': observation['identity']}
        save_new(directory / 'request.json', plan)
    print(json.dumps({'gateDirectory': str(directory), 'verificationKey': plan['verificationKey']}), flush=True)
    if args.observe_only:
        view = mcp(args.controller_config, 'winbridge_get_verification', {'verificationKey': plan['verificationKey']})
    else:
        view = mcp(args.controller_config, 'winbridge_verify_pair', plan)
        deadline = time.monotonic() + 250
        last = None
        while view['status'] in ['accepted', 'running']:
            current = (view['status'], view['stage'], tuple(view['taskIds']))
            if current != last:
                print(json.dumps({'status': view['status'], 'stage': view['stage'], 'taskIds': view['taskIds']}), flush=True)
                last = current
            if time.monotonic() >= deadline:
                raise RuntimeError('Observation deadline; keep saved key/identity and reconcile, no automatic resubmit')
            time.sleep(.5)
            view = mcp(args.controller_config, 'winbridge_get_verification', {'verificationKey': plan['verificationKey']})
    if view['status'] != 'succeeded':
        raise RuntimeError({'status': view['status'], 'stage': view['stage'], 'detail': view['detail'], 'taskIds': view['taskIds']})
    readiness = mcp(args.controller_config, 'winbridge_contract_readiness', {**query, 'receiptId': view['receiptId']})
    if readiness['identity'] != plan['identity'] or not all(readiness['readiness'][fact]
            for fact in ['supported', 'buildVerified', 'pairTested', 'enabled']):
        raise RuntimeError({'detail': 'Receipt cannot enable this exact live tuple', 'readiness': readiness['readiness']})
    if not args.observe_only:
        duplicate = mcp(args.controller_config, 'winbridge_verify_pair', plan)
        if duplicate['receiptId'] != view['receiptId'] or duplicate['taskIds'] != view['taskIds']:
            raise RuntimeError('Verification duplicate changed task/receipt identity')
        changed = json.loads(json.dumps(plan))
        changed['identity']['projectPolicy']['sha256'] = '0' * 64
        try:
            mcp(args.controller_config, 'winbridge_verify_pair', changed)
        except RuntimeError as error:
            if 'different frozen identity' not in str(error):
                raise
        else:
            raise RuntimeError('Existing verification key accepted a changed identity')
        unknown = mcp(args.controller_config, 'winbridge_contract_readiness', {
            'projectId': 'fixture', 'contract': {'id': 'future.unknown', 'version': 1}})
        if unknown['readiness']['supported'] or unknown['readiness']['enabled']:
            raise RuntimeError('Unknown contract acquired support/permission')
    outcome = {'passed': True, 'view': view, 'readiness': readiness,
               'checks': ['actual-mcp-producer', 'pinned-pair-receipt', 'duplicate-no-new-tasks',
                          'changed-key-identity-rejected', 'unknown-contract-no-enable']}
    if args.observe_only:
        outcome['checks'] = ['saved-receipt', 'same-tuple-fresh-route', 'no-submission']
    name = 'completed.json'
    if args.observe_only or (directory / name).exists():
        name = 'observation-' + uuid.uuid4().hex + '.json'
    save_new(directory / name, outcome)
    print(json.dumps({'passed': True, 'receiptId': view['receiptId'], 'taskIds': view['taskIds'],
                      'sessionId': readiness['sessionId'], 'readiness': readiness['readiness']}), flush=True)


if __name__ == '__main__':
    main()
