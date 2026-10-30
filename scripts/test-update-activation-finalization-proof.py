"""Synthetic reader regression tests; these are NOT native execution evidence."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest
import uuid


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(path))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


p = load('finalization_proof', 'update-activation-finalization-proof.py')
base = load('finalization_fixture_prefix', 'test-update-activation-resident-proof.py')


def fixture():
    case = base.fixture()
    def schema(v):
        if isinstance(v, dict):
            return {k: (23 if k == 'updaterJournalSchema' else schema(x)) for k,x in v.items()}
        if isinstance(v,list): return [schema(x) for x in v]
        return v
    case = schema(case)
    w = case['activationWriter']; local = w['record']; life = case['activationLifetime']['record']; old = life['command']; lb = old['binding']
    identity = local['local']['identity']; identity['image'] = dict(path=r'K:\_proj-26\winbridge\runtime-test-204\winbridge-worker.exe')
    local['ready'] = dict(identity=copy.deepcopy(identity)); local['created']['process'] = local['stateOwned']['process']
    local['intent']['context'].update(workerJournalSchema=19, updaterJournalSchema=23)
    case['replacement'] = dict(record=dict(schemaVersion=1, state=dict(phase='files_moved')))
    local['intent']['source']['replacementSha256'] = p.digest('winbridge.activation.replacement.v1',case['replacement']['record'])
    lb['compatibility']['identity'] = copy.deepcopy(identity)
    cr = case['activationController']['record']; cr['state']['receipt']['compatibility'] = copy.deepcopy(lb['compatibility'])
    lb['localRecordSha256'] = p.digest('winbridge.activation.lifetime-local.v1',local)
    lb['controllerRecordSha256'] = p.digest('winbridge.activation.lifetime-controller.v1',cr)
    entry = life['state']['receipt']; entry['compatibility']['identity'] = copy.deepcopy(identity)
    entry['commandSha256'] = p.digest('winbridge.activation.lifetime-command.v1',old)
    w.update(actualProcess=local['stateOwned']['process'], actualJob=dict(jobName=lb['jobName'],processId=42,creationTime=100),
             actualProcessInJob=True, actualJobOccupied=True,childInstanceLockOccupied=True,
             workerLaunchCount=1,stateHandoffCount=1,savedWorkerCommandCount=1,serviceDispatchCount=1)
    root = dict(updateId=lb['updateId'],revision=lb['rootRevision'],phase='awaiting_controller',updatedAtMs=100,reason=None)
    lease = dict(spec=dict(updateId=lb['updateId']),revision=5,phase='quiesced',acceptedAtMs=1,updatedAtMs=5,reason='quiesced')
    shutdown = dict(delivery=dict(receipt=dict(lease=lease)))
    case['beforeReplacement'] = dict(shutdown=shutdown)
    obs = copy.deepcopy(case['activationResident']['second']['activationResident']['observation'])
    obs['result']['entry'] = copy.deepcopy(entry); obs['result']['frame']['providerBuild'] = identity['descriptor']['build']
    query = dict(residentObservationVersion=7,requestId=obs['requestId'],command=old)
    at = obs['result']['observedAtMs']+1
    info = dict(schemaVersion=23,installationId=str(uuid.uuid4()),scopeSha256='c'*64,ownerEpoch=lb['updaterEpoch'],activeUpdateId=lb['updateId'])
    encoded = json.dumps(shutdown,separators=(',',':'),ensure_ascii=False)
    command = dict(schemaVersion=1,commandId=str(uuid.uuid4()),issuedAtMs=at,binding=dict(lifetimeCommand=copy.deepcopy(old),
        lifetimeRecordSha256=p.digest('winbridge.activation.finalization-lifetime.v1',life),shutdownRecordSha256=p.hashlib.sha256(encoded.encode()).hexdigest(),
        installationId=info['installationId'],scopeSha256=info['scopeSha256'],committedRootRevision=root['revision']+1,expectedLeaseRevision=5,
        sourceManifestSha256='b'*64,residentRequestId=query['requestId'],residentObservationSha256=p.digest('winbridge.activation.finalization-observation.v1',obs)))
    new_root = dict(root,phase='committed',revision=root['revision']+1,updatedAtMs=at,reason='confirmed')
    released = dict(lease,phase='released',revision=6,updatedAtMs=at+1,reason=p.REASON)
    receipt = dict(schemaVersion=1,commandId=command['commandId'],commandSha256=p.digest('winbridge.activation.finalization-command.v1',command),
        compatibility=dict(entry['compatibility'],observedAtMs=at+2),authority=entry['authority'],mode='active',releasedLease=released,releasedAtMs=at+1)
    history = [dict(schemaVersion=1,revision=i,command=copy.deepcopy(command),state=dict(phase=phase),updatedAtMs=at+i+2)
               for i,phase in enumerate(('release_pending','indeterminate','confirmed'))]
    history[0]['updatedAtMs'] = at; history[2]['state']['receipt'] = receipt
    saved = history[2]
    def view(t):
        o = dict(finalizationObservationVersion=1,requestId=str(uuid.uuid4()),result=dict(kind='observed',receipt=copy.deepcopy(receipt),observedAtMs=t))
        return dict(schemaVersion=21,update=new_root,driverActive=False,driverError=None,activationFinalization=dict(phase='confirmed',durablePhase='confirmed',commandId=command['commandId'],
            residentProcess=identity['process'],processAlive=True,originalCustodyHeld=True,verificationPinsRetired=True,deliveryAttempted=True,interruptedReplyPrefix=[128,0,0,0],observation=o))
    task = dict(taskId=str(uuid.uuid4()),status='succeeded',exitCode=0,submission=dict(command=dict(executable='cmd.exe',arguments=['/d','/c','echo ACTIVE>>main30-active-count.txt'])))
    accepted = dict(requestContractVersion=2,outcome='responded',context=lb['compatibility']['context'],response=dict(kind='task',task=task))
    rejected = dict(finalizationVersion=8,result=dict(kind='rejected'))
    rejected_obs = dict(finalizationObservationVersion=1,result=dict(kind='rejected'))
    case['activationFinalization'] = dict(first=view(at+3),second=view(at+6),record=saved,intent=dict(command=command,rootBefore=root,query=query,observation=obs),history=history,
        shutdownEncoded=encoded,releasePayload=dict(commandId=command['commandId'],expectedRevision=5,action='release',sourceManifestSha256='b'*64,reason=p.REASON),
        releaseReceipt=released,updaterBefore=[info,root],updaterAfter=[dict(info,activeUpdateId=None),new_root],infoBefore=dict(maintenanceAvailable=True,generation=lb['compatibility']['generation'],fencedRequests=dict(context=lb['compatibility']['context']),build=identity['descriptor']['build']),
        wrongPeerMutation=rejected,wrongPeerObservation=rejected_obs,pidSpoof=copy.deepcopy(rejected_obs),
        configBefore=dict(fileIdentity='first',sha256='a'*64),configAfter=dict(fileIdentity='second',sha256='b'*64),configWriteSucceeded=True,atomicConfigReplaceSucceeded=True,
        accepted=accepted,duplicate=copy.deepcopy(accepted),task=task,taskEffect='ACTIVE\n',taskEffectLines=1,releaseCommandCount=1,
        physicalV8Proved=True,authenticatedActiveProved=True,permanentKeeperProved=False)
    cv = dict(phase='no_original_custody',durablePhase='confirmed',commandId=command['commandId'],**{k:None for k in ('residentProcess','processAlive','originalCustodyHeld','verificationPinsRetired','deliveryAttempted','interruptedReplyPrefix','observation')})
    case['afterOwnedExit'] = dict(view=dict(schemaVersion=21,update=new_root,driverActive=False,driverError=None,activationFinalization=cv),record=saved,originalWorkerExited=True,automaticColdLaunchCount=0,coldObservationOnly=True)
    case['ownedJobEmpty'] = True
    source = dict(sourceCommit='d'*40,manifestSha256='e'*64); target = dict(sourceCommit='a'*40,manifestSha256='b'*64)
    proof = dict(schemaVersion=1,kind=p.KIND,platform='windows',passed=True,normalMain=True,actualOldRoles=True,actualCandidateBuild=True,validationFeature=True,runtimeApplied=True,installedRuntimeApplied=False,windowsOpened=False,
        target=target,build=dict(sourceCommit=source['sourceCommit'],sourceManifestSha256=source['manifestSha256'],sourceDirty=False),cases=[case])
    return json.loads(json.dumps(proof)),source,target,dict(runtimeDirectory=r'K:\_proj-26\winbridge\runtime-test-204')


class FinalizationProofTests(unittest.TestCase):
    def test_independent_graph_receipt_real_effect_and_all_negative_audits(self):
        result = p.audit(*fixture())
        self.assertEqual(result['finalizationNegativeAudits'],22)
        self.assertFalse(result['permanentKeeperProved'])

    def test_exact_profile_never_relabels_main29_or_schema22(self):
        self.assertEqual(p.profile_schemas('worker19-updater23'),(19,23))
        for profile in ('worker19-updater22',None,True):
            with self.assertRaises(ValueError): p.profile_schemas(profile)
        args = ['test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--test','update_shutdown','--locked','--offline','--','--exact',p.TEST,'--ignored','--test-threads=1','--nocapture']
        log = f'running 1 test\ntest {p.TEST} ... diagnostics\nok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5s\n'
        p.verify_main_selection(dict(arguments=args),log)
        for bad in (args+['another'], [x.replace(p.TEST,p.resident.TEST) for x in args]):
            with self.assertRaises(ValueError): p.verify_main_selection(dict(arguments=bad),log)

    def test_coherent_wrong_shutdown_bytes_and_duplicate_correlation_are_rejected(self):
        proof,source,target,reservation = fixture()
        f = proof['cases'][0]['activationFinalization']
        f['shutdownEncoded'] = '{}'
        f['record']['command']['binding']['shutdownRecordSha256'] = p.hashlib.sha256(b'{}').hexdigest()
        with self.assertRaises(ValueError): p.verify(proof,source,target,reservation)
        proof,source,target,reservation = fixture()
        f = proof['cases'][0]['activationFinalization']
        f['second']['activationFinalization']['observation']['requestId'] = f['first']['activationFinalization']['observation']['requestId']
        with self.assertRaises(ValueError): p.verify(proof,source,target,reservation)


if __name__ == '__main__': unittest.main()
