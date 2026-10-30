"""Independent actual23 ->24 copy migration, bound to a successful Main30."""
import copy
import hashlib
import importlib.util
import re
import subprocess
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def load(name,file):
    s=importlib.util.spec_from_file_location(name,ROOT/'scripts'/file);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
old=load('keeper_migration_json','update-activation-finalization-migration-proof.py')
need,parse_json=old.need,old.parse_json
NETWORK_SESSION_PATH=old.NETWORK_SESSION_PATH

def derive_reference(meta,report,proof,audit,checks):
    need(meta['profile']=='worker19-updater23' and audit['kind']=='main30_independent_audit' and all(audit[k] is True for k in ('physicalV8Proved','authenticatedActiveProved','interruptedV8ReplyProved','installedWorkerUnchanged')) and audit['permanentKeeperProved'] is False and audit['run']==meta['run'] and audit['taskId']==meta['taskId'] and audit['runtimeArchive']==report['runtimeArchive'],'Exact successful Main30 reference required')
    need(report['kind']=='activation_finalization_main_native_gate' and report['passed'] is True and report['error'] is None and report['sourceCommit']==meta['source']['sourceCommit'] and report['sourceArchiveSha256']==meta['source']['archiveSha256'] and report['target']==meta['target'],'Another Main30 source/result')
    case,=proof['cases'];physical=case['activationWriter']['record'];writer=case['activationWriter']['writerRecord'];service=case['activationWriter']['serviceRecord'];controller=case['activationController']['record'];lifetime=case['activationLifetime']['record'];final=case['activationFinalization']['record'];root=case['activationFinalization']['updaterAfter'][1]
    need(physical['revision']==7 and physical['state']==dict(phase='local_verified') and physical['intent']['context']['updaterJournalSchema']==23 and physical['intent']['context']['workerJournalSchema']==19 and all(v['state']['phase']=='confirmed' for v in (writer,service,controller,lifetime,final)) and root['phase']=='committed' and case['afterOwnedExit']['record']==final,'Actual23 confirmed prefix required')
    windows=checks.windows_path;archive=windows(report['runtimeArchive']['directory'])
    need(archive.is_absolute() and archive.drive.lower()=='k:' and archive.parent==windows(meta['workingDirectory'])/'shutdown-main-cases' and archive.name.startswith('runtime-archive-'),'Archive outside exact Main30 fixture')
    import uuid
    need(str(uuid.UUID(archive.name.removeprefix('runtime-archive-')))==archive.name.removeprefix('runtime-archive-'),'Noncanonical archive identity')
    images=case['replacement']['actualInstalledFiles'];manifest=case['replacement']['actualManifest'];artifact,=(a for a in manifest['artifacts'] if a['name']=='winbridge-update.exe')
    need(len(images)==5 and windows(images[3]['path']).parent==windows(meta['testRuntimeDirectory']) and artifact['sha256']==images[3]['sha256'] and artifact['descriptor']['build']['sourceCommit']==meta['target']['sourceCommit'] and artifact['descriptor']['build']['sourceManifestSha256']==meta['target']['manifestSha256'],'Actual23 updater artifact differs')
    image=dict(images[3],path='\\\\?\\'+str(archive/'winbridge-update.exe'))
    sha=lambda v:hashlib.sha256(checks.canonical(v)).hexdigest()
    reference=dict(schemaVersion=1,kind='activation_schema23_reference',referenceRun=meta['run'],reportSha256=audit['reportSha256'],proofSha256=audit['proofSha256'],source=meta['target'],image=image,descriptor=artifact['descriptor'],manifestSha256=images[4]['sha256'],updateId=physical['intent']['source']['updateId'],activationSha256=sha(physical),writerSha256=sha(writer),serviceSha256=sha(service),controllerSha256=sha(controller),lifetimeSha256=sha(lifetime),finalizationSha256=sha(final),rootSha256=sha(root))
    return reference,dict(originalImage=images[3],archivedImage=image,archive=report['runtimeArchive'])

def reference_from_main30(run_info,source_archive,target_archive):
    result=subprocess.run([sys.executable,str(ROOT/'scripts/verify-update-finalization-main-native.py'),'--run-info',str(run_info),'--source-archive',str(source_archive),'--target-archive',str(target_archive)],check=True,capture_output=True,text=True)
    audit,meta=parse_json(result.stdout),parse_json(run_info.read_bytes())
    reader=load('keeper_reference_files','summarize-update-timings.py');checks=load('keeper_reference_canonical','update-activation-preparation-proof.py')
    files=reader.verified_files(Path(meta['reportDirectory']))
    need(files==reader.verified_files(Path(meta['reportDirectory'])),'Main30 reference changed')
    need(hashlib.sha256(files['report.json']).hexdigest()==audit['reportSha256'] and hashlib.sha256(files['candidate-health-actor-proof.json']).hexdigest()==audit['proofSha256'],'Main30 reference SHA differs')
    report,proof=(parse_json(files[n].decode('utf-8-sig')) for n in ('report.json','candidate-health-actor-proof.json'))
    reference,location=derive_reference(meta,report,proof,audit,checks)
    return reference,location,audit,meta

TEST='actual_twenty_three_finalization_survives_twenty_four_and_old_twenty_three_refuses'
def verify_selection(command,log):
    need(command['executable']=='cargo' and type(command['exitCode']) is int and command['exitCode']==0 and command['arguments']==['test','-p','winbridge-update','--features','bundled-sqlite','--test','activation_keeper_migration','--locked','--offline','--','--exact',TEST,'--ignored','--test-threads=1','--nocapture'],'Another keeper migration command selected')
    text=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',log);summaries=re.findall(r'^test result: ([^\r\n]+)',text,re.MULTILINE)
    need(len(re.findall(r'^test '+re.escape(TEST)+r' \.\.\.',text,re.MULTILINE))==1 and len(summaries)==1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;') and re.search(r'^running 1 test\s*$',text,re.MULTILINE),'Actual23 migration did not run exactly once')
def proof_context(reference_proof):
    case,=reference_proof['cases'];view=case['afterOwnedExit']['view']
    need(view['schemaVersion']==21,'Actual23 cold finalization view required')
    return dict(expected_cold_view=copy.deepcopy(view))

def verify(proof,source,reference,checks,*,expected_cold_view):
    checks.strict(proof,uuid_session_paths=frozenset({NETWORK_SESSION_PATH}))
    load('keeper_migration_frames','update-activation-service-proof.py').strict(proof)
    common={'schemaVersion','kind','passed','platform','build','referenceProofSha256','referenceReportSha256','referenceImage','referenceManifestSha256','before','after','afterColdObservation','backupInfo','retainedActivation','retainedRoot','retainedWriter','rowsBefore','rowsAfter','backupRows','gateRows','sourceBytesUnchanged','copyBytesUnchangedByOld','noFreshReplacementPermit','existingReplacementObserved','migrationBackupVerified','runtimeApplied','installedRuntimeApplied','windowsOpened','oldTwentyThreeInitRefused','oldTwentyThreeStatusRefused','noFreshKeeperSelection','retainedService','retainedController','retainedLifetime','retainedFinalization','coldFinalization'}
    need(set(proof)==common,'Unknown/missing keeper migration fields')
    need(proof['schemaVersion']==1 and proof['kind']=='activation_keeper_schema_migration_proof' and proof['passed'] is True and proof['platform']=='windows' and all(proof[k] is False for k in ('runtimeApplied','installedRuntimeApplied','windowsOpened')),'Not a completed copy-only keeper migration')
    need(proof['build']['sourceCommit']==source['sourceCommit'] and proof['build']['sourceManifestSha256']==source['manifestSha256'] and proof['build']['sourceDirty'] is False and proof['referenceImage']==reference['image'] and proof['referenceManifestSha256']==reference['manifestSha256'] and proof['referenceProofSha256']==reference['proofSha256'] and proof['referenceReportSha256']==reference['reportSha256'],'Migration producer/reference differs')
    before,after=proof['before'],proof['after']
    need(before['schemaVersion']==23 and after==dict(before,schemaVersion=24,ownerEpoch=before['ownerEpoch']+1) and proof['afterColdObservation']==dict(after,ownerEpoch=after['ownerEpoch']+1) and proof['backupInfo']==before and before['activeUpdateId'] is None,'Migration changed installation/root/sequence/fencing')
    values=[('retainedActivation','activationSha256'),('retainedWriter','writerSha256'),('retainedService','serviceSha256'),('retainedController','controllerSha256'),('retainedLifetime','lifetimeSha256'),('retainedFinalization','finalizationSha256'),('retainedRoot','rootSha256')]
    need(all(hashlib.sha256(checks.canonical(proof[v])).hexdigest()==reference[h] for v,h in values),'Migration changed retained23 prefix')
    physical,root=proof['retainedActivation'],proof['retainedRoot']
    need(physical['revision']==7 and physical['state']==dict(phase='local_verified') and physical['intent']['context']['updaterJournalSchema']==23 and physical['intent']['context']['workerJournalSchema']==19 and physical['intent']['source']['updateId']==reference['updateId']==root['updateId'] and root['phase']=='committed' and all(proof[k]['state']['phase']=='confirmed' for k in ('retainedWriter','retainedService','retainedController','retainedLifetime','retainedFinalization')),'Historical23 data were relabelled')
    need(before['installationId']==physical['intent']['source']['installationId']==root['request']['expected']['installationId'] and before['scopeSha256']==physical['intent']['source']['scopeSha256']==root['request']['expected']['scopeSha256'] and before['ownerEpoch']==physical['intent']['source']['ownerEpoch'] and before['lastSequence']>=root['revision']+1,'Migration headers detached from pinned installation/epoch/history')
    need(type(proof['gateRows']) is int and proof['gateRows']==0 and proof['rowsBefore']==proof['rowsAfter']==proof['backupRows'] and re.fullmatch(r'[0-9a-f]{64}',proof['rowsBefore']) and all(proof[k] is True for k in ('oldTwentyThreeInitRefused','oldTwentyThreeStatusRefused','sourceBytesUnchanged','copyBytesUnchangedByOld','noFreshReplacementPermit','existingReplacementObserved','migrationBackupVerified','noFreshKeeperSelection')),'Missing retention/backup/old refusal/no-keeper evidence')
    view=proof['coldFinalization']
    need(view['schemaVersion']==21 and view['update']==root and view['driverActive'] is False and view['driverError'] is None and type(view['observedAtMs']) is int and view['observedAtMs']>=expected_cold_view['observedAtMs'] and checks.canonical({k:v for k,v in view.items() if k!='observedAtMs'})==checks.canonical({k:v for k,v in expected_cold_view.items() if k!='observedAtMs'}),'Cold read changed pinned history or recreated custody')
    cold=view['activationFinalization'];need(cold['phase']=='no_original_custody' and cold['durablePhase']=='confirmed' and all(cold[k] is None for k in ('residentProcess','processAlive','originalCustodyHeld','verificationPinsRetired','deliveryAttempted','interruptedReplyPrefix','observation')),'Cold migration invented a live original owner')

def audit(proof,source,reference,checks,*,expected_cold_view):
    verify(proof,source,reference,checks,expected_cold_view=expected_cold_view)
    changes=[('before/schemaVersion',22),('after/schemaVersion',23),('after/ownerEpoch',0),('afterColdObservation/lastSequence',0),('backupInfo/ownerEpoch',0),('gateRows',True),('gateRows',1),('retainedFinalization/state/phase','indeterminate'),('retainedRoot/phase','awaiting_controller'),('retainedActivation/revision',6),('rowsAfter','0'*64),('backupRows','0'*64),('oldTwentyThreeInitRefused',False),('oldTwentyThreeStatusRefused',False),('sourceBytesUnchanged',False),('copyBytesUnchangedByOld',False),('noFreshReplacementPermit',False),('noFreshKeeperSelection',False),('installedRuntimeApplied',True),('coldFinalization/activationFinalization/originalCustodyHeld',True),('coldFinalization/activationFinalization/phase','confirmed'),('coldFinalization/driverActive',True)]
    for path,value in changes:
        bad=copy.deepcopy(proof);node=bad;parts=path.split('/')
        for part in parts[:-1]:node=node[part]
        need(checks.canonical(node[parts[-1]])!=checks.canonical(value),'No-op keeper migration negative '+path);node[parts[-1]]=value
        try:verify(bad,source,reference,checks,expected_cold_view=expected_cold_view)
        except (ValueError,KeyError,TypeError):continue
        raise ValueError('Keeper migration observer accepted mutation '+path)
    return len(changes)
