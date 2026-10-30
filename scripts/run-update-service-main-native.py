#!/usr/bin/env python3
"""Run one pinned Main25 fixture through an already connected worker; never install."""
import argparse
import base64
import hashlib
import importlib.util
import ipaddress
import json
import os
from pathlib import Path, PureWindowsPath
import re
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_new(path, value):
    with path.open('x') as file:
        json.dump(value, file, indent=2, ensure_ascii=False)
        file.flush()
        os.fsync(file.fileno())


def submit_fenced(native, endpoint, request):
    payload = json.dumps(request).encode()
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(15)
        stream.connect(str(endpoint))
        stream.sendall(struct.pack('<I', len(payload)) + payload)
        length, = struct.unpack('<I', native.exact(stream, 4))
        if not 0 < length <= 1048576:
            raise ValueError('Invalid fenced IPC response size; acceptance unknown')
        return json.loads(native.exact(stream, length))


def main(controller=False, lifetime=False, ready_loss=False, resident=False, finalization=False, keeper=False):
    finalization = finalization or keeper
    resident = resident or finalization
    ready_loss = ready_loss or resident
    lifetime = lifetime or ready_loss
    controller_gate=controller or lifetime
    mode='Main31' if keeper else 'Main30' if finalization else 'Main29' if resident else 'Main28' if ready_loss else ('Main27' if lifetime else ('Main26' if controller_gate else 'Main25'))
    namespace='main31-204' if keeper else 'main30-204' if finalization else 'main29-204' if resident else 'main28-204' if ready_loss else ('main27-204' if lifetime else ('main26-204' if controller_gate else 'main25-204'))
    parser = argparse.ArgumentParser(description=f'Run one pinned {mode} fixture through the installed worker; never install.')
    parser.add_argument('--source-archive', required=True, type=Path)
    parser.add_argument('--target-archive', required=True, type=Path)
    parser.add_argument('--cache-run', required=True)
    parser.add_argument('--worker17-reference-sha256')
    parser.add_argument('--worker17-reference', help='Retained worker17 EXE, independent of the installed worker (absolute K: path)')
    parser.add_argument('--test-runtime-directory', required=True)
    parser.add_argument('--profile', choices=(('worker19-updater24',) if keeper else ('worker19-updater23',) if finalization else ('worker19-updater22',) if lifetime else (('worker19-updater21',) if controller_gate else ('worker17-updater20', 'worker19-updater20'))), default=('worker19-updater24' if keeper else 'worker19-updater23' if finalization else 'worker19-updater22' if lifetime else ('worker19-updater21' if controller_gate else 'worker17-updater20')))
    parser.add_argument('--controller-config', required=True, type=Path)
    parser.add_argument('--controller-binary', required=True, type=Path)
    args = parser.parse_args()
    native = load('main25_native', 'validate-workflow-native.py')
    health = load('main25_archive', 'validate-update-health-actor-native.py')
    checks = load('main_gate_checks', 'update-activation-keeper-proof.py' if keeper else 'update-activation-finalization-proof.py' if finalization else 'update-activation-resident-proof.py' if resident else 'update-activation-lifetime-ready-loss-proof.py' if ready_loss else ('update-activation-lifetime-proof.py' if lifetime else ('update-activation-controller-proof.py' if controller_gate else 'update-activation-service-proof.py')))
    reference = load('main25_references', 'worker-migration-reference.py').reference_for_profile(
        args.profile, args.worker17_reference, args.worker17_reference_sha256)
    source, target = map(health.archive_pin, (args.source_archive, args.target_archive))
    expected = checks.profile_schemas(args.profile)
    for path in (args.source_archive, args.target_archive):
        with zipfile.ZipFile(path) as archive:
            actual = (native.worker_schema_from_source(archive.read('winbridge/apps/worker/src/store.rs')),
                      native.updater_schema_from_source(archive.read('winbridge/apps/update/src/journal.rs')))
        checks.need(actual == expected, 'Profile differs from independently verified archive schemas')
    checks.need(source['sourceCommit'] != target['sourceCommit'], 'Distinct source/target pins required')
    checks.need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.cache_run), 'Retained native cache run ID required')
    cfg = json.loads(args.controller_config.read_text())
    endpoint = Path(cfg['stateDirectory']) / ('controller-' + cfg['instanceId'] + '.sock')
    before = native.call(endpoint, {'method': 'info'})['info']
    checks.need(before['platform'] == 'windows' and before['activeTaskCount'] == 0
                and not before.get('maintenanceError') and not before.get('journalRecoveryRequired'), 'Healthy idle installed worker required')
    context = before['fencedRequests']['context']
    host = cfg['controller']['listenAddress'].rsplit(':', 1)[0]
    checks.need(ipaddress.IPv4Address(host).is_private, 'Literal private IPv4 test listener required')
    runtime = PureWindowsPath(args.test_runtime_directory)
    checks.need(runtime.is_absolute() and runtime.drive.lower() == 'k:' and '..' not in runtime.parts,
                'Explicit owned local WinBridge test runtime required')
    if controller_gate:
        checks.need(runtime==PureWindowsPath(r'K:\_proj-26\winbridge\runtime-test-204'),
                    'Main26 requires approved stable runtime-test-204; legacy slot is not authorized')
    project = next(p for p in before['projects'] if p['id'] == 'snapshot-fixture')
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime()) + '-' + uuid.uuid4().hex[:8]
    local = ROOT / '.winbridge/update-preparation' / (namespace+'-' + run)
    local.mkdir(parents=True)
    shared = Path('/mnt/hgfs/shared/WinBridge/workflow-validation') / run
    shared.mkdir()
    work = PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / ('tmp-'+namespace+'-' + run)
    cache = PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / args.cache_run
    windows_report = PureWindowsPath('K:/VMs/shared/WinBridge/workflow-validation') / run
    instance = namespace+'-' + uuid.uuid4().hex[:12]
    url = 'ws://' + host + ':1144'
    # Keep Unix socket paths short; never change a persistent controller config.
    state = Path(tempfile.mkdtemp(prefix='wb31-' if keeper else 'wb30-' if finalization else 'wb29-' if resident else 'wb28-' if ready_loss else ('wb27-' if lifetime else ('wb26-' if controller_gate else 'wb25-'))))
    test_config = state / 'controller.json'
    write_new(test_config, {'schemaVersion': 1, 'instanceId': instance, 'nodeId': ('main31-debian-'+host.rsplit('.',1)[-1] if keeper else 'main30-debian-'+host.rsplit('.',1)[-1] if finalization else 'main29-debian-'+host.rsplit('.',1)[-1] if resident else 'main28-debian-'+host.rsplit('.',1)[-1] if ready_loss else ('main27-debian-'+host.rsplit('.',1)[-1] if lifetime else ('main26-debian-'+host.rsplit('.',1)[-1] if controller_gate else 'main25-debian-204'))),
              'stateDirectory': str(state), 'controller': {'listenAddress': host + ':1144', 'workerNodeId': 'shutdown-main-test'}, 'projects': []})
    test_endpoint = state / ('controller-' + instance + '.sock')
    log = (local / 'controller.log').open('x')
    # Unknown bootstrap acceptance must not kill its fixture when the observer's
    # command group exits. Ownership remains explicit in controller-owner.json.
    controller = subprocess.Popen([str(args.controller_binary.resolve()), '--config', str(test_config), 'serve'],
                                  stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    terminal = False
    submission_sent = False
    task = None
    try:
        write_new(local / 'controller-owner.json', {'pid': controller.pid, 'config': str(test_config), 'endpoint': str(test_endpoint), 'instanceId': instance, 'url': url})
        until = time.monotonic() + 10
        while not test_endpoint.exists():
            checks.need(controller.poll() is None and time.monotonic() < until, 'Fixture controller startup failed; inspect its log')
            time.sleep(.05)
        native.call(test_endpoint, {'method': 'controller_status'})
        ps = native.ps_string
        archives = [str(PureWindowsPath('K:/VMs/shared') / path.resolve().relative_to('/mnt/hgfs/shared'))
                    for path in (args.source_archive, args.target_archive)]
        migration_arg = (' -Worker17Reference ' + ps(str(reference))
                         + ' -Worker17ReferenceSha256 ' + ps(args.worker17_reference_sha256)) if reference else ''
        gate_switch='KeeperRetirementGate' if keeper else 'ActivationFinalizationGate' if finalization else 'ResidentConnectionGate' if resident else 'ActivationLifetimeReadyLossGate' if ready_loss else ('ActivationLifetimeGate' if lifetime else ('ActivationControllerGate' if controller_gate else 'ActivationServiceGate'))
        script = f'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$local={ps(str(work))};$archive={ps(archives[0])};$target={ps(archives[1])}
if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(source['archiveSha256'])}){{throw 'Source archive changed'}}
if((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(target['archiveSha256'])}){{throw 'Target archive changed'}}
if(Test-Path -LiteralPath $local){{throw 'Existing fixture preserved'}}
if([IO.DriveInfo]::new([IO.Path]::GetPathRoot($local)).AvailableFreeSpace -lt 6GB){{throw 'Less than 6 GiB; no build'}}
New-Item -ItemType Directory -Path $local|Out-Null
$copy=Join-Path $local 'source.zip';[IO.File]::Copy($archive,$copy,$false)
Expand-Archive -LiteralPath $copy -DestinationPath $local
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $local 'winbridge/scripts/test-update-health-actor.ps1') -SourceDirectory (Join-Path $local 'winbridge') -SourceArchiveSha256 {ps(source['archiveSha256'])} -TargetArchivePath $target -TargetArchiveSha256 {ps(target['archiveSha256'])} -ValidationCacheDirectory {ps(str(cache))} -ReportDirectory {ps(str(windows_report))} -{gate_switch} -FixtureInstanceId {ps(instance)} -FixtureControllerUrl {ps(url)} -TestRuntimeDirectory {ps(str(runtime))}{migration_arg}
exit $LASTEXITCODE
'''
        submission = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': 'snapshot-fixture',
                      'idempotencyKey': 'native-'+namespace+'-' + run, 'timeoutMs': 3600000,
                      'command': {'kind': 'direct', 'executable': 'pwsh.exe', 'arguments': ['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()], 'workingDirectory': '.'}}
        request = {'requestContractVersion': 2, 'context': context, 'request': {'method': 'submit', 'request': submission}}
        write_new(local / 'request.json', request)
        write_new(local / 'info-before.json', before)
        submission_sent = True
        reply = submit_fenced(native, endpoint, request)
        if controller_gate:
            write_new(local/'acceptance.json',reply)
        if reply.get('requestContractVersion') == 2 and reply.get('outcome') == 'rejected_before_dispatch':
            terminal = True
        checks.need(reply.get('requestContractVersion') == 2 and reply.get('context') == context and reply.get('outcome') == 'responded' and reply['response'].get('kind') == 'task', 'Task ACK unavailable; inspect same saved context/key, never resubmit')
        task = reply['response']['task']
        meta = {'run': run, 'taskId': task['taskId'], 'reportDirectory': str(shared), 'instanceId': instance,
                'source': source, 'target': target, 'profile': args.profile, 'worker17ReferenceSha256': args.worker17_reference_sha256,
                'worker17Reference': str(reference) if reference else None,
                'sourceArchive': str(args.source_archive.resolve()), 'targetArchive': str(args.target_archive.resolve()),
                'requestSha256': hashlib.sha256((local / 'request.json').read_bytes()).hexdigest(), 'controllerPid': controller.pid,
                'testRuntimeDirectory': str(runtime), 'workingDirectory': str(work), 'cacheDirectory': str(cache)}
        write_new(local / 'run-info.json', meta)
        print(json.dumps({'taskId': task['taskId'], 'runInfo': str(local / 'run-info.json'), 'profile': args.profile}), flush=True)
        sequence = 0
        deadline = time.monotonic() + submission['timeoutMs']/1000 + 60
        with (local / 'controller-observations.jsonl').open('x') as observations, (local / 'output.jsonl').open('x') as output:
            while True:
                observed = native.call(test_endpoint, {'method': 'controller_status'})
                checks.need(controller.poll() is None, 'Fixture controller stopped; do not restart/adopt')
                observations.write(json.dumps({'pidAlive': True, 'response': observed, 'atMs': int(time.time()*1000)}) + '\n'); observations.flush()
                task = native.call(endpoint, {'method': 'get_task', 'taskId': task['taskId']})['task']
                while True:
                    page = native.call(endpoint, {'method': 'read_output', 'taskId': task['taskId'], 'afterSequence': sequence})['chunks']
                    if not page:
                        break
                    for chunk in page:
                        output.write(json.dumps(chunk) + '\n')
                        sys.stdout.write(bytes(chunk['bytes']).decode('utf-8', errors='replace'))
                    output.flush(); sys.stdout.flush(); sequence = page[-1]['sequence']
                if task['status'] not in ('queued', 'running'):
                    terminal = True
                    break
                checks.need(time.monotonic() < deadline, 'Observation deadline; task may continue, inspect same task ID')
                time.sleep(1)
        write_new(local / 'terminal.json', task)
        after = native.call(endpoint, {'method': 'info'})['info']
        write_new(local / 'info-after.json', after)
        checks.need(task['status'] == 'succeeded' and task['exitCode'] == 0, mode+' failed; preserve slot and observe original attempt, no automatic replay')
        if controller_gate:
            # A short successful session may fall between the 1s status samples.
            # Use the existing rotating controller activity log as independent
            # evidence, without extending the worker startup window or adding a
            # fixed sleep. Stop only this known fixture after its task ends.
            final_status=native.call(test_endpoint,{'method':'controller_status'})
            checks.need(controller.poll() is None, 'Fixture controller exited unexpectedly')
            write_new(local/'controller-final-status.json',final_status)
            controller.send_signal(signal.SIGINT);code=controller.wait(timeout=12);log.close()
            write_new(local/'controller-exit.json',dict(pid=controller.pid,exitCode=code))
            checks.need(code==0,'Fixture controller shutdown failed')
            names=['activity.jsonl.'+str(i) for i in range(5,0,-1)]+['activity.jsonl']
            captured={name:(state/name).read_bytes() for name in names if (state/name).is_file()}
            checks.need(captured and captured=={name:(state/name).read_bytes() for name in captured},'Controller activity changed after shutdown')
            with (local/'controller-activity.jsonl').open('xb') as saved_activity:
                saved_activity.write(b''.join(captured.values()));saved_activity.flush();os.fsync(saved_activity.fileno())
            write_new(local/'controller-activity-sources.json',{name:dict(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest()) for name,raw in captured.items()})
        verifier='verify-update-keeper-main-native.py' if keeper else 'verify-update-finalization-main-native.py' if finalization else 'verify-update-resident-main-native.py' if resident else 'verify-update-lifetime-ready-loss-main-native.py' if ready_loss else ('verify-update-lifetime-main-native.py' if lifetime else ('verify-update-controller-main-native.py' if controller_gate else 'verify-update-service-main-native.py'))
        result = subprocess.run([sys.executable, str(ROOT/'scripts'/verifier), '--profile', args.profile,
                    '--run-info', str(local/'run-info.json'), '--source-archive', str(args.source_archive), '--target-archive', str(args.target_archive)], capture_output=True, text=True)
        (local / 'independent-audit.stdout').write_text(result.stdout)
        (local / 'independent-audit.stderr').write_text(result.stderr)
        checks.need(result.returncode == 0, 'Independent '+mode+' audit refused; inspect saved audit output')
        print(result.stdout, flush=True)
        checks.need(after['generation'] == before['generation'] and after['build'] == before['build']
                    and after['fencedRequests']['context'] == context and after['activeTaskCount'] == 0, 'Installed worker changed or still has work')
        print(json.dumps({'passed': True, 'installedUnchanged': True, 'runInfo': str(local/'run-info.json')}), flush=True)
    finally:
        if terminal or not submission_sent:
            if controller.poll() is None:
                controller.send_signal(signal.SIGINT)
                controller.wait(timeout=12)
            if not log.closed:log.close()
        else:
            print('Observation interrupted; leave the recorded fixture controller alive until the same task terminates', file=sys.stderr)


if __name__ == '__main__':
    main()
