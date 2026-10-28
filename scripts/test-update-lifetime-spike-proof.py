"""Synthetic corruption tests for the observer; never counted as native proof."""
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('keeper_proof',Path(__file__).with_name('update-lifetime-spike-proof.py'))
c=importlib.util.module_from_spec(spec);spec.loader.exec_module(c)
SOURCE=dict(sourceCommit='a'*40,manifestSha256='b'*64)
BASE=r'K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\fixture\cases'


def fixture(mode,task_pid=20,next_pid=12):
    root='\\\\?\\'+BASE+'\\'+mode;scope=hashlib.sha256(root.encode()).hexdigest()
    owner='00000000-0000-4000-8000-'+scope[:12]
    meta=dict(schemaVersion=1,mode=mode,root=root,scopeId=scope,ownerId=owner)
    def process(pid,birth):return dict(processId=pid,birth=str(100000000+birth),hostId='d'*64,sessionId=1)
    driver=process(1,1);keeper=process(10,10);worker=process(11,11);nxt=process(next_pid,12);nw=process(13,13)
    task=process(task_pid,20);leaf=process(21,21)
    def job(p,suffix):return dict(jobName='Global\\WinBridge-LifetimeSpike-'+scope+'-'+suffix,
        processId=p['processId'],creationTime=int(p['birth']),hostId=p['hostId'],sessionId=p['sessionId'])
    domain=dict(jobName='Global\\WinBridge-Domain-'+hashlib.sha256((root+'|session:1|owner:'+owner).encode()).hexdigest(),
                hostId='d'*64,sessionId=1,ownerId=owner)
    old_worker=dict(process=worker,domain=domain);old=dict(process=keeper,worker=old_worker,job=job(worker,'old'),limits=8192)
    cancelled=dict(process=task,leaf=leaf,taskJob=job(task,'task'),domain=domain,domainLimits=8192,taskLimits=8192,rootExited=True,leafExited=True,empty=True)
    nodes={'metadata.json':meta,'old-keeper.json':old,'old-worker.json':old_worker,'task-domain.json':domain,
           'task.json':dict(process=task,leaf=leaf),'task-leaf.json':dict(process=leaf),'cancelled-task.json':cancelled}
    build=dict(version='0.2.0',sourceCommit=SOURCE['sourceCommit'],sourceDirty=False,sourceManifestSha256=SOURCE['manifestSha256'],buildId='1')
    proof=dict(schemaVersion=1,platform='windows',passed=True,build=build,driver=driver,transportJob=job(keeper,'transport'),
        old=old,allHelpersExited=True,limitsChanged=False,breakawayUsed=False,networkStarted=False,runtimeApplied=False,installedRuntimeApplied=False,windowsOpened=False)
    if mode=='two-generation':
        new_worker=dict(process=nw,inOldJob=False);new=dict(process=nxt,worker=new_worker,job=job(nw,'new'),limits=8192,inOldJob=False,inOldDomain=False)
        survived=dict(process=nxt,worker=new_worker,oldKeeper=keeper,oldKeeperExited=True)
        old_done=dict(process=keeper,oldJobEmpty=True,next=nxt);new_done=dict(process=nxt,newJobEmpty=True,limits=8192)
        nodes.update({'new-keeper.json':new,'new-worker.json':new_worker,'survived.json':survived,
                      'old-keeper-done.json':old_done,'new-keeper-done.json':new_done,
                      'request-next.json':dict(requester=worker),'start-next-keeper.json':dict(process=nxt,requester=worker)})
        proof.update(kind='activation_lifetime_keeper_os_spike',next=new,survived=survived,oldDone=old_done,newDone=new_done,taskCancellation=cancelled)
    else:proof.update(kind='activation_lifetime_keeper_loss_os_spike',keeperExitCode=73,workerExited=True,automaticRecoveryCount=0)
    nodes['proof.json']=proof
    return c.decode(json.dumps(nodes))


class Observer(unittest.TestCase):
    def test_both_proofs_and_all_negative_mutations(self):
        for mode,expected in [('two-generation',19),('keeper-loss',17)]:
            _,count=c.audit_case(fixture(mode),SOURCE,BASE,mode);self.assertEqual(count,expected)

    def test_reused_historical_pid_is_not_a_reused_process(self):
        c.case(fixture('two-generation',task_pid=12),SOURCE,BASE,'two-generation')

    def test_concurrent_roles_cannot_share_pid_even_with_other_birth(self):
        with self.assertRaises(ValueError):c.case(fixture('two-generation',next_pid=10),SOURCE,BASE,'two-generation')

    def test_correlated_copies_do_not_hide_limits_change(self):
        nodes=fixture('two-generation')
        nodes['old-keeper.json']['limits']=nodes['proof.json']['old']['limits']=0
        with self.assertRaises(ValueError):c.case(nodes,SOURCE,BASE,'two-generation')

    def test_boolean_integer_alias_is_not_exit_evidence(self):
        nodes=fixture('two-generation')
        nodes['survived.json']['oldKeeperExited']=nodes['proof.json']['survived']['oldKeeperExited']=1
        with self.assertRaises(ValueError):c.case(nodes,SOURCE,BASE,'two-generation')

    def test_unknown_ownership_permission_is_rejected(self):
        nodes=fixture('keeper-loss');nodes['proof.json']['canAdopt']=True
        with self.assertRaises(ValueError):c.case(nodes,SOURCE,BASE,'keeper-loss')

    def test_noncanonical_json_is_rejected(self):
        for raw in ('{"a":1,"a":2}','{"a":NaN}','{"a":1.0}'):
            with self.assertRaises(ValueError):c.decode(raw)

    def test_ready_marker_is_required_for_each_native_json(self):
        files={}
        for mode in c.MODES:
            for name,value in fixture(mode).items():
                files[mode+'-'+name]=json.dumps(value).encode();files[mode+'-'+name+'.ready']=b''
        self.assertEqual(c.audit(files,SOURCE,BASE)['negativeAudits'],36)
        del files['keeper-loss-proof.json.ready']
        with self.assertRaises(ValueError):c.audit(files,SOURCE,BASE)


if __name__=='__main__':unittest.main()
