"""Frozen profile metadata shared by the native runner and readonly verifier."""
PROFILES = {
    'lifetime-spike': dict(kind='activation_lifetime_keeper_native_gate',meta=None,reader='update-lifetime-spike-proof.py',verify='verify-update-lifetime-spike-native.py',switch='',commands=2,selection='legacy'),
    'bootstrap-exclusion': dict(kind='activation_bootstrap_exclusion_native_gate',meta='bootstrapExclusion',reader='update-lifetime-spike-proof.py',verify='verify-update-bootstrap-exclusion-native.py',switch='-BootstrapExclusion',commands=3,selection='legacy'),
    'keeper-probe': dict(kind='keeper_probe_native_gate',meta='keeperProbe',reader='update-keeper-probe-proof.py',verify='verify-update-keeper-probe-native.py',switch='-KeeperProbe',commands=1,selection='single'),
    'bootstrap-files': dict(kind='bootstrap_files_native_gate',meta='bootstrapFiles',reader='update-bootstrap-files-proof.py',verify='verify-update-bootstrap-files-native.py',switch='-BootstrapFiles',commands=2,selection='batch'),
    'keeper-launch-primitives': dict(kind='keeper_launch_primitives_native_gate',meta='keeperLaunchPrimitives',reader='update-keeper-launch-primitives-proof.py',verify='verify-update-keeper-launch-primitives-native.py',switch='-KeeperLaunchPrimitives',commands=2,selection='batch'),
    'keeper-exit': dict(kind='keeper_exit_native_gate',meta='keeperExit',reader='update-keeper-exit-proof.py',verify='verify-update-keeper-exit-native.py',switch='-KeeperExit',commands=2,selection='batch'),
}
def select(*,bootstrap=False,keeper_probe=False,bootstrap_files=False,keeper_launch=False,keeper_exit=False):
    choices=[name for flag,name in ((bootstrap,'bootstrap-exclusion'),(keeper_probe,'keeper-probe'),(bootstrap_files,'bootstrap-files'),(keeper_launch,'keeper-launch-primitives'),(keeper_exit,'keeper-exit')) if flag]
    if len(choices)>1:raise ValueError('Choose one explicit native profile')
    name=choices[0] if choices else 'lifetime-spike'
    return dict(PROFILES[name],namespace=name)
