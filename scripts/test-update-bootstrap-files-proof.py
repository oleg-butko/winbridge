"""Synthetic reader checks, not evidence of a native file-sharing run."""
import copy,importlib.util,json,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location('files_proof',Path(__file__).with_name('update-bootstrap-files-proof.py'));p=importlib.util.module_from_spec(s);s.loader.exec_module(p)
def fixture():
    source=dict(sourceCommit='a'*40,manifestSha256='b'*64,archiveName='source.zip',archiveSha256='c'*64)
    build=dict(version='0.2.0',buildId='synthetic',sourceCommit=source['sourceCommit'],sourceManifestSha256=source['manifestSha256'],sourceDirty=False)
    proof=dict(schemaVersion=1,kind='bootstrap_files_native_proof',platform='windows',build=build,queriedBuild=copy.deepcopy(build),source=copy.deepcopy(source),scope=p.SCOPE,freshImageSha256='d'*64,stagedImageSha256='d'*64,**{k:True for k in p.TRUE},**{k:False for k in p.FALSE})
    return proof,source
class Reader(unittest.TestCase):
    def test_shape_and_24_negative_audits(self):
        proof,source=fixture();self.assertEqual(p.audit({'bootstrap-files-proof.json':json.dumps(proof).encode()},source,None)['negativeAudits'],24)
    def test_wrong_current_and_candidate_snapshots_refused(self):
        for key in ('build','queriedBuild'):
            proof,source=fixture();proof[key]['sourceManifestSha256']='0'*64
            with self.assertRaises(ValueError):p.verify(proof,source)
    def test_exact_commands_and_skipped_or_duplicate_unit_refused(self):
        args=['test','-p','winbridge-update','--features','bundled-sqlite','--lib','--locked','--offline','--','--exact',p.TEST,'--ignored','--test-threads=1','--nocapture']
        commands=[dict(name='bootstrap-files-build',log='bootstrap-files-build.log',executable='cargo',exitCode=0,arguments=['build','-p','winbridge-update','--features','bundled-sqlite','--bin','winbridge-update','--locked','--offline']),dict(name='bootstrap-files',log='bootstrap-files.log',executable='cargo',exitCode=0,arguments=args)]
        log=f'running 1 test\ntest {p.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
        p.selections(commands,{'bootstrap-files.log':log.encode()})
        for bad in (log+log,log.replace('1 passed','0 passed'),log.replace(p.TEST,'another')):
            with self.assertRaises(ValueError):p.selections(commands,{'bootstrap-files.log':bad.encode()})
        commands[0]['exitCode']=False
        with self.assertRaises(ValueError):p.selections(commands,{'bootstrap-files.log':log.encode()})
if __name__=='__main__':unittest.main()
