"""Independent Main31: original keeper, closed writer and real new OS owner.

The finalization prefix is checked directly against its actual24 schema; frozen
Main30 stays23. No saved role grants a Job, writer-close fact or bootstrap.
"""
import copy
import importlib.util
from pathlib import Path
s=importlib.util.spec_from_file_location('keeper_finalization_prefix',Path(__file__).with_name('update-activation-finalization-proof.py'))
f=importlib.util.module_from_spec(s);s.loader.exec_module(f)
need,canonical,digest=f.need,f.canonical,f.digest
verify_worker_migration=f.verify_worker_migration
audit_auxiliary=f.audit_auxiliary
verify_activity=f.verify_activity
verify_resident_activity=f.verify_resident_activity
service,controller=f.service,f.controller
PROFILE='worker19-updater24'
KIND='activation_keeper_main_native_proof'
TEST='actual_main_retires_coordinator_without_losing_worker_job_or_readers'
REASON='Original coordinator selected keeper retirement; writer/authority release require live confirmation'

def profile_schemas(profile):
    need(type(profile) is str and profile==PROFILE,'Main31 requires explicit worker19/updater24')
    return 19,24

def verify_main_selection(command,log):
    # Reuse exact framing checks without changing archived Main30's selector.
    import re
    need(command['arguments']==['test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--test','update_shutdown','--locked','--offline','--','--exact',TEST,'--ignored','--test-threads=1','--nocapture'],'Main31 exact selection differs')
    log=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',log)
    summaries=re.findall(r'^test result: ([^\r\n]+)',log,re.MULTILINE)
    need(len(re.findall(r'^test '+re.escape(TEST)+r' \.\.\.',log,re.MULTILINE))==1 and re.search(r'^running 1 test\s*$',log,re.MULTILINE) and len(summaries)==1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;'),'Main31 did not execute exactly once')

def verify_boundaries(commands,files):
    f.verify_boundaries(commands,files,finalization_model_count=29)
    duplicate,=[c for c in commands if c['name']=='keeper-control-duplicates']
    need(duplicate['arguments']==['test','-p','winbridge-update','--features','bundled-sqlite,update-validation','--lib','supervisor::tests::supervisor_dispatch_preserves_duplicate_field_rejection_for_both_wire_contracts','--locked','--offline','--','--exact','--test-threads=1','--nocapture'] and 'test result: ok. 1 passed; 0 failed; 0 ignored;' in files[duplicate['log']].decode('utf-8-sig'),'Main31 strict duplicate-key regression missing')
    command,=[c for c in commands if c['name']=='keeper-control-cold']
    need(command['arguments']==['test','-p','winbridge-update','--features','bundled-sqlite,update-validation','--lib','supervisor::tests::keeper_control_rejects_cold_unbound_and_malformed_requests_without_retiring','--locked','--offline','--','--exact','--test-threads=1','--nocapture'],'Main31 cold keeper selection differs')
    need('test result: ok. 1 passed; 0 failed; 0 ignored;' in files[command['log']].decode('utf-8-sig'),'Main31 cold keeper test missing')

def verify(proof,source,target,reservation):
    command=f.verify(proof,source,target,reservation,updater_schema=24,kind=KIND)
    case,=proof['cases']; k=case['activationKeeper']; p=case['activationFinalization']
    life=case['activationLifetime']['record']['command']['binding']
    selection=k['selection']; before=selection['before']; selected=selection['selected']; final=p['record']; receipt=final['state']['receipt']
    need(selection['schemaVersion']==1 and selection['finalization']==final and selected==k['role'],'Keeper selection detached from retained finalization/role')
    need(before['phase']=='active' and before['spec']['updateId']==life['updateId'] and before['spec']['launchId']==life['launchId'] and before['process']==life['keeper'] and before['ownerEpoch']==life['updaterEpoch'] and before['ownerId']==life['updaterOwnerId'],'Another coordinator selected Keeper')
    need(f.integer(before['revision']) and f.integer(selected['updatedAtMs']) and selected==dict(before,phase='keeper',revision=before['revision']+1,updatedAtMs=selected['updatedAtMs'],reason=REASON),'Keeper changed immutable supervisor fields')
    query,observation=selection['query'],selection['observation']
    need(query['finalizationObservationVersion']==observation['finalizationObservationVersion']==1 and query['command']==final['command'] and f.uuid_string(query['requestId']) and observation['requestId']==query['requestId'] and observation['result']['kind']=='observed' and observation['result']['receipt']==receipt,'Keeper lacks a fresh independent original Active observation')
    at=observation['result']['observedAtMs']
    need(f.integer(at) and at>=receipt['compatibility']['observedAtMs'] and selected['updatedAtMs']>=max(before['updatedAtMs'],final['updatedAtMs'],at) and selected['updatedAtMs']-at<=10000,'Keeper observation expired or clock regressed')
    binding=dict(updateId=life['updateId'],launchId=life['launchId'],coordinator=life['keeper'],worker=receipt['compatibility']['identity']['process'],jobName=life['jobName'],generation=receipt['compatibility']['generation'],context=receipt['compatibility']['context'],finalizationCommandId=command,finalizationCommandSha256=receipt['commandSha256'],selectionSha256=digest('winbridge.keeper.selection.v1',selection))
    for name in ('first','duplicate','second'):
        status=k[name]
        need(status['phase']=='keeper' and status['binding']==binding and all(status[x] is True for x in ('roleSelectionKnown','writerClosed','authorityReleased','originalCustodyHeld','workerAlive')) and status['jobTreeEmpty'] is False and status['reason'] is None,'Keeper did not retain the same original Job after closing both writer and authority')
    old,new=k['before'],k['newOwner']
    need(old==p['updaterAfter'][0] and old['schemaVersion']==new['schemaVersion']==24 and f.integer(new['ownerEpoch'],old['ownerEpoch']+1) and all(new[x]==old[x] for x in ('installationId','scopeSha256','activeUpdateId','lastSequence')) and k['actualNewOwnerAcquired'] is True,'No real new owner epoch while original worker is alive')
    info=k['infoAfter']; active=p['infoBefore']
    need(info['generation']==active['generation']==binding['generation'] and info['build']==active['build'] and info['fencedRequests']['context']==active['fencedRequests']['context']==binding['context'] and info['maintenanceAvailable'] is True and k['sameWorkerAndJobRetained'] is True and k['nextGenerationStarted'] is False,'Same Active worker was not observed after OS ownership transfer')
    accepted,duplicate,task=k['accepted'],k['duplicateTask'],k['task']
    need(all(r['requestContractVersion']==2 and r['outcome']=='responded' and r['context']==binding['context'] and r['response']['kind']=='task' and r['response']['task']['taskId']==task['taskId'] for r in (accepted,duplicate)) and task['status']=='succeeded' and f.integer(task['exitCode'],0) and task['submission']==accepted['response']['task']['submission']==duplicate['response']['task']['submission'] and task['submission']['command']['executable']=='cmd.exe' and task['submission']['command']['arguments']==['/d','/c','echo KEEPER>>main31-keeper-count.txt'] and k['taskEffect'].splitlines()==['KEEPER'],'Post-retirement fenced task or dedup failed')
    need(f.integer(k['idleWallMs']) and k['idleWallMs']>=3000 and f.integer(k['idleCpuTicks100ns']) and k['idleCpuTicks100ns']<=k['idleWallMs']*1000,'Keeper idle sample missing or busy polling')
    return command

def audit(proof,source,target,reservation,*,profile=PROFILE):
    profile_schemas(profile)
    command=verify(proof,source,target,reservation)
    changes=[('first/writerClosed',False),('first/authorityReleased',False),('first/originalCustodyHeld',False),('first/jobTreeEmpty',True),('duplicate/binding/selectionSha256','0'*64),('second/workerAlive',False),('role/phase','active'),('selection/finalization/revision',1),('selection/query/requestId','invalid'),('selection/observation/result/kind','pending'),('newOwner/ownerEpoch',proof['cases'][0]['activationKeeper']['before']['ownerEpoch']),('actualNewOwnerAcquired',False),('sameWorkerAndJobRetained',False),('nextGenerationStarted',True),('idleCpuTicks100ns',9999999999),('idleWallMs',True),('infoAfter/generation','other'),('taskEffect','KEEPER\nKEEPER\n'),('task/exitCode',True)]
    for path,value in changes:
        bad=copy.deepcopy(proof);node=bad['cases'][0]['activationKeeper'];parts=path.split('/')
        for part in parts[:-1]:node=node[part]
        need(canonical(node[parts[-1]])!=canonical(value),'No-op keeper negative '+path)
        node[parts[-1]]=value
        try:verify(bad,source,target,reservation)
        except (ValueError,KeyError,TypeError,IndexError):continue
        raise ValueError('Keeper observer accepted mutation '+path)
    return dict(kind='main31_independent_audit',finalizationCommandId=command,keeperNegativeAudits=len(changes),physicalV8Proved=True,authenticatedActiveProved=True,permanentKeeperProved=True,newOsOwnerProved=True,nextGenerationStarted=False,installedRuntimeApplied=False)
