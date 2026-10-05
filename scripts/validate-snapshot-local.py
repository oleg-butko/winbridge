#!/usr/bin/env python3
"""Own a temporary controller/worker pair; exercise the same gate against real HGFS."""
import json
from pathlib import Path
import signal
import struct
import socket
import subprocess
import time
import uuid
ROOT = Path(__file__).resolve().parents[1]
run = ROOT / '.winbridge/sg' / uuid.uuid4().hex[:8]
run.mkdir(parents=True)
project = run / 'project'
project.mkdir()
with socket.socket() as probe:
    probe.bind(('127.0.0.1', 0))
    port = probe.getsockname()[1]
instance = 'snapshot-' + uuid.uuid4().hex[:8]
source = json.loads((ROOT / '.winbridge/controller-snapshot.local.json').read_text())['projects']
controller = {'schemaVersion': 1, 'instanceId': instance, 'nodeId': 'debian-test', 'stateDirectory': str(run / 'c'), 'controller': {'listenAddress': f'127.0.0.1:{port}', 'workerNodeId': 'unix-test'}, 'delivery': {'adapter': 'shared-folder', 'root': '/mnt/hgfs/shared', 'namespace': 'WinBridge'}, 'projects': source}
worker = {'schemaVersion': 1, 'instanceId': instance, 'nodeId': 'unix-test', 'stateDirectory': str(run / 'w'), 'worker': {'controllerUrl': f'ws://127.0.0.1:{port}', 'keepRunningOnUiExit': True}, 'delivery': controller['delivery'], 'projects': [{'id': 'snapshot-fixture', 'workspaceRoot': str(project), 'allowDirectCommands': True, 'maxConcurrentMutations': 1, 'recipes': []}]}
processes = []
logs = []
try:
    for role, cfg in [('controller', controller), ('worker', worker)]:
        path = run / (role + '.local.json')
        with path.open('x') as f:
            json.dump(cfg, f)
        log = (run / (role + '.log')).open('x')
        logs.append(log)
        processes.append(subprocess.Popen([str(ROOT / f'target/debug/winbridge-{role}'), '--config', str(path), 'serve'], stdout=log, stderr=log))
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        result = subprocess.run([str(ROOT / 'target/debug/winbridge-controller'), '--config', str(run / 'controller.local.json'), 'status'], capture_output=True, text=True)
        if result.returncode == 0 and json.loads(result.stdout)['status']['connected']:
            break
        if any(process.poll() is not None for process in processes):
            raise RuntimeError('A gate process exited; inspect logs in ' + str(run))
        time.sleep(.1)
    else:
        raise TimeoutError('Temporary session not connected')
    output=subprocess.check_output(['python3', str(ROOT / 'scripts/validate-snapshot.py'), '--controller-config', str(run / 'controller.local.json'), '--expected-platform', 'linux'],text=True)
    print(output,end='')
    shared_report=Path(next(line.removeprefix('Report: ') for line in output.splitlines() if line.startswith('Report: ')))
    gate=json.loads(shared_report.read_text())
    large=gate['largeReady']
    request={'protocolVersion':1,'requestId':str(uuid.uuid4()),'projectId':'snapshot-fixture','snapshotId':large['snapshotId'],'manifestSha256':large['manifestSha256'],'idempotencyKey':'owned-process-crash','timeoutMs':180000}
    request_path=run/'crash-import.json'
    with request_path.open('x') as f:json.dump(request,f)
    def worker_call(*args):
        return json.loads(subprocess.check_output([str(ROOT/'target/debug/winbridge-worker'),'--config',str(run/'worker.local.json'),*args],stderr=subprocess.DEVNULL))
    before=worker_call('info')['info']['generation']
    accepted=worker_call('import',str(request_path))['import']
    deadline=time.monotonic()+10
    while time.monotonic()<deadline:
        current=worker_call('get-import',accepted['importId'])['import']
        if current['status']=='running' and current['phase'] in ('copy_payload','extract'):break
        if current['status'] not in ('queued','running'):raise RuntimeError('Import finished before crash checkpoint')
        time.sleep(.01)
    else:raise TimeoutError('Crash checkpoint not reached')
    processes[1].kill();processes[1].wait() # Only this harness-owned worker process.
    processes.append(subprocess.Popen([str(ROOT/'target/debug/winbridge-worker'),'--config',str(run/'worker.local.json'),'serve'],stdout=logs[1],stderr=logs[1]))
    deadline=time.monotonic()+10
    while time.monotonic()<deadline:
        try:
            recovered=worker_call('get-import',accepted['importId'])['import']
            after=worker_call('info')['info']['generation']
            break
        except (subprocess.CalledProcessError,KeyError):time.sleep(.1)
    else:raise TimeoutError('Worker did not recover after owned crash')
    assert recovered['status']=='interrupted' and before!=after,recovered
    assert not (project/'snapshots'/large['snapshotId']).exists()
    assert len(worker_call('list')['tasks'])==2,'Recovery must not repeat a build'
    proof={'schemaVersion':1,'platform':'linux','passed':True,'checks':['actual process killed during import I/O','same durable import recovered interrupted','no buildable incomplete generation','new worker generation without automatic import/build retry'],'import':recovered,'beforeGeneration':before,'afterGeneration':after}
    with (run/'crash-report.json').open('x') as f:json.dump(proof,f,indent=2)
    print('Actual owned import crash/recovery passed:',run/'crash-report.json')
finally:
    for process in reversed(processes):
        if process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
    for log in logs:
        log.close()
    print(f'Owned integration logs: {run}')
