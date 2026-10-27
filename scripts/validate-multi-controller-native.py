#!/usr/bin/env python3
"""Exercise one Windows worker through four isolated Debian controller processes.

Uses a pinned source archive and the fixed firewall-approved test executable.
Never replaces the installed worker. Saved mutations are submitted once only.
"""
import argparse
import base64
import hashlib
import importlib.util
import ipaddress
import json
from pathlib import Path, PureWindowsPath
import re
import signal
import socket
import struct
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


native = module('multi_native', 'validate-workflow-native.py')
health = module('multi_archive', 'validate-update-health-actor-native.py')


def need(value, message):
    if not value:
        raise RuntimeError(message)


def save(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)


def call(endpoint, request):
    raw = json.dumps(request).encode()
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(15)
        stream.connect(str(endpoint))
        stream.sendall(struct.pack('<I', len(raw)) + raw)
        length, = struct.unpack('<I', native.exact(stream, 4))
        need(0 < length <= 1048576, 'Invalid response size; acceptance unknown')
        reply = json.loads(native.exact(stream, length))
    need(reply.get('kind') != 'error', str(reply))
    return reply


def wait_for(function, predicate, seconds=30, interval=.1):
    until = time.monotonic() + seconds
    while True:
        result = function()
        if predicate(result):
            return result
        need(time.monotonic() < until, 'Observation deadline; original tasks preserved')
        time.sleep(interval)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-archive', required=True, type=Path)
    parser.add_argument('--controller-config', required=True, type=Path)
    parser.add_argument('--controller-binary', required=True, type=Path)
    parser.add_argument('--test-runtime-directory', required=True)
    parser.add_argument('--cache-run', required=True)
    args = parser.parse_args()
    pin = health.archive_pin(args.source_archive)
    need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.cache_run), 'Explicit cache run required')
    cfg = json.loads(args.controller_config.read_text())
    production = Path(cfg['stateDirectory']) / ('controller-' + cfg['instanceId'] + '.sock')
    before = call(production, {'method': 'info'})['info']
    need(before['platform'] == 'windows' and before['activeTaskCount'] == 0
         and not before.get('maintenanceError') and not before.get('journalRecoveryRequired'), 'Healthy idle installed worker required')
    host = cfg['controller']['listenAddress'].rsplit(':', 1)[0]
    need(ipaddress.IPv4Address(host).is_private, 'Literal private controller host required')
    runtime = PureWindowsPath(args.test_runtime_directory)
    need(runtime.is_absolute() and runtime.drive.lower() == 'k:' and '..' not in runtime.parts, 'Explicit local test runtime required')
    build = json.loads(subprocess.check_output([str(args.controller_binary.resolve()), '--describe'], text=True))['build']
    need(build['sourceCommit'] == pin['sourceCommit'] and build['sourceDirty'] is False, 'Controller must be built from the same clean source commit')
    project = next(p for p in before['projects'] if p['id'] == 'snapshot-fixture')
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime()) + '-' + uuid.uuid4().hex[:8]
    local = ROOT / '.winbridge/multi-controller-native' / run
    local.mkdir(parents=True)
    shared = Path('/mnt/hgfs/shared/WinBridge/workflow-validation') / run
    shared.mkdir()
    win_shared = PureWindowsPath('K:/VMs/shared/WinBridge/workflow-validation') / run
    work = PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / ('tmp-multi-204-' + run)
    cache = PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / args.cache_run / 'test-build-cache'
    archive = PureWindowsPath('K:/VMs/shared') / args.source_archive.resolve().relative_to('/mnt/hgfs/shared')
    instance = 'multi-204-' + uuid.uuid4().hex[:12]
    peers = []
    sent = False
    terminal = False
    bootstrap = None
    transcript = (local / 'requests.jsonl').open('x')
    count = 0

    def query(index, request):
        return call(peers[index]['endpoint'], request)

    def mutate(index, request):
        nonlocal count
        count += 1
        envelope = {'requestContractVersion': 2, 'context': context, 'request': request}
        save(local / f'mutation-{count:03}.json', {'controller': index, 'envelope': envelope})
        reply = query(index, envelope)  # Never retry an unknown mutation.
        transcript.write(json.dumps({'controller': index, 'request': envelope, 'reply': reply}) + '\n'); transcript.flush()
        need(reply.get('outcome') == 'responded' and reply.get('context') == context, str(reply))
        need(reply['response'].get('kind') != 'error', str(reply))
        return reply['response']

    def diagnostics(index, method, task_id=None):
        request = {'context': context}
        if task_id is not None:
            request['query'] = {'kind': 'task', 'operationId': task_id}
        reply = query(index, {'controllerDiagnosticsVersion': 1, 'request': {'method': method, 'request': request}})
        need(reply.get('controllerDiagnosticsVersion') == 1 and reply['result']['kind'] == method, str(reply))
        page = reply['result']['observation']
        need(page['context' if method == 'controller_connections' else 'reference'] ==
             (context if method == 'controller_connections' else {'context': context, 'kind': 'task', 'operationId': task_id}), 'Diagnostic scope differs')
        return page

    def start_peer(index, restart=False):
        peer = peers[index]
        suffix = '-restart' if restart else ''
        log = (local / f'controller-{index}{suffix}.log').open('x')
        peer['process'] = subprocess.Popen([str(args.controller_binary.resolve()), '--config', str(peer['config']), 'serve'], stdout=log, stderr=subprocess.STDOUT)
        peer['log'] = log
        save(local / f'controller-{index}{suffix}-owner.json', {'pid': peer['process'].pid, 'config': str(peer['config']), 'endpoint': str(peer['endpoint']), 'url': peer['url']})
        wait_for(lambda: (peer['process'].poll(), peer['endpoint'].exists()), lambda x: x == (None, True), 10)
        query(index, {'method': 'controller_status'})

    def stop_peer(index):
        peer = peers[index]
        peer['process'].send_signal(signal.SIGINT)
        peer['process'].wait(timeout=15)
        peer['log'].close()

    try:
        for index in range(4):
            state = Path(tempfile.mkdtemp(prefix='wbmc-'))
            url = f'ws://{host}:{1145 + index}'
            config = state / 'controller.json'
            save(config, {'schemaVersion': 1, 'instanceId': instance, 'nodeId': f'multi-controller-{index}', 'stateDirectory': str(state),
                          'controller': {'listenAddress': f'{host}:{1145 + index}', 'workerNodeId': 'multi-worker'}, 'projects': []})
            peers.append({'config': config, 'endpoint': state / ('controller-' + instance + '.sock'), 'url': url})
            start_peer(index)
        metadata = {'schemaVersion': 1, 'run': run, 'source': pin, 'controllerBuild': build,
                    'controllerSha256': hashlib.sha256(args.controller_binary.read_bytes()).hexdigest(),
                    'testRuntimeDirectory': str(runtime), 'workingDirectory': str(work), 'reportDirectory': str(shared),
                    'instanceId': instance, 'controllers': [p['url'] for p in peers]}
        save(local / 'run-info.json', metadata)
        worker_config = {'schemaVersion': 1, 'instanceId': instance, 'nodeId': 'multi-worker', 'stateDirectory': str(work / 'state'),
                         'worker': {'keepRunningOnUiExit': True, 'controllerTopology': {'schemaVersion': 1, 'primary': 'c3',
                             'controllers': [{'id': f'c{i}', 'url': p['url']} for i, p in enumerate(peers)]}},
                         'workScheduling': {'maxConcurrentWork': 4},
                         'projects': [{'id': f'p{i}', 'workspaceRoot': str(work / f'p{i}'), 'allowDirectCommands': True, 'maxConcurrentMutations': 1, 'recipes': []} for i in range(4)]}
        save(shared / 'worker.json', worker_config)
        script = f'''$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'
$work={native.ps_string(str(work))};$archive={native.ps_string(str(archive))};$cache={native.ps_string(str(cache))};$shared={native.ps_string(str(win_shared))}
if(Test-Path -LiteralPath $work){{throw 'Existing experiment preserved'}}
if((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne {native.ps_string(pin['archiveSha256'])}){{throw 'Source ZIP changed'}}
if([IO.DriveInfo]::new('K:\\').AvailableFreeSpace -lt 6GB){{throw 'Less than 6 GiB; no build'}}
New-Item -ItemType Directory -Path $work|Out-Null
Expand-Archive -LiteralPath $archive -DestinationPath $work
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $work 'winbridge/scripts/test-multi-controller-native.ps1') -WorkDirectory $work -CacheDirectory $cache -SharedReportDirectory $shared -TestRuntimeDirectory {native.ps_string(str(runtime))} -SourceCommit {native.ps_string(pin['sourceCommit'])} -ManifestSha256 {native.ps_string(pin['manifestSha256'])}
exit $LASTEXITCODE
'''
        request = {'method': 'submit', 'request': {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'snapshot-fixture',
            'idempotencyKey': 'native-multi-' + run, 'timeoutMs': 1800000,
            'command': {'kind': 'direct', 'executable': 'pwsh.exe', 'arguments': ['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()], 'workingDirectory': '.'}}}
        envelope = {'requestContractVersion': 2, 'context': before['fencedRequests']['context'], 'request': request}
        save(local / 'bootstrap-request.json', envelope)
        save(local / 'info-before.json', before)
        sent = True
        reply = call(production, envelope)
        if reply.get('outcome') == 'rejected_before_dispatch':
            terminal = True
        need(reply.get('outcome') == 'responded' and reply['response'].get('kind') == 'task', 'Bootstrap acceptance unknown; do not replay')
        bootstrap = reply['response']['task']
        save(local / 'bootstrap-task.json', bootstrap)
        print(json.dumps({'taskId': bootstrap['taskId'], 'runInfo': str(local / 'run-info.json')}), flush=True)

        def ready():
            task = call(production, {'method': 'get_task', 'taskId': bootstrap['taskId']})['task']
            need(task['status'] in ('queued', 'running'), 'Bootstrap stopped before network ready; inspect original task/report')
            return (shared / 'network-ready.json').exists()

        wait_for(ready, bool, 900, interval=1)
        ready_data = json.loads((shared / 'network-ready.json').read_text(encoding='utf-8-sig'))
        context = ready_data['context']
        wait_for(lambda: [query(i, {'method': 'controller_status'})['status'] for i in range(4)],
                 lambda xs: all(x['connected'] is True for x in xs), 30)
        infos = [query(i, {'method': 'info'})['info'] for i in range(4)]
        need(all(x['fencedRequests']['context'] == context and x['generation'] == infos[0]['generation'] for x in infos), 'Different worker authority/generation')
        need(all(x['build']['sourceCommit'] == pin['sourceCommit'] and x['build']['sourceManifestSha256'] == pin['manifestSha256'] and x['build']['sourceDirty'] is False for x in infos), 'Worker build differs from source')
        for i, info in enumerate(infos):
            need(info['controllerConnection']['url'] == peers[i]['url'], 'Info reports another caller connection')
            page = diagnostics(i, 'controller_connections')
            need(page['primary'] == 'c3' and len(page['connections']) == 4 and page['rpc']['limit'] == 32
                 and all(c['rpc']['limit'] == 16 and c['connection']['state'] == 'connected' for c in page['connections']), 'Incomplete topology/budget')
        save(local / 'connected-infos.json', infos)
        tasks = []
        for i in range(4):
            submission = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': f'p{i}', 'idempotencyKey': f'multi-long-{i}', 'timeoutMs': 240000,
                          'command': {'kind': 'direct', 'executable': str(work / f'p{i}' / 'winbridge-fixture.exe'), 'arguments': ['tree'], 'workingDirectory': '.'}}
            tasks.append(mutate(i, {'method': 'submit', 'request': submission})['task'])
        task = lambda i: query(3, {'method': 'get_task', 'taskId': tasks[i]['taskId']})['task']
        parallel = wait_for(lambda: [task(i) for i in range(4)], lambda xs: all(x['status'] == 'running' for x in xs), 20)
        creators = [diagnostics(3, 'operation_attribution', t['taskId'])['creator'] for t in tasks]
        need(all(c['controllerId'] == f'c{i}' and c['kind'] == 'controller' for i, c in enumerate(creators)), 'Creator attribution differs')
        duplicates = []
        for i in range(4):
            duplicate = mutate(i, {'method': 'submit', 'request': tasks[0]['submission']})['task']
            need(duplicate['taskId'] == tasks[0]['taskId'], 'Cross-controller duplicate created a new task')
            duplicates.append(duplicate)
        queued_submission = dict(tasks[0]['submission'], requestId=str(uuid.uuid4()), idempotencyKey='multi-next',
                                 command=dict(tasks[0]['submission']['command'], arguments=['success']))
        queued = mutate(3, {'method': 'submit', 'request': queued_submission})['task']
        need(queued['status'] == 'queued', 'Same project did not serialize')
        before_disconnect = diagnostics(3, 'controller_connections')
        stop_peer(2)
        lost = wait_for(lambda: diagnostics(3, 'controller_connections'),
                        lambda p: next(x for x in p['connections'] if x['id'] == 'c2')['connection']['state'] != 'connected', 20)
        need(all(x['connection']['state'] == 'connected' for x in lost['connections'] if x['id'] != 'c2'), 'Disconnect affected another link')
        disconnected_task = task(2)
        need(disconnected_task['status'] == 'running', 'Disconnect cancelled an accepted task')
        start_peer(2, restart=True)
        reconnected = wait_for(lambda: diagnostics(3, 'controller_connections'),
                              lambda p: all(x['connection']['state'] == 'connected' for x in p['connections']), 30)
        old = next(x for x in before_disconnect['connections'] if x['id'] == 'c2')
        new = next(x for x in reconnected['connections'] if x['id'] == 'c2')
        need(old['connection']['controllerGeneration'] != new['connection']['controllerGeneration'], 'Controller restart did not change generation')
        creator_after_restart = diagnostics(2, 'operation_attribution', tasks[2]['taskId'])['creator']
        need(creator_after_restart == creators[2], 'Restart rewrote historical creator')
        cancellations = []
        for i in range(4):
            actor = (i + 1) % 4
            mutate(actor, {'method': 'cancel', 'taskId': tasks[i]['taskId']})
            intent = diagnostics(3, 'operation_cancellation', tasks[i]['taskId'])['intent']
            need(intent['actor']['controllerId'] == f'c{actor}', 'Cancellation actor differs')
            cancellations.append(intent)
        wait_for(lambda: [task(i) for i in range(4)], lambda xs: all(x['status'] == 'cancelled' for x in xs), 25)
        finished = wait_for(lambda: query(3, {'method': 'get_task', 'taskId': queued['taskId']})['task'], lambda x: x['status'] not in ('queued', 'running'), 25)
        need(finished['status'] == 'succeeded', 'Serialized queue did not advance')
        final_tasks = query(3, {'method': 'list_tasks'})['tasks']
        need(len(final_tasks) == 5, 'Duplicate or missing task history')
        output = query(0, {'method': 'read_output', 'taskId': queued['taskId'], 'afterSequence': 0})
        need('Привет' in ''.join(bytes(c['bytes']).decode('utf-8', errors='replace') for c in output['chunks']), 'Output lost or unreadable from another controller')
        evidence = {'schemaVersion': 1, 'kind': 'multi_controller_native', 'passed': True, 'platform': 'windows', 'context': context,
                    'generation': infos[0]['generation'], 'build': infos[0]['build'], 'source': pin, 'controllerBuild': build,
                    'controllers': 4, 'primary': 'c3', 'sameRuntime': True, 'parallelProjects': True, 'sameProjectSerialized': True,
                    'crossControllerDedup': True, 'disconnectPreservedTask': True, 'controllerRestartPreservedCreator': True,
                    'crossControllerCancellation': True, 'sharedOutput': True, 'tasks': tasks, 'creators': creators,
                    'parallel': parallel, 'duplicates': duplicates, 'cancellations': cancellations,
                    'disconnectedTask': disconnected_task, 'creatorAfterRestart': creator_after_restart,
                    'queued': finished, 'beforeDisconnect': before_disconnect, 'lost': lost, 'reconnected': reconnected,
                    'finalTasks': final_tasks, 'output': output, 'installedRuntimeChanged': False, 'windowsOpened': False,
                    'scope': 'four isolated Debian controller processes on .204; no claim of simultaneous .203+.204 VM execution'}
        save(local / 'network-evidence.json', evidence)
        save(shared / 'network-complete.json', evidence)
        bootstrap = wait_for(lambda: call(production, {'method': 'get_task', 'taskId': bootstrap['taskId']})['task'], lambda t: t['status'] not in ('queued', 'running'), 90)
        terminal = True
        save(local / 'terminal.json', bootstrap)
        need(bootstrap['status'] == 'succeeded' and bootstrap['exitCode'] == 0, 'Windows bootstrap failed; inspect retained report')
        after = call(production, {'method': 'info'})['info']
        save(local / 'info-after.json', after)
        need(after['generation'] == before['generation'] and after['build'] == before['build'] and after['fencedRequests']['context'] == before['fencedRequests']['context'] and after['activeTaskCount'] == 0, 'Installed worker changed')
        files = module('multi_files', 'summarize-update-timings.py').verified_files(shared)
        report = json.loads(files['report.json'].decode('utf-8-sig'))
        need(report['passed'] is True and report['isolatedWorkerExitConfirmed'] is True and report['remainingFixtureProcesses'] == 0,
             'Windows cleanup/counter checks incomplete')
        audit = module('multi_evidence', 'multi-controller-native-proof.py').audit(files, pin, build, str(runtime))
        audit.update(module('multi_migration', 'update-activation-service-proof.py').verify_worker_migration(
            json.loads(files['worker-journal-attribution-migration-proof.json'].decode('utf-8-sig')),
            '536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712', pin))
        need(module('multi_files_again', 'summarize-update-timings.py').verified_files(shared) == files, 'Report changed on second read')
        save(local / 'verified.json', {'passed': True, 'audit': audit, 'filesVerifiedTwice': len(files), 'reportSha256': hashlib.sha256(files['report.json']).hexdigest(), 'installedUnchanged': True})
        print(json.dumps({'passed': True, 'runInfo': str(local / 'run-info.json'), 'filesVerifiedTwice': len(files)}), flush=True)
    finally:
        if sent and not terminal and bootstrap is not None:
            # Request observation only. Never recreate an accepted operation.
            observed = call(production, {'method': 'get_task', 'taskId': bootstrap['taskId']})['task']
            terminal = observed['status'] not in ('queued', 'running')
            save(local / 'last-bootstrap-observation.json', observed)
        if terminal or not sent:
            for i in range(len(peers)):
                if peers[i].get('process') is not None and peers[i]['process'].poll() is None:
                    stop_peer(i)
        else:
            print('Bootstrap may continue; saved controller owners retained; do not replay', flush=True)
        transcript.close()


if __name__ == '__main__':
    main()
