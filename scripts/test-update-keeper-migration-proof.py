"""Synthetic migration-reader tests; not Windows migration evidence."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import unittest

def load(name,file):
    s=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file));m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
p=load('keeper_migration_observer','update-activation-keeper-migration-proof.py')
base=load('keeper_migration_base','test-update-finalization-migration-proof.py')
checks=base.checks

def fixture():
    proof,source,reference,_=base.fixture()
    proof['kind']='activation_keeper_schema_migration_proof'
    proof['retainedActivation']['intent']['context']['updaterJournalSchema']=23
    proof['retainedRoot']['phase']='committed'
    final=dict(schemaVersion=1,revision=2,state=dict(phase='confirmed'),command=dict(commandId='synthetic'))
    proof['retainedFinalization']=final
    proof.update(oldTwentyThreeInitRefused=True,oldTwentyThreeStatusRefused=True,noFreshKeeperSelection=True)
    for k in ('oldTwentyTwoInitRefused','oldTwentyTwoStatusRefused','noFreshFinalizationIntent','oldLifetimeReceiverRefused','coldResident'):proof.pop(k)
    for k in ('before','backupInfo'):proof[k].update(schemaVersion=23,activeUpdateId=None)
    for k in ('after','afterColdObservation'):proof[k].update(schemaVersion=24,activeUpdateId=None)
    cold=dict(phase='no_original_custody',durablePhase='confirmed',commandId='synthetic',**{k:None for k in ('residentProcess','processAlive','originalCustodyHeld','verificationPinsRetired','deliveryAttempted','interruptedReplyPrefix','observation')})
    proof['coldFinalization']=dict(schemaVersion=21,update=copy.deepcopy(proof['retainedRoot']),driverActive=False,driverError=None,observedAtMs=40,activationFinalization=cold)
    for value,key in [('retainedActivation','activationSha256'),('retainedRoot','rootSha256'),('retainedFinalization','finalizationSha256')]:reference[key]=hashlib.sha256(checks.canonical(proof[value])).hexdigest()
    return proof,source,reference,dict(expected_cold_view=copy.deepcopy(proof['coldFinalization']))
class Reader(unittest.TestCase):
    def test_actual23_shape_and_all_22_negatives(self):
        proof,source,reference,context=fixture()
        self.assertEqual(p.audit(proof,source,reference,checks,**context),22)
    def test_correlated_headers_cannot_rebind_frozen_history(self):
        for field,value in [('installationId','00000000-0000-0000-0000-000000000001'),('scopeSha256','0'*64),('lastSequence',0)]:
            proof,source,reference,context=fixture()
            for key in ('before','after','afterColdObservation','backupInfo'):proof[key][field]=value
            with self.assertRaises(ValueError):p.verify(proof,source,reference,checks,**context)
    def test_foreign_or_skipped_selection_fails(self):
        args=['test','-p','winbridge-update','--features','bundled-sqlite','--test','activation_keeper_migration','--locked','--offline','--','--exact',p.TEST,'--ignored','--test-threads=1','--nocapture']
        command=dict(executable='cargo',exitCode=0,arguments=args)
        log=f'running 1 test\ntest {p.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
        p.verify_selection(command,log)
        for bad in (log+log,log.replace('1 passed','0 passed'),log.replace(p.TEST,'another')):
            with self.assertRaises(ValueError):p.verify_selection(command,bad)
        with self.assertRaises(ValueError):p.verify_selection(dict(command,exitCode=False),log)
if __name__=='__main__':unittest.main()
