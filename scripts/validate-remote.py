#!/usr/bin/env python3
"""Native Windows session gate; mutates only the explicitly registered fixture project."""
import argparse
import json
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--config', type=Path, required=True)
parser.add_argument('--managed-controller', action='store_true', help='Start/stop only a controller subprocess owned by this run to check reconnect')
parser.add_argument('--allow-unix-fixture', action='store_true', help='Development check only; never record it as Windows validation')
args = parser.parse_args()
config_path = args.config.resolve()
config = json.loads(config_path.read_text())
CONTROLLER = ROOT / 'target/debug/winbridge-controller'
RUN_ID = time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
LOCAL = ROOT / '.winbridge' / 'remote-validation' / RUN_ID
LOCAL.mkdir(parents=True)
endpoint = Path(config['stateDirectory']) / ('controller-' + config['instanceId'] + '.sock')
controller = watch = None
checks = []
platform = 'unknown'
active_tasks = []
report = {'schemaVersion': 1, 'passed': False, 'platform': platform, 'runId': RUN_ID, 'checks': checks, 'error': None}


def cli(*operations, error=False):
    result = subprocess.run([str(CONTROLLER), '--config', str(config_path), *map(str, operations)], capture_output=True, timeout=15)
    if error:
        assert result.returncode != 0, result.stdout
        return result.stderr.decode('utf-8')
    assert result.returncode == 0, result.stderr.decode('utf-8')
    return json.loads(result.stdout)


def until(check, explanation, seconds=25):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = check()
            if value:
                return value
        except (AssertionError, OSError, ValueError):
            pass
        time.sleep(.1)
    raise AssertionError(explanation)


def start_controller():
    log = open(LOCAL / ('controller-' + uuid.uuid4().hex[:6] + '.log'), 'wb')
    process = subprocess.Popen([str(CONTROLLER), '--config', str(config_path), 'serve'], stdout=log, stderr=log)
    log.close()
    until(lambda: process.poll() is None and b'WinBridge controller ready:' in Path(log.name).read_bytes() and cli('status'), 'Owned controller did not start; stop the previous listener first')
    return process


def stop(process):
    if process is None or process.poll() is not None:
        return
    process.send_signal(signal.SIGINT)
    try:
        process.wait(10)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
        raise AssertionError('Owned subprocess did not stop before deadline')


def get(task_id):
    return cli('get', task_id)['task']


def finish(task_id, expected):
    def check():
        task = get(task_id)
        return task if task['status'] not in ['queued', 'running'] else False
    task = until(check, 'Task did not reach a terminal state')
    assert task['status'] == expected, task
    if task_id in active_tasks:
        active_tasks.remove(task_id)
    return task


def request(mode, timeout=10000):
    return {'protocolVersion': info['protocolVersion'], 'requestId': str(uuid.uuid4()), 'idempotencyKey': 'session-gate-' + uuid.uuid4().hex,
            'projectId': 'fixture', 'timeoutMs': timeout,
            'command': {'kind': 'direct', 'executable': executable, 'arguments': [mode], 'workingDirectory': '.'}}


def submit(value, lose_ack=False):
    path = LOCAL / (value['idempotencyKey'] + '.request.json')
    path.write_text(json.dumps(value, ensure_ascii=False), encoding='utf-8')
    if lose_ack:
        payload = json.dumps({'method': 'submit', 'request': value}).encode()
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(10)
            client.connect(str(endpoint))
            client.sendall(struct.pack('<I', len(payload)) + payload)
        return None
    task = cli('submit', path)['task']
    if task['status'] in ['queued', 'running'] and task['taskId'] not in active_tasks:
        active_tasks.append(task['taskId'])
    return task


def save_task(task, name):
    chunks = []
    after = 0
    while True:
        page = cli('output', task['taskId'], after)['chunks']
        if not page:
            break
        chunks.extend(page)
        after = page[-1]['sequence']
    assert sum(len(c['bytes']) for c in chunks) == task['outputBytes'], task
    value = {'schemaVersion': 1, 'platform': platform, 'task': task, 'output': chunks, 'outputComplete': True}
    (LOCAL / (name + '.task-report.json')).write_text(json.dumps(value, ensure_ascii=False), encoding='utf-8')
    return {stream: b''.join(bytes(c['bytes']) for c in chunks if c['stream'] == stream).decode('utf-8') for stream in ['stdout', 'stderr']}


try:
    if args.managed_controller:
        controller = start_controller()
    until(lambda: cli('status')['status']['connected'], 'Windows worker did not connect to the configured local pair')
    info = cli('info')['info']
    platform = info['platform']
    report['platform'] = platform
    assert platform == 'windows' or args.allow_unix_fixture, 'Native gate requires info.platform windows; Unix results are development evidence only'
    assert info['instanceId'] == config['instanceId']
    assert info['nodeId'] == config['controller']['workerNodeId']
    assert [p['id'] for p in info['projects']] == ['fixture'], 'Gate requires the isolated fixture registry created by prepare-windows-session.ps1'
    project = info['projects'][0]
    recipe = next(r for r in project['recipes'] if r['id'] == 'fixture-success')
    executable = recipe['command']['executable']
    assert executable.replace('\\', '/').split('/')[-1] in ['winbridge-fixture.exe', 'winbridge-fixture'], 'Gate refuses an executable other than the dedicated fixture'
    assert recipe['command']['arguments'] == ['success'] and project['allowDirectCommands']
    (LOCAL / 'worker-info.json').write_text(json.dumps(info, ensure_ascii=False), encoding='utf-8')
    checks.append('actual-worker-platform-pair-and-fixture-registry')

    value = request('success')
    submit(value, lose_ack=True)
    accepted = submit(value)
    assert submit(value)['taskId'] == accepted['taskId']
    # A terminal replay is still the original task and does not launch another process.
    task = finish(accepted['taskId'], 'succeeded')
    assert submit(value)['taskId'] == task['taskId']
    decoded = save_task(task, 'success')
    assert task['exitCode'] == 0 and 'Привет ✓' in decoded['stdout'] and 'diagnostic' in decoded['stderr']
    checks.append('remote-success-lost-client-ack-dedup-unicode-both-streams')

    value = request('arguments')
    expected = ['hello with spaces', '& literal | $', 'quote"', 'trailing\\', 'Привет ✓']
    value['command']['arguments'].extend(expected)
    task = finish(submit(value)['taskId'], 'succeeded')
    decoded = save_task(task, 'arguments')
    assert json.loads(decoded['stdout'].splitlines()[1]) == expected, decoded
    checks.append('remote-literal-argv')

    task = finish(submit(request('failure'))['taskId'], 'failed')
    assert task['exitCode'] == 7
    save_task(task, 'failure')
    value = request('success')
    value['command'] = {'kind': 'recipe', 'recipeId': 'fixture-success', 'arguments': []}
    task = finish(submit(value)['taskId'], 'succeeded')
    save_task(task, 'recipe')
    checks.append('remote-failure-exit-and-recipe')

    task = finish(submit(request('tree', 300))['taskId'], 'failed')
    assert 'timeout' in task['reason'].lower()
    save_task(task, 'timeout')
    tree = submit(request('tree', 60000))
    until(lambda: get(tree['taskId'])['processId'], 'Tree did not start')
    cli('cancel', tree['taskId'])
    save_task(finish(tree['taskId'], 'cancelled'), 'cancelled')
    checks.append('remote-timeout-and-managed-tree-cancel')

    if args.managed_controller:
        long = submit(request('long', 60000))
        pid = until(lambda: get(long['taskId'])['processId'], 'Long fixture did not start')
        previous = cli('status')['status']
        watch_log = open(LOCAL / 'watch.jsonl', 'wb')
        watch = subprocess.Popen([str(CONTROLLER), '--config', str(config_path), 'watch', long['taskId']], stdout=watch_log, stderr=watch_log)
        watch_log.close()
        until(lambda: (LOCAL / 'watch.jsonl').stat().st_size, 'Watch did not receive journal events')
        stop(controller)
        controller = None
        time.sleep(2)
        controller = start_controller()
        until(lambda: cli('status')['status']['connected'], 'Worker did not reconnect')
        current = cli('status')['status']
        task = get(long['taskId'])
        assert task['status'] == 'running' and task['processId'] == pid
        assert current['generation'] != previous['generation'] and current['sessionId'] != previous['sessionId']
        assert current['workerGeneration'] == previous['workerGeneration']
        assert task['taskId'] == long['taskId']
        cli('cancel', task['taskId'])
        save_task(finish(task['taskId'], 'cancelled'), 'reconnect')
        watch.wait(15)
        assert watch.returncode == 0
        updates = [json.loads(line) for line in (LOCAL / 'watch.jsonl').read_text().splitlines()]
        assert updates[-1]['outputComplete']
        for field in ['events', 'output']:
            sequences = [item['sequence'] for update in updates if update['kind'] == 'task_update' for item in update[field]]
            assert sequences == sorted(set(sequences)), field
        checks.append('owned-controller-restart-same-worker-generation-task-pid-watch-replay')
    report['passed'] = True
    report['reconnectVerified'] = args.managed_controller
except Exception as error:
    report['error'] = str(error)
    raise
finally:
    # Only fixture task IDs accepted by this run can be cancelled during cleanup.
    for task_id in active_tasks:
        try:
            cli('cancel', task_id)
        except Exception:
            pass
    for process in [watch, controller]:
        try:
            stop(process)
        except Exception as error:
            report.setdefault('cleanupErrors', []).append(str(error))
    (LOCAL / 'validation-report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    if platform == 'windows':
        shared = Path('/mnt/hgfs/shared/WinBridge/session-validation') / ('debian-' + RUN_ID)
        try:
            shared.mkdir(parents=True, exist_ok=False)
            for file in LOCAL.iterdir():
                if file.suffix == '.json' or file.name == 'watch.jsonl':
                    target = shared / file.name
                    with target.open('xb') as output:
                        output.write(file.read_bytes())
            print('Shared reports: ' + str(shared))
        except OSError as error:
            print('Shared publication failed: ' + str(error))
    print(json.dumps({'reportRoot': str(LOCAL), **report}, ensure_ascii=False, indent=2))
