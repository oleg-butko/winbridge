#!/usr/bin/env python3
"""Read-only Windows validation inventory through the existing native controller."""
import argparse
import base64
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('native', ROOT / 'scripts/validate-workflow-native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def endpoint(config):
    cfg = json.loads(config.read_text())
    return Path(cfg['stateDirectory']) / ('controller-' + cfg['instanceId'] + '.sock')


def observe(path):
    meta = json.loads(path.read_text())
    if meta['taskId'] is None:
        raise ValueError('Unknown acceptance: reconcile saved request; never resubmit blindly')
    ep = endpoint(Path(meta['controllerConfig']))
    while True:
        task = native.call(ep, dict(method='get_task', taskId=meta['taskId']))['task']
        if task['submission'] != meta['request']:
            raise ValueError('Task does not match saved request')
        if task['status'] not in {'queued', 'running'}:
            break
        time.sleep(1)
    path.with_name('terminal.json').write_text(json.dumps(task, indent=2))
    chunks, sequence = [], 0
    while True:
        page = native.call(ep, dict(method='read_output', taskId=meta['taskId'], afterSequence=sequence))['chunks']
        if not page:
            break
        chunks.extend(page)
        sequence = page[-1]['sequence']
    for stream in ('stdout', 'stderr'):
        output = b''.join(bytes(c['bytes']) for c in chunks if c['stream'] == stream).decode('utf-8-sig', errors='replace')
        path.with_name(stream + '.log').write_text(output)
    if task['status'] != 'succeeded' or task['exitCode'] != 0:
        raise RuntimeError('Inventory failed; inspect saved terminal and stderr: ' + str(path))
    report = json.loads(path.with_name('stdout.log').read_text())
    path.with_name('inventory.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(dict(report=str(path.with_name('inventory.json')), logicalGiB=report['logicalBytes'] / 1024**3,
        freeGiB=report['freeBytes'] / 1024**3,
        largest=[dict(name=r['name'], GiB=round(r['bytes'] / 1024**3, 3), errors=r['errors'], skippedLinks=r['skippedLinks'],
            groups=[dict(name=g['name'], role=g['role'], GiB=round(g['measurement']['bytes'] / 1024**3, 3)) for g in r['groups'][:4]])
            for r in report['runs'][:12]]), indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path)
    parser.add_argument('--observe', type=Path)
    args = parser.parse_args()
    if args.observe:
        observe(args.observe)
        return
    if not args.controller_config:
        parser.error('Supply --controller-config or --observe')
    ep = endpoint(args.controller_config)
    info = native.call(ep, dict(method='info'))['info']
    if info['platform'] != 'windows' or info['activeTaskCount'] or info.get('maintenanceError') or info.get('journalRecoveryRequired'):
        raise ValueError('Requires idle healthy Windows worker')
    project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
    root = str(PureWindowsPath(project['workspaceRoot']) / 'workflow-validation')
    code = (ROOT / 'scripts/inspect-windows-validation-storage.ps1').read_bytes()
    script = ("$code=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('"
        + base64.b64encode(code).decode() + "')); & ([scriptblock]::Create($code)) -Root " + native.ps_string(root))
    request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId=project['id'],
        idempotencyKey='storage-inventory-' + str(uuid.uuid4()), timeoutMs=600000,
        command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
            arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()]))
    if len(request['command']['arguments'][-1]) > 30000:
        raise ValueError('Inventory command exceeds bounded argv')
    directory = ROOT / '.winbridge/storage-inventory' / (time.strftime('%Y%m%d-%H%M%S', time.gmtime()) + '-' + uuid.uuid4().hex[:8])
    directory.mkdir(parents=True)
    path = directory / 'submission.json'
    meta = dict(controllerConfig=str(args.controller_config.resolve()), taskId=None, request=request,
                scriptSha256=hashlib.sha256(code).hexdigest())
    path.write_text(json.dumps(meta, indent=2))
    task = native.call(ep, dict(method='submit', request=request))['task']
    meta['taskId'] = task['taskId']
    path.write_text(json.dumps(meta, indent=2))
    print(json.dumps(dict(taskId=task['taskId'], metadata=str(path))), flush=True)
    observe(path)


if __name__ == '__main__':
    main()
