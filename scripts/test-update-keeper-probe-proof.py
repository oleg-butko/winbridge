"""Synthetic observer regressions, never evidence of Windows execution."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest
s=importlib.util.spec_from_file_location('probe_reader',Path(__file__).with_name('update-keeper-probe-proof.py'));p=importlib.util.module_from_spec(s);s.loader.exec_module(p)
def fixture():
    source=dict(sourceCommit='a'*40,manifestSha256='b'*64)
    keeper=dict(processId=10,birth='100',hostId='c'*64,sessionId=1)
    worker=dict(keeper,processId=11,birth='200')
    client=dict(keeper=copy.deepcopy(keeper),worker=copy.deepcopy(worker),changedBindingRejected=True,duplicateFieldsRejected=True,exactCaptureObserved=True,captureExpired=True)
    proof=dict(schemaVersion=1,kind='keeper_probe_native_unit_proof',platform='windows',passed=True,build=dict(version='0.2.0',buildId='synthetic',sourceCommit=source['sourceCommit'],sourceManifestSha256=source['manifestSha256'],sourceDirty=False),scope=p.SCOPE,syntheticRoleHistory=True,keeper=keeper,worker=worker,client=client,wrongPeerRejected=True,observedResponses=1,childExitCode=0,originalJobEmpty=True,readersFinished=True,runtimeApplied=False,installedRuntimeApplied=False,windowsOpened=False,networkUsed=False)
    log=f'running 1 test\ntest {p.TEST} ... WINBRIDGE_PROBE_SERVER='+json.dumps(dict(keeper=keeper,worker=worker))+'\nok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
    return proof,source,dict(keeper=copy.deepcopy(keeper),worker=copy.deepcopy(worker)),log
class Reader(unittest.TestCase):
    def test_shape_and_25_mutations(self):
        proof,source,_,log=fixture()
        self.assertEqual(p.audit({'keeper-probe.log':log.encode(),'keeper-probe-proof.json':json.dumps(proof).encode()},source,None)['negativeAudits'],25)
    def test_correlated_process_rebinding_refused(self):
        proof,source,expected,_=fixture()
        proof['worker']['processId']=proof['client']['worker']['processId']=99
        with self.assertRaises(ValueError):p.verify(proof,source,expected)
    def test_exact_selection_trace_and_boolean_exit(self):
        _,_,_,log=fixture()
        args=['test','-p','winbridge-update','--features','bundled-sqlite','--lib','--locked','--offline','--','--exact',p.TEST,'--ignored','--test-threads=1','--nocapture']
        command=dict(name='keeper-probe',log='keeper-probe.log',executable='cargo',exitCode=0,arguments=args)
        p.selection(command,log);p.trace(log)
        for bad in (log+log,log.replace('1 passed','0 passed'),log.replace(p.TEST,'another')):
            with self.assertRaises(ValueError):p.selection(command,bad)
        with self.assertRaises(ValueError):p.selection(dict(command,exitCode=False),log)
        with self.assertRaises(ValueError):p.trace(log+log)
if __name__=='__main__':unittest.main()
