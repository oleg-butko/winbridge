"""Read-only OS-spike evidence. Never a keeper owner, install or recovery grant."""
import copy
import hashlib
import json
from pathlib import PureWindowsPath
import re

MODES=('two-generation','keeper-loss')
TESTS=dict(zip(MODES,(
    'resident_keeper_launches_next_updater_outside_old_worker_job_without_breakaway',
    'keeper_process_loss_kills_waiting_worker_without_recovery_or_limit_changes')))


def need(condition,message):
    if not condition:raise ValueError(message)


def decode(raw):
    def pairs(items):
        value={}
        for k,v in items:
            need(k not in value,'Duplicate JSON field');value[k]=v
        return value
    def invalid(_):raise ValueError('Non-integer native evidence')
    return json.loads(raw.decode('utf-8-sig') if isinstance(raw,bytes) else raw,
                      object_pairs_hook=pairs,parse_float=invalid,parse_constant=invalid)


def integer(value,maximum=2**64-1,minimum=0):
    need(type(value) is int and minimum<=value<=maximum,'Invalid integer evidence')


def process(value):
    need(set(value)=={'processId','birth','hostId','sessionId'},'Unknown process fields')
    integer(value['processId'],2**32-1,1);integer(value['sessionId'],2**32-1)
    need(type(value['birth']) is str and re.fullmatch('[1-9][0-9]*',value['birth'])
         and int(value['birth'])<=2**64-1,'Invalid process creation time')
    need(type(value['hostId']) is str and re.fullmatch('[0-9a-f]{64}',value['hostId']),'Invalid host identity')
    return (value['processId'],value['birth'],value['hostId'],value['sessionId'])


def job(value,expected_process,name):
    need(set(value)=={'jobName','processId','creationTime','hostId','sessionId'},'Unknown Job identity fields')
    integer(value['creationTime'],minimum=1)
    integer(value['processId'],2**32-1,1);integer(value['sessionId'],2**32-1)
    need(value['jobName']==name and dict(processId=value['processId'],birth=str(value['creationTime']),
         hostId=value['hostId'],sessionId=value['sessionId'])==expected_process,'Job/process scope differs')


def local_path(value):
    need(type(value) is str,'Windows path required')
    if value.startswith('\\\\?\\'):value=value[4:]
    p=PureWindowsPath(value)
    need(p.is_absolute() and p.drive.lower()=='k:' and '..' not in p.parts,'Owned local K: fixture required')
    return p


def selection(command,log,mode):
    expected=['test','-p','winbridge-platform','--lib','--locked','--offline','--','--exact',
              'lifetime_spike::'+TESTS[mode],'--ignored','--test-threads=1','--nocapture']
    need(command['executable']=='cargo' and command['arguments']==expected,'Wrong exact native selection')
    integer(command['exitCode'],minimum=0);need(command['exitCode']==0,'Native command failed')
    log=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',log)
    summaries=re.findall(r'^test result: ([^\r\n]+)',log,re.MULTILINE)
    need(len(summaries)==1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;')
         and len(re.findall(r'^test lifetime_spike::'+TESTS[mode]+r' \.\.\.',log,re.MULTILINE))==1,
         'Native experiment did not execute exactly once')


def case(nodes,source,base,mode):
    p=nodes['proof.json'];meta=nodes['metadata.json'];old=p['old']
    common={'schemaVersion','kind','passed','platform','build','driver','transportJob','old','allHelpersExited',
            'limitsChanged','breakawayUsed','networkStarted','runtimeApplied','installedRuntimeApplied','windowsOpened'}
    need(set(p)==common|({'next','survived','oldDone','newDone','taskCancellation'} if mode=='two-generation'
                        else {'keeperExitCode','workerExited','automaticRecoveryCount'}),'Unknown proof fields/permissions')
    integer(meta['schemaVersion']);need(meta['schemaVersion']==1 and meta['mode']==mode,'Wrong fixture metadata')
    need(set(meta)=={'schemaVersion','mode','root','scopeId','ownerId'}
         and local_path(meta['root'])==local_path(str(PureWindowsPath(base)/mode)),'Wrong fixture root')
    scope=hashlib.sha256(meta['root'].encode()).hexdigest()
    need(meta['scopeId']==scope and meta['ownerId']=='00000000-0000-4000-8000-'+scope[:12],'Wrong Job namespace')
    name=lambda suffix:'Global\\WinBridge-LifetimeSpike-'+scope+'-'+suffix
    integer(p['schemaVersion']);need(p['schemaVersion']==1 and p['platform']=='windows' and p['passed'] is True,'Wrong native proof')
    need(all(p[k] is False for k in ('limitsChanged','breakawayUsed','networkStarted','runtimeApplied','installedRuntimeApplied','windowsOpened'))
         and p['allHelpersExited'] is True,'Unexpected native effect or unconfirmed cleanup')
    build=p['build']
    need(set(build)=={'version','sourceCommit','sourceManifestSha256','sourceDirty','buildId'}
         and type(build['version'])is str and build['version'],'Unknown native build fields')
    need(build['sourceCommit']==source['sourceCommit'] and build['sourceManifestSha256']==source['manifestSha256']
         and build['sourceDirty'] is False and type(build['buildId']) is str and build['buildId'],'Wrong native build')
    process(p['driver']);process(old['process']);process(old['worker']['process'])
    need(old==nodes['old-keeper.json'] and old['worker']==nodes['old-worker.json'],'Old keeper/worker changed')
    need(set(old)=={'process','worker','job','limits'} and set(old['worker'])=={'process','domain'},'Unknown old-role fields')
    job(old['job'],old['worker']['process'],name('old'))
    job(p['transportJob'],old['process'],name('transport'))
    integer(old['limits']);need(old['limits']==8192,'Original kill-on-close Job limits changed')
    domain=old['worker']['domain'];integer(domain['sessionId'],2**32-1)
    expected_domain='Global\\WinBridge-Domain-'+hashlib.sha256(
        (meta['root']+'|session:'+str(domain['sessionId'])+'|owner:'+meta['ownerId']).encode()).hexdigest()
    need(domain==nodes['task-domain.json'] and domain['jobName']==expected_domain
         and domain['hostId']==old['process']['hostId'] and domain['sessionId']==old['process']['sessionId']
         and domain['ownerId']==meta['ownerId'],'Original execution domain differs')
    task=nodes['cancelled-task.json'];process(task['process']);process(task['leaf'])
    need(task['process']==nodes['task.json']['process'] and task['leaf']==nodes['task.json']['leaf']==nodes['task-leaf.json']['process']
         and task['process']!=task['leaf'] and task['domain']==domain
         and all(task[k] is True for k in ('rootExited','leafExited','empty')),'Task/grandchild cancellation differs')
    job(task['taskJob'],task['process'],name('task'))
    for k in ('domainLimits','taskLimits'):integer(task[k]);need(task[k]==8192,'Task containment limits changed')
    observed=[p['driver'],old['process'],old['worker']['process'],task['process'],task['leaf']]
    if mode=='two-generation':
        need(p['kind']=='activation_lifetime_keeper_os_spike','Wrong two-generation proof')
        nxt=p['next'];process(nxt['process']);process(nxt['worker']['process'])
        need(nxt==nodes['new-keeper.json'] and nxt['worker']==nodes['new-worker.json']
             and nxt['inOldJob'] is False and nxt['inOldDomain'] is False and nxt['worker']['inOldJob'] is False,
             'Next generation inherited old activation Job/domain')
        need(set(nxt)=={'process','worker','job','limits','inOldJob','inOldDomain'}
             and set(nxt['worker'])=={'process','inOldJob'},'Unknown next-role fields')
        job(nxt['job'],nxt['worker']['process'],name('new'));integer(nxt['limits']);need(nxt['limits']==8192,'Next Job limits changed')
        need(nodes['request-next.json']['requester']==old['worker']['process']
             and nodes['start-next-keeper.json']==dict(process=nxt['process'],requester=old['worker']['process']),
             'Next request was not bound to its original requester/process')
        need(p['survived']==nodes['survived.json'] and p['survived']['oldKeeperExited']is True
             and p['survived']==dict(process=nxt['process'],worker=nxt['worker'],oldKeeper=old['process'],oldKeeperExited=True),
             'Next generation did not survive the original keeper exit')
        need(p['oldDone']==nodes['old-keeper-done.json']==dict(process=old['process'],oldJobEmpty=True,next=nxt['process'])
             and p['oldDone']['oldJobEmpty']is True and p['newDone']['newJobEmpty']is True
             and p['newDone']==nodes['new-keeper-done.json']==dict(process=nxt['process'],newJobEmpty=True,limits=8192),
             'Both generation Jobs must end empty')
        integer(p['newDone']['limits'])
        need(p['taskCancellation']==task,'Copied cancellation evidence differs')
        concurrent=[p['driver'],old['process'],old['worker']['process'],nxt['process'],nxt['worker']['process']]
        need(len({v['processId'] for v in concurrent})==len(concurrent),'Concurrent roles reused a PID')
        observed += [nxt['process'],nxt['worker']['process']]
    else:
        need(p['kind']=='activation_lifetime_keeper_loss_os_spike','Wrong keeper loss proof')
        integer(p['keeperExitCode']);integer(p['automaticRecoveryCount'])
        need(p['keeperExitCode']==73 and p['automaticRecoveryCount']==0 and p['workerExited'] is True
             and 'new-keeper.json' not in nodes,'Keeper loss did not stop the original worker without recovery')
    keys=[process(v) for v in observed]
    need(len(set(keys))==len(keys) and len({(v['hostId'],v['sessionId']) for v in observed})==1,
         'Process identities alias or cross a host/session')
    return p


def audit_case(nodes,source,base,mode):
    proof=case(nodes,source,base,mode)
    changes=[('proof.json/passed',False),('proof.json/schemaVersion',True),('proof.json/allHelpersExited',False),
        ('proof.json/limitsChanged',True),('proof.json/breakawayUsed',True),('proof.json/installedRuntimeApplied',True),
        ('proof.json/build/sourceDirty',True),('metadata.json/scopeId','0'*64),
        ('proof.json/transportJob/creationTime',0),('proof.json/old/limits',0),
        ('cancelled-task.json/domainLimits',0),('cancelled-task.json/leafExited',False),
        ('cancelled-task.json/leaf/birth','1'),('proof.json/driver/hostId','0'*64)]
    changes+= [('proof.json/next/inOldDomain',True),('proof.json/next/inOldJob',True),
        ('proof.json/survived/oldKeeperExited',False),('start-next-keeper.json/process/birth','1'),
        ('proof.json/newDone/newJobEmpty',False)] if mode=='two-generation' else [
        ('proof.json/keeperExitCode',0),('proof.json/workerExited',False),('proof.json/automaticRecoveryCount',1)]
    for path,value in changes:
        bad=copy.deepcopy(nodes);parts=path.split('/');item=bad
        for part in parts[:-1]:item=item[part]
        need(item[parts[-1]]!=value or type(item[parts[-1]]) is not type(value),'No-op negative mutation')
        item[parts[-1]]=value
        try:case(bad,source,base,mode)
        except (ValueError,TypeError,KeyError):continue
        raise ValueError('Observer accepted mutation: '+path)
    return proof,len(changes)


def audit(files,source,base):
    count=0;proofs=[]
    for mode in MODES:
        prefix=mode+'-';nodes={name[len(prefix):]:decode(data) for name,data in files.items()
                             if name.startswith(prefix) and name.endswith('.json')}
        for name in nodes:need(files.get(prefix+name+'.ready')==b'','Native publication marker missing')
        proof,negative=audit_case(nodes,source,base,mode);count+=negative;proofs.append(proof)
    need(proofs[0]['build']==proofs[1]['build'],'Native cases used different executable builds')
    return dict(passed=True,kind='activation_lifetime_keeper_independent_audit',negativeAudits=count,
                nativeCases=2,build=proofs[0]['build'])
