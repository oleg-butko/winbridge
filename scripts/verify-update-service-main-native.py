#!/usr/bin/env python3
"""Readonly audit of a saved Main25 run; never submit/restart/adopt an updater."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT/'scripts'/path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-info', required=True, type=Path)
    parser.add_argument('--source-archive', required=True, type=Path)
    parser.add_argument('--target-archive', required=True, type=Path)
    args = parser.parse_args()
    checks = load('service_checks', 'update-activation-service-proof.py')
    health = load('service_archive', 'validate-update-health-actor-native.py')
    reader = load('service_reports', 'summarize-update-timings.py')
    local = args.run_info.resolve().parent
    meta = json.loads(args.run_info.read_text())
    source, target = map(health.archive_pin, (args.source_archive, args.target_archive))
    checks.need(meta['source'] == source and meta['target'] == target and meta['taskId'], 'Saved source/task scope changed')
    raw_request = (local/'request.json').read_bytes()
    checks.need(hashlib.sha256(raw_request).hexdigest() == meta['requestSha256'], 'Saved request changed')
    request = json.loads(raw_request)
    task = json.loads((local/'terminal.json').read_text())
    checks.need(task['taskId'] == meta['taskId'] and task['status'] == 'succeeded' and task['exitCode'] == 0
                and task['submission'] == request['request']['request'], 'Task is not the saved terminal submission')
    files = reader.verified_files(Path(meta['reportDirectory']))
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    checks.need(report['kind'] == 'activation_service_main_native_gate' and report['passed'] is True
                and report['error'] is None and report['sourceCommit'] == source['sourceCommit']
                and report['sourceArchiveSha256'] == source['archiveSha256'] and report['target'] == target
                and report['installedRuntimeApplied'] is False and report['windowsOpened'] is False
                and report['migrationChecked'] is False, 'Wrong or incomplete native report')
    commands = report['commands']
    checks.need(all(type(c['exitCode']) is int and c['exitCode'] == 0 for c in commands), 'A native command failed')
    for name in ('shutdown-old-real-set', 'service-validation-worker-build', 'shutdown-main-pipeline'):
        checks.need(sum(c['name'] == name for c in commands) == 1, 'Missing/repeated full pipeline step: '+name)
    main_command = next(c for c in commands if c['name'] == 'shutdown-main-pipeline')
    log = files[main_command['log']].decode('utf-8-sig')
    checks.need('actual_main_activates_one_local_service_without_controller_or_replay ... ok' in log
                and 'test result: ok. 1 passed; 0 failed;' in log, 'Main25 was not selected')
    proof = json.loads(files['candidate-health-actor-proof.json'].decode('utf-8-sig'))
    result = checks.audit(proof, source, target, report['activationReservation'])
    result.update(checks.verify_auxiliary(files, proof, source))
    observations_raw = (local/'controller-observations.jsonl').read_bytes()
    observations = [json.loads(line) for line in observations_raw.splitlines()]
    preflight = proof['cases'][0]['activationWriter']['controllerPreflight']
    checks.need(observations and all(x['pidAlive'] is True and x['response']['status']['instanceId'] == meta['instanceId'] for x in observations)
                and any(x['response']['status']['connected'] is True
                        and x['response']['status']['sessionId'] == preflight['connection']['sessionId'] for x in observations),
                'Isolated controller availability/session not observed')
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
