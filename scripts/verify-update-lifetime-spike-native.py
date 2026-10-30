#!/usr/bin/env python3
"""Audit retained OS-spike results twice without Windows RPC or process adoption."""
import argparse
import hashlib
import importlib.util
from pathlib import Path, PureWindowsPath
import re

ROOT=Path(__file__).resolve().parents[1]


def load(name,file):
    spec=importlib.util.spec_from_file_location(name,ROOT/'scripts'/file)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module


def verify(run_info,archive,*,bootstrap=False,keeper_probe=False,bootstrap_files=False,keeper_launch=False):
    if sum((bootstrap,keeper_probe,bootstrap_files,keeper_launch))>1:raise ValueError("Choose one native profile")
    c=load('keeper_proof','update-keeper-launch-primitives-proof.py' if keeper_launch else 'update-bootstrap-files-proof.py' if bootstrap_files else ('update-keeper-probe-proof.py' if keeper_probe else 'update-lifetime-spike-proof.py'))
    health=load('keeper_pin','validate-update-health-actor-native.py')
    reader=load('keeper_ready','summarize-update-timings.py')
    local=run_info.resolve().parent;meta=c.decode(run_info.read_bytes());source=health.archive_pin(archive)
    namespace='bootstrap-files' if bootstrap_files else ('keeper-probe' if keeper_probe else ('bootstrap-exclusion' if bootstrap else 'lifetime-spike'))
    if keeper_launch:
        namespace='keeper-launch-primitives'
        c.need(meta.get('keeperLaunchPrimitives') is True,'Explicit keeper launch primitives profile required')
    if bootstrap_files:c.need(meta.get('bootstrapFiles') is True,'Explicit bootstrap files profile required')
    if keeper_probe:c.need(meta.get('keeperProbe') is True,'Explicit keeper probe profile required')
    if bootstrap:c.need(meta.get('bootstrapExclusion') is True,'Explicit bootstrap exclusion profile required')
    c.need(meta['source']==source and re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}',meta['run']),'Saved source/run changed')
    c.need(Path(meta['reportDirectory'])==Path('/mnt/hgfs/shared/WinBridge/workflow-validation')/meta['run']/'result','Wrong report namespace')
    raw=(local/'request.json').read_bytes();c.need(hashlib.sha256(raw).hexdigest()==meta['requestSha256'],'Request changed')
    request=c.decode(raw);ack=c.decode((local/'acceptance.json').read_bytes());task=c.decode((local/'terminal.json').read_bytes())
    before,after=(c.decode((local/name).read_bytes()) for name in ('info-before.json','info-after.json'))
    context=before['fencedRequests']['context']
    c.need(request['requestContractVersion']==ack['requestContractVersion']==2 and request['context']==ack['context']==context
        and ack['outcome']=='responded' and ack['response']['kind']=='task' and task['taskId']==ack['response']['task']['taskId']
        and task['status']=='succeeded' and type(task['exitCode'])is int and task['exitCode']==0
        and task['submission']==request['request']['request']
        and task['submission']['idempotencyKey']=='native-'+namespace+'-'+meta['run']
        and task['submission']['projectId']=='snapshot-fixture','Wrong terminal task/context')
    project=next(p for p in before['projects']if p['id']=='snapshot-fixture')
    base=str(PureWindowsPath(project['workspaceRoot'])/'workflow-validation'/('tmp-'+namespace+'-'+meta['run'])/'cases')
    c.need(meta['windowsBase']==base,'Wrong case namespace')
    files=reader.verified_files(Path(meta['reportDirectory']));report=c.decode(files['report.json'])
    c.need(report['kind']==('keeper_launch_primitives_native_gate' if keeper_launch else 'bootstrap_files_native_gate' if bootstrap_files else ('keeper_probe_native_gate' if keeper_probe else ('activation_bootstrap_exclusion_native_gate' if bootstrap else 'activation_lifetime_keeper_native_gate'))) and report['passed']is True and report['error']is None
        and report['sourceCommit']==source['sourceCommit'] and report['sourceArchiveSha256']==source['archiveSha256']
        and report['sourceManifestSha256']==source['manifestSha256'] and report['platform']=='windows'
        and all(report[k]is False for k in ('runtimeApplied','installedRuntimeApplied','windowsOpened','networkStarted')),'Wrong native report')
    c.need(len(report['commands'])==(2 if bootstrap_files or keeper_launch else (1 if keeper_probe else (3 if bootstrap else 2))),'Wrong native command count')
    if bootstrap:
        command=report['commands'][0]
        load('bootstrap_selector','update-bootstrap-exclusion-proof.py').selection(command,files[command['log']].decode('utf-8-sig'))
    if bootstrap_files or keeper_launch:c.selections(report['commands'],files)
    if keeper_probe:
        c.selection(report['commands'][0],files['keeper-probe.log'].decode('utf-8-sig'))
    for command,mode in zip(report['commands'][1:] if bootstrap else report['commands'],() if keeper_probe or bootstrap_files or keeper_launch else c.MODES):
        c.need(command['name']==mode and command['log']==mode+'.log','Wrong command order/log')
        c.selection(command,files[command['log']].decode('utf-8-sig'),mode)
    result=c.audit(files,source,base)
    if bootstrap:result.update(bootstrapExclusionProved=True,keeperRuntimeProved=False)
    for info in (before,after):c.integer(info['activeTaskCount'])
    c.need(before['build']==after['build'] and before['generation']==after['generation']
        and after['fencedRequests']['context']==context and before['activeTaskCount']==after['activeTaskCount']==0
        and all(not i.get(k)for i in (before,after)for k in ('maintenanceError','journalRecoveryRequired')),
        'Installed worker changed or unhealthy')
    c.need(reader.verified_files(Path(meta['reportDirectory']))==files,'Report changed on repeated read')
    return dict(result,run=meta['run'],taskId=task['taskId'],filesVerifiedTwice=len(files),installedUnchanged=True,
        reportSha256=hashlib.sha256(files['report.json']).hexdigest(),
        proofSha256={mode:hashlib.sha256(files[mode+'-proof.json']).hexdigest() for mode in c.MODES},storage=report['storage'])


if __name__=='__main__':
    import json
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-info',required=True,type=Path);parser.add_argument('--source-archive',required=True,type=Path)
    args=parser.parse_args();print(json.dumps(verify(args.run_info,args.source_archive),indent=2))
