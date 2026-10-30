"""Synthetic observer checks only; not evidence of Windows keeper execution."""
import copy,importlib.util,json,unittest
from pathlib import Path,PureWindowsPath
s=importlib.util.spec_from_file_location('exit_proof',Path(__file__).with_name('update-keeper-exit-proof.py'));p=importlib.util.module_from_spec(s);s.loader.exec_module(p)
def fixture():
    source=dict(sourceCommit='a'*40,manifestSha256='b'*64)
    build=dict(version='0.2.0',buildId='synthetic',sourceCommit=source['sourceCommit'],sourceManifestSha256=source['manifestSha256'],sourceDirty=False)
    base=r'K:\fixture\cases';cases=[];files={}
    def proc(pid,born):return dict(processId=pid,birth=str(born),hostId='c'*64,sessionId=1)
    for index,mode in enumerate(('confirmed','unconfirmed')):
        keeper,worker,leaf,successor=(proc(100+index*100+i,1000+i*100)for i in range(4))
        jobs=[r'Local\WinBridge-Exit-'+mode,r'Global\WinBridge-Domain-'+mode]
        identity=lambda n:f'00000000-0000-4000-8000-{n:012x}'
        binding=dict(updateId=identity(1),launchId=identity(2),coordinator=keeper,worker=worker,jobName=jobs[0],generation=identity(3),context=dict(instanceId='synthetic',journalIncarnation=identity(4)),finalizationCommandId=identity(5),finalizationCommandSha256='d'*64,selectionSha256='e'*64)
        def status(alive,empty,finished):return dict(phase='keeper' if alive else 'worker_exited',binding=copy.deepcopy(binding),roleSelectionKnown=True,writerClosed=True,authorityReleased=True,originalCustodyHeld=True,workerAlive=alive,jobTreeEmpty=empty,stdoutTail='',stderrTail='',outputIncomplete=False,readersFinished=finished,reason=None)
        case=dict(mode=mode,root=str(PureWindowsPath(base)/mode),keeper=keeper,worker=worker,leaf=leaf,successor=successor,jobs=jobs,binding=binding,beforeRootExit=status(True,False,False),afterRootExit=status(False,False,False),unconfirmedAfterDrain=None if index==0 else status(False,True,True),idleSamples=[dict(elapsedNs=500_000_000,cpuTicks100ns=0)for _ in range(index+1)],keeperExitSeen=True,keeperExitedNaturally=index==0,successorOutsideBothJobsBeforeExit=True,successorSurvivedKeeper=True,successorExitSeen=True,workerExitSeen=True,leafExitSeen=True,stdoutLog=mode+'-helper.stdout.log',stderrLog=mode+'-helper.stderr.log')
        event=dict(event='original_keeper_exit',keeperPid=keeper['processId'],workerPid=worker['processId'],successorReadyKnown=True,workerAlive=False,jobTreeEmpty=True,readersFinished=True,outputIncomplete=False)
        files[case['stdoutLog']]=b'test result: ok. 1 passed; 0 failed; 0 ignored;\n' if index==0 else b'running 1 test\n'
        files[case['stderrLog']]=('WinBridge keeper '+json.dumps(event)+'\n').encode() if index==0 else b''
        cases.append(case)
    commands=[]
    for name,package,test in [('dual-job-launch','winbridge-platform',p.DUAL),('keeper-exit','winbridge-update',p.TEST)]:
        args=['test','-p',package]+(['--features','bundled-sqlite'] if name=='keeper-exit' else [])+['--lib','--locked','--offline','--','--exact',test]+(['--ignored'] if name=='keeper-exit' else [])+['--test-threads=1','--nocapture']
        commands.append(dict(name=name,log=name+'.log',executable='cargo',exitCode=0,arguments=args))
        files[name+'.log']=f'running 1 test\ntest {test} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'.encode()
    proof=dict(schemaVersion=1,kind='keeper_exit_native_proof',platform='windows',build=build,scope=p.SCOPE,cases=cases,**{k:True for k in p.TRUE},**{k:False for k in p.FALSE})
    files['keeper-exit-proof.json']=json.dumps(proof).encode();files['report.json']=json.dumps(dict(commands=commands)).encode()
    return files,source,base
class Reader(unittest.TestCase):
    def test_complete_scope_and_forty_negative_audits(self):
        files,source,base=fixture();r=p.audit(files,source,base);self.assertEqual(r['negativeAudits'],40)
        self.assertFalse(r['updaterWorkflowProved']);self.assertFalse(r['bootstrapCreationProved'])
    def test_unknown_file_wrong_source_and_missing_production_event_refused(self):
        for fault in range(3):
            f,s,b=fixture()
            if fault==0:f['unexpected.log']=b'x'
            elif fault==1:s['sourceCommit']='0'*40
            else:f['confirmed-helper.stderr.log']=b''
            with self.assertRaises(ValueError):p.audit(f,s,b)
    def test_skipped_repeated_or_wrong_unit_refused(self):
        for name in ('dual-job-launch','keeper-exit'):
            for kind in range(3):
                f,s,b=fixture();log=f[name+'.log']
                f[name+'.log']=log+log if kind==0 else log.replace(b'1 passed',b'0 passed') if kind==1 else log.replace(b' ... ok',b' ... ignored')
                with self.assertRaises(ValueError):p.audit(f,s,b)
    def test_correlated_scope_and_cpu_rebinding_refused(self):
        f,s,b=fixture();proof=p.decode(f['keeper-exit-proof.json']);proof['cases'][1]['idleSamples'][1]['cpuTicks100ns']=2**63
        f['keeper-exit-proof.json']=json.dumps(proof).encode()
        with self.assertRaises(ValueError):p.audit(f,s,b)
if __name__=='__main__':unittest.main()
