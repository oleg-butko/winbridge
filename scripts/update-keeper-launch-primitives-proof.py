"""Audit exact native OS units; this is NOT a full keeper bootstrap proof."""
import copy
import importlib.util
from pathlib import Path
import re
s=importlib.util.spec_from_file_location('launch_primitives_base',Path(__file__).with_name('update-lifetime-spike-proof.py'))
b=importlib.util.module_from_spec(s);s.loader.exec_module(b)
need,decode,integer=b.need,b.decode,b.integer
MODES=()
CASES=(('dual-job-launch','winbridge-platform','infrastructure::tests::keeper_launch_requires_two_existing_jobs_and_an_exact_local_parent'),
       ('predecessor-exit','winbridge-update','runtime_replacement::physical::activation_files::activation_process::finalization::keeper::predecessor_tests::retained_predecessor_handle_rejects_live_missing_and_changed_exit_claims'))
SCOPE='production dual-Job creation primitive and retained predecessor handle OS units; no delegated coordinator workflow'
def arguments(package,test):
    return ['test','-p',package]+(['--features','bundled-sqlite'] if package=='winbridge-update' else [])+['--lib','--locked','--offline','--','--exact',test,'--test-threads=1','--nocapture']
def selections(commands,files):
    need(len(commands)==2,'Exactly two OS units required')
    for command,(name,package,test) in zip(commands,CASES):
        need(command['name']==name and command['log']==name+'.log' and command['executable']=='cargo'
             and type(command['exitCode']) is int and command['exitCode']==0,'Wrong primitive command/log/exit')
        need(command['arguments']==arguments(package,test),'Wrong native primitive unit selected')
        text=files[name+'.log'].decode('utf-8-sig');summaries=re.findall(r'^test result: ([^\r\n]+)',text,re.M)
        need(len(summaries)==1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;')
             and len(re.findall(r'^test '+re.escape(test)+r' \.\.\. ok\s*$',text,re.M))==1
             and len(re.findall(r'^running 1 test\s*$',text,re.M))==1,'Primitive did not execute exactly once')
def audit(files,_source,_base):
    need(set(files)=={'report.json',*(name+'.log' for name,_,_ in CASES)},'Unknown/missing primitive result files')
    commands=decode(files['report.json'])['commands'];selections(commands,files)
    mutations=0
    for index,(name,_,_) in enumerate(CASES):
        for field,value in [('exitCode',False),('executable','other'),('arguments',[])]:
            bad=copy.deepcopy(commands);bad[index][field]=value
            try:selections(bad,files)
            except (ValueError,KeyError,TypeError):mutations+=1;continue
            raise ValueError('Primitive audit accepted changed '+field)
        log=files[name+'.log']
        for altered in [log+log,log.replace(b'1 passed',b'0 passed'),log.replace(b'running 1 test',b'running 0 tests')]:
            bad=dict(files);bad[name+'.log']=altered
            try:selections(commands,bad)
            except (ValueError,KeyError,TypeError):mutations+=1;continue
            raise ValueError('Primitive audit accepted skipped/repeated unit')
    return dict(passed=True,scope=SCOPE,negativeAudits=mutations,nativeOsUnits=2,
                bootstrapCreationProved=False,keeperRuntimeProved=False,liveAdmissionProved=False)
