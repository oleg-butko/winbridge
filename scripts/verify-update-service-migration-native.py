#!/usr/bin/env python3
"""Readonly audit of a retained actual19 ->20 run, with no Windows RPC or replay."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify(run_info, source_archive, reference_source_archive, reference_target_archive, *, profile='Service19To20'):
    if profile not in ('Service19To20', 'Controller20To21'):
        raise ValueError('Unknown fixed migration profile')
    controller = profile == 'Controller20To21'
    prefix = 'controller' if controller else 'service'
    legacy = load('retained_service_migration_legacy', 'validate-update-activation-preparation-native.py')
    evidence = load('retained_service_migration_evidence', 'update-activation-service-migration-proof.py')
    if controller:
        evidence = load('retained_controller_migration_evidence', 'update-activation-controller-migration-proof.py')
    checks, native, health = legacy.checks, legacy.native, legacy.health
    meta = json.loads(run_info.read_text())
    local = run_info.resolve().parent
    source, old_source, old_target = map(health.archive_pin, (source_archive, reference_source_archive, reference_target_archive))
    checks.need(meta['source'] == source and meta['referenceSource'] == old_source and meta['referenceTarget'] == old_target,
                'Archive pins differ from retained run')
    schemas = (21, 20, 20) if controller else (20, 19, 19)
    for path, version in zip((source_archive, reference_source_archive, reference_target_archive), schemas):
        with zipfile.ZipFile(path) as package:
            checks.need(native.updater_schema_from_source(package.read('winbridge/apps/update/src/journal.rs')) == version, 'Wrong migration schema')
    checks.need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', meta['run']), 'Invalid migration run')
    reference = meta['reference']
    checks.need(re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', reference['referenceRun']), 'Invalid old run')
    reports = Path('/mnt/hgfs/shared/WinBridge/workflow-validation')
    checks.need(Path(meta['reportDirectory']) == reports / meta['run'] / 'result', 'Wrong report namespace')
    before, after = (json.loads((local / name).read_text()) for name in ('info-before.json', 'info-after.json'))
    context = before['fencedRequests']['context']
    request_raw = (local / 'request.json').read_bytes()
    checks.need(hashlib.sha256(request_raw).hexdigest() == meta['requestSha256'], 'Saved request changed')
    request = json.loads(request_raw)
    task = json.loads((local / 'terminal.json').read_text())
    accepted = json.loads((local / 'acceptance.json').read_text())
    checks.need(request['requestContractVersion'] == 2 and request['context'] == context
                and accepted['requestContractVersion'] == 2 and accepted['context'] == context
                and accepted['outcome'] == 'responded' and accepted['response']['kind'] == 'task'
                and task['taskId'] == accepted['response']['task']['taskId']
                and task['status'] == 'succeeded' and type(task['exitCode']) is int and task['exitCode'] == 0
                and task['submission'] == request['request']['request']
                and task['submission']['idempotencyKey'] == 'native-' + prefix + '-migration-' + meta['run'], 'Wrong terminal task or request')
    old_task = json.loads((local / 'reference-terminal.json').read_text())
    checks.need(old_task['taskId'] == meta['referenceTaskId'] and old_task['status'] == 'succeeded'
                and type(old_task['exitCode']) is int and old_task['exitCode'] == 0
                and old_task['submission']['projectId'] == 'snapshot-fixture'
                and old_task['submission']['idempotencyKey'] == ('native-main25-204-' if controller else 'native-activation-writer-') + reference['referenceRun'], 'Wrong retained reference task')
    old_files = legacy.support.ready_files(reports / reference['referenceRun'])
    checks.need(old_files == legacy.support.ready_files(reports / reference['referenceRun']), 'Old files changed')
    checks.need(hashlib.sha256(old_files['report.json']).hexdigest() == reference['reportSha256']
                and hashlib.sha256(old_files['candidate-health-actor-proof.json']).hexdigest() == reference['proofSha256'], 'Old report/proof SHA differs')
    old_report = json.loads(old_files['report.json'].decode('utf-8-sig'))
    checks.need(old_report['kind'] == ('activation_service_main_native_gate' if controller else 'activation_writer_main_native_gate') and old_report['passed'] is True
                and old_report['error'] is None and old_report['sourceCommit'] == old_source['sourceCommit']
                and old_report['sourceArchiveSha256'] == old_source['archiveSha256'] and old_report['target'] == old_target, 'Old report source/result differs')
    old_proof = json.loads(old_files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
    project = next(p for p in before['projects'] if p['id'] == 'snapshot-fixture')
    root = str(PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / reference['referenceRun'])
    if controller:
        checks.need(meta['profile'] == profile, 'Wrong fixed profile')
        derived, location, audit, old_meta = evidence.reference_from_main25(Path(meta['referenceRunInfo']), reference_source_archive, reference_target_archive)
        checks.need(old_meta['run'] == reference['referenceRun'] and old_meta['taskId'] == meta['referenceTaskId']
                    and location == meta['referenceArchiveLocation'], 'Old20 runtime archive/task changed')
        old_audits = audit['negativeAudits'] + audit['nativeMarkerNegativeAudits']
    else:
        old_audits = legacy.writer_checks.audit(old_proof, old_source, old_target, root, checks, legacy.state_checks,
                                               health, startup_timeout_ms=legacy.startup_budget(reference_source_archive))
        old_audits += len(legacy.replacement_audit.audit(dict(old_proof, kind='runtime_replacement_main_native_proof'), old_source, old_target, True, worker_schema=17))
        derived = evidence.derive_reference(dict(run=reference['referenceRun'], target=old_target,
                         reportSha256=reference['reportSha256'], proofSha256=reference['proofSha256']), old_proof, checks.canonical)
    checks.need(derived == reference and old_audits == meta['referenceNegativeAudits'], 'Derived reference/audits changed')
    files = legacy.support.ready_files(Path(meta['reportDirectory']))
    checks.need(files == legacy.support.ready_files(Path(meta['reportDirectory'])), 'Result files changed')
    reference_sha = hashlib.sha256((reports / meta['run'] / 'reference-input.json').read_bytes()).hexdigest()
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    checks.need(report['kind'] == 'activation_' + prefix + '_migration_native_gate'
                and report['passed'] is True and report['error'] is None and report['sourceCommit'] == source['sourceCommit']
                and report['sourceArchiveSha256'] == source['archiveSha256'] and report['referenceSha256'] == reference_sha
                and all(report[k] is False for k in ('runtimeApplied', 'installedRuntimeApplied', 'windowsOpened'))
                and hashlib.sha256(files['reference.json']).hexdigest() == reference_sha
                and json.loads(files['reference.json'].decode('utf-8-sig')) == reference, 'Migration report/reference differs')
    evidence.verify_selection(report['command'], files['migration.log'].decode('utf-8-sig'))
    proof_file = 'activation-' + prefix + '-migration-proof.json'
    proof = json.loads(files[proof_file].decode('utf-8-sig'))
    rejected = evidence.audit(proof, source, reference, checks)
    checks.need(before['platform'] == after['platform'] == 'windows' and before['activeTaskCount'] == after['activeTaskCount'] == 0
                and after['generation'] == before['generation'] and after['build'] == before['build']
                and after['fencedRequests']['context'] == context
                and all(not value.get(k) for value in (before, after) for k in ('maintenanceError', 'journalRecoveryRequired')), 'Installed worker changed/unhealthy')
    return dict(passed=True, taskId=task['taskId'], filesVerifiedTwice=len(files), negativeAudits=rejected,
                referenceNegativeAudits=old_audits, installedUnchanged=True,
                reportSha256=hashlib.sha256(files['report.json']).hexdigest(),
                proofSha256=hashlib.sha256(files[proof_file]).hexdigest())


def main(profile='Service19To20'):
    parser = argparse.ArgumentParser(description=f'Readonly audit of an actual{20 if profile == "Controller20To21" else 19} ->{21 if profile == "Controller20To21" else 20} copy migration; no Windows RPC.')
    for name in ('run-info', 'source-archive', 'reference-source-archive', 'reference-target-archive'):
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.run_info, args.source_archive, args.reference_source_archive, args.reference_target_archive, profile=profile)))


if __name__ == '__main__':
    main()
