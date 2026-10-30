"""Preserve public wrapper selections and reject mixed native profiles."""
import importlib.util,itertools,unittest
from pathlib import Path
s=importlib.util.spec_from_file_location('profiles',Path(__file__).with_name('update-lifetime-native-profiles.py'));p=importlib.util.module_from_spec(s);s.loader.exec_module(p)
class Profiles(unittest.TestCase):
    def test_existing_wrapper_names_and_command_counts_are_frozen(self):
        for args,namespace,count in [({},'lifetime-spike',2),({'bootstrap':True},'bootstrap-exclusion',3),({'keeper_probe':True},'keeper-probe',1),({'bootstrap_files':True},'bootstrap-files',2),({'keeper_launch':True},'keeper-launch-primitives',2),({'keeper_exit':True},'keeper-exit',2)]:
            selected=p.select(**args);self.assertEqual(selected['namespace'],namespace);self.assertEqual(selected['commands'],count)
            self.assertEqual(selected['verify'],'verify-update-'+namespace+'-native.py')
    def test_every_mixed_profile_is_rejected_before_rpc(self):
        for first,second in itertools.combinations(('bootstrap','keeper_probe','bootstrap_files','keeper_launch','keeper_exit'),2):
            with self.assertRaises(ValueError):p.select(**{first:True,second:True})
    def test_selected_metadata_cannot_change_later_calls(self):
        old=p.select();old['commands']=999;self.assertEqual(p.select()['commands'],2)
if __name__=='__main__':unittest.main()
