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
    work = str(root/('build-matrix-'+run))
    helpers = str(root/('perf-script-'+run))
    report = Path('/mnt/hgfs/shared/WinBridge/run-speed')/run
    shared = str(PureWindowsPath('K:/VMs/shared/WinBridge/run-speed')/run)
    script_name = 'test-windows-build-matrix.ps1'
    extra_args = ' -InventoryOnly' if args.probe == 'source-inventory' else ''
    code = (ROOT/'scripts'/script_name).read_text()
    payload = base64.b64encode(gzip.compress(code.encode())).decode()
    cache = str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/pin['cacheRun']/'test-build-cache')
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
try{{$code=$reader.ReadToEnd()}}finally{{$reader.Dispose()}}
New-Item -ItemType Directory -Path (Join-Path $helpers 'lib')|Out-Null
$path=Join-Path $helpers {ps(script_name)}
$handle=[IO.File]::Open($path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
try{{$bytes=[Text.UTF8Encoding]::new($false).GetBytes($code);$handle.Write($bytes,0,$bytes.Length)}}finally{{$handle.Dispose()}}
foreach($name in @('session-runtime.ps1','lib/build-cache.ps1')){{[IO.File]::Copy((Join-Path $source ('scripts/'+$name)),(Join-Path $helpers $name),$false)}}
& pwsh.exe -NoProfile -NonInteractive -File $path -SourceDirectory $source -ManifestSha256 {ps(pin['source']['manifestSha256'])} -CacheDirectory {ps(cache)} -WorkDirectory {ps(work)}{extra_args}
$exitCode=$LASTEXITCODE
$work={ps(work)}
$shared={ps(shared)}
if(Test-Path -LiteralPath (Join-Path $work 'report.ready.json')){{
 New-Item -ItemType Directory -Path $shared|Out-Null
 foreach($file in Get-ChildItem -LiteralPath $work -File|Where-Object {{$_.Name -cne 'report.ready.json'}}){{[IO.File]::Copy($file.FullName,(Join-Path $shared $file.Name),$false)}}
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
                source=pin['source'], scriptSha256=hashlib.sha256(code.encode()).hexdigest(), debianFreeBytes=free)
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
    parser.add_argument('--probe', choices=['build-matrix','source-inventory'])
    parser.add_argument('--reference', type=Path)
    parser.add_argument('--controller-config', type=Path)
    args = parser.parse_args()
    if args.observe:
        previous.observe(args.observe)
    elif all((args.probe, args.reference, args.controller_config)):
        submit(args)
    else:
        parser.error('Supply --observe or all submit parameters')
