"""Reviewed, fixed migration profiles. Never infer schemas from result JSON."""
PROFILES = {
    'Service19To20': dict(prefix='service', schemas=(20, 19, 19),
        reference_report_kind='activation_writer_main_native_gate',
        task_prefix='native-activation-writer-', reference_helper=None,
        reference_audits=()),
    'Controller20To21': dict(prefix='controller', schemas=(21, 20, 20),
        reference_report_kind='activation_service_main_native_gate',
        task_prefix='native-main25-204-',
        reference_helper=('update-activation-controller-migration-proof.py', 'reference_from_main25'),
        reference_audits=('negativeAudits', 'nativeMarkerNegativeAudits')),
    'Lifetime21To22': dict(prefix='lifetime', schemas=(22, 21, 21),
        reference_report_kind='activation_controller_main_native_gate',
        task_prefix='native-main26-204-',
        reference_helper=('update-activation-lifetime-migration-proof.py', 'reference_from_main26'),
        reference_audits=('negativeAudits', 'controllerNegativeAudits', 'nativeMarkerNegativeAudits')),
}


def get(profile):
    if profile not in PROFILES:
        raise ValueError('Unknown fixed migration profile')
    return dict(PROFILES[profile])
