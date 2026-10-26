#!/usr/bin/env python3
"""Submit one isolated profile pilot, or observe its saved task without resubmission."""
import argparse
import base64
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import shutil
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


native = load('native', 'validate-workflow-native.py')
timings = load('timings', 'summarize-update-timings.py')


def endpoint(config):
    cfg = json.loads(config.read_text())
    return Path(cfg['stateDirectory']) / ('controller-'+cfg['instanceId']+'.sock')


def observe(meta_path):
    meta = json.loads(meta_path.read_text())
    ep = endpoint(Path(meta['controllerConfig']))
    if meta['taskId'] is None:
        raise ValueError('Submit outcome unknown; reconcile saved request with task list. Never resubmit blindly.')
    task = native.call(ep, dict(method='get_task', taskId=meta['taskId']))['task']
    chunks, sequence = [], 0
    while True:
        page = native.call(ep, dict(method='read_output', taskId=meta['taskId'], afterSequence=sequence))['chunks']
        if not page:
            break
        chunks.extend(page)
        sequence = page[-1]['sequence']
    output = b''.join(bytes(c['bytes']) for c in chunks if c['stream'] == 'stdout').decode('utf-8', errors='replace')
    if task['status'] not in {'queued', 'running'}:
        terminal = meta_path.with_name('terminal.json')
        if not terminal.exists():
            terminal.write_text(json.dumps(task, indent=2))
    print(json.dumps(dict(taskId=task['taskId'], status=task['status'], exitCode=task['exitCode'],
                         output=output[-4000:], report=meta['reportDirectory']), ensure_ascii=False, indent=2))


def submit(args):
    ep = endpoint(args.controller_config)
    info = native.call(ep, dict(method='info'))['info']
    if not (info['platform'] == 'windows' and not info['activeTaskCount'] and not info.get('maintenanceError')
            and not info.get('journalRecoveryRequired') and info.get('recoveryCoverage', {}).get('complete') is True):
        raise ValueError('Healthy idle installed worker required')
    owner = native.call(ep, dict(method='get_task', taskId=args.cache_owner_task))['task']
    if owner['status'] in {'queued', 'running'}:
        raise ValueError('Cache owner task still active')
    cache_files = timings.verified_files(args.cache_report)
    if hashlib.sha256(cache_files['report.json']).hexdigest() != args.cache_report_sha256:
        raise ValueError('Cache owner report changed')
    cache_report = json.loads(cache_files['report.json'].decode('utf-8-sig'))
    if not cache_report['commands'] or any(x['exitCode'] != 0 for x in cache_report['commands']):
        raise ValueError('Cache compiler commands did not succeed')
    archive_raw = args.archive.read_bytes()
    archive_hash = hashlib.sha256(archive_raw).hexdigest()
    with zipfile.ZipFile(args.archive) as package:
        manifest_raw = package.read('winbridge/winbridge-source-manifest.json')
        manifest = json.loads(manifest_raw)
    if manifest['dirty'] is not False:
        raise ValueError('Clean exported source required')
    manifest_hash = hashlib.sha256(manifest_raw).hexdigest()
    windows_archive = str(PureWindowsPath('K:/VMs/shared') / args.archive.relative_to('/mnt/hgfs/shared'))
    free = shutil.disk_usage(ROOT).free
    if free < 6*1024**3:
        raise ValueError('Less than 6GiB on Debian; stop new builds')
    project = next(p for p in info['projects'] if p['id'] == 'snapshot-fixture')
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime())+'-'+uuid.uuid4().hex[:8]
    local = str(PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / run)
    work = str(PureWindowsPath(local) / ('profile-v1-'+run))
    report = Path('/mnt/hgfs/shared/WinBridge/run-speed') / run
    peer_report = str(PureWindowsPath('K:/VMs/shared/WinBridge/run-speed') / run)
    meta_root = ROOT / '.winbridge/run-speed' / run
    meta_root.mkdir(parents=True)
    ps = native.ps_string
    script = f'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$PSNativeCommandUseErrorActionPreference=$false
$archive={ps(windows_archive)}
$local={ps(local)}
$work={ps(work)}
$report={ps(peer_report)}
$cache={ps(cache_report['buildCacheDirectory'])}
if(Test-Path -LiteralPath $local){{throw 'Existing isolated source preserved'}}
if(Test-Path -LiteralPath $report){{throw 'Existing shared report preserved'}}
if([IO.DriveInfo]::new([IO.Path]::GetPathRoot($local)).AvailableFreeSpace -lt 6GB){{throw 'Less than 6GiB before extraction'}}
if((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -cne {ps(archive_hash)}){{throw 'Archive changed'}}
New-Item -ItemType Directory -Path $local | Out-Null
$copy=Join-Path $local 'source.zip'
[IO.File]::Copy($archive,$copy,$false)
if((Get-FileHash -LiteralPath $copy).Hash.ToLowerInvariant() -cne {ps(archive_hash)}){{throw 'Copied archive changed'}}
Expand-Archive -LiteralPath $copy -DestinationPath $local
$source=Join-Path $local 'winbridge'
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $source 'scripts/test-windows-update-profile.ps1') -SourceDirectory $source -ManifestSha256 {ps(manifest_hash)} -WorkDirectory $work -CacheDirectory $cache -Execute
$code=$LASTEXITCODE
if(Test-Path -LiteralPath (Join-Path $work 'report.ready.json')){{
 New-Item -ItemType Directory -Path $report | Out-Null
 foreach($file in Get-ChildItem -LiteralPath $work -File | Where-Object {{$_.Name -cne 'report.ready.json'}}){{[IO.File]::Copy($file.FullName,(Join-Path $report $file.Name),$false)}}
 [IO.File]::Copy((Join-Path $work 'report.ready.json'),(Join-Path $report 'report.ready.json'),$false)
}}
exit $code
'''
    request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId='snapshot-fixture', idempotencyKey='profile-pilot-'+run,
                   timeoutMs=3600000, command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
                   arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()]))
    meta = dict(run=run, taskId=None, request=request, controllerConfig=str(args.controller_config.resolve()),
                archive=str(args.archive), archiveSha256=archive_hash, manifestSha256=manifest_hash, sourceCommit=manifest['sourceCommit'],
                reportDirectory=str(report), windowsWorkDirectory=work, debianFreeBytes=free, cacheOwnerTask=args.cache_owner_task,
                cacheReportSha256=args.cache_report_sha256, workerGeneration=info['generation'])
    meta_path = meta_root / 'submission.json'
    meta_path.write_text(json.dumps(meta, indent=2))
    task = native.call(ep, dict(method='submit', request=request))['task']
    meta['taskId'] = task['taskId']
    meta_path.write_text(json.dumps(meta, indent=2))
    print(json.dumps(dict(taskId=task['taskId'], metadata=str(meta_path), report=str(report), debianFreeBytes=free)))


def hotspots(pilot_meta_path):
    pilot_meta = json.loads(pilot_meta_path.read_text())
    ep = endpoint(Path(pilot_meta['controllerConfig']))
    task = native.call(ep, dict(method='get_task', taskId=pilot_meta['taskId']))['task']
    if task['status'] != 'succeeded' or task['exitCode'] != 0:
        raise ValueError('Require successful terminal pilot, never overlap its binaries/tests')
    comparison = load('comparison', 'verify-update-profile.py')
    comparison.verify(Path(pilot_meta['reportDirectory']))
    files = timings.verified_files(Path(pilot_meta['reportDirectory']))
    report_hash = hashlib.sha256(files['report.json']).hexdigest()
    info = native.call(ep, dict(method='info'))['info']
    if info['platform'] != 'windows' or info['activeTaskCount'] or info.get('maintenanceError') or info.get('journalRecoveryRequired') or info.get('recoveryCoverage', {}).get('complete') is not True:
        raise ValueError('Requires idle healthy worker')
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime())+'-'+uuid.uuid4().hex[:8]
    pilot = PureWindowsPath(pilot_meta['windowsWorkDirectory'])
    output = str(pilot.parent / ('hotspots-v1-'+run))
    helpers = str(pilot.parent / ('hotspots-script-'+run))
    shared = str(PureWindowsPath('K:/VMs/shared/WinBridge/run-speed') / run)
    report = Path('/mnt/hgfs/shared/WinBridge/run-speed') / run
    payload = {name: (ROOT / 'scripts' / name).read_text() for name in ['test-windows-update-profile-hotspots.ps1', 'session-runtime.ps1']}
    packed = base64.b64encode(gzip.compress(json.dumps(payload).encode())).decode()
    ps = native.ps_string
    script = f'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$PSNativeCommandUseErrorActionPreference=$false
$stream=[IO.MemoryStream]::new([Convert]::FromBase64String({ps(packed)}))
$gzip=[IO.Compression.GzipStream]::new($stream,[IO.Compression.CompressionMode]::Decompress)
$reader=[IO.StreamReader]::new($gzip)
try{{$files=$reader.ReadToEnd()|ConvertFrom-Json -AsHashtable}}finally{{$reader.Dispose()}}
$helpers={ps(helpers)}
if(Test-Path -LiteralPath $helpers){{throw 'Existing helpers preserved'}}
New-Item -ItemType Directory -Path $helpers|Out-Null
foreach($name in $files.Keys){{
 $handle=[IO.File]::Open((Join-Path $helpers $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{{$bytes=[Text.UTF8Encoding]::new($false).GetBytes($files[$name]);$handle.Write($bytes,0,$bytes.Length)}}finally{{$handle.Dispose()}}
}}
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $helpers 'test-windows-update-profile-hotspots.ps1') -PilotDirectory {ps(str(pilot))} -ReportSha256 {ps(report_hash)} -ReportDirectory {ps(output)}
$code=$LASTEXITCODE
$output={ps(output)}
$shared={ps(shared)}
if(Test-Path -LiteralPath (Join-Path $output 'report.ready.json')){{
 New-Item -ItemType Directory -Path $shared|Out-Null
 foreach($file in Get-ChildItem -LiteralPath $output -File|Where-Object {{$_.Name -cne 'report.ready.json'}}){{[IO.File]::Copy($file.FullName,(Join-Path $shared $file.Name),$false)}}
 [IO.File]::Copy((Join-Path $output 'report.ready.json'),(Join-Path $shared 'report.ready.json'),$false)
}}
exit $code
'''
    request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId='snapshot-fixture', idempotencyKey='profile-hotspots-'+run,
                   timeoutMs=900000, command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
                   arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()]))
    meta = dict(run=run, taskId=None, request=request, controllerConfig=pilot_meta['controllerConfig'], reportDirectory=str(report),
                pilotMetadata=str(pilot_meta_path.resolve()), pilotReportSha256=report_hash,
                scriptSha256=hashlib.sha256(payload['test-windows-update-profile-hotspots.ps1'].encode()).hexdigest())
    local = ROOT / '.winbridge/run-speed' / run
    local.mkdir(parents=True)
    path = local / 'submission.json'
    path.write_text(json.dumps(meta, indent=2))
    task = native.call(ep, dict(method='submit', request=request))['task']
    meta['taskId'] = task['taskId']
    path.write_text(json.dumps(meta, indent=2))
    print(json.dumps(dict(taskId=task['taskId'], metadata=str(path), report=str(report))))


def followup(pilot_meta_path, spec_path):
    original = json.loads(pilot_meta_path.read_text())
    ep = endpoint(Path(original['controllerConfig']))
    task = native.call(ep, dict(method='get_task', taskId=original['taskId']))['task']
    info = native.call(ep, dict(method='info'))['info']
    if task['status'] != 'succeeded' or task['exitCode'] != 0 or info['activeTaskCount'] or info.get('maintenanceError') or info.get('journalRecoveryRequired') or info.get('recoveryCoverage', {}).get('complete') is not True:
        raise ValueError('Requires terminal successful original pilot and healthy idle worker')
    load('comparison', 'verify-update-profile.py').verify(Path(original['reportDirectory']))
    free = shutil.disk_usage(ROOT).free
    if free < 6*1024**3:
        raise ValueError('Less than 6GiB on Debian')
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime())+'-'+uuid.uuid4().hex[:8]
    root = PureWindowsPath(original['windowsWorkDirectory']).parent
    source = str(root / 'winbridge')
    output = str(root / ('profile-v1-'+run))
    helpers = str(root / ('profile-script-'+run))
    report = Path('/mnt/hgfs/shared/WinBridge/run-speed') / run
    shared = str(PureWindowsPath('K:/VMs/shared/WinBridge/run-speed') / run)
    original_files = timings.verified_files(Path(original['reportDirectory']))
    cache = json.loads(original_files['report.json'].decode('utf-8-sig'))['builds'][0]['space'][2]['path']
    payload = {'test-windows-update-profile.ps1': (ROOT/'scripts/test-windows-update-profile.ps1').read_text(), 'spec.json': spec_path.read_text()}
    packed = base64.b64encode(gzip.compress(json.dumps(payload).encode())).decode()
    ps = native.ps_string
    script = f'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$PSNativeCommandUseErrorActionPreference=$false
$source={ps(source)}
$helpers={ps(helpers)}
if(Test-Path -LiteralPath $helpers){{throw 'Existing profile helpers preserved'}}
$stream=[IO.MemoryStream]::new([Convert]::FromBase64String({ps(packed)}))
$gzip=[IO.Compression.GzipStream]::new($stream,[IO.Compression.CompressionMode]::Decompress)
$reader=[IO.StreamReader]::new($gzip)
try{{$files=$reader.ReadToEnd()|ConvertFrom-Json -AsHashtable}}finally{{$reader.Dispose()}}
New-Item -ItemType Directory -Path (Join-Path $helpers 'lib')|Out-Null
foreach($name in $files.Keys){{
 $handle=[IO.File]::Open((Join-Path $helpers $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{{$bytes=[Text.UTF8Encoding]::new($false).GetBytes($files[$name]);$handle.Write($bytes,0,$bytes.Length)}}finally{{$handle.Dispose()}}
}}
foreach($name in @('session-runtime.ps1','lib/build-cache.ps1')){{[IO.File]::Copy((Join-Path $source ('scripts/'+$name)),(Join-Path $helpers $name),$false)}}
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $helpers 'test-windows-update-profile.ps1') -SourceDirectory $source -ManifestSha256 {ps(original['manifestSha256'])} -WorkDirectory {ps(output)} -CacheDirectory {ps(cache)} -SpecPath (Join-Path $helpers 'spec.json') -Execute
$code=$LASTEXITCODE
$output={ps(output)}
$shared={ps(shared)}
if(Test-Path -LiteralPath (Join-Path $output 'report.ready.json')){{
 New-Item -ItemType Directory -Path $shared|Out-Null
 foreach($file in Get-ChildItem -LiteralPath $output -File|Where-Object {{$_.Name -cne 'report.ready.json'}}){{[IO.File]::Copy($file.FullName,(Join-Path $shared $file.Name),$false)}}
 [IO.File]::Copy((Join-Path $output 'report.ready.json'),(Join-Path $shared 'report.ready.json'),$false)
}}
exit $code
'''
    encoded = base64.b64encode(script.encode('utf-16le')).decode()
    if len(encoded) > 30000:
        raise ValueError('Encoded command too long; do not submit')
    request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId='snapshot-fixture', idempotencyKey='profile-followup-'+run,
                   timeoutMs=3600000, command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
                   arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand', encoded]))
    meta = dict(run=run, taskId=None, request=request, controllerConfig=original['controllerConfig'], reportDirectory=str(report),
                originalPilot=str(pilot_meta_path.resolve()), sourceCommit=original['sourceCommit'], manifestSha256=original['manifestSha256'],
                windowsWorkDirectory=output, specPath=str(spec_path.resolve()), specSha256=hashlib.sha256(spec_path.read_bytes()).hexdigest(),
                runnerSha256=hashlib.sha256(payload['test-windows-update-profile.ps1'].encode()).hexdigest(), debianFreeBytes=free)
    local = ROOT / '.winbridge/run-speed' / run
    local.mkdir(parents=True)
    path = local / 'submission.json'
    path.write_text(json.dumps(meta, indent=2))
    task = native.call(ep, dict(method='submit', request=request))['task']
    meta['taskId'] = task['taskId']
    path.write_text(json.dumps(meta, indent=2))
    print(json.dumps(dict(taskId=task['taskId'], metadata=str(path), report=str(report))))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--observe', type=Path)
    parser.add_argument('--hotspots', type=Path)
    parser.add_argument('--followup', type=Path)
    parser.add_argument('--spec', type=Path)
    parser.add_argument('--archive', type=Path)
    parser.add_argument('--controller-config', type=Path)
    parser.add_argument('--cache-owner-task')
    parser.add_argument('--cache-report', type=Path)
    parser.add_argument('--cache-report-sha256')
    args = parser.parse_args()
    if args.observe:
        observe(args.observe)
    elif args.hotspots:
        hotspots(args.hotspots)
    elif args.followup and args.spec:
        followup(args.followup, args.spec)
    elif all((args.archive, args.controller_config, args.cache_owner_task, args.cache_report, args.cache_report_sha256)):
        submit(args)
    else:
        parser.error('Supply --observe or all submit parameters')
