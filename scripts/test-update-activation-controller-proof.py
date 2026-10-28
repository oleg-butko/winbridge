#!/usr/bin/env python3
"""Synthetic boundary data only. Never claims native execution or full Main26."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
import uuid

spec=importlib.util.spec_from_file_location('proof',Path(__file__).with_name('update-activation-controller-proof.py'))
p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)
uid=lambda n:str(uuid.UUID(int=n))


def fixture():
    build=dict(version='0.2.0',sourceCommit='a'*40,sourceManifestSha256='b'*64,sourceDirty=False,buildId='build')
    context=dict(instanceId='isolated',journalIncarnation=uid(1));generation=uid(2)
    process=dict(processId=42,birth='100',hostId='c'*64,sessionId=1)
    compat=dict(schemaVersion=1,kind='worker_runtime_compatibility',identity=dict(process=process,descriptor=dict(build=build)),context=context,generation=generation,workerJournalSchema=19,updaterJournalSchema=21,observedAtMs=100)
    authority=dict(incarnation=uid(1),epoch=2,fingerprintSha256='d'*64)
    local_record=dict(revision=7,state=dict(phase='local_verified'),updatedAtMs=100,local=compat,stateOwned=dict(context=context,generation=generation,process=process))
    sc=dict(commandId=uid(3),deadlineAtMs=1000)
    sr=dict(commandId=uid(3),updateId=uid(4),attemptId=uid(5),authority=authority,compatibility=compat)
    saved_service=dict(command=sc,state=dict(phase='confirmed',receipt=sr),updatedAtMs=100)
    url='ws://127.0.0.1:1144'
    topology=dict(schemaVersion=1,primary='legacy',confirmationEndpointId='legacy',controllers=[dict(id='legacy',url=url)])
    command=dict(schemaVersion=1,commandId=uid(6),issuedAtMs=101,deadlineAtMs=1000,
        binding=dict(serviceCommand=sc,serviceReceipt=sr,serviceRecordSha256=p.digest('winbridge.activation.controller-service.v1',saved_service),localRecordSha256=p.digest('winbridge.activation.controller-local.v1',local_record),topology=topology))
    frame=dict(schemaVersion=1,controllerBuild=build,controllerGeneration=uid(7),sessionId=uid(8),providerBuild=build,providerGeneration=generation,context=context,sequence=1,views=[],verifications=[],truncated=False)
    session=dict(controllerId='legacy',controllerUrl=url,controllerNodeId='debian',controllerGeneration=uid(7),sessionId=uid(8),controllerBuild=build,
        workerContext=context,workerGeneration=generation,workerBuild=build,connectionAttempt=1,projectionSequence=1,projectionSha256=p.digest('winbridge.activation.controller-projection.v1',frame),connectedAtMs=102)
    receipt=dict(schemaVersion=1,commandId=uid(6),updateId=uid(4),attemptId=uid(5),commandSha256=p.digest('winbridge.activation.controller-command.v1',command),authority=authority,compatibility=dict(compat,observedAtMs=103),session=session)
    record=dict(schemaVersion=1,revision=1,command=command,state=dict(phase='confirmed',receipt=receipt),updatedAtMs=104)
    link=dict(id='legacy',attempts='1',connection=dict(state='connected',error=None,url=url,sessionId=uid(8),controllerGeneration=uid(7),controllerNodeId='debian',lastConfirmedAtMs=103),lastConnectedAtMs=102)
    root=dict(updateId=uid(4),revision=16,phase='awaiting_controller')
    cv=dict(phase='observed',durablePhase='confirmed',commandId=uid(6),revision=1,savedReceipt=receipt,residentProcess=process,processAlive=True,observedProjection=frame,observedLink=link)
    view=dict(schemaVersion=18,update=root,activationController=cv,activationService=dict(phase='confirmed'),driverActive=False,driverError=None)
    info=dict(generation=generation,build=build,activeTaskCount=0,maintenanceError=None,journalRecoveryRequired=None,
        fencedRequests=dict(context=context,legacyMutationsAllowed=False),updateCoordination=dict(readOnly=True,lease=dict(phase='quiesced')))
    info.update({k:False for k in ('maintenanceAvailable','workflowAvailable','snapshotImportAvailable','snapshotDeltaAvailable','snapshotResumeAvailable')})
    history=[dict(taskId=uid(9),mutationRefused=True)]
    local=dict(record=local_record,serviceRecord=saved_service,writerRecord=dict(command=dict(commandId=uid(10))),controllerPreflight=dict(connection=dict(url=url)),confirmed=dict(update=root),workerBytesAfter='unchanged',normalHistory=history)
    observation=dict(controllerObservationVersion=1,result=dict(kind='observed',receipt=receipt,projection=frame,link=link))
    current=dict(record=record,history=[dict(schemaVersion=1,revision=0,command=command,state=dict(phase='issued'),updatedAtMs=101),record],confirmed=view,duplicate=view,physicalRecord=local_record,normalObservation=observation,normalInfo=info,
        connections=dict(schemaVersion=1,context=context,workerGeneration=generation,provider=build,primary='legacy',connections=[link]),normalHistory=history,workerBytesAfter='unchanged',savedControllerCommandCount=1,
        lostControllerAckSentOnce=True,sameChildVerified=True,sameAuthorityVerified=True,sameLocalPrefixVerified=True,rootCommitted=False,maintenanceReleased=False,publicActivationEnabled=False)
    cold_view=copy.deepcopy(view);cold=cv.copy();cold.update(phase='not_resident',residentProcess=None,processAlive=None,observedProjection=None,observedLink=None);cold_view['activationController']=cold
    case=dict(activationWriter=local,activationController=current,coldActivationController=dict(view=cold_view,historyUnchanged=True,automaticColdLaunchCount=0,noColdNetworkReplay=True))
    # Detach aliases; a mutation must alter exactly the requested observation.
    return json.loads(json.dumps(case))


class ControllerEvidenceTests(unittest.TestCase):
    def test_causal_scope_and_all_negative_mutations(self):
        case=fixture();key,count=p.audit_controller_case(case)
        self.assertEqual(key,uid(6));self.assertEqual(count,27)

    def test_float_private_grants_and_alias_counters_are_not_valid_observations(self):
        for field,value in [('busyObservations',True),('busyObservations',1.0),('canCommit',False),('nonce','secret')]:
            case=fixture();case['activationController'][field]=value
            with self.subTest(field=field,value=value),self.assertRaises(ValueError):p.verify_controller(case)

    def test_main25_profiles_remain_frozen_and_cannot_be_relabelled_as_controller_sdk(self):
        with self.assertRaises(ValueError):p.service.profile_schemas(p.PROFILE)
        for schemas,kind in [((19,21),'activation_service_main_native_proof'),((19,20),p.KIND),((17,21),p.KIND)]:
            with self.subTest(schemas=schemas,kind=kind),self.assertRaises(ValueError):
                p.service.verify_local_prefix({}, {}, {}, {},profile='test',expected_schemas=schemas,proof_kind=kind)

    def test_nullable_and_catalog_revisions_remain_distinct_from_required_ledger_revision(self):
        p.typed(dict(revision=None,contractCatalog=dict(queueControl=dict(revision='1'))))
        case=fixture();case['activationController']['record']['revision']='1'
        with self.assertRaises(ValueError):p.verify_controller(case)

    def test_short_session_is_proved_by_the_actual_controller_event_not_sampled_connected_flag(self):
        receipt=fixture()['activationController']['record']['state']['receipt']
        row=dict(event='worker_connected',sessionId=receipt['session']['sessionId'],generation=receipt['compatibility']['generation'])
        raw=json.dumps(row).encode()+b'\n';sources={'activity.jsonl':dict(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())}
        meta=dict(controllerPid=42,instanceId='isolated')
        status=dict(status=dict(instanceId='isolated',generation=uid(7),nodeId='debian',activityLog=dict(droppedRecords=0,lastError=None)))
        exit=dict(pid=42,exitCode=0)
        p.verify_activity(raw,sources,status,exit,meta,receipt)
        for changed in [raw.replace(uid(8).encode(),uid(9).encode()),b'',raw+b' ']:
            with self.assertRaises(ValueError):p.verify_activity(changed,sources,status,exit,meta,receipt)
        altered=dict(row,generation=uid(99));forged=json.dumps(altered).encode()+b'\n'
        with self.assertRaises(ValueError):p.verify_activity(forged,{'activity.jsonl':dict(bytes=len(forged),sha256=hashlib.sha256(forged).hexdigest())},status,exit,meta,receipt)


if __name__=='__main__':unittest.main()
