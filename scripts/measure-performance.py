#!/usr/bin/env python3
"""Measure real commit/ZIP/HGFS/Windows import/build without updating the worker.

Run from Debian. Requires the already approved Windows worker; desktop is not
needed and no test windows open. New fixture snapshots/caches/history are retained.
An unexpected RPC/worker failure stops the run; mutations are never replayed.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PureWindowsPath
import shutil
import statistics
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
# Reuse the approved bounded IPC implementation despite its executable's
# hyphenated filename. Importing it does not invoke its guarded CLI main.
_spec = importlib.util.spec_from_file_location('native_rpc', ROOT / 'scripts/validate-workflow-native.py')
_rpc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_rpc)
call = _rpc.call
POLL_SECONDS = 0.15


def write_new(path, value):
    data = json.dumps(value, ensure_ascii=False, indent=2).encode()
    with path.open('xb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    if path.read_bytes() != data:
        raise RuntimeError(f'Report readback mismatch: {path}')
    return {'path': path.name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


def elapsed(start):
    return (time.perf_counter() - start) * 1000


def stats(values):
    if not values:
        return {'count': 0, 'medianMs': None, 'p95Ms': None, 'maxMs': None}
    values = sorted(values)
    # Nearest-rank p95; raw samples are retained, not inferred from a small set.
    return {'count': len(values), 'medianMs': statistics.median(values),
            'p95Ms': values[max(0, (95 * len(values) + 99) // 100 - 1)], 'maxMs': max(values)}


class Measurement:
    def __init__(self, args):
        self.args = args
        self.config = json.loads(args.controller_config.read_text())
        self.endpoint = Path(self.config['stateDirectory']) / f"controller-{self.config['instanceId']}.sock"
        self.worker = call(self.endpoint, {'method': 'info'})['info']
        if self.worker['platform'] != 'windows' or not self.worker.get('snapshotImportAvailable'):
            raise RuntimeError('A connected Windows worker with snapshot import is required')
        if self.worker.get('journalRecoveryRequired') or self.worker.get('maintenanceError'):
            raise RuntimeError('Worker reports a journal/maintenance problem; stop and reconcile')
        if self.worker.get('activeTaskCount') != 0:
            raise RuntimeError('Start baseline with no active tasks; do not interrupt user work')
        self.project = next(p for p in self.worker['projects'] if p['id'] == args.project and p['allowDirectCommands'])
        if any(r['status'] in ('queued', 'running') and r['submission']['projectId'] == args.project
               for r in call(self.endpoint, {'method': 'list_imports'})['imports']):
            raise RuntimeError('Fixture import is already active; wait without cancelling it')
        if any(r['status'] in ('starting', 'ready', 'stopping', 'indeterminate') and r['projectId'] == args.project
               for r in call(self.endpoint, {'method': 'list_applications'})['applications']):
            raise RuntimeError('Fixture application is active/unknown; baseline must not disturb it')
        self.run_id = time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()) + '-' + uuid.uuid4().hex[:8]
        self.local = ROOT / '.winbridge/performance' / self.run_id
        self.local.mkdir(parents=True)
        self.shared = Path(self.config['delivery']['root']) / self.config['delivery']['namespace'] / 'performance' / self.run_id
        self.shared.mkdir(parents=True)  # New owned namespace; no existing data changed.
        self.records = {'schemaVersion': 1, 'runId': self.run_id, 'passed': False, 'error': None,
                        'worker': self.identity(self.worker), 'parameters': vars(args) | {'controller_config': str(args.controller_config), 'controller': str(args.controller)},
                        'debianEnvironment': {'uname': list(os.uname()), 'cpuCount': os.cpu_count()},
                        'pollSeconds': POLL_SECONDS, 'cases': [], 'operations': [], 'controlLatency': [],
                        'limitations': ['OS/HGFS caches are not flushed; first build uses a new Cargo target directory.',
                                        'Tree is a verified I/O microbenchmark, not a production delivery adapter.',
                                        'Completion wall time includes RPC and polling; host clocks are never subtracted.',
                                        'Journal/WAL deltas include metadata and background activity; no VACUUM or cleanup.']}

    @staticmethod
    def identity(info):
        # Preserve build/health evidence without copying unrelated recipe argv.
        return {key: value for key, value in info.items() if key != 'projects'}

    def log(self, text):
        print(text, flush=True)

    def rpc(self, request):
        return call(self.endpoint, request)

    def probe(self, phase):
        start = time.perf_counter()
        current = self.rpc({'method': 'info'})['info']
        latency = elapsed(start)
        if current['generation'] != self.worker['generation'] or current.get('journalRecoveryRequired') or current.get('maintenanceError'):
            raise RuntimeError('Worker generation/health changed unexpectedly; stop the measurement')
        self.records['controlLatency'].append({'phase': phase, 'ms': latency})

    def submit(self, kind, payload, label):
        # Retain exact request/key before crossing the execution boundary. An ACK
        # failure is not automatically retried, even if the key would permit it.
        request = {'method': 'import_snapshot' if kind == 'import' else 'submit', 'request': payload}
        write_new(self.local / f'{label}.request.json', request)
        operation = {'label': label, 'request': request, 'record': None}
        self.records['operations'].append(operation)
        start = time.perf_counter()
        accepted = self.rpc(request)[kind]
        operation['record'] = accepted
        operation['ackMs'] = elapsed(start)
        return self.wait(kind, accepted, label, start, operation)

    def wait(self, kind, accepted, label, start, operation):
        identity = accepted['importId' if kind == 'import' else 'taskId']
        deadline = time.monotonic() + 240
        while time.monotonic() < deadline:
            self.probe(label)
            result = self.rpc({'method': 'get_import' if kind == 'import' else 'get_task',
                               'importId' if kind == 'import' else 'taskId': identity})[kind]
            operation['record'] = result
            if result['status'] not in ('queued', 'running'):
                operation['observedWallMs'] = elapsed(start)
                if result['status'] != 'succeeded' or (kind == 'task' and result['exitCode'] != 0):
                    raise RuntimeError(f"Unexpected {label}: {result['status']} {result.get('reason')}; retained {identity}")
                duration = result['finishedAtMs'] - result['acceptedAtMs']
                if duration < 0:
                    raise RuntimeError('Worker wall clock moved backwards; durations cannot be compared')
                operation['workerTotalMs'] = duration
                if kind == 'task':
                    operation['workerQueueMs'] = result['startedAtMs'] - result['acceptedAtMs']
                    operation['workerExecutionMs'] = result['finishedAtMs'] - result['startedAtMs']
                self.log(f'{label}: succeeded, worker {duration} ms, observed {operation["observedWallMs"]:.1f} ms')
                return operation
            time.sleep(POLL_SECONDS)
        raise TimeoutError(f'{label} {identity}: deadline exceeded; retained for reconciliation')

    def output(self, operation):
        identity = operation['record']['taskId']
        streams = {'stdout': bytearray(), 'stderr': bytearray()}
        after = 0
        chunks = 0
        start = time.perf_counter()
        while True:
            page = self.rpc({'method': 'read_output', 'taskId': identity, 'afterSequence': after})['chunks']
            if not page:
                break
            for chunk in page:
                if chunk['sequence'] <= after or chunk['offset'] != len(streams[chunk['stream']]):
                    raise RuntimeError('Output cursor/offset gap; stop instead of reporting complete output')
                streams[chunk['stream']].extend(chunk['bytes'])
                after = chunk['sequence']
                chunks += 1
                if sum(map(len, streams.values())) > 4 * 1024 * 1024:
                    raise RuntimeError('Benchmark output exceeded the 4 MiB read bound')
        if operation['record']['outputTruncated'] or sum(map(len, streams.values())) != operation['record']['outputBytes']:
            raise RuntimeError('Output is incomplete/truncated; baseline not valid')
        operation['readOutputMs'] = elapsed(start)
        operation['outputChunks'] = chunks
        operation['streams'] = {name: {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()} for name, data in streams.items()}
        return streams

    def task(self, executable, arguments, snapshot, label):
        return self.submit('task', {'protocolVersion': 1, 'requestId': str(uuid.uuid4()),
                                   'projectId': self.args.project, 'idempotencyKey': str(uuid.uuid4()),
                                   'snapshotId': snapshot, 'timeoutMs': 180000,
                                   'command': {'kind': 'direct', 'executable': executable, 'arguments': arguments, 'workingDirectory': '.'}}, label)

    def windows_path(self, path):
        relative = path.resolve().relative_to(Path(self.config['delivery']['root']).resolve())
        return str(PureWindowsPath(self.config['delivery']['peerRoot']) / str(relative).replace('/', '\\'))

    def make_source(self, case, repeat):
        name = f'{case}-{repeat}'
        source = self.local / name
        shutil.copytree(ROOT / 'fixtures/snapshot-project', source)
        shutil.copyfile(ROOT / 'scripts/measure-hgfs-tree.ps1', source / 'measure-hgfs-tree.ps1')
        (source / 'measurement-id.txt').write_text(f'{self.run_id}/{name}\n')
        main = source / 'src/main.rs'
        prefix = '''
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode == "flood" {
        let bytes = [b'x';8192];
        for _ in 0..128 { std::io::Write::write_all(&mut std::io::stdout(),&bytes).unwrap(); std::io::Write::write_all(&mut std::io::stderr(),&bytes).unwrap(); }
        return;
    }
    if mode == "tree" {
        let mut child=std::process::Command::new(std::env::current_exe().unwrap()).arg("long").spawn().unwrap();
        println!("measurement child pid={}",child.id());std::io::Write::flush(&mut std::io::stdout()).unwrap();
        let _=child.wait();return;
    }
    if mode == "long" { loop { println!("measurement heartbeat");std::io::Write::flush(&mut std::io::stdout()).unwrap();std::thread::sleep(std::time::Duration::from_millis(100)); } }
'''
        main.write_text(main.read_text().replace('fn main() {', 'fn main() {\n' + prefix, 1))
        data = source / 'padding'
        data.mkdir()
        if case == 'many-files':
            for i in range(self.args.small_files):
                # Deterministic bytes; archive format currently stores, not compresses.
                (data / f'{i:05}.dat').write_bytes(hashlib.sha256(f'file:{i}'.encode()).digest() * 128)
        elif case == 'large-blob':
            block = hashlib.sha256(b'large-blob-baseline-v1').digest() * 32768
            with (data / 'blob.dat').open('xb') as stream:
                for _ in range(self.args.blob_mib):
                    stream.write(block)
        self.git(source, 'init', '-q')
        self.commit(source, name)
        return source

    @staticmethod
    def git(source, *args):
        return subprocess.check_output(['git', '-C', str(source), *args], stderr=subprocess.PIPE).decode().strip()

    def commit(self, source, message):
        self.git(source, 'add', '.')
        self.git(source, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@local', 'commit', '-qm', f'{self.run_id}: {message}')

    def publish_tree(self, source, manifest, name):
        parent = self.shared / name
        parent.mkdir()
        tree = parent / 'tree'
        tree.mkdir()
        start = time.perf_counter()
        for entry in manifest['files']:
            original = source / entry['path']
            if original.is_symlink() or original.stat().st_size != entry['bytes'] or hashlib.sha256(original.read_bytes()).hexdigest() != entry['sha256']:
                raise RuntimeError('Mutable fixture differs from the captured Git commit')
        source_verify_ms = elapsed(start)
        start = time.perf_counter()
        for entry in manifest['files']:
            original = source / entry['path']
            target = tree / entry['path']
            target.parent.mkdir(parents=True, exist_ok=True)
            with original.open('rb') as input_file, target.open('xb') as output:
                shutil.copyfileobj(input_file, output, 65536)
                output.flush()
                os.fsync(output.fileno())
        write_ms = elapsed(start)
        start = time.perf_counter()
        for entry in manifest['files']:
            target = tree / entry['path']
            if target.stat().st_size != entry['bytes'] or hashlib.sha256(target.read_bytes()).hexdigest() != entry['sha256']:
                raise RuntimeError('HGFS tree readback differs from the captured manifest')
        verify_ms = elapsed(start)
        marker = write_new(parent / 'tree-manifest.json', {'schemaVersion': 1, 'files': manifest['files']})
        return tree, {'sourceVerifyMs': source_verify_ms, 'sharedWriteMs': write_ms, 'sharedReadbackMs': verify_ms, 'manifest': marker}

    def measure_case(self, source, name, cache_name=None, include_tree=True):
        self.log(f'Measuring {name}')
        config = self.config | {'projects': [{'id': self.args.project, 'sourceRoot': str(source), 'sourceMode': 'commit'}],
                                'stateDirectory': str(self.local / 'capture-state')}
        path = self.local / f'{name}.controller.local.json'
        write_new(path, config)
        publish_start = time.perf_counter()
        measured = json.loads(subprocess.check_output([str(self.args.controller), '--config', str(path), 'measure-publish', self.args.project, 'HEAD']))
        publication_wall = elapsed(publish_start)
        if measured['reusedPackage']:
            raise RuntimeError('Expected a new snapshot, got existing package reuse; baseline would be misleading')
        ready = measured['ready']
        package = Path(self.config['delivery']['root']) / self.config['delivery']['namespace'] / 'deliveries' / self.args.project / ready['snapshotId']
        manifest = json.loads((package / 'manifest.json').read_bytes())
        case = {'sample': name, 'commit': manifest['source']['commit'], 'publication': measured,
                'publicationCliWallMs': publication_wall, 'sourceFiles': len(manifest['files']), 'sourceBytes': manifest['totalBytes']}
        self.records['cases'].append(case)
        request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': self.args.project,
                   'snapshotId': ready['snapshotId'], 'manifestSha256': ready['manifestSha256'],
                   'idempotencyKey': str(uuid.uuid4()), 'timeoutMs': 180000}
        imported = self.submit('import', request, name + '-import')
        if imported['record']['completedFiles'] != len(manifest['files']) or imported['record']['completedBytes'] != manifest['totalBytes']:
            raise RuntimeError('Windows imported file/byte counts differ from manifest')
        case['import'] = imported
        cache = str(PureWindowsPath(self.project['workspaceRoot']) / 'performance' / self.run_id / (cache_name or name) / 'build-cache')
        build_args = ['build', '--locked', '--offline', '--target-dir', cache]
        first = self.task('cargo.exe', build_args, ready['snapshotId'], name + '-build-first')
        self.output(first)
        case['firstBuild'] = first
        case['firstBuildCache'] = 'existing from preceding sample' if cache_name else 'new empty directory'
        warm = self.task('cargo.exe', build_args, ready['snapshotId'], name + '-build-warm')
        self.output(warm)
        case['warmBuild'] = warm
        binary = str(PureWindowsPath(cache) / 'debug/snapshot-fixture.exe')
        executed = self.task(binary, [], ready['snapshotId'], name + '-execute')
        output = self.output(executed)
        if 'Проверенный commit → Windows build ✓' not in output['stdout'].decode('utf-8'):
            raise RuntimeError('Compiled fixture did not print the delivered Unicode marker')
        case['execution'] = executed
        case['observedTimeToFirstBuildMs'] = publication_wall + imported['observedWallMs'] + first['observedWallMs']
        if include_tree:
            tree, tree_measure = self.publish_tree(source, manifest, name)
            operation = self.task('pwsh.exe', ['-NoProfile', '-NonInteractive', '-File', './measure-hgfs-tree.ps1',
                                             '-SharedTree', self.windows_path(tree), '-RunId', self.run_id, '-Sample', name], ready['snapshotId'], name + '-tree-io')
            output = self.output(operation)
            native = json.loads(output['stdout'].decode('utf-8-sig'))
            if native['verified'] is not True or native['bytes'] != manifest['totalBytes'] or native['files'] != len(manifest['files']):
                raise RuntimeError('Native tree measurement did not verify the same source bytes/files')
            case['tree'] = tree_measure | {'windows': native, 'operation': operation}
        return case, binary

    def measure_output_and_cancel(self, case, binary):
        snapshot = case['publication']['ready']['snapshotId']
        self.records['beforeOutputStorage'] = self.rpc({'method': 'storage_usage'})['usage']
        flood = self.task(binary, ['flood'], snapshot, 'output-2mib')
        output = self.output(flood)
        expected = b'x' * 1048576
        if any(bytes(value) != expected for value in output.values()):
            raise RuntimeError('Flood stdout/stderr bytes differ from the exact 1 MiB per stream')
        self.records['outputMeasurement'] = flood
        self.records['afterOutputStorage'] = self.rpc({'method': 'storage_usage'})['usage']
        # Submit an explicitly long managed tree, observe running/PID, then cancel
        # exactly this new task. No timeout/unknown outcome is silently replayed.
        request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': self.args.project,
                   'idempotencyKey': str(uuid.uuid4()), 'snapshotId': snapshot, 'timeoutMs': 120000,
                   'command': {'kind': 'direct', 'executable': binary, 'arguments': ['tree'], 'workingDirectory': '.'}}
        rpc = {'method': 'submit', 'request': request}
        write_new(self.local / 'cancel-tree.request.json', rpc)
        operation = {'label': 'cancel-tree', 'request': rpc, 'record': None}
        self.records['operations'].append(operation)
        operation['record'] = self.rpc(rpc)['task']
        identity = operation['record']['taskId']
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            self.probe('cancel-tree-running')
            record = self.rpc({'method': 'get_task', 'taskId': identity})['task']
            operation['record'] = record
            if record['status'] == 'running' and record['processId'] is not None:
                # Ensure the child really started; process spawning is not readiness.
                chunks = self.rpc({'method': 'read_output', 'taskId': identity, 'afterSequence': 0})['chunks']
                text = b''.join(bytes(c['bytes']) for c in chunks if c['stream'] == 'stdout').decode()
                if 'measurement child pid=' in text:
                    operation['runningOutput'] = text
                    break
            elif record['status'] not in ('queued', 'running'):
                raise RuntimeError('Tree ended unexpectedly before explicit cancellation')
            time.sleep(POLL_SECONDS)
        else:
            raise TimeoutError('Tree child was not observed; task retained for reconciliation')
        cancellation = {'method': 'cancel', 'taskId': identity}
        write_new(self.local / 'cancel-tree.cancel-request.json', cancellation)
        start = time.perf_counter()
        operation['cancelResponse'] = self.rpc(cancellation)
        while time.monotonic() < deadline + 15:
            self.probe('cancel-tree')
            record = self.rpc({'method': 'get_task', 'taskId': identity})['task']
            operation['record'] = record
            if record['status'] not in ('queued', 'running'):
                if record['status'] != 'cancelled':
                    raise RuntimeError(f'Unexpected cancellation result: {record["status"]}')
                operation['cancelObservedMs'] = elapsed(start)
                self.records['cancellation'] = operation
                self.log(f'cancel-tree: cancelled in {operation["cancelObservedMs"]:.1f} ms')
                return
            time.sleep(POLL_SECONDS)
        raise TimeoutError('Cancellation did not reach terminal state; retained task, no replay')

    def run(self):
        self.records['controllerStatus'] = self.rpc({'method': 'controller_status'})['status']
        self.records['publisherBuild'] = json.loads(subprocess.check_output([str(self.args.controller), '--describe']))
        self.records['beforeStorage'] = self.rpc({'method': 'storage_usage'})['usage']
        first_small = None
        for repeat in range(1, self.args.repeats + 1):
            for case in ('small', 'many-files', 'large-blob'):
                source = self.make_source(case, repeat)
                measured, binary = self.measure_case(source, f'{case}-{repeat}')
                if first_small is None:
                    first_small = (measured, binary)
                if case == 'many-files':
                    changed = source / 'padding/00000.dat'
                    changed.write_bytes(b'changed-one-file' + changed.read_bytes()[16:])
                    self.commit(source, 'one-file-change')
                    delta, _ = self.measure_case(source, f'one-file-change-{repeat}', cache_name=f'many-files-{repeat}', include_tree=False)
                    delta['changedSourceFiles'] = 1
                    delta['previousSnapshotId'] = measured['publication']['ready']['snapshotId']
        self.measure_output_and_cancel(*first_small)
        self.records['afterStorage'] = self.rpc({'method': 'storage_usage'})['usage']
        self.probe('final')
        self.records['finalWorker'] = self.identity(self.rpc({'method': 'info'})['info'])
        if self.records['finalWorker']['activeTaskCount'] != 0:
            raise RuntimeError('Unexpected active task at the end of baseline')
        self.records['latencySummary'] = {phase: stats([v['ms'] for v in self.records['controlLatency'] if v['phase'] == phase])
                                          for phase in sorted({v['phase'] for v in self.records['controlLatency']})}
        self.records['passed'] = True

    def save(self):
        artifact = write_new(self.local / 'performance-report.json', self.records)
        shared_artifact = write_new(self.shared / 'performance-report.json', self.records)
        if shared_artifact != artifact:
            raise RuntimeError('Local/shared report bytes or SHA-256 differ')
        write_new(self.shared / 'ready.json', {'schemaVersion': 1, 'runId': self.run_id,
                                              'passed': self.records['passed'], 'report': shared_artifact})
        self.log(f'Report: {self.shared}/performance-report.json; passed={self.records["passed"]}; SHA-256={artifact["sha256"]}')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--controller-config', type=Path, default=ROOT / '.winbridge/controller-snapshot.local.json')
    parser.add_argument('--controller', type=Path, default=ROOT / 'target/debug/winbridge-controller')
    parser.add_argument('--project', default='snapshot-fixture', help='Approved fixture project; not a production project')
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--small-files', type=int, default=512, help='4096 bytes per small file')
    parser.add_argument('--blob-mib', type=int, default=32)
    args = parser.parse_args()
    if not 1 <= args.repeats <= 10 or not 1 <= args.small_files <= 2048 or not 1 <= args.blob_mib <= 64:
        parser.error('Limits: repeats 1–10, small-files 1–2048, blob-mib 1–64')
    measurement = Measurement(args)
    try:
        measurement.run()
    except BaseException as exc:
        measurement.records['error'] = f'{type(exc).__name__}: {exc}'
        raise
    finally:
        measurement.save()


if __name__ == '__main__':
    main()
