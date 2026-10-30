"""Synthetic reader tests only, not evidence of Windows execution."""
import copy,importlib.util,json,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location('launch_primitives',Path(__file__).with_name('update-keeper-launch-primitives-proof.py'))
p=importlib.util.module_from_spec(s);s.loader.exec_module(p)
def fixture():
    commands=[dict(name=n,log=n+'.log',executable='cargo',exitCode=0,arguments=p.arguments(package,t))for n,package,t in p.CASES]
    files={n+'.log':f'running 1 test\ntest {t} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'.encode()for n,_,t in p.CASES}
    files['report.json']=json.dumps(dict(commands=commands)).encode();return commands,files
class Reader(unittest.TestCase):
    def test_exact_units_and_twelve_negative_audits(self):
        _,f=fixture();r=p.audit(f,None,None);self.assertEqual(r['negativeAudits'],12)
        self.assertFalse(r['bootstrapCreationProved']);self.assertFalse(r['keeperRuntimeProved'])
    def test_missing_extra_wrong_order_refused(self):
        c,f=fixture()
        for changed in [c[::-1],c[:1],c+c[:1]]:
            with self.assertRaises(ValueError):p.selections(changed,f)
        f['unrelated-proof.json']=b'{}'
        with self.assertRaises(ValueError):p.audit(f,None,None)
    def test_wrong_exact_unit_and_ignored_helper_refused(self):
        c,f=fixture()
        for arguments in [c[1]['arguments']+['--ignored'],p.arguments('winbridge-update','unrelated'),['test','--all']]:
            changed=copy.deepcopy(c);changed[1]['arguments']=arguments
            with self.assertRaises(ValueError):p.selections(changed,f)
if __name__=='__main__':unittest.main()
