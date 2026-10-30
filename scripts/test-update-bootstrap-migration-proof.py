"""Synthetic migration-reader tests; not Windows migration evidence."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import unittest

def load(name,file):
    s=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file));m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
p=load('keeper_migration_observer','update-activation-bootstrap-migration-proof.py')
base=load('keeper_migration_base','test-update-keeper-migration-proof.py')
checks=base.checks

def fixture():
    proof,source,reference,_=base.fixture()
    proof['kind']='activation_bootstrap_schema_migration_proof'
    proof['retainedActivation']['intent']['context']['updaterJournalSchema']=24
    historical=proof['before']['ownerEpoch']
    keeper=dict(schemaVersion=1,selected=dict(phase='keeper',ownerEpoch=historical),finalization=copy.deepcopy(proof['retainedFinalization']))
    proof['retainedKeeper']=keeper
    proof.update(oldTwentyFourInitRefused=True,oldTwentyFourStatusRefused=True,noFreshBootstrapReservation=True)
    for k in ('oldTwentyThreeInitRefused','oldTwentyThreeStatusRefused','noFreshKeeperSelection'):proof.pop(k)
    for k in ('before','backupInfo'):proof[k].update(schemaVersion=24,ownerEpoch=historical+1)
    proof['after'].update(schemaVersion=25,ownerEpoch=historical+2)
    proof['afterColdObservation'].update(schemaVersion=25,ownerEpoch=historical+3)
    reference['currentJournal']=copy.deepcopy(proof['before'])
    for value,key in [('retainedActivation','activationSha256'),('retainedKeeper','keeperSha256')]:reference[key]=hashlib.sha256(checks.canonical(proof[value])).hexdigest()
    return proof,source,reference,dict(expected_cold_view=copy.deepcopy(proof['coldFinalization']))
class Reader(unittest.TestCase):
    def test_actual24_shape_and_all_24_negatives(self):
        proof,source,reference,context=fixture()
        self.assertEqual(p.audit(proof,source,reference,checks,**context),24)
    def test_correlated_headers_cannot_rebind_frozen_history(self):
        for field,value in [('installationId','00000000-0000-0000-0000-000000000001'),('scopeSha256','0'*64),('lastSequence',0),('ownerEpoch',99)]:
            proof,source,reference,context=fixture()
            for key in ('before','after','afterColdObservation','backupInfo'):proof[key][field]=value
            if field=='ownerEpoch':
                proof['after'][field]=value+1
                proof['afterColdObservation'][field]=value+2
            with self.assertRaises(ValueError):p.verify(proof,source,reference,checks,**context)
    def test_foreign_or_skipped_selection_fails(self):
        args=['test','-p','winbridge-update','--features','bundled-sqlite','--test','activation_bootstrap_migration','--locked','--offline','--','--exact',p.TEST,'--ignored','--test-threads=1','--nocapture']
        command=dict(executable='cargo',exitCode=0,arguments=args)
        log=f'running 1 test\ntest {p.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
        p.verify_selection(command,log)
        for bad in (log+log,log.replace('1 passed','0 passed'),log.replace(p.TEST,'another')):
            with self.assertRaises(ValueError):p.verify_selection(command,bad)
        with self.assertRaises(ValueError):p.verify_selection(dict(command,exitCode=False),log)
if __name__=='__main__':unittest.main()
