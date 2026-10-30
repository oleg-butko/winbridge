#!/usr/bin/env python3
"""One pinned local-process OS spike via installed worker. Never install/retry."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import shutil
import subprocess
import sys
import time
import uuid

ROOT=Path(__file__).resolve().parents[1]


def load(name,file):
    spec=importlib.util.spec_from_file_location(name,ROOT/'scripts'/file)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module


def main(bootstrap=False, keeper_probe=False, bootstrap_files=False):
    if sum((bootstrap,keeper_probe,bootstrap_files))>1:raise ValueError("Choose one explicit native profile")
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-archive',required=True,type=Path)
    parser.add_argument('--cache-run',required=True)
    parser.add_argument('--controller-config',required=True,type=Path)
    args=parser.parse_args()
    namespace="bootstrap-files" if bootstrap_files else ("keeper-probe" if keeper_probe else ("bootstrap-exclusion" if bootstrap else "lifetime-spike"))
    checks=load('lifetime_checks','update-lifetime-spike-proof.py')
    native=load('lifetime_rpc','validate-workflow-native.py')
    health=load('lifetime_source','validate-update-health-actor-native.py')
    common=load('lifetime_submit','run-update-service-main-native.py')
    pin=health.archive_pin(args.source_archive)
    checks.need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}',args.cache_run),'Explicit retained cache run required')
    free=shutil.disk_usage(ROOT).free;checks.need(free>=6*1024**3,'6 GiB Debian free required')
    cfg=json.loads(args.controller_config.read_text())
    ep=Path(cfg['stateDirectory'])/('controller-'+cfg['instanceId']+'.sock')
    before=native.call(ep,dict(method='info'))['info']
    checks.need(before['platform']=='windows' and before['activeTaskCount']==0
                and not before.get('maintenanceError') and not before.get('journalRecoveryRequired'),'Healthy idle installed worker required')
    project=next(p for p in before['projects'] if p['id']=='snapshot-fixture')
    run=time.strftime('%Y%m%d-%H%M%S',time.gmtime())+'-'+uuid.uuid4().hex[:8]
    local=ROOT/'.winbridge/update-preparation'/(namespace+'-'+run);local.mkdir(parents=True)
    report=Path('/mnt/hgfs/shared/WinBridge/workflow-validation')/run
    report.mkdir()
    work=PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/('tmp-'+namespace+'-'+run)
    cache=PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/args.cache_run
    archive=PureWindowsPath('K:/VMs/shared')/args.source_archive.resolve().relative_to('/mnt/hgfs/shared')
    peer=PureWindowsPath('K:/VMs/shared/WinBridge/workflow-validation')/run/'result'
    ps=native.ps_string
    script=f'''$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'
$work={ps(str(work))};$archive={ps(str(archive))}
if(Test-Path -LiteralPath $work){{throw 'Existing lifetime fixture preserved'}}
if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(pin['archiveSha256'])}){{throw 'Source archive changed'}}
if([IO.DriveInfo]::new('K:\\').AvailableFreeSpace -lt 6GB){{throw 'Less than 6 GiB; no build'}}
New-Item -ItemType Directory -Path $work|Out-Null
[IO.File]::Copy($archive,(Join-Path $work 'source.zip'),$false)
if((Get-FileHash -LiteralPath (Join-Path $work 'source.zip') -Algorithm SHA256).Hash.ToLowerInvariant() -cne {ps(pin['archiveSha256'])}){{throw 'Local archive copy differs before extraction'}}
Expand-Archive -LiteralPath (Join-Path $work 'source.zip') -DestinationPath $work
& pwsh.exe -NoProfile -NonInteractive -File (Join-Path $work 'winbridge/scripts/test-update-lifetime-spike.ps1') -SourceDirectory (Join-Path $work 'winbridge') -SourceArchiveSha256 {ps(pin['archiveSha256'])} -ValidationCacheDirectory {ps(str(cache))} -ReportDirectory {ps(str(peer))}{' -BootstrapFiles -SourceArchiveName '+ps(pin['archiveName']) if bootstrap_files else (' -KeeperProbe' if keeper_probe else (' -BootstrapExclusion' if bootstrap else ''))}
exit $LASTEXITCODE
'''
    context=before['fencedRequests']['context']
    submission=dict(protocolVersion=1,requestId=str(uuid.uuid4()),projectId=project['id'],
        idempotencyKey='native-'+namespace+'-'+run,timeoutMs=900000,
        command=dict(kind='direct',executable='pwsh.exe',arguments=['-NoProfile','-NonInteractive','-Command',script],workingDirectory='.'))
    request=dict(requestContractVersion=2,context=context,request=dict(method='submit',request=submission))
    common.write_new(local/'request.json',request);common.write_new(local/'info-before.json',before)
    common.write_new(local/'run-info.json',dict(run=run,source=pin,reportDirectory=str(report/'result'),
        windowsBase=str(work/'cases'),debianFreeBytesBefore=free,
        requestSha256=hashlib.sha256((local/'request.json').read_bytes()).hexdigest(),**({'bootstrapFiles':True} if bootstrap_files else ({'keeperProbe':True} if keeper_probe else ({'bootstrapExclusion':True} if bootstrap else {})))))
    # The same saved context/key is the only continuation source on unknown ACK.
    reply=common.submit_fenced(native,ep,request);common.write_new(local/'acceptance.json',reply)
    checks.need(reply.get('requestContractVersion')==2 and reply.get('context')==context
                and reply.get('outcome')=='responded' and reply['response']['kind']=='task',
                'Acceptance unknown/rejected; inspect saved context/key, never resubmit')
    task=reply['response']['task'];print(json.dumps(dict(taskId=task['taskId'],runInfo=str(local/'run-info.json'))),flush=True)
    cursor=0;deadline=time.monotonic()+submission['timeoutMs']/1000+60
    with(local/'output.jsonl').open('x')as output:
        while True:
            task=native.call(ep,dict(method='get_task',taskId=task['taskId']))['task']
            while True:
                chunks=native.call(ep,dict(method='read_output',taskId=task['taskId'],afterSequence=cursor))['chunks']
                if not chunks:break
                for chunk in chunks:
                    output.write(json.dumps(chunk)+'\n');print(bytes(chunk['bytes']).decode('utf-8',errors='replace'),end='',flush=True)
                output.flush();cursor=chunks[-1]['sequence']
            if task['status']not in ('queued','running'):break
            checks.need(time.monotonic()<deadline,'Observation deadline; inspect this same task, no replay')
            time.sleep(1)
    common.write_new(local/'terminal.json',task)
    after=native.call(ep,dict(method='info'))['info'];common.write_new(local/'info-after.json',after)
    checks.need(task['status']=='succeeded' and task['exitCode']==0,'OS spike failed; retain original artifacts and diagnose')
    result=subprocess.run([sys.executable,str(ROOT/'scripts'/('verify-update-bootstrap-files-native.py' if bootstrap_files else ('verify-update-keeper-probe-native.py' if keeper_probe else ('verify-update-bootstrap-exclusion-native.py' if bootstrap else 'verify-update-lifetime-spike-native.py')))),
        '--run-info',str(local/'run-info.json'),'--source-archive',str(args.source_archive)],capture_output=True,text=True)
    (local/'independent-audit.stdout').write_text(result.stdout);(local/'independent-audit.stderr').write_text(result.stderr)
    checks.need(result.returncode==0,'Independent OS spike audit refused; re-read retained evidence, no Windows replay')
    print(result.stdout,flush=True)


if __name__=='__main__':main()
