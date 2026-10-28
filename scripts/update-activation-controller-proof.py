"""Independent Main26 evidence: fixed19/21, same owner, no commit or replay."""
import copy
import importlib.util
import json
from pathlib import Path
import re
import uuid

spec=importlib.util.spec_from_file_location('main26_local_prefix',Path(__file__).with_name('update-activation-service-proof.py'))
service=importlib.util.module_from_spec(spec);spec.loader.exec_module(service)
need,canonical,digest=service.need,service.canonical,service.digest
audit_auxiliary=service.audit_auxiliary
verify_worker_migration=service.verify_worker_migration
PROFILE='worker19-updater21'
KIND='activation_controller_main_native_proof'


def profile_schemas(profile):
    need(type(profile) is str and profile==PROFILE,'Main26 requires explicit worker19/updater21')
    return (19,21)


def verify_activity(raw,sources,final_status,exit_record,meta,receipt):
    """Actual controller events survive sessions shorter than the status poll."""
    need(type(exit_record['exitCode']) is int and exit_record['exitCode']==0
         and exit_record['pid']==meta['controllerPid'],'Wrong fixture shutdown/PID')
    status=final_status['status'];s=receipt['session']
    need(status['instanceId']==meta['instanceId'] and status['generation']==s['controllerGeneration']
         and status['nodeId']==s['controllerNodeId'] and status['activityLog']['droppedRecords']==0
         and status['activityLog']['lastError'] is None,'Controller log health/generation differs')
    offset=0
    import hashlib
    for name,pin in sources.items():
        need(re.fullmatch(r'activity\.jsonl(?:\.[1-5])?',name) and type(pin['bytes']) is int
             and 0<=pin['bytes']<=1048576,'Unexpected bounded activity source')
        chunk=raw[offset:offset+pin['bytes']];offset+=pin['bytes']
        need(hashlib.sha256(chunk).hexdigest()==pin['sha256'],'Captured activity part changed')
    need(sources and offset==len(raw) and raw.endswith(b'\n'),'Activity source lengths/tail differ')
    rows=[json.loads(line) for line in raw.splitlines()]
    need(any(r.get('event')=='worker_connected' and r.get('sessionId')==s['sessionId']
             and r.get('generation')==receipt['compatibility']['generation'] for r in rows),
         'No actual Debian controller event for the NEW same-child session')
    return rows


def verify_main_selection(command,log):
    name='actual_main_connects_same_verified_worker_to_controller_without_commit_or_replay'
    need(command['arguments']==['test','-p','winbridge-worker','--features','bundled-sqlite,update-validation',
         '--test','update_shutdown','--locked','--offline','--','--exact',name,'--ignored','--test-threads=1','--nocapture'],
         'Main26 exact native test differs')
    log=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',log)
    summaries=re.findall(r'^test result: ([^\r\n]+)',log,re.MULTILINE)
    need(len(re.findall(r'^test '+re.escape(name)+r' \.\.\.',log,re.MULTILINE))==1
         and re.search(r'^running 1 test\s*$',log,re.MULTILINE) and len(summaries)==1
         and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;'),'Main26 did not execute exactly once')


def typed(value):
    """Strict additional counters without rejecting typed stdout/stderr vectors."""
    if isinstance(value,dict):
        for key,child in value.items():
            need(key not in ('authorityId','authority_id','startupPermit','writeGrant','permit','nonce','token','canConnect','canCommit','canAdopt'),
                 'Private material or invented execution permission: '+key)
            if key in ('revision','ownerEpoch','parentRevision','expectedRevision','epoch','sequence','projectionSequence','connectionAttempt','savedControllerCommandCount','busyObservations','automaticColdLaunchCount','schemaVersion','workerJournalSchema','updaterJournalSchema','workerConfigSchema','protocolVersion','activeTaskCount','queryContract') or key.endswith('AtMs'):
                # Shared readonly views include nullable counters for phases
                # that were never started. Required facts are checked below.
                # Queue-control catalog revisions use canonical decimal u64
                # strings. Controller ledger/view revisions are separately
                # required to equal their numeric causal revisions below.
                decimal_revision=key=='revision' and type(child) is str and re.fullmatch(r'0|[1-9][0-9]*',child) and int(child)<=2**64-1
                need(child is None or type(child) is int and 0<=child<=9007199254740991 or decimal_revision,'Invalid counter: '+key)
            if key=='bytes' and isinstance(child,list):
                need(all(type(b) is int and 0<=b<=255 for b in child),'Invalid typed output byte vector')
            typed(child)
    elif isinstance(value,list):
        for child in value:typed(child)
    elif type(value) is float:
        raise ValueError('Floating point evidence is not a typed contract observation')


def link_matches(link,receipt):
    s=receipt['session'];c=link['connection']
    need(link['id']==s['controllerId'] and link['attempts']==str(s['connectionAttempt'])
         and c['state']=='connected' and c['error'] is None and c['url']==s['controllerUrl']
         and c['sessionId']==s['sessionId'] and c['controllerGeneration']==s['controllerGeneration']
         and c['controllerNodeId']==s['controllerNodeId'] and link['lastConnectedAtMs']==s['connectedAtMs']
         and type(c['lastConfirmedAtMs']) is int and c['lastConfirmedAtMs']>=s['connectedAtMs'],
         'Selected current link/Welcome/attempt differs')


def frame_matches(frame,receipt):
    s=receipt['session']
    need(frame['schemaVersion']==1 and frame['controllerBuild']==s['controllerBuild']
         and frame['controllerGeneration']==s['controllerGeneration'] and frame['sessionId']==s['sessionId']
         and frame['providerBuild']==s['workerBuild'] and frame['providerGeneration']==s['workerGeneration']
         and frame['context']==s['workerContext'] and frame['sequence']==s['projectionSequence']
         and digest('winbridge.activation.controller-projection.v1',frame)==s['projectionSha256'],
         'Full accepted projection differs from selected session/provider/digest')


def receipt_matches(receipt,command,local):
    need(set(receipt)=={'schemaVersion','commandId','updateId','attemptId','commandSha256','authority','compatibility','session'},'Unknown controller receipt fields')
    old=command['binding']['serviceReceipt'];s=receipt['session'];compat=receipt['compatibility']
    need(receipt['schemaVersion']==1 and receipt['commandId']==command['commandId']
         and receipt['updateId']==old['updateId'] and receipt['attemptId']==old['attemptId']
         and receipt['commandSha256']==digest('winbridge.activation.controller-command.v1',command)
         and receipt['authority']==old['authority']
         and compat==dict(local['local'],observedAtMs=compat['observedAtMs'])
         and command['issuedAtMs']<=compat['observedAtMs']<command['deadlineAtMs'], 'Controller receipt changed authority/child/window')
    endpoint,=(p for p in command['binding']['topology']['controllers'] if p['id']==command['binding']['topology']['confirmationEndpointId'])
    need(s['controllerId']==endpoint['id'] and s['controllerUrl']==endpoint['url']
         and s['workerContext']==compat['context'] and s['workerGeneration']==compat['generation']
         and s['workerBuild']==compat['identity']['descriptor']['build']
         and s['connectionAttempt']==1 and type(s['connectionAttempt']) is int
         and 0<s['projectionSequence']<=9007199254740991
         and command['issuedAtMs']<=s['connectedAtMs']<=compat['observedAtMs']
         and re.fullmatch(r'[A-Za-z0-9_-]{1,80}',s['controllerNodeId']), 'Controller selected endpoint/provider/time differs')
    for key in ('sessionId','controllerGeneration'):
        need(str(uuid.UUID(s[key]))==s[key],'Noncanonical controller UUID')


def verify_controller(case):
    local=case['activationWriter'];p=case['activationController'];cold=case['coldActivationController']
    record=p['record'];command=record['command'];b=command['binding'];receipt=record['state']['receipt']
    typed(p);typed(cold);service.strict(p);service.strict(cold)
    need(set(record)=={'schemaVersion','revision','command','state','updatedAtMs'}
         and record['schemaVersion']==1 and record['revision']==1 and record['state']['phase']=='confirmed', 'Controller record differs')
    need(set(command)=={'schemaVersion','commandId','binding','issuedAtMs','deadlineAtMs'}
         and set(b)=={'serviceCommand','serviceReceipt','serviceRecordSha256','localRecordSha256','topology'}, 'Unknown command/binding fields')
    need(command['schemaVersion']==1 and str(uuid.UUID(command['commandId']))==command['commandId']
         and command['commandId'] not in (local['serviceRecord']['command']['commandId'],local['writerRecord']['command']['commandId'],receipt['updateId'],receipt['attemptId'])
         and b['serviceCommand']==local['serviceRecord']['command'] and b['serviceReceipt']==local['serviceRecord']['state']['receipt']
         and b['serviceRecordSha256']==digest('winbridge.activation.controller-service.v1',local['serviceRecord'])
         and b['localRecordSha256']==digest('winbridge.activation.controller-local.v1',local['record'])
         and max(local['record']['updatedAtMs'],local['serviceRecord']['updatedAtMs'])<=command['issuedAtMs']<command['deadlineAtMs']
         and command['deadlineAtMs']==b['serviceCommand']['deadlineAtMs'], 'Controller command detached from immutable local/service prefix')
    url=local['controllerPreflight']['connection']['url']
    need(b['topology']==dict(schemaVersion=1,primary='legacy',confirmationEndpointId='legacy',controllers=[dict(id='legacy',url=url)]), 'Main26 changed pinned fixture topology/selection')
    receipt_matches(receipt,command,local['record'])
    rows=p['history']
    need(rows==[dict(schemaVersion=1,revision=0,command=command,state=dict(phase='issued'),updatedAtMs=command['issuedAtMs']),record]
         and record['updatedAtMs']>=receipt['compatibility']['observedAtMs'], 'Controller skipped/reissued causal history')
    view=p['confirmed'];cv=view['activationController']
    need(view['schemaVersion']==18 and view['update']==local['confirmed']['update'] and view['update']['phase']=='awaiting_controller'
         and p['physicalRecord']==local['record'] and cv['phase']=='observed' and cv['durablePhase']=='confirmed'
         and cv['commandId']==command['commandId'] and cv['revision']==record['revision'] and cv['savedReceipt']==receipt
         and cv['residentProcess']==receipt['compatibility']['identity']['process'] and cv['processAlive'] is True
         and view['activationService']['phase']=='confirmed' and view['driverActive'] is False and view['driverError'] is None,
         'Controller fact was mistaken for final commit or changed resident local service')
    frame_matches(cv['observedProjection'],receipt);link_matches(cv['observedLink'],receipt)
    observed=p['normalObservation'];actual=observed['result']['receipt']
    need(observed['controllerObservationVersion']==1 and observed['result']['kind']=='observed','No independent normal IPC observation')
    receipt_matches(actual,command,local['record']);frame_matches(observed['result']['projection'],actual);link_matches(observed['result']['link'],actual)
    need(actual['session']['sessionId']==receipt['session']['sessionId']
         and actual['session']['controllerGeneration']==receipt['session']['controllerGeneration']
         and actual['session']['projectionSequence']>=receipt['session']['projectionSequence'], 'Normal observation is another/stale session')
    info=p['normalInfo'];physical=local['record']['stateOwned']
    need(info['generation']==physical['generation'] and info['build']==receipt['compatibility']['identity']['descriptor']['build']
         and type(info['activeTaskCount']) is int and info['activeTaskCount']==0 and info['maintenanceError'] is None and info['journalRecoveryRequired'] is None
         and info['fencedRequests']['context']==physical['context'] and info['fencedRequests']['legacyMutationsAllowed'] is False
         and info['updateCoordination']['readOnly'] is True and info['updateCoordination']['lease']['phase']=='quiesced'
         and all(info[k] is False for k in ('maintenanceAvailable','workflowAvailable','snapshotImportAvailable','snapshotDeltaAvailable','snapshotResumeAvailable')),
         'Network startup opened dispatch or changed provider/authority')
    connections=p['connections']
    need(connections['schemaVersion']==1 and connections['context']==physical['context'] and connections['workerGeneration']==physical['generation']
         and connections['provider']==info['build'] and connections['primary']=='legacy' and len(connections['connections'])==1,'Current connections scope differs')
    link_matches(connections['connections'][0],receipt)
    need(p['duplicate']['activationController']==cv and p['duplicate']['update']==view['update']
         and p['workerBytesAfter']==local['workerBytesAfter'] and p['normalHistory']==local['normalHistory']
         and p['savedControllerCommandCount']==1 and type(p['savedControllerCommandCount']) is int
         and all(p[k] is True for k in ('lostControllerAckSentOnce','sameChildVerified','sameAuthorityVerified','sameLocalPrefixVerified'))
         and all(p[k] is False for k in ('rootCommitted','maintenanceReleased','publicActivationEnabled')), 'Duplicate effect/history changed or unsupported grant claimed')
    v=cold['view'];c=v['activationController']
    need(v['schemaVersion']==18 and v['update']==view['update'] and c['phase']=='not_resident' and c['savedReceipt']==receipt
         and all(c[k] is None for k in ('residentProcess','processAlive','observedProjection','observedLink'))
         and cold['historyUnchanged'] is True and cold['automaticColdLaunchCount']==0 and cold['noColdNetworkReplay'] is True,
         'Cold controller history invented custody or automatic network replay')
    return command['commandId']


def audit_controller_case(case):
    key=verify_controller(case)
    base='activationController/'
    mutations=[('record/command/binding/serviceRecordSha256','0'*64),('record/command/binding/localRecordSha256','0'*64),
        ('record/command/deadlineAtMs',0),('record/revision',True),('record/state/phase','issued'),('record/state/receipt/authority/epoch',0),
        ('record/state/receipt/session/connectionAttempt',2),('record/state/receipt/session/projectionSequence',0),
        ('record/state/receipt/session/sessionId',str(uuid.uuid4())),('record/state/receipt/compatibility/updaterJournalSchema',20),
        ('confirmed/schemaVersion',17),('confirmed/update/phase','committed'),('confirmed/activationController/phase','not_resident'),
        ('confirmed/activationController/processAlive',None),('confirmed/activationController/observedProjection/sequence',0),
        ('confirmed/activationController/observedLink/attempts','2'),('normalObservation/result/receipt/authority/epoch',0),
        ('normalInfo/maintenanceAvailable',True),('normalInfo/fencedRequests/legacyMutationsAllowed',True),
        ('connections/workerGeneration','foreign'),('savedControllerCommandCount',2),('lostControllerAckSentOnce',False),
        ('rootCommitted',True),('maintenanceReleased',True)]
    changes=[(base+p,v) for p,v in mutations]+[('coldActivationController/noColdNetworkReplay',False),('coldActivationController/automaticColdLaunchCount',1),('coldActivationController/view/activationController/processAlive',True)]
    for path,value in changes:
        bad=copy.deepcopy(case);node=bad;parts=path.split('/')
        for k in parts[:-1]:node=node[k]
        need(canonical(node[parts[-1]])!=canonical(value),'No-op controller mutation: '+path);node[parts[-1]]=value
        try:verify_controller(bad)
        except (ValueError,KeyError,TypeError,IndexError):continue
        raise ValueError('Controller observer accepted mutation: '+path)
    return key,len(changes)


def audit(proof,source,target,reservation,*,profile=PROFILE):
    profile_schemas(profile)
    result=service.audit_local_prefix(proof,source,target,reservation,profile=profile,expected_schemas=(19,21),proof_kind=KIND)
    case,=proof['cases'];key,count=audit_controller_case(case)
    return dict(result,kind='main26_independent_audit',controllerCommandId=key,controllerNegativeAudits=count)
