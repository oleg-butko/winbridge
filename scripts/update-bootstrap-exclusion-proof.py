"""Strict selector for the production native exclusion primitive, not a grant."""
import re
TEST='infrastructure::tests::pinned_process_requires_exact_live_identity_and_observes_owned_exit'
def need(value,message):
    if not value:raise ValueError(message)
def selection(command,log):
    need(command['name']=='pinned-process-exclusion' and command['log']=='pinned-process-exclusion.log' and command['executable']=='cargo' and type(command['exitCode']) is int and command['exitCode']==0 and command['arguments']==['test','-p','winbridge-platform','--lib','--locked','--offline','--','--exact',TEST,'--test-threads=1','--nocapture'],'Production exclusion test selection changed')
    text=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',log)
    summaries=re.findall(r'^test result: ([^\r\n]+)',text,re.MULTILINE)
    need(re.search(r'^running 1 test\s*$',text,re.MULTILINE) and len(re.findall(r'^test '+re.escape(TEST)+r' \.\.\.',text,re.MULTILINE))==1 and len(summaries)==1 and summaries[0].startswith('ok. 1 passed; 0 failed; 0 ignored;'),'Production exclusion test did not run exactly once')
