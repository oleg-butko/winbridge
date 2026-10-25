"""Independent actor22 evidence: one actual pre-writer process, no writer grant."""
import copy
import hashlib
import json
import re


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def strict(value, preparation):
    preparation.strict(value)
    def visit(v):
        if isinstance(v, dict):
            for key, item in v.items():
                need(key not in ('stateGrant','canHandoff','writerOwned','canWriteState'),'Fabricated process permission: '+key)
                if key=='creationTime':
                    need(type(item) is int and 0<=item<=2**64-1,'Invalid physical FILETIME')
                if key in ('processCustodyHeld','processAlive','outputIncomplete','captureComplete'):
                    need(item is None or type(item) is bool, 'Invalid nullable process observation: '+key)
                visit(item)
        elif isinstance(v, list):
            for item in v: visit(item)
    visit(value)


def verify(proof, source, target, root, preparation, health):
    strict(proof, preparation)
    need(proof['kind']=='activation_process_main_native_proof','Wrong process proof kind')
    health.verify_runtime_replacement_proof(dict(proof,kind='runtime_replacement_main_native_proof'),source,target,True)
    case,=proof['cases']; before=case['beforeReplacement']; replaced=case['replacement']
    p,cold=case['activationProcess'],case['coldActivationProcess']
    prepared=p['prepared']; intent=prepared['intent']; ctx=intent['context']; pin=intent['source']
    preparation.verify_scope(prepared,p,before,replaced,root)
    need(replaced['view']['update']['updatedAtMs']<=intent['admittedAtMs'],'Activation admission predates FilesVerified')
    expected_prepared=dict(schemaVersion=1,intent=intent,revision=0,updatedAtMs=intent['admittedAtMs'],state={'phase':'prepared'},
                           createIssuedAtMs=None,created=None,resumeGrantedAtMs=None,ready=None,handoffIssuedAtMs=None,stateOwned=None,local=None)
    need(prepared==expected_prepared,'Prepared data already contains an execution grant')
    record=p['record']; history=p['history']
    need(record==cold['record'] and history==cold['history'] and len(history)==5 and history[0]==prepared and history[-1]==record,
         'Execution history missing/replayed/changed cold')
    need(record['intent']==intent and record['state']=={'phase':'ready_for_state'} and record['revision']==4
         and all(record[k] is None for k in ('handoffIssuedAtMs','stateOwned','local')), 'Receiver advanced into state ownership')
    created=record['created']; process=created['process']; ready=record['ready']; identity=ready['identity']
    need(process==p['createdProcess'] and process['processId']>0
         and process['processId'] not in (ctx['supervisor']['processId'],ctx['previousWorker']['processId'])
         and str(int(process['birth']))==process['birth'] and 0<int(process['birth'])<=2**64-1
         and process['hostId']==ctx['supervisor']['hostId'] and process['sessionId']==ctx['supervisor']['sessionId'],
         'Created process is not a distinct actual same-host/session child')
    job='Global\\WinBridge-Activation-'+hashlib.sha256(b'winbridge.activation.job.v1\n'+preparation.canonical(intent)).hexdigest()
    need(created['jobName']==job,'Actual Job does not bind the complete frozen intent')
    need(ready==dict(attemptId=intent['attemptId'],identity=identity,workerConfigSha256=ctx['workerConfig']['sha256'],observedAtMs=ready['observedAtMs'])
         and identity==dict(schemaVersion=1,kind='loaded_worker_identity',process=process,runtimeIdentity=pin['runtimeIdentity'],
                            setId=pin['setId'],manifestSha256=pin['manifest']['sha256'],descriptor=pin['workerDescriptor'],image=pin['worker']),
         'Ready changed child/attempt/config/loaded generation')
    phases=['prepared','creating','created','resume_granted','ready_for_state']
    last=intent['admittedAtMs'];deadline=last+ctx['startupTimeoutMs']
    for i,entry in enumerate(history):
        expected=dict(prepared,revision=i,updatedAtMs=entry['updatedAtMs'],state={'phase':phases[i]})
        if i>=1: expected['createIssuedAtMs']=record['createIssuedAtMs']
        if i>=2: expected['created']=created
        if i>=3: expected['resumeGrantedAtMs']=record['resumeGrantedAtMs']
        if i>=4: expected['ready']=ready
        need(entry==expected and last<=entry['updatedAtMs'],'Execution prefix rewrote/skipped a receipt')
        last=entry['updatedAtMs']
    need(history[1]['updatedAtMs']==record['createIssuedAtMs']<=deadline
         and history[1]['updatedAtMs']<=created['observedAtMs']<=history[2]['updatedAtMs']
         and history[2]['updatedAtMs']<=record['resumeGrantedAtMs']==history[3]['updatedAtMs']<=deadline
         and history[3]['updatedAtMs']<=ready['observedAtMs']<=history[4]['updatedAtMs'], 'Execution grants/facts are not causal or expired')
    for phase,index in [('creating',1),('created',2)]:
        marker=p[phase];inp=marker['input'];encoded=json.loads(marker['inputEncoded'])
        expected_input=dict(schemaVersion=1,kind='winbridge_activation_process_validation',updateId=pin['updateId'],launchId=pin['launchId'],
                            ownerId=pin['ownerId'],ownerEpoch=pin['ownerEpoch'],attemptId=intent['attemptId'],runtimeIdentity=pin['runtimeIdentity'],
                            updaterConfigSha256=pin['updaterConfigSha256'])
        need(marker['schemaVersion']==1 and marker['kind']=='activation_process_before_effect' and marker['phase']==phase
             and marker['processId']==ctx['supervisor']['processId'] and marker['record']==history[index]
             and inp==encoded==expected_input and hashlib.sha256(marker['inputEncoded'].encode()).hexdigest()==marker['inputSha256']
             and marker['recordSha256']==hashlib.sha256(b'winbridge.activation.barrier.record.v1\n'+preparation.canonical(marker['record'])).hexdigest()
             and marker['build']['sourceCommit']==source['sourceCommit'] and marker['build']['sourceDirty'] is False
             and marker['build']['sourceManifestSha256']==source['manifestSha256'], 'Before-effect marker has foreign build/scope/record')
    expected_job=dict(jobName=job,hostId=process['hostId'],sessionId=process['sessionId'],processId=process['processId'],creationTime=int(process['birth']))
    need(p['createdJob']==expected_job and p['jobBeforeCreate']==dict(expected_job,processId=0,creationTime=0)
         and all(p[k] is True for k in ('jobEmptyBeforeCreate','createdJobOccupied','createdProcessInJob')),'Actual before/after scoped Job observations missing')
    set_=dict(schemaVersion=1,kind='installed_set_observation',readOnly=True,runtimeDirectory=pin['runtimeDirectory'],runtimeIdentity=pin['runtimeIdentity'],
              manifestFile=replaced['actualInstalledFiles'][4],manifest=replaced['actualManifest'],images=replaced['actualInstalledFiles'][:4])
    need(p['actualSet']==set_,'Loaded five-file set differs from the moved set')
    files=dict(phase='bound',attemptId=intent['attemptId'],setId=pin['setId'],heldFiles=5,worker=pin['worker'],
               manifestSha256=pin['manifest']['sha256'],originalStateHeld=True,reason=None)
    empty=dict(phase='not_started',attemptId=intent['attemptId'],revision=0,durablePhase='prepared',savedProcess=None,jobName=None,savedReady=None,
               residentProcess=None,processCustodyHeld=False,processAlive=None,originalStateHeld=True,stdoutTail=None,stderrTail=None,
               outputIncomplete=None,captureComplete=None,reason=None)
    root_record=replaced['view']['update']
    for name in ('before','preparedView'):
        v=p[name]
        need(v['schemaVersion']==12 and v['update']==root_record and v['driverActive'] is False and v['driverError'] is None
             and 'activationFiles' not in v and 'activationProcess' not in v,'Old preparation view changed shape/root')
    need(p['before']['activationPreparation']['phase']=='not_prepared'
         and p['preparedView']['activationPreparation']['phase']=='prepared'
         and p['preparedView']['activationPreparation']['attemptId']==intent['attemptId'],'Preparation was not explicit/unchanged')
    need(p['bound']['schemaVersion']==13 and p['bound']['update']==root_record and p['bound']['activationFiles']==files
         and p['bound']['driverActive'] is False and p['bound']['driverError'] is None
         and 'activationProcess' not in p['bound'],'Read binding was not completed before receiver launch')
    need(p['binding']['schemaVersion']==13 and p['binding']['update']==root_record and p['binding']['driverActive'] is True
         and p['binding']['activationFiles']==dict(files,phase='binding',heldFiles=0,worker=None,manifestSha256=None),
         'Binding ACK fabricated completed file custody')
    for name,phase,index,active in [('initial','not_started',0,False),('creatingView','launching',1,True),('duplicateCreating','launching',1,True),
                                  ('createdView','launching',2,True),('duplicateCreated','launching',2,True),('ready','ready_for_state',4,False),('duplicate','ready_for_state',4,False)]:
        view=p[name];expected=dict(empty,phase=phase,revision=index,durablePhase=phases[index])
        if index in (1,2): expected['processCustodyHeld']=None
        if index>=2:expected.update(savedProcess=process,jobName=job)
        if index==4:expected.update(savedReady=identity,residentProcess=process,processCustodyHeld=True,processAlive=True,stdoutTail='',stderrTail='',outputIncomplete=False,captureComplete=False)
        need(view['schemaVersion']==14 and view['update']==root_record and view['driverActive'] is active and view['driverError'] is None
             and view['activationProcess']==expected and view['activationFiles']==files,'Live/duplicate process view changed scope or invented resources: '+name)
    need(p['legacy']['schemaVersion']==13 and 'activationProcess' not in p['legacy'] and p['legacy']['activationFiles']==files,
         'Old view changed shape or lost held files after process-owner transfer')
    cold_process=dict(empty,phase='not_resident',revision=4,durablePhase='ready_for_state',savedProcess=process,jobName=job,savedReady=identity,originalStateHeld=False)
    need(cold['view']['schemaVersion']==14 and cold['view']['activationProcess']==cold_process
         and cold['view']['activationFiles']==dict(files,phase='not_bound',heldFiles=0,worker=None,manifestSha256=None,originalStateHeld=False)
         and cold['view']['update']==root_record and cold['view']['driverActive'] is False and cold['view']['driverError'] is None,
         'Cold history recreated process/file/state custody')
    need(p['workerBytesBefore']==p['workerBytesAfter']==replaced['workerBytesBefore']
         and all(p[k] is True for k in ('lostAckSentOnce','originalInstanceLockHeld','workerWriteDenied','workerDeleteDenied'))
         and all(cold[k] is True for k in ('actualRetainedProcessExited','ownedJobEmpty','activationJobEmpty','workerBytesUnchanged'))
         and p['workerLaunchCount']==cold['workerLaunchCount']==1 and p['stateHandoffCount']==cold['stateHandoffCount']==0
         and cold['automaticColdLaunchCount']==0 and p['publicActivationEnabled'] is False,'No-writer/one-process/actual exit evidence differs')


def audit(proof,source,target,root,preparation,health):
    verify(proof,source,target,root,preparation,health)
    prefix='cases/0/activationProcess/'
    edits=[(prefix+'record/revision',3),(prefix+'history',[]),(prefix+'createdProcess/processId',1),
           (prefix+'createdJob/jobName','foreign'),(prefix+'jobEmptyBeforeCreate',False),(prefix+'createdJobOccupied',False),
           (prefix+'createdProcessInJob',False),(prefix+'jobBeforeCreate/creationTime',False),(prefix+'actualSet/images/0/fileIdentity','foreign'),
           (prefix+'creating/processId',1),(prefix+'creating/recordSha256','0'*64),(prefix+'created/inputSha256','0'*64),
           (prefix+'created/record/state/phase','resume_granted'),(prefix+'ready/activationProcess/processCustodyHeld',1),
           (prefix+'ready/activationProcess/processAlive',False),(prefix+'ready/activationProcess/residentProcess/processId',1),
           (prefix+'duplicate/activationProcess/revision',5),(prefix+'duplicateCreated/activationProcess/savedProcess/birth','1'),
           (prefix+'workerBytesAfter',[]),(prefix+'stateHandoffCount',1),(prefix+'workerLaunchCount',2),
           (prefix+'workerWriteDenied',False),(prefix+'workerDeleteDenied',False),(prefix+'lostAckSentOnce',False),
           ('cases/0/coldActivationProcess/view/activationProcess/processCustodyHeld',True),
           ('cases/0/coldActivationProcess/view/activationProcess/processAlive',True),
           ('cases/0/coldActivationProcess/actualRetainedProcessExited',False),('cases/0/coldActivationProcess/automaticColdLaunchCount',True)]
    for path,value in edits:
        bad=copy.deepcopy(proof);at=bad;parts=path.split('/')
        for part in parts[:-1]:at=at[int(part)] if isinstance(at,list) else at[part]
        at[parts[-1]]=value
        try:verify(bad,source,target,root,preparation,health)
        except (ValueError,KeyError,TypeError,IndexError):continue
        raise ValueError('Accepted mutated process proof: '+path)
    return len(edits)
