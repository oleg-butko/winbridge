"""Synthetic selector regressions, not native Job-membership evidence."""
import copy
import importlib.util
from pathlib import Path
import unittest
s=importlib.util.spec_from_file_location('bootstrap_selector',Path(__file__).with_name('update-bootstrap-exclusion-proof.py'));p=importlib.util.module_from_spec(s);s.loader.exec_module(p)
def fixture():
    return dict(name='pinned-process-exclusion',log='pinned-process-exclusion.log',executable='cargo',exitCode=0,arguments=['test','-p','winbridge-platform','--lib','--locked','--offline','--','--exact',p.TEST,'--test-threads=1','--nocapture']),f'running 1 test\ntest {p.TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;\n'
class Selector(unittest.TestCase):
    def test_one_exact_native_selection(self):
        command,log=fixture();p.selection(command,log)
    def test_duplicate_skipped_foreign_or_multiple_tests_refused(self):
        command,log=fixture()
        for bad in (log+log,log.replace('1 passed','0 passed'),log.replace('0 ignored','1 ignored'),log.replace('running 1 test','running 2 tests'),log.replace(p.TEST,'foreign')):
            with self.assertRaises(ValueError):p.selection(command,bad)
    def test_boolean_exit_or_ignored_flag_refused(self):
        command,log=fixture()
        with self.assertRaises(ValueError):p.selection(dict(command,exitCode=False),log)
        bad=copy.deepcopy(command);bad['arguments'].append('--ignored')
        with self.assertRaises(ValueError):p.selection(bad,log)
if __name__=='__main__':unittest.main()
