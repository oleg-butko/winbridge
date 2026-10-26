#!/usr/bin/env python3
"""Build one pinned Windows patch and test its RPC against the actual old controller.

Creates an isolated controller/state and release directories; never stops or
replaces the installed pair. Uncertain submissions are saved, never replayed.
"""
import argparse
import base64
import gzip
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path, PureWindowsPath
import shutil
import signal
import socket
import subprocess
import tempfile
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


native = module('native', 'validate-workflow-native.py')
fenced = module('fenced', 'validate-installed-provider.py')
timings = module('timings', 'summarize-update-timings.py')
health = module('health', 'validate-update-health-actor-native.py')
release_proof = module('release_proof', 'worker-release-proof.py')


def save_json(path, value):
    temporary = path.with_name(path.name + '.part')
    with temporary.open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)


def stable_runtime_directory(installed_worker, chosen=None):
    production = PureWindowsPath(installed_worker).parent
    candidate = PureWindowsPath(chosen) if chosen else production.with_name('runtime-test')
    if (not candidate.is_absolute() or not re.fullmatch('[A-Za-z]:', candidate.drive)
            or '..' in candidate.parts or candidate.is_relative_to(production) or production.is_relative_to(candidate)):
        raise ValueError('Stable test runtime must be a separate absolute local directory')
    return str(candidate)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--controller-config', required=True, type=Path)
    p.add_argument('--reference', required=True, type=Path)
    p.add_argument('--source-ready', required=True, type=Path)
    p.add_argument('--controller-baseline', required=True, type=Path)
    p.add_argument('--preflight', required=True, type=Path, help='Fresh installed build/config/hash/manifest observation')
    p.add_argument('--test-runtime-directory', help='Stable Windows test executable directory; defaults to runtime-test beside installed runtime')
    p.add_argument('--firewall-wait-seconds', type=int, default=900, help='Connection approval budget only; 60..3600 seconds')
    p.add_argument('--test-controller-port', type=int, default=1144, help='Fixed isolated controller port, separate from installed1143')
    args = p.parse_args()
    if not 60 <= args.firewall_wait_seconds <= 3600 or not 1024 <= args.test_controller_port <= 65535:
        raise ValueError('Invalid test connection budget/port')
    cfg = json.loads(args.controller_config.read_text())
    ep = Path(cfg['stateDirectory']) / ('controller-' + cfg['instanceId'] + '.sock')
    info = native.call(ep, dict(method='info'))['info']
    if info['activeTaskCount'] or info.get('maintenanceError') or info.get('journalRecoveryRequired'):
        raise ValueError('Healthy idle installed worker required')
    project = next(x for x in info['projects'] if x['id'] == 'snapshot-fixture' and x['allowDirectCommands'])
    ready = json.loads(args.source_ready.read_text())
    archive = args.source_ready.parent / ready['archive']
    if archive.name != ready['archive'] or ready['dirty'] or hashlib.sha256(archive.read_bytes()).hexdigest() != ready['sha256']:
        raise ValueError('Clean source ZIP pin differs')
    with zipfile.ZipFile(archive) as z:
        manifest = z.read('winbridge/winbridge-source-manifest.json')
    manifest_sha = hashlib.sha256(manifest).hexdigest()
    pin = health.archive_pin(archive)
    if pin['sourceCommit'] != ready['sourceCommit'] or pin['manifestSha256'] != manifest_sha:
        raise ValueError('Source manifest/actual payload pin differs')
    ref = json.loads(args.reference.read_text())
    files = timings.verified_files(Path('/mnt/hgfs/shared/WinBridge/workflow-validation') / ref['run'])
    if ref['passed'] is not True or hashlib.sha256(files['report.json']).hexdigest() != ref['reportSha256']:
        raise ValueError('Prior native reference differs')
    proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
    retained = proof['cases'][0]['replacement']['actualRetainedFiles']
    baseline, verifier = retained[0], retained[3]
    if not baseline['path'].endswith('\\winbridge-worker.exe') or not verifier['path'].endswith('\\winbridge-update.exe'):
        raise ValueError('Pinned retained binaries required')
    preflight = json.loads(args.preflight.read_text())
    if info['build'] != preflight['installedBuild'] or preflight['windows']['installedSetExists']:
        raise ValueError('Working runtime changed or requires coordinated installed-set release')
    if shutil.disk_usage(ROOT).free < 6 * 1024**3:
        raise ValueError('Need >=6GiB Debian free')
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime()) + '-' + uuid.uuid4().hex[:8]
    local = ROOT / '.winbridge/releases' / run
    local.mkdir(parents=True)
    shared = Path('/mnt/hgfs/shared/WinBridge/releases') / run
    shared.mkdir(parents=True)
    win_shared = str(PureWindowsPath('K:/VMs/shared/WinBridge/releases') / run)
    work = str(PureWindowsPath(project['workspaceRoot']) / 'releases' / ('worker-0.1.1-' + run))
    test_runtime = stable_runtime_directory(preflight['windows']['installedWorkerPath'], args.test_runtime_directory)
    instance = 'release-' + uuid.uuid4().hex[:8]
    state = Path(tempfile.mkdtemp(prefix='wb-rel-'))
    host = cfg['controller']['listenAddress'].rsplit(':', 1)[0]
    with socket.socket() as reserve:
        reserve.bind((host, args.test_controller_port))
        port = reserve.getsockname()[1]
    controller_config = dict(schemaVersion=1, instanceId=instance, nodeId='release-controller',
        stateDirectory=str(state), controller=dict(listenAddress=f'{host}:{port}', workerNodeId='release-worker'),
        projects=[dict(id='fixture', sourceRoot=str(ROOT), sourceMode='commit')])
    config_path = state / 'controller.json'
    config_path.write_text(json.dumps(controller_config, indent=2))
    isolated_ep = state / ('controller-' + instance + '.sock')
    controller_log = (local / 'controller.log').open('w')
    process = subprocess.Popen([str(args.controller_baseline.resolve()), '--config', str(config_path), 'serve'],
                               stdout=controller_log, stderr=subprocess.STDOUT)
    meta_path = local / 'submission.json'
    try:
        deadline = time.monotonic() + 15
        while not isolated_ep.exists():
            if process.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError('Isolated controller failed; inspect ' + str(local / 'controller.log'))
            time.sleep(.1)
        with zipfile.ZipFile(archive) as z:
            bundle = {name: z.read('winbridge/scripts/' + name).decode() for name in
                ('build-windows-worker-release.ps1', 'session-runtime.ps1', 'lib/build-cache.ps1', 'lib/validation-storage.ps1', 'lib/test-worker-runtime.ps1')}
        parameters = dict(SourceArchive=str(PureWindowsPath('K:/VMs/shared/WinBridge/sources') / ready['archive']),
            ArchiveSha256=ready['sha256'], ManifestSha256=manifest_sha, SourceCommit=ready['sourceCommit'],
            WorkDirectory=work, CacheDirectory=str(PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / ref['cacheRun'] / 'test-build-cache'),
            VerifierExecutable=verifier['path'], VerifierSha256=verifier['sha256'], BaselineWorker=baseline['path'], BaselineSha256=baseline['sha256'],
            ControllerUrl=f'ws://{host}:{port}', InstanceId=instance, SharedReportDirectory=win_shared,
            InstalledConfig=preflight['windows']['configPath'],
            InstalledConfigSha256=preflight['windows']['configSha256'],
            InstalledWorker=preflight['windows']['installedWorkerPath'],
            InstalledWorkerSha256=preflight['windows']['installedWorkerSha256'], InstalledGeneration=info['generation'],
            TestRuntimeDirectory=test_runtime, FirewallWaitSeconds=args.firewall_wait_seconds)
        helper_hashes = {k: hashlib.sha256(v.encode()).hexdigest() for k, v in bundle.items()}
        packed = base64.b64encode(gzip.compress(json.dumps(dict(files=helper_hashes, parameters=parameters)).encode())).decode()
        helpers = str(PureWindowsPath(work).parent / ('tmp-release-script-' + run))
        script = "$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'\n$helpers=" + native.ps_string(helpers) + "\n"
        script += "if(Test-Path -LiteralPath $helpers){throw 'Existing release helpers preserved'}\n"
        script += "$s=[IO.MemoryStream]::new([Convert]::FromBase64String(" + native.ps_string(packed) + "))\n"
        script += "$g=[IO.Compression.GzipStream]::new($s,[IO.Compression.CompressionMode]::Decompress);$r=[IO.StreamReader]::new($g)\n"
        script += "try{$b=$r.ReadToEnd()|ConvertFrom-Json -AsHashtable}finally{$r.Dispose()}\nNew-Item -ItemType Directory -Path (Join-Path $helpers 'lib')|Out-Null\n"
        script += "$archiveStream=[IO.File]::Open($b.parameters.SourceArchive,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)\n"
        script += "try{$sha=[Security.Cryptography.SHA256]::Create();try{$hash=[Convert]::ToHexString($sha.ComputeHash($archiveStream)).ToLowerInvariant()}finally{$sha.Dispose()};if($hash -cne $b.parameters.ArchiveSha256){throw 'Source archive differs'};$archiveStream.Position=0;$zip=[IO.Compression.ZipArchive]::new($archiveStream,[IO.Compression.ZipArchiveMode]::Read,$true)\n"
        script += "try{foreach($name in $b.files.Keys){$path=Join-Path $helpers $name;$entry=$zip.GetEntry('winbridge/scripts/'+$name);if($null -eq $entry){throw 'Missing helper'};$from=$entry.Open();try{$to=[IO.File]::Open($path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None);try{$from.CopyTo($to)}finally{$to.Dispose()}}finally{$from.Dispose()};if((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $b.files[$name]){throw 'Helper differs'};$tokens=$null;$errors=$null;[Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)|Out-Null;if($errors.Count){throw ($errors|Out-String)}}}finally{$zip.Dispose()}}finally{$archiveStream.Dispose()}\n"
        script += "$spec=$b.parameters;& (Join-Path $helpers 'build-windows-worker-release.ps1') @spec\n"
        encoded = base64.b64encode(script.encode('utf-16le')).decode()
        if len(encoded) > 30000:
            raise ValueError('Release command exceeds bounded argv')
        request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId=project['id'], idempotencyKey='worker-release-' + run,
            timeoutMs=1800000 + args.firewall_wait_seconds * 1000, command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.', arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand', encoded]))
        meta = dict(run=run, taskId=None, request=request, source=ready, manifestSha256=manifest_sha,
            windowsWorkDirectory=work, reportDirectory=str(shared), controllerConfig=str(config_path), controllerEndpoint=str(ep), installedConfig=preflight['windows']['configPath'],
            controllerBaselineSha256=hashlib.sha256(args.controller_baseline.read_bytes()).hexdigest(),
            testRuntimeDirectory=test_runtime, firewallWaitSeconds=args.firewall_wait_seconds, testControllerPort=port,
            debianFreeBytesBefore=shutil.disk_usage(ROOT).free, bundleSha256={k: hashlib.sha256(v.encode()).hexdigest() for k, v in bundle.items()})
        save_json(meta_path, meta)
        task = native.call(ep, dict(method='submit', request=request))['task']
        meta['taskId'] = task['taskId']
        save_json(meta_path, meta)
        print(json.dumps(dict(taskId=task['taskId'], metadata=str(meta_path), report=str(shared))), flush=True)
        sequence = 0
        network_done = False
        while task['status'] in {'queued', 'running'}:
            chunks = native.call(ep, dict(method='read_output', taskId=task['taskId'], afterSequence=sequence))['chunks']
            for chunk in chunks:
                sequence = chunk['sequence']
                if chunk['stream'] == 'stdout':
                    print(bytes(chunk['bytes']).decode('utf8', errors='replace'), end='', flush=True)
            marker = shared / 'network-ready.json'
            if marker.exists() and not network_done:
                try:
                    network_ready = json.loads(marker.read_text(encoding='utf-8-sig'))
                except (OSError, json.JSONDecodeError):
                    # The marker may be visible while HGFS is still publishing bytes.
                    time.sleep(.1)
                    task = native.call(ep, dict(method='get_task', taskId=task['taskId']))['task']
                    continue
                live = native.call(isolated_ep, dict(method='info'))['info']
                if live['build']['version'] != '0.1.1' or live['build']['sourceCommit'] != ready['sourceCommit'] or live['build']['sourceManifestSha256'] != manifest_sha or live['build']['sourceDirty'] is not False or live['fencedRequests']['context'] != network_ready['context']:
                    raise ValueError('Unexpected provider on isolated controller')
                context = live['fencedRequests']['context']
                envelope = dict(requestContractVersion=2, context=context, request=dict(method='submit', request=dict(protocolVersion=1,
                    requestId=str(uuid.uuid4()), projectId='fixture', idempotencyKey='release-rpc-' + run, timeoutMs=10000,
                    command=dict(kind='recipe', recipeId='fixture-success', arguments=[]))))
                save_json(local / 'network-request.json', envelope)
                accepted = fenced.call_fenced(isolated_ep, envelope)['task']
                duplicate = fenced.call_fenced(isolated_ep, envelope)['task']
                assert duplicate['taskId'] == accepted['taskId']
                wrong = json.loads(json.dumps(envelope))
                wrong['context']['journalIncarnation'] = str(uuid.uuid4())
                try:
                    fenced.call_fenced(isolated_ep, wrong)
                except RuntimeError as error:
                    stale = str(error)
                    assert 'not transmitted' in stale or 'incarnation' in stale.lower()
                else:
                    raise AssertionError('Stale scope accepted')
                deadline = time.monotonic() + 30
                while accepted['status'] in {'queued', 'running'}:
                    if time.monotonic() > deadline:
                        raise TimeoutError('Accepted isolated task outcome unknown; never resubmit')
                    time.sleep(.1)
                    accepted = native.call(isolated_ep, dict(method='get_task', taskId=accepted['taskId']))['task']
                assert accepted['status'] == 'succeeded' and accepted['exitCode'] == 0
                output = native.call(isolated_ep, dict(method='read_output', taskId=accepted['taskId'], afterSequence=0))['chunks']
                text = '\n'.join(b''.join(bytes(c['bytes']) for c in output if c['stream'] == stream).decode('utf8')
                                 for stream in ('stdout', 'stderr'))
                assert 'Привет' in text
                history = native.call(isolated_ep, dict(method='list_tasks'))['tasks']
                assert len(history) == 1
                done = dict(passed=True, context=context, build=live['build'], taskId=accepted['taskId'],
                    duplicateSameTask=True, staleScopeRejected=True, output=text, retainedTaskCount=len(history))
                save_json(local / 'network-proof.json', done)
                save_json(shared / 'network-complete.json', done)
                network_done = True
                print('Old-controller/new-worker RPC, deduplication, stale scope and Unicode passed', flush=True)
            time.sleep(1)
            task = native.call(ep, dict(method='get_task', taskId=task['taskId']))['task']
        (local / 'terminal.json').write_text(json.dumps(task, indent=2))
        if task['status'] != 'succeeded' or task['exitCode'] != 0:
            raise RuntimeError('Release task failed; inspect saved task/logs; no automatic retry')
        files = timings.verified_files(shared)
        report = json.loads(files['report.json'].decode('utf-8-sig'))
        if report['passed'] is not True or not network_done or report['isolatedWorkerExitConfirmed'] is not True:
            raise ValueError('Incomplete release proof')
        verified = release_proof.verify(files, ready, manifest_sha, json.loads((local / 'network-proof.json').read_text()), baseline['sha256'], test_runtime, args.firewall_wait_seconds)
        readme = ('WinBridge worker 0.1.1, Windows x86_64, local dev profile with safety checks.\n'
            'Fix: indexed pending-only scheduler reads; idle CPU diagnostics and build/PID startup identity.\n'
            'Source: ' + ready['sourceCommit'] + '\n\n'
            'This package has NOT replaced the installed worker. Config/state/journal stay in place.\n'
            'Manual application: wait for zero active work, stop the old worker with Ctrl+C,\n'
            'preserve its exe for recovery, copy this winbridge-worker.exe over the old exe,\n'
            'then use the SAME --config ' + preflight['windows']['configPath'] + ' serve.\n'
            'If winbridge-installed-set.json exists, do not replace one exe: coordinated installed-set update required.\n'
            'The tested current runtime has no installed-set manifest. Never delete config/state/journal.\n'
            'For periodic diagnostics set $env:WINBRIDGE_SCHEDULER_DIAGNOSTICS=\'1\' before serve.\n'
            'The new worker migrates inactive journal16 to17 with a consistent automatic backup.\n'
            'The old worker cannot open journal17. Do not roll back only the exe after new startup.\n'
            'After new tasks run, never restore an old journal automatically: preserve state and request recovery.\n'
            'Multiple Debian controllers and public updater installation are not enabled by this release.\n')
        package = dict({'winbridge-worker.exe': files['winbridge-worker.exe'], 'build-identity.json': files['build-identity.json'], 'README.txt': readme.encode()})
        sums = ''.join(hashlib.sha256(b).hexdigest() + '  ' + n + '\n' for n, b in package.items()).encode()
        package['SHA256SUMS.txt'] = sums
        zip_path = shared / ('winbridge-worker-0.1.1-' + ready['sourceCommit'][:7] + '.zip')
        with zipfile.ZipFile(zip_path, 'x', compression=zipfile.ZIP_DEFLATED) as z:
            for name, data in package.items():
                z.writestr(name, data)
        with zipfile.ZipFile(zip_path) as z:
            assert z.testzip() is None and all(z.read(n) == b for n, b in package.items())
        result = dict(schemaVersion=1, kind='winbridge_worker_patch_release', passed=True,
            sourceCommit=ready['sourceCommit'], sourceManifestSha256=manifest_sha,
            taskId=task['taskId'], run=run, archive=str(zip_path), archiveSha256=hashlib.sha256(zip_path.read_bytes()).hexdigest(),
            workerSha256=hashlib.sha256(files['winbridge-worker.exe']).hexdigest(), reportSha256=hashlib.sha256(files['report.json']).hexdigest(),
            filesVerifiedTwice=len(files), nativeTestsPassed=verified['nativeTestsPassed'],
            testRuntimeChecksPassed=verified['testRuntimeChecksPassed'], testRuntimeDirectory=test_runtime,
            firewallWaitSeconds=args.firewall_wait_seconds, testControllerPort=port,
            cpuPercentOfOneCore=verified['cpuPercentOfOneCore'], networkVerified=True,
            installedRuntimeChanged=False, debianFreeBytesAfter=shutil.disk_usage(ROOT).free)
        (local / 'verified.json').write_text(json.dumps(result, indent=2))
        (shared / 'release.ready.json').write_text(json.dumps(result, indent=2))
        print(json.dumps(result, indent=2), flush=True)
    finally:
        if process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        controller_log.close()


if __name__ == '__main__':
    main()
