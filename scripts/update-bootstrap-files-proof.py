"""Native file-sharing evidence only; never a keeper/admission/spawn permit."""
import copy
import importlib.util
from pathlib import Path
import re
s=importlib.util.spec_from_file_location('file_os_evidence',Path(__file__).with_name('update-lifetime-spike-proof.py'));old=importlib.util.module_from_spec(s);s.loader.exec_module(old)
need,decode,integer=old.need,old.decode,old.integer
MODES=('bootstrap-files',)
TEST='bootstrap::files::native::fresh_helper_and_config_pins_allow_only_the_intended_file_access'
SCOPE='fresh helper files and Windows sharing; synthetic worker state, no coordinator'
TRUE=('passed','syntheticWorkerState','oldInstalledImageUnchanged','imageWriteBlocked','configWriteBlocked','workerConfigWriteBlocked','configReplaceBlocked','outputWriteAllowed','outputReplaceBlocked','pinsRetiredWriteAndReplace','rootUnchanged','noSupervisorIntent','noBootstrapReservation')
FALSE=('coordinatorCreated','runtimeApplied','installedRuntimeApplied','windowsOpened','networkUsed')
def selections(commands,files):
    build,command=commands
    for item,name in ((build,'bootstrap-files-build'),(command,'bootstrap-files')):
        need(item['executable']=='cargo' and item['name']==name and item['log']==name+'.log' and type(item['exitCode']) is int and item['exitCode']==0,'Wrong bootstrap-files command/log/exit')
    need(build['arguments']==['build','-p','winbridge-update','--features','bundled-sqlite','--bin','winbridge-update','--locked','--offline'],'Wrong candidate build selected')
    need(command['arguments']==['test','-p','winbridge-update','--features','bundled-sqlite','--lib','--locked','--offline','--','--exact',TEST,'--ignored','--test-threads=1','--nocapture'],'Wrong file-sharing unit selected')
    text=files['bootstrap-files.log'].decode('utf-8-sig');summaries=re.findall(r'^test result: ([^\r\n]+)',text,re.M)
    need(len(summaries)==1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;') and len(re.findall(r'^test '+re.escape(TEST)+r' \.\.\.',text,re.M))==1 and re.search(r'^running 1 test\s*$',text,re.M),'File-sharing unit did not execute exactly once')
def verify(p,source):
    need(set(p)==set(TRUE+FALSE+('schemaVersion','kind','platform','build','queriedBuild','source','scope','freshImageSha256','stagedImageSha256')),'Unknown/missing bootstrap files fields')
    integer(p['schemaVersion'],1,1)
    need(p['kind']=='bootstrap_files_native_proof' and p['platform']=='windows' and p['scope']==SCOPE,'Wrong native files scope')
    need(all(p[k] is True for k in TRUE) and all(p[k] is False for k in FALSE),'Missing file protection/retirement or overbroad effect claim')
    need(p['source']==source,'Fresh helper detached from exact requested snapshot')
    for b in (p['build'],p['queriedBuild']):
        need(set(b)=={'buildId','version','sourceCommit','sourceManifestSha256','sourceDirty'} and b['sourceCommit']==source['sourceCommit'] and b['sourceManifestSha256']==source['manifestSha256'] and b['sourceDirty'] is False,'Dirty/foreign producer or queried helper')
        need(all(type(b[k]) is str and 0<len(b[k])<=128 for k in ('version','buildId')),'Invalid build identity')
    need(type(p['freshImageSha256']) is str and re.fullmatch('[0-9a-f]{64}',p['freshImageSha256']) and p['stagedImageSha256']==p['freshImageSha256'],'Staged helper differs from queried fresh image')
def audit(files,source,_base):
    p=decode(files['bootstrap-files-proof.json']);verify(p,source)
    changes=[(k,False)for k in TRUE]+[(k,True)for k in FALSE]+[('schemaVersion',True),('scope','full updater'),('stagedImageSha256','0'*64),('build/sourceDirty',True),('queriedBuild/sourceCommit','0'*40),('source/archiveSha256','0'*64)]
    for path,value in changes:
        bad=copy.deepcopy(p);parts=path.split('/');node=bad
        for part in parts[:-1]:node=node[part]
        need(node[parts[-1]]!=value or type(node[parts[-1]])!=type(value),'No-op file audit '+path);node[parts[-1]]=value
        try:verify(bad,source)
        except (ValueError,KeyError,TypeError):continue
        raise ValueError('File observer accepted mutation '+path)
    return dict(passed=True,scope=SCOPE,negativeAudits=len(changes),bootstrapCreationProved=False,liveAdmissionProved=False)
