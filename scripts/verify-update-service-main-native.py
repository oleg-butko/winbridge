#!/usr/bin/env python3
"""Readonly audit of a saved Main25 run; never submit/restart/adopt an updater."""
import argparse
import hashlib
import importlib.util
import json
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT/'scripts'/path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main(controller=False, lifetime=False, ready_loss=False, resident=False, finalization=False):
    resident = resident or finalization
    ready_loss = ready_loss or resident
    lifetime = lifetime or ready_loss
    controller = controller or lifetime
    parser = argparse.ArgumentParser(description='Readonly native audit; no Windows commands or replay.')
    parser.add_argument('--profile', choices=(('worker19-updater23',) if finalization else ('worker19-updater22',) if lifetime else (('worker19-updater21',) if controller else ('worker17-updater20', 'worker19-updater20'))), default=('worker19-updater23' if finalization else 'worker19-updater22' if lifetime else ('worker19-updater21' if controller else 'worker17-updater20')))
    parser.add_argument('--run-info', required=True, type=Path)
    parser.add_argument('--source-archive', required=True, type=Path)
    parser.add_argument('--target-archive', required=True, type=Path)
    args = parser.parse_args()
    checks = load('main_gate_checks', 'update-activation-finalization-proof.py' if finalization else 'update-activation-resident-proof.py' if resident else 'update-activation-lifetime-ready-loss-proof.py' if ready_loss else ('update-activation-lifetime-proof.py' if lifetime else ('update-activation-controller-proof.py' if controller else 'update-activation-service-proof.py')))
    health = load('service_archive', 'validate-update-health-actor-native.py')
    reader = load('service_reports', 'summarize-update-timings.py')
    local = args.run_info.resolve().parent
    meta = json.loads(args.run_info.read_text())
    source, target = map(health.archive_pin, (args.source_archive, args.target_archive))
    # Expected schemas come from a selected reviewed profile, then independently
    # bind to both verified source archives. Never infer them from the proof.
    expected_worker, expected_updater = checks.profile_schemas(args.profile)
    for path, pin in ((args.source_archive, source), (args.target_archive, target)):
        with zipfile.ZipFile(path) as archive:
            worker = health.native.worker_schema_from_source(archive.read('winbridge/apps/worker/src/store.rs'))
            updater = health.native.updater_schema_from_source(archive.read('winbridge/apps/update/src/journal.rs'))
        checks.need((worker, updater) == (expected_worker, expected_updater), 'Selected profile differs from verified source/target schemas')
        checks.need(hashlib.sha256(path.read_bytes()).hexdigest() == pin['archiveSha256'], 'Schema archive changed after verification')
    checks.need(meta['source'] == source and meta['target'] == target and meta['taskId'], 'Saved source/task scope changed')
    raw_request = (local/'request.json').read_bytes()
    checks.need(hashlib.sha256(raw_request).hexdigest() == meta['requestSha256'], 'Saved request changed')
    request = json.loads(raw_request)
    task = json.loads((local/'terminal.json').read_text())
    checks.need(task['taskId'] == meta['taskId'] and task['status'] == 'succeeded' and task['exitCode'] == 0
                and task['submission'] == request['request']['request'], 'Task is not the saved terminal submission')
    if controller:
        before=json.loads((local/'info-before.json').read_text())
        acceptance=json.loads((local/'acceptance.json').read_text())
        checks.need(meta['profile']==args.profile and request['requestContractVersion']==acceptance['requestContractVersion']==2
                    and request['context']==acceptance['context']==before['fencedRequests']['context']
                    and acceptance['outcome']=='responded' and acceptance['response']['kind']=='task'
                    and acceptance['response']['task']['taskId']==task['taskId']
                    and type(task['exitCode']) is int and task['submission']['projectId']=='snapshot-fixture'
                    and task['submission']['idempotencyKey']==('native-main30-204-' if finalization else 'native-main29-204-' if resident else 'native-main28-204-' if ready_loss else ('native-main27-204-' if lifetime else 'native-main26-204-'))+meta['run'], 'Main26 admission/context/task identity changed')
    files = reader.verified_files(Path(meta['reportDirectory']))
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    checks.need(report['kind'] == ('activation_finalization_main_native_gate' if finalization else 'activation_resident_main_native_gate' if resident else 'activation_lifetime_ready_loss_main_native_gate' if ready_loss else ('activation_lifetime_main_native_gate' if lifetime else ('activation_controller_main_native_gate' if controller else 'activation_service_main_native_gate'))) and report['passed'] is True
                and report['error'] is None and report['sourceCommit'] == source['sourceCommit']
                and report['sourceArchiveSha256'] == source['archiveSha256'] and report['target'] == target
                and report['installedRuntimeApplied'] is False and report['windowsOpened'] is False
                and report['migrationChecked'] is False, 'Wrong or incomplete native report')
    commands = report['commands']
    checks.need(all(type(c['exitCode']) is int and c['exitCode'] == 0 for c in commands), 'A native command failed')
    if controller:
        stack_commands=[c for c in commands if c['name']=='main-driver-owner-stack']
        checks.need(len(stack_commands)==1,'Missing/repeated original IO owner-stack regression')
        stack_log=files[stack_commands[0]['log']].decode('utf-8-sig')
        checks.need('update_control::validation::owner_stack::tests' in stack_commands[0]['arguments']
                    and 'test result: ok. 2 passed; 0 failed;' in stack_log,
                    'Original IO owner-stack regressions not selected')
    for name in ('shutdown-old-real-set', 'service-validation-worker-build', 'shutdown-main-pipeline'):
        checks.need(sum(c['name'] == name for c in commands) == 1, 'Missing/repeated full pipeline step: '+name)
    main_command = next(c for c in commands if c['name'] == 'shutdown-main-pipeline')
    log = files[main_command['log']].decode('utf-8-sig')
    checks.verify_main_selection(main_command, log)
    if ready_loss:
        checks.verify_boundaries(commands, files)
    proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
    result = checks.audit(proof, source, target, report['activationReservation'], profile=args.profile)
    result.update(checks.audit_auxiliary(files, proof, source))
    if args.profile in ('worker19-updater20','worker19-updater21','worker19-updater22','worker19-updater23'):
        epoch_commands = [c for c in commands if c['name'] == 'service-history-epoch-check']
        checks.need(len(epoch_commands) == 1, 'Missing policy epoch/history corruption regression')
        epoch_log = files[epoch_commands[0]['log']].decode('utf-8-sig')
        epoch_name = 'activation_writer::local_service_history_hash_allows_only_the_exact_policy_owner_epoch'
        checks.need(epoch_name in epoch_commands[0]['arguments'] and '--exact' in epoch_commands[0]['arguments']
                    and epoch_name in epoch_log and 'test result: ok. 1 passed; 0 failed;' in epoch_log,
                    'Policy epoch/history regression not selected')
        reference = meta['worker17ReferenceSha256']
        checks.need(report.get('worker17ReferenceSha256') == reference, 'Native migration reference differs from saved request')
        migration_command = [c for c in commands if c['name'] == 'worker17-to19-cold-migration']
        checks.need(len(migration_command) == 1, 'Missing/repeated native worker migration gate')
        migration_log = files[migration_command[0]['log']].decode('utf-8-sig')
        checks.need('actual_seventeen_history_migrates_to_nineteen_without_replay_or_old_worker_write ... ok' in migration_log,
                    'Actual worker17 migration test was not selected')
        result.update(checks.verify_worker_migration(json.loads(files['worker-journal-attribution-migration-proof.json'].decode('utf-8-sig')), reference, source))
    observations_raw = (local/'controller-observations.jsonl').read_bytes()
    observations = [json.loads(line) for line in observations_raw.splitlines()]
    preflight = proof['cases'][0]['activationWriter']['controllerPreflight']
    activity=[]
    if controller:
        activity_raw=(local/'controller-activity.jsonl').read_bytes()
        receipt=proof['cases'][0]['activationController']['record']['state']['receipt']
        activity=checks.verify_activity(activity_raw,json.loads((local/'controller-activity-sources.json').read_text()),
                    json.loads((local/'controller-final-status.json').read_text()),json.loads((local/'controller-exit.json').read_text()),meta,receipt)
    if resident:
        checks.verify_resident_activity(activity, proof)
    preflight_seen=any(x['response']['status']['connected'] is True
                        and x['response']['status']['sessionId'] == preflight['connection']['sessionId'] for x in observations)
    if controller:
        preflight_seen=preflight_seen or any(r.get('event')=='worker_connected'
                      and r.get('sessionId')==preflight['connection']['sessionId']
                      and r.get('generation')==preflight['workerGeneration'] for r in activity)
    checks.need(observations and all(x['pidAlive'] is True and x['response']['status']['instanceId'] == meta['instanceId'] for x in observations)
                and preflight_seen,
                'Isolated controller availability/session not observed')
    if controller:
        checks.need(report['profile']==args.profile,'Wrong Main26 report profile')
        checks.need((local/'controller-activity.jsonl').read_bytes()==activity_raw,'Controller activity changed on second read')
        result.update(controllerActivitySha256=hashlib.sha256(activity_raw).hexdigest(),controllerActivityRecords=len(activity))
    before, after = (json.loads((local/name).read_text()) for name in ('info-before.json', 'info-after.json'))
    checks.need(before['build'] == after['build'] and before['generation'] == after['generation']
                and before['fencedRequests']['context'] == after['fencedRequests']['context']
                and after['activeTaskCount'] == 0 and after.get('maintenanceError') is None
                and after.get('journalRecoveryRequired') is None, 'Installed worker changed or needs recovery')
    checks.need(reader.verified_files(Path(meta['reportDirectory'])) == files, 'Report changed on second complete read')
    result.update(run=meta['run'], taskId=meta['taskId'], filesVerifiedTwice=len(files),
                  reportSha256=hashlib.sha256(files['report.json']).hexdigest(),
                  proofSha256=hashlib.sha256(files['candidate-health-actor-proof.json']).hexdigest(),
                  controllerObservationsSha256=hashlib.sha256(observations_raw).hexdigest(),
                  controllerObservations=len(observations), installedWorkerUnchanged=True,
                  runtimeArchive=report['runtimeArchive'], storage=report['storage'])
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
