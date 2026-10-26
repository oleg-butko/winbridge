#!/usr/bin/env python3
"""Run an isolated build probe against an already verified immutable native source."""
import argparse
import base64
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import shutil
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('previous', ROOT/'scripts/run-update-profile-native.py')
previous = importlib.util.module_from_spec(spec)
spec.loader.exec_module(previous)


def submit(args):
    pin = json.loads(args.reference.read_text())
    if not (pin['passed'] is True and re.fullmatch(r'\d{8}-\d{6}-[a-f0-9]{8}', pin['run'])):
        raise ValueError('Passed reference pin required')
    files = previous.timings.verified_files(Path('/mnt/hgfs/shared/WinBridge/workflow-validation')/pin['run'])
    if hashlib.sha256(files['report.json']).hexdigest() != pin['reportSha256']:
        raise ValueError('Reference report differs')
    ep = previous.endpoint(args.controller_config)
    task = previous.native.call(ep, dict(method='get_task', taskId=pin['taskId']))['task']
    info = previous.native.call(ep, dict(method='info'))['info']
    if task['status'] != 'succeeded' or task['exitCode'] != 0 or info['platform'] != 'windows' or info['activeTaskCount'] or info.get('maintenanceError') or info.get('journalRecoveryRequired') or info.get('recoveryCoverage', {}).get('complete') is not True:
        raise ValueError('Requires successful terminal reference and healthy idle worker')
    free = shutil.disk_usage(ROOT).free
    if free < 6*1024**3:
        raise ValueError('Need at least 6GiB Debian free')
    project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture' and p['allowDirectCommands'])
    root = PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/pin['run']
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime())+'-'+uuid.uuid4().hex[:8]
    prefix = args.probe if args.probe in {'desktop-release', 'build-profile-check', 'worker-idle'} else 'build-matrix'
    work = str(root/('tmp-'+prefix+'-'+run))
    helpers = str(root/('tmp-perf-script-'+run))
    report = Path('/mnt/hgfs/shared/WinBridge/run-speed')/run
    shared = str(PureWindowsPath('K:/VMs/shared/WinBridge/run-speed')/run)
    script_name = 'test-windows-'+prefix+'.ps1'
    extra_args = ' -InventoryOnly' if args.probe == 'source-inventory' else ''
    code = (ROOT/'scripts'/script_name).read_text()
    bundle = {script_name: code,
              'lib/validation-storage.ps1': (ROOT/'scripts/lib/validation-storage.ps1').read_text()}
    if args.probe == 'build-profile-check':
        bundle['lib/update-build-profile.ps1'] = (ROOT/'scripts/lib/update-build-profile.ps1').read_text()
    payload = base64.b64encode(gzip.compress(json.dumps(bundle).encode())).decode()
    cache = str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/pin['cacheRun']/'test-build-cache')
    manifest_sha = pin['source']['manifestSha256']
    candidate_source = None
    if args.probe == 'worker-idle':
        if not args.source_ready:
            raise ValueError('Worker idle probe requires --source-ready for the clean candidate ZIP')
        import zipfile
        ready = json.loads(args.source_ready.read_text())
        archive_path = args.source_ready.parent / ready['archive']
        if archive_path.name != ready['archive'] or ready['dirty'] or hashlib.sha256(archive_path.read_bytes()).hexdigest() != ready['sha256']:
            raise ValueError('Clean candidate archive pin differs')
        with zipfile.ZipFile(archive_path) as archive_file:
            manifest = archive_file.read('winbridge/winbridge-source-manifest.json')
        manifest_sha = hashlib.sha256(manifest).hexdigest()
        candidate_source = dict(ready, manifestSha256=manifest_sha)
        proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
        baseline = proof['cases'][0]['replacement']['actualRetainedFiles'][0]
        if not baseline['path'].endswith('\\displaced-runtime\\winbridge-worker.exe'):
            raise ValueError('Pinned retained baseline worker required')
        archive = str(PureWindowsPath('K:/VMs/shared/WinBridge/sources')/ready['archive'])
        extra_args = (' -SourceArchivePath '+previous.native.ps_string(archive)
                      +' -ArchiveSha256 '+previous.native.ps_string(ready['sha256'])
                      +' -BaselineExecutable '+previous.native.ps_string(baseline['path'])
                      +' -BaselineSha256 '+previous.native.ps_string(baseline['sha256']))
    if args.probe == 'desktop-release':
        proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
        helper = proof['cases'][0]['replacement']['actualRetainedFiles'][3]
        if not helper['path'].endswith('\\winbridge-update.exe'):
            raise ValueError('Pinned retained updater required')
        archive = str(PureWindowsPath('K:/VMs/shared/WinBridge/sources')/pin['source']['archiveName'])
        cache = str(PureWindowsPath(cache).parent)
        extra_args = (' -SourceArchivePath '+previous.native.ps_string(archive)
                      +' -ArchiveSha256 '+previous.native.ps_string(pin['source']['archiveSha256'])
                      +' -VerifierExecutable '+previous.native.ps_string(helper['path'])
                      +' -VerifierSha256 '+previous.native.ps_string(helper['sha256']))
    ps = previous.native.ps_string
    script = f'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$PSNativeCommandUseErrorActionPreference=$false
$source={ps(str(root/'winbridge'))}
$helpers={ps(helpers)}
if(Test-Path -LiteralPath $helpers){{throw 'Existing probe helpers preserved'}}
$stream=[IO.MemoryStream]::new([Convert]::FromBase64String({ps(payload)}))
$gzip=[IO.Compression.GzipStream]::new($stream,[IO.Compression.CompressionMode]::Decompress)
$reader=[IO.StreamReader]::new($gzip)
try{{$bundle=$reader.ReadToEnd()|ConvertFrom-Json -AsHashtable}}finally{{$reader.Dispose()}}
New-Item -ItemType Directory -Path (Join-Path $helpers 'lib')|Out-Null
$path=Join-Path $helpers {ps(script_name)}
foreach($name in $bundle.Keys){{
 $handle=[IO.File]::Open((Join-Path $helpers $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{{$bytes=[Text.UTF8Encoding]::new($false).GetBytes($bundle[$name]);$handle.Write($bytes,0,$bytes.Length)}}finally{{$handle.Dispose()}}
}}
foreach($name in @('session-runtime.ps1','lib/build-cache.ps1')){{[IO.File]::Copy((Join-Path $source ('scripts/'+$name)),(Join-Path $helpers $name),$false)}}
& pwsh.exe -NoProfile -NonInteractive -File $path -SourceDirectory $source -ManifestSha256 {ps(manifest_sha)} -CacheDirectory {ps(cache)} -WorkDirectory {ps(work)}{extra_args}
$exitCode=$LASTEXITCODE
$work={ps(work)}
if(Test-Path -LiteralPath $work){{
 . (Join-Path $helpers 'lib/build-cache.ps1')
 . (Join-Path $helpers 'lib/validation-storage.ps1')
 Write-WorkflowStorageGuide -RunDirectory $work
}}
$shared={ps(shared)}
if(Test-Path -LiteralPath (Join-Path $work 'report.ready.json')){{
 New-Item -ItemType Directory -Path $shared|Out-Null
 $ready=Get-Content -Raw -LiteralPath (Join-Path $work 'report.ready.json')|ConvertFrom-Json
 foreach($file in $ready.files){{
  if($file.path -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$'){{throw 'Unsafe ready filename'}}
  [IO.File]::Copy((Join-Path $work $file.path),(Join-Path $shared $file.path),$false)
 }}
 [IO.File]::Copy((Join-Path $work 'report.ready.json'),(Join-Path $shared 'report.ready.json'),$false)
}}
exit $exitCode
'''
    encoded = base64.b64encode(script.encode('utf-16le')).decode()
    if len(encoded) > 30000:
        raise ValueError('Command exceeds bounded Windows argv size')
    request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId='snapshot-fixture', idempotencyKey='perf-'+args.probe+'-'+run,
                   timeoutMs=3600000, command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
                   arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand', encoded]))
    meta = dict(run=run, taskId=None, request=request, reference=str(args.reference.resolve()), referenceReportSha256=pin['reportSha256'],
                controllerConfig=str(args.controller_config.resolve()), reportDirectory=str(report), windowsWorkDirectory=work,
                source=pin['source'], candidateSource=candidate_source, scriptSha256=hashlib.sha256(code.encode()).hexdigest(),
                bundleSha256={name: hashlib.sha256(text.encode()).hexdigest() for name, text in bundle.items()}, debianFreeBytes=free)
    local = ROOT/'.winbridge/run-speed'/run
    local.mkdir(parents=True)
    path = local/'submission.json'
    path.write_text(json.dumps(meta, indent=2))
    task = previous.native.call(ep, dict(method='submit', request=request))['task']
    meta['taskId'] = task['taskId']
    path.write_text(json.dumps(meta, indent=2))
    print(json.dumps(dict(taskId=task['taskId'], metadata=str(path), report=str(report))))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--observe', type=Path)
    parser.add_argument('--probe', choices=['build-matrix','source-inventory','desktop-release','build-profile-check','worker-idle'])
    parser.add_argument('--source-ready', type=Path)
    parser.add_argument('--reference', type=Path)
    parser.add_argument('--controller-config', type=Path)
    args = parser.parse_args()
    if args.observe:
        previous.observe(args.observe)
    elif all((args.probe, args.reference, args.controller_config)):
        submit(args)
    else:
        parser.error('Supply --observe or all submit parameters')
