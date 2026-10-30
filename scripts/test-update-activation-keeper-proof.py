"""Synthetic observer checks only; never native ownership/CPU evidence."""
import copy
import importlib.util
from pathlib import Path
import unittest
import uuid

def load(name,file):
    s=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file));m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
p=load('keeper_observer','update-activation-keeper-proof.py')
base=load('keeper_base_fixture','test-update-activation-finalization-proof.py')
def fixture():
    proof,source,target,reservation=base.fixture(updater_schema=24)
    proof['kind']=p.KIND
    case,=proof['cases'];final=case['activationFinalization'];lb=case['activationLifetime']['record']['command']['binding']
    before=dict(schemaVersion=1,phase='active',revision=3,spec=dict(updateId=lb['updateId'],launchId=lb['launchId']),process=lb['keeper'],ownerEpoch=lb['updaterEpoch'],ownerId=lb['updaterOwnerId'],updatedAtMs=final['record']['updatedAtMs'],reason='original')
    observation=copy.deepcopy(final['second']['activationFinalization']['observation']);observation['requestId']=str(uuid.uuid4())
    selected=dict(before,phase='keeper',revision=4,updatedAtMs=observation['result']['observedAtMs']+1,reason=p.REASON)
    query=dict(finalizationObservationVersion=1,requestId=observation['requestId'],command=final['record']['command'])
    selection=dict(schemaVersion=1,before=before,selected=selected,finalization=final['record'],query=query,observation=observation)
    receipt=final['record']['state']['receipt']
    binding=dict(updateId=lb['updateId'],launchId=lb['launchId'],coordinator=lb['keeper'],worker=receipt['compatibility']['identity']['process'],jobName=lb['jobName'],generation=receipt['compatibility']['generation'],context=receipt['compatibility']['context'],finalizationCommandId=receipt['commandId'],finalizationCommandSha256=receipt['commandSha256'],selectionSha256=p.digest('winbridge.keeper.selection.v1',selection))
    status=dict(phase='keeper',binding=binding,roleSelectionKnown=True,writerClosed=True,authorityReleased=True,originalCustodyHeld=True,workerAlive=True,jobTreeEmpty=False,reason=None)
    final['updaterAfter'][0]['lastSequence']=19
    old=copy.deepcopy(final['updaterAfter'][0]);new=dict(old,ownerEpoch=old['ownerEpoch']+1)
    case['activationKeeper']=dict(before=old,first=status,duplicate=copy.deepcopy(status),second=copy.deepcopy(status),role=selected,selection=selection,newOwner=new,infoAfter=copy.deepcopy(final['infoBefore']),idleCpuTicks100ns=156250,idleWallMs=3001,actualNewOwnerAcquired=True,sameWorkerAndJobRetained=True,nextGenerationStarted=False)
    task=copy.deepcopy(final['task']);task['submission']['command']['arguments']=['/d','/c','echo KEEPER>>main31-keeper-count.txt']
    accepted=copy.deepcopy(final['accepted']);accepted['response']['task']=copy.deepcopy(task)
    case['activationKeeper'].update(accepted=accepted,duplicateTask=copy.deepcopy(accepted),task=task,taskEffect='KEEPER\n')
    return proof,source,target,reservation
class Observer(unittest.TestCase):
    def test_complete_explicit24_and_negative_audits(self):
        proof,source,target,reservation=fixture()
        result=p.audit(proof,source,target,reservation)
        self.assertTrue(result['permanentKeeperProved'])
        self.assertGreaterEqual(result['keeperNegativeAudits'],17)
        with self.assertRaises(ValueError):p.f.verify(proof,source,target,reservation)
    def test_old_main30_defaults_and_profiles_remain_frozen(self):
        proof,source,target,reservation=base.fixture()
        self.assertTrue(p.f.audit(proof,source,target,reservation)['physicalV8Proved'])
        with self.assertRaises(ValueError):p.verify(proof,source,target,reservation)
        for bad in ('worker19-updater23','worker19-updater25',True,None):
            with self.assertRaises(ValueError):p.profile_schemas(bad)
if __name__=='__main__':unittest.main()
