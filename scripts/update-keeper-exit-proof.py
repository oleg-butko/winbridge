"""Closed keeper OS-loop evidence, never an updater SQL/bootstrap proof."""
import copy,importlib.util,re,uuid
from pathlib import Path,PureWindowsPath
s=importlib.util.spec_from_file_location('exit_os_base',Path(__file__).with_name('update-lifetime-spike-proof.py'));b=importlib.util.module_from_spec(s);s.loader.exec_module(b)
need,decode,integer,process=b.need,b.decode,b.integer,b.process
MODES=('keeper-exit',)
TEST='supervisor::keeper_control::exit_tests::keeper_exit_waits_for_known_successor_original_job_and_readers'
DUAL='infrastructure::tests::keeper_launch_requires_two_existing_jobs_and_an_exact_local_parent'
SCOPE='real closed keeper loop/root exit/Job drain/independent successor; synthetic role history and Ready claim, no updater workflow'
TRUE=('passed','syntheticRoleHistory','syntheticReadyClaim')
FALSE=('runtimeApplied','installedRuntimeApplied','windowsOpened','networkUsed','bootstrapCreationProved','updaterWorkflowProved')
def selections(commands,files):
    need(len(commands)==2,'Exactly two native commands required')
    for c,name,package,test in zip(commands,('dual-job-launch','keeper-exit'),('winbridge-platform','winbridge-update'),(DUAL,TEST)):
        args=['test','-p',package]+(['--features','bundled-sqlite'] if name=='keeper-exit' else [])+['--lib','--locked','--offline','--','--exact',test]+(['--ignored'] if name=='keeper-exit' else [])+['--test-threads=1','--nocapture']
        need(c['name']==name and c['log']==name+'.log' and c['executable']=='cargo' and type(c['exitCode']) is int and c['exitCode']==0 and c['arguments']==args,'Wrong keeper-exit command/selection')
        log=files[name+'.log'].decode('utf-8-sig');summaries=re.findall(r'^test result: ([^\r\n]+)',log,re.M)
        need(len(summaries)==1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;') and len(re.findall(r'^test '+re.escape(test)+r' \.\.\. ok\s*$',log,re.M))==1 and len(re.findall(r'^running 1 test\s*$',log,re.M))==1,'Keeper-exit unit did not run exactly once')
def status(value,case,*,alive,empty,finished):
    fields={'phase','binding','roleSelectionKnown','writerClosed','authorityReleased','originalCustodyHeld','workerAlive','jobTreeEmpty','stdoutTail','stderrTail','outputIncomplete','readersFinished','reason'}
    need(set(value)==fields,'Unknown/missing original keeper status')
    need(value['binding']==case['binding'] and value['phase']==('keeper' if alive else 'worker_exited'),'Status changed original binding/phase')
    need(all(value[k] is True for k in ('roleSelectionKnown','writerClosed','authorityReleased','originalCustodyHeld')),'Original custody lost')
    need(value['workerAlive'] is alive and value['jobTreeEmpty'] is empty and value['readersFinished'] is finished and value['reason'] is None,'Wrong root/Job/readers observation')
    need(type(value['outputIncomplete']) is bool and all(type(value[k]) is str and len(value[k])<=512 for k in ('stdoutTail','stderrTail')),'Unbounded output diagnostics')
def verify(p,source,base,files):
    need(set(p)==set(TRUE+FALSE+('schemaVersion','kind','platform','build','scope','cases')),'Unknown/missing keeper-exit proof fields')
    integer(p['schemaVersion'],1,1)
    need(p['kind']=='keeper_exit_native_proof' and p['platform']=='windows' and p['scope']==SCOPE,'Wrong keeper exit scope')
    need(all(p[k] is True for k in TRUE) and all(p[k] is False for k in FALSE),'Incorrect effect/synthetic-role claims')
    build=p['build'];need(set(build)=={'version','buildId','sourceCommit','sourceManifestSha256','sourceDirty'},'Unknown producer identity')
    need(build['sourceCommit']==source['sourceCommit'] and build['sourceManifestSha256']==source['manifestSha256'] and build['sourceDirty'] is False,'Producer is not pinned clean source')
    need(all(type(build[k]) is str and 0<len(build[k])<=128 for k in ('version','buildId')),'Invalid producer identity')
    need(type(p['cases']) is list and len(p['cases'])==2,'Two distinct keeper-exit cases required')
    for case,mode in zip(p['cases'],('confirmed','unconfirmed')):
        fields={'mode','root','keeper','worker','leaf','successor','jobs','binding','beforeRootExit','afterRootExit','unconfirmedAfterDrain','idleSamples','keeperExitSeen','keeperExitedNaturally','successorOutsideBothJobsBeforeExit','successorSurvivedKeeper','successorExitSeen','workerExitSeen','leafExitSeen','stdoutLog','stderrLog'}
        need(set(case)==fields and case['mode']==mode and PureWindowsPath(case['root'])==PureWindowsPath(base)/mode,'Wrong case namespace/shape/order')
        procs=[process(case[k]) for k in ('keeper','worker','leaf','successor')]
        need(len({v[0] for v in procs})==4 and all(v[2:]==procs[0][2:] for v in procs) and all(int(v[1])>int(procs[0][1]) for v in procs[1:]),'Aliased/foreign process scope')
        jobs=case['jobs'];need(type(jobs) is list and len(jobs)==2 and jobs[0]!=jobs[1] and all(type(j) is str and 0<len(j)<=1024 and not any(ord(c)<32 for c in j) for j in jobs),'Two existing distinct Job scopes required')
        binding=case['binding']
        need(set(binding)=={'updateId','launchId','coordinator','worker','jobName','generation','context','finalizationCommandId','finalizationCommandSha256','selectionSha256'},'Unknown keeper binding fields')
        need(set(binding['context'])=={'instanceId','journalIncarnation'},'Unknown worker context')
        for value in [binding[k] for k in ('updateId','launchId','generation','finalizationCommandId')]+[binding['context']['journalIncarnation']]:
            need(type(value) is str and str(uuid.UUID(value))==value,'Noncanonical original keeper identity')
        need(type(binding['context']['instanceId']) is str and re.fullmatch('[A-Za-z0-9][A-Za-z0-9._-]{0,127}',binding['context']['instanceId']),'Invalid worker instance')
        need(all(type(binding[k]) is str and re.fullmatch('[0-9a-f]{64}',binding[k]) for k in ('finalizationCommandSha256','selectionSha256')),'Keeper role identity missing')
        need(binding['coordinator']==case['keeper'] and binding['worker']==case['worker'] and binding['jobName']==jobs[0],'Original process/Job binding changed')
        for k in ('keeperExitSeen','successorOutsideBothJobsBeforeExit','successorSurvivedKeeper','successorExitSeen','workerExitSeen','leafExitSeen'):need(case[k] is True,'Missing OS observation '+k)
        confirmed=mode=='confirmed';need(case['keeperExitedNaturally'] is confirmed,'Manual stop confused with natural keeper exit')
        status(case['beforeRootExit'],case,alive=True,empty=False,finished=False)
        status(case['afterRootExit'],case,alive=False,empty=False,finished=False)
        if confirmed:need(case['unconfirmedAfterDrain'] is None,'Confirmed case contains unrelated retention state')
        else:status(case['unconfirmedAfterDrain'],case,alive=False,empty=True,finished=True)
        samples=case['idleSamples'];need(type(samples) is list and len(samples)==(1 if confirmed else 2),'Both idle wait phases must be sampled')
        for sample in samples:
            need(set(sample)=={'elapsedNs','cpuTicks100ns'},'Unknown idle sample fields');integer(sample['elapsedNs'],minimum=400_000_000);integer(sample['cpuTicks100ns'])
            need(sample['cpuTicks100ns']*100<=sample['elapsedNs']//4,'Keeper idle sample exceeds25% of one CPU')
        need(case['stdoutLog']==mode+'-helper.stdout.log' and case['stderrLog']==mode+'-helper.stderr.log','Helper logs detached from case')
        log=files[case['stderrLog']].decode('utf-8-sig')
        events=[]
        for line in log.splitlines():
            if line.startswith('WinBridge keeper '):
                e=decode(line[len('WinBridge keeper '):])
                if e.get('event')=='original_keeper_exit':events.append(e)
        need(len(events)==int(confirmed),'Wrong production keeper-exit event count')
        if confirmed:
            e=events[0];need(set(e)=={'event','keeperPid','workerPid','successorReadyKnown','workerAlive','jobTreeEmpty','readersFinished','outputIncomplete'},'Unknown exit event')
            integer(e['keeperPid'],2**32-1,1);integer(e['workerPid'],2**32-1,1)
            need(e['keeperPid']==case['keeper']['processId'] and e['workerPid']==case['worker']['processId'] and e['successorReadyKnown'] is True and e['workerAlive'] is False and e['jobTreeEmpty'] is True and e['readersFinished'] is True and type(e['outputIncomplete']) is bool,'Exit event contradicts retained process/Job/readers')
            stdout=files[case['stdoutLog']].decode('utf-8-sig');need('test result: ok. 1 passed; 0 failed; 0 ignored;' in stdout,'Keeper helper did not exit normally')
def audit(files,source,base):
    expected={'report.json','dual-job-launch.log','keeper-exit.log','keeper-exit-proof.json',*(m+'-helper.'+stream+'.log' for m in ('confirmed','unconfirmed') for stream in ('stdout','stderr'))}
    need(set(files)==expected,'Unknown/missing keeper exit result files')
    selections(decode(files['report.json'])['commands'],files)
    p=decode(files['keeper-exit-proof.json']);verify(p,source,base,files)
    changes=[(k,False)for k in TRUE]+[(k,True)for k in FALSE]+[('schemaVersion',True),('scope','full updater workflow'),('build/sourceDirty',True),('build/sourceCommit','0'*40)]
    for index in (0,1):
        prefix=f'cases/{index}/'
        changes += [(prefix+k,False)for k in ('keeperExitSeen','successorOutsideBothJobsBeforeExit','successorSurvivedKeeper','successorExitSeen','workerExitSeen','leafExitSeen')]
        changes += [(prefix+'keeper/birth','0'),(prefix+'afterRootExit/jobTreeEmpty',True),(prefix+'afterRootExit/readersFinished',True),(prefix+'afterRootExit/originalCustodyHeld',False),(prefix+'idleSamples/0/cpuTicks100ns',2**63)]
    changes += [('cases/0/keeperExitedNaturally',False),('cases/1/keeperExitedNaturally',True),('cases/1/unconfirmedAfterDrain/jobTreeEmpty',False),('cases/1/unconfirmedAfterDrain/readersFinished',False)]
    for path,value in changes:
        bad=copy.deepcopy(p);parts=path.split('/');node=bad
        for part in parts[:-1]:node=node[int(part)] if isinstance(node,list) else node[part]
        key=int(parts[-1]) if isinstance(node,list) else parts[-1];need(node[key]!=value or type(node[key])!=type(value),'No-op exit mutation '+path);node[key]=value
        try:verify(bad,source,base,files)
        except (ValueError,KeyError,TypeError):continue
        raise ValueError('Exit observer accepted mutation '+path)
    # An independently captured production event must also resist coordinated
    # edits of every proof-side keeper identity/binding/status.
    bad=copy.deepcopy(p)
    for case in bad['cases']:
        case['keeper']['processId']+=4;case['binding']['coordinator']['processId']+=4
        for k in ('beforeRootExit','afterRootExit','unconfirmedAfterDrain'):
            if case[k] is not None:case[k]['binding']['coordinator']['processId']+=4
    try:verify(bad,source,base,files)
    except (ValueError,KeyError,TypeError):pass
    else:raise ValueError('Production exit trace was not independently bound')
    return dict(passed=True,scope=SCOPE,negativeAudits=len(changes)+1,closedKeeperExitUnitProved=True,bootstrapCreationProved=False,updaterWorkflowProved=False)
