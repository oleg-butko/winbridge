"""Strict proof of the native peer/Job observation unit. Never a launch grant."""
import copy
import importlib.util
from pathlib import Path
import re
s=importlib.util.spec_from_file_location('probe_os_evidence',Path(__file__).with_name('update-lifetime-spike-proof.py'));old=importlib.util.module_from_spec(s);s.loader.exec_module(old)
need,decode,integer,process=old.need,old.decode,old.integer,old.process
MODES=('keeper-probe',)
TEST='supervisor::keeper_probe::tests::native::original_child_probe_uses_real_peer_and_exact_handles'
SCOPE='real OS peer/Job/capture; synthetic role history, no updater workflow'
def selection(command,log):
    need(command['name']=='keeper-probe' and command['log']=='keeper-probe.log' and command['executable']=='cargo' and type(command['exitCode']) is int and command['exitCode']==0 and command['arguments']==['test','-p','winbridge-update','--features','bundled-sqlite','--lib','--locked','--offline','--','--exact',TEST,'--ignored','--test-threads=1','--nocapture'],'Wrong native probe command')
    summaries=re.findall(r'^test result: ([^\r\n]+)',log,re.M)
    need(len(summaries)==1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;') and len(re.findall(r'^test '+re.escape(TEST)+r' \.\.\.',log,re.M))==1 and re.search(r'^running 1 test\s*$',log,re.M),'Probe unit did not run exactly once')
def trace(log):
    values=re.findall(r'WINBRIDGE_PROBE_SERVER=(\{[^\r\n]+\})',log)
    need(len(values)==1,'One real server process trace required')
    value=decode(values[0]);need(set(value)=={'keeper','worker'},'Unknown process trace')
    return value

def verify(p,source,expected):
    fields={'schemaVersion','kind','platform','passed','build','scope','syntheticRoleHistory','keeper','worker','client','wrongPeerRejected','observedResponses','childExitCode','originalJobEmpty','readersFinished','runtimeApplied','installedRuntimeApplied','windowsOpened','networkUsed'}
    need(set(p)==fields,'Unknown/missing native probe proof fields')
    integer(p['schemaVersion'],1,1);integer(p['observedResponses'],1,1);integer(p['childExitCode'],0,0)
    need(p['kind']=='keeper_probe_native_unit_proof' and p['platform']=='windows' and p['scope']==SCOPE,'Another probe scope')
    need(all(p[k] is True for k in ('passed','syntheticRoleHistory','wrongPeerRejected','originalJobEmpty','readersFinished')) and all(p[k] is False for k in ('runtimeApplied','installedRuntimeApplied','windowsOpened','networkUsed')),'Wrong probe result/effect claims')
    b=p['build'];need(set(b)=={'version','buildId','sourceCommit','sourceManifestSha256','sourceDirty'},'Unknown build identity fields')
    need(all(type(b[k]) is str and 0<len(b[k])<=128 and not any(ord(c)<32 for c in b[k]) for k in ('version','buildId')),'Invalid producer identity')
    need(b['sourceCommit']==source['sourceCommit'] and b['sourceManifestSha256']==source['manifestSha256'] and b['sourceDirty'] is False,'Native producer differs from pinned clean source')
    parent,child=process(p['keeper']),process(p['worker'])
    need(parent[0]!=child[0] and int(parent[1])<int(child[1]) and parent[2:]==child[2:],'Invalid actual keeper/child identities')
    need(p['keeper']==expected['keeper'] and p['worker']==expected['worker'],'Proof processes detached from separately captured OS server trace')
    client=p['client'];need(set(client)=={'keeper','worker','changedBindingRejected','duplicateFieldsRejected','exactCaptureObserved','captureExpired'},'Unknown client fields')
    need(client['keeper']==p['keeper'] and client['worker']==p['worker'] and all(client[k] is True for k in ('changedBindingRejected','duplicateFieldsRejected','exactCaptureObserved','captureExpired')),'Actual client did not verify binding/strict decoder/exact handles/deadline')

def audit(files,source,_base):
    log=files['keeper-probe.log'].decode('utf-8-sig');expected=trace(log)
    p=decode(files['keeper-probe-proof.json']);verify(p,source,expected)
    changes=[('schemaVersion',True),('kind','keeper_runtime_proof'),('scope','complete updater'),('syntheticRoleHistory',False),('passed',False),('wrongPeerRejected',False),('observedResponses',True),('observedResponses',2),('childExitCode',True),('childExitCode',1),('originalJobEmpty',False),('readersFinished',False),('runtimeApplied',True),('installedRuntimeApplied',True),('networkUsed',True),('windowsOpened',True),('build/sourceDirty',True),('build/sourceCommit','0'*40),('keeper/birth','0'),('worker/hostId','0'*64),('client/captureExpired',False),('client/exactCaptureObserved',False),('client/duplicateFieldsRejected',False),('client/changedBindingRejected',False)]
    for path,value in changes:
        bad=copy.deepcopy(p);parts=path.split('/');n=bad
        for part in parts[:-1]:n=n[part]
        need(n[parts[-1]]!=value or type(n[parts[-1]])!=type(value),'No-op probe negative '+path);n[parts[-1]]=value
        try:verify(bad,source,expected)
        except (ValueError,KeyError,TypeError):continue
        raise ValueError('Native probe accepted mutation '+path)
    bad=copy.deepcopy(p)
    for node in (bad,bad['client']):node['worker']['processId']+=4
    try:verify(bad,source,expected)
    except ValueError:pass
    else:raise ValueError('Correlated process rebinding accepted')
    return dict(passed=True,scope=SCOPE,negativeAudits=len(changes)+1,keeperRuntimeProved=False,bootstrapCreationProved=False)
