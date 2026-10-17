#!/usr/bin/env python3
"""Run isolated, real Windows regression tests through the already approved worker."""
import argparse
import base64
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import re
import socket
import struct
import subprocess
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def prepare_resume_package(config, shared, run_id):
    """Fresh committed Debian fixture; Windows must import precisely these pinned bytes."""
    local = ROOT / '.winbridge' / 'resume-validation' / run_id
    source = local / 'source'
    source.mkdir(parents=True)
    (source / 'src').mkdir()
    (source / 'Cargo.toml').write_text('[package]\nname = "resume-fixture"\nversion = "0.1.0"\nedition = "2021"\n')
    (source / 'Cargo.lock').write_text('version = 3\n[[package]]\nname = "resume-fixture"\nversion = "0.1.0"\n')
    (source / 'src' / 'main.rs').write_text('fn main(){println!("Проверенный resume → Windows build ✓");}\n')
    blob = bytes([0xB6]) * (32*1024*1024+31)
    (source / 'blob.dat').write_bytes(blob)
    for arguments in (['init', '-q'], ['add', '.'], ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@local', 'commit', '-qm', 'Cross-host resume fixture']):
        subprocess.run(['git', '-C', str(source), *arguments], check=True, capture_output=True)
    delivery_root = shared / config['delivery']['namespace'] / 'resume-validation' / run_id
    delivery_root.mkdir(parents=True)
    publication_config = dict(config, stateDirectory=str(local / 'capture'),
                              projects=[{'id':'fixture', 'sourceRoot':str(source), 'sourceMode':'commit'}],
                              delivery=dict(config['delivery'], root=str(delivery_root)))
    config_path = local / 'publisher.local.json'
    config_path.write_text(json.dumps(publication_config, indent=2))
    process = subprocess.run([str(ROOT / 'target/debug/winbridge-controller'), '--config', str(config_path), 'publish-resumable', 'fixture', 'HEAD'], check=True, capture_output=True, text=True)
    publication = json.loads(process.stdout)
    if publication['ready']['schemaVersion'] != 3 or publication['reusedPackage']:
        raise RuntimeError('Resume gate requires a fresh Debian schema-3 publication')
    measurement = delivery_root / 'debian-publication.json'
    with measurement.open('x') as stream:
        json.dump(publication, stream, indent=2)
    windows_root = config['delivery']['peerRoot'].rstrip('\\/') + '\\' + str(delivery_root.relative_to(shared)).replace('/', '\\')
    return publication, hashlib.sha256(blob).hexdigest(), windows_root, windows_root + '\\debian-publication.json'


def exact(stream, count):
    data = bytearray()
    while len(data) < count:
        chunk = stream.recv(count - len(data))
        if not chunk:
            raise RuntimeError('IPC ended before a complete response')
        data.extend(chunk)
    return bytes(data)


def call(endpoint, request):
    data = json.dumps(request).encode()
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(15)
        stream.connect(str(endpoint))
        stream.sendall(struct.pack('<I', len(data)) + data)
        length, = struct.unpack('<I', exact(stream, 4))
        if not 0 < length <= 1048576:
            raise RuntimeError('Invalid IPC response size')
        response = json.loads(exact(stream, length))
    if response['kind'] == 'error':
        raise RuntimeError(response)
    return response


def ps_string(value):
    return "'" + value.replace("'", "''") + "'"


def verify_reusable_diagnostic(base, run_id, archive_sha):
    """Reuse only a successful, checksum-verified diagnostic of this exact archive."""
    if not re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', run_id):
        raise ValueError('Invalid diagnostic run ID')
    report = base / run_id
    if report.is_symlink() or report.resolve().parent != base.resolve():
        raise ValueError('Diagnostic report must stay inside its validation namespace')
    ready = json.loads((report / 'report.ready.json').read_text(encoding='utf-8-sig'))
    names = set()
    for item in ready['files']:
        name = item['path']
        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name) or name in names or (report / name).is_symlink():
            raise ValueError('Unsafe diagnostic artifact')
        data = (report / name).read_bytes()
        if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
            raise ValueError('Diagnostic artifact checksum mismatch: ' + name)
        names.add(name)
    expected = {'report.json', 'process-resume-guard.log', 'process-runtime-recovery-diagnostic.log'}
    if not expected.issubset(names) or not ready['passed'] or ready['platform'] != 'windows':
        raise ValueError('Incomplete successful diagnostic receipt')
    result = json.loads((report / 'report.json').read_text(encoding='utf-8-sig'))
    if not result['passed'] or not result['diagnosticOnly'] or result['sourceDirty'] or result['platform'] != 'windows' or result['sourceArchiveSha256'] != archive_sha:
        raise ValueError('Diagnostic must have passed with this exact clean archive')
    if [g['name'] for g in result['commands']] != ['process-resume-guard', 'process-runtime-recovery-diagnostic'] or any(g['exitCode'] != 0 for g in result['commands']):
        raise ValueError('Incomplete diagnostic test groups')
    return result


def verify_reusable_build_cache(base, run_id, visited=None):
    """A failed test is usable compiler cache, never inherited test evidence."""
    visited = set() if visited is None else set(visited)
    if run_id in visited or len(visited) >= 32:
        raise ValueError('Cyclic or oversized build-cache provenance chain')
    visited.add(run_id)
    if not re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', run_id):
        raise ValueError('Invalid build-cache run ID')
    report = base / run_id
    if report.is_symlink() or report.resolve().parent != base.resolve():
        raise ValueError('Build-cache receipt must stay inside its validation namespace')
    ready = json.loads((report / 'report.ready.json').read_text(encoding='utf-8-sig'))
    names = set()
    for item in ready['files']:
        name = item['path']
        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name) or name in names or (report / name).is_symlink():
            raise ValueError('Unsafe build-cache artifact')
        data = (report / name).read_bytes()
        if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
            raise ValueError('Build-cache artifact checksum mismatch: ' + name)
        names.add(name)
    if ready['platform'] != 'windows' or not {'report.json', 'task-cas-native-writers.log'}.issubset(names):
        raise ValueError('Incomplete build-cache receipt')
    result = json.loads((report / 'report.json').read_text(encoding='utf-8-sig'))
    if result['sourceDirty'] or result['platform'] != 'windows' or result.get('diagnosticOnly') or not re.fullmatch(r'[0-9a-f]{40}', result['sourceCommit']) or not re.fullmatch(r'[0-9a-f]{64}', result['sourceArchiveSha256']) or result['passed'] != ready['passed']:
        raise ValueError('Build cache requires a clean full-CAS source receipt')
    expected = ['task-cas-pure-fsm', 'task-cas-process-resume-guard', 'task-cas-fenced-contract', 'task-cas-frozen-contracts', 'task-cas-native-writers']
    if [g['name'] for g in result['commands'][:5]] != expected or any(g['exitCode'] != 0 for g in result['commands'][:4]) or result['commands'][4]['exitCode'] not in {0, 101}:
        raise ValueError('Prior run did not reach native test execution')
    log = (report / 'task-cas-native-writers.log').read_text(encoding='utf-8-sig')
    if not re.search(r'Finished `test` profile', log) or not re.search(r'test result: (ok|FAILED)\.', log):
        raise ValueError('No completed compilation/test execution in prior cache')
    receipt = {'runId': run_id, 'sourceCommit': result['sourceCommit'], 'reportSha256': hashlib.sha256((report / 'report.json').read_bytes()).hexdigest()}
    parent = result.get('buildCacheReusedFrom')
    if parent:
        # The report is fresh, but its Cargo directory may belong to an earlier
        # run. Resolve only verified receipts; never accept a free-form path.
        owner = verify_reusable_build_cache(base, parent['runId'], visited)
        prior = base / parent['runId'] / 'report.json'
        prior_bytes = prior.read_bytes()
        prior_result = json.loads(prior_bytes.decode('utf-8-sig'))
        if parent != {'runId': parent['runId'], 'sourceCommit': prior_result['sourceCommit'], 'reportSha256': hashlib.sha256(prior_bytes).hexdigest()}:
            raise ValueError('Build-cache parent receipt identity mismatch')
    else:
        owner = receipt
    recorded_path = result.get('buildCacheDirectory')
    if recorded_path and PureWindowsPath(recorded_path).parts[-3:] != ('workflow-validation', owner['runId'], 'test-build-cache'):
        raise ValueError('Build-cache directory differs from its verified owner run')
    return owner


def verify_build_cache_usage(result, requested, local):
    if not requested or result.get('buildCacheReusedFrom') == requested:
        return
    expected_cache = PureWindowsPath(local) / 'test-build-cache'
    if result.get('buildCacheRequested') != requested or result.get('buildCacheReuseSkipped') != 'cache_missing' or PureWindowsPath(result.get('buildCacheDirectory', '')) != expected_cache:
        raise RuntimeError('Native build-cache receipt does not match the pinned prior run/fresh fallback')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path, required=True)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--project', default='snapshot-fixture')
    parser.add_argument('--delta-only', action='store_true', help='Check pinned-base delta on actual HGFS with isolated journal/build, without desktop windows')
    parser.add_argument('--profiles-only', action='store_true', help='Check two registered projects with a reusable recipe profile, real IPC, cancel and crash/restart; no desktop windows')
    parser.add_argument('--projects-only', action='store_true', help='Publish separate committed Netwatch/WinBridge clones and run both through a common build profile; no desktop or network from test instances')
    parser.add_argument('--identity-only', action='store_true', help='Check protocol/build identity on actual Windows/Git without desktop or test workers')
    parser.add_argument('--update-preparation-only', action='store_true', help='Test readonly updater/source planning and build a complete candidate without changing installed runtime or opening windows')
    parser.add_argument('--update-preparation-reuse-run', help='Reuse only compiler caches of a terminal update-preparation run with checksummed successful native groups; always use new source/state/staging')
    parser.add_argument('--task-fsm-only', action='store_true', help='Check Task FSM, frozen v1 baseline, real launch/PID commit faults, cancel/tree/timeout/recovery and workflow regressions; no desktop windows')
    parser.add_argument('--readiness-only', action='store_true', help='Check readiness, operation pages, scheduling/queue policy, native IPC and desktop compile; no UI windows')
    parser.add_argument('--queue-reference-worker', help='Read-only Windows path to a retained schema-12 exe for queue migration refusal; defaults to installed worker before the first schema-13 upgrade')
    parser.add_argument('--resource-reference-worker', help='Read-only Windows path to a retained schema-13 exe for resource migration refusal; defaults to installed worker before the first schema-14 upgrade')
    parser.add_argument('--verify-readiness-run', help='Read-only recheck of an existing terminal readiness run/report; never submit or rebuild')
    parser.add_argument('--task-cas-only', action='store_true', help='Check durable leases/CAS, schema-6/7/8/9/10/11 migration, physical reconciliation, fenced requests, desktop compile and old-worker refusal; no desktop windows')
    parser.add_argument('--task-cas-runtime-diagnostic', action='store_true', help='With --task-cas-only: narrow process/recovery diagnostic with trace and disk usage, without UI or full CAS/reference gate')
    parser.add_argument('--task-cas-reuse-diagnostic', help='Full CAS only: reuse the source/build cache of a successful diagnostic run ID for this exact archive; new tests/state/report, process trace enabled')
    parser.add_argument('--task-cas-reuse-build-cache', help='Full CAS only: reuse only Cargo artifacts from a checksum-verified terminal CAS run; always extract new source and create new test state/report')
    parser.add_argument('--task-cas-reference-worker', help='Read-only Windows path to a schema-6 worker exe, used only with NEW isolated state to prove schema-12 refusal')
    parser.add_argument('--resume-only', action='store_true', help='Check manifest-pinned chunks, actual owned crash/restart and explicit resume on HGFS, without desktop windows')
    parser.add_argument('--output-only', action='store_true', help='Check bounded output and native IPC/cursor/control against an isolated reference worker, without desktop windows')
    parser.add_argument('--output-reference-worker', help='Read-only path to the installed worker exe, used with a NEW isolated config/state for matched output samples')
    parser.add_argument('--attention-only', action='store_true', help='Run the stage-5 attention tests without repeating the completed workflow gate')
    parser.add_argument('--workflow-attention-only', action='store_true', help='Check workflow/application attention and native workflow target navigation')
    parser.add_argument('--history-diagnostics-only', action='store_true', help='Check history, task diagnostics and the native preview/save UI')
    parser.add_argument('--settings-retention-only', action='store_true', help='Check local configuration, output cleanup and native save/restart UI')
    parser.add_argument('--companion-only', action='store_true', help='Build and exercise an isolated native tray/popup desktop and worker')
    parser.add_argument('--netwatch-profile', type=Path, help='Prepared isolated Netwatch workflow; run using the newly built test worker')
    args = parser.parse_args()
    if args.update_preparation_reuse_run and (not args.update_preparation_only or not re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.update_preparation_reuse_run)):
        parser.error('--update-preparation-reuse-run requires --update-preparation-only and a valid run ID')
    if args.verify_readiness_run and (not args.readiness_only or not re.fullmatch(r'\d{8}-\d{6}-[0-9a-f]{8}', args.verify_readiness_run)):
        parser.error('--verify-readiness-run requires --readiness-only and a valid saved run ID')
    if (args.queue_reference_worker or args.resource_reference_worker) and not args.readiness_only:
        parser.error('Queue/resource reference workers require --readiness-only')
    if sum(bool(v) for v in (args.attention_only, args.companion_only, args.workflow_attention_only, args.history_diagnostics_only, args.settings_retention_only, args.netwatch_profile, args.delta_only, args.output_only, args.resume_only, args.profiles_only, args.projects_only, args.identity_only, args.task_fsm_only, args.task_cas_only, args.readiness_only, args.update_preparation_only)) > 1:
        parser.error('Choose exactly one isolated Windows scenario; see --help')
    if args.output_only and not args.output_reference_worker:
        parser.error('--output-only requires --output-reference-worker')
    if args.task_cas_runtime_diagnostic and not args.task_cas_only:
        parser.error('--task-cas-runtime-diagnostic requires --task-cas-only')
    if args.task_cas_reuse_diagnostic and (not args.task_cas_only or args.task_cas_runtime_diagnostic):
        parser.error('--task-cas-reuse-diagnostic requires a full --task-cas-only gate')
    if args.task_cas_reuse_build_cache and (not (args.task_cas_only or args.readiness_only) or args.task_cas_runtime_diagnostic or args.task_cas_reuse_diagnostic):
        parser.error('--task-cas-reuse-build-cache requires full CAS/readiness and cannot reuse diagnostic source')
    if args.task_cas_only and not args.task_cas_runtime_diagnostic and not args.task_cas_reference_worker:
        parser.error('--task-cas-only requires --task-cas-reference-worker')
    if args.task_cas_reference_worker and not args.task_cas_only:
        parser.error('--task-cas-reference-worker requires --task-cas-only')
    config = json.loads(args.controller_config.read_text())
    endpoint = Path(config['stateDirectory']) / ('controller-' + config['instanceId'] + '.sock')
    info = call(endpoint, {'method': 'info'})['info']
    if info.get('journalRecoveryRequired') or info.get('maintenanceError') or info.get('activeTaskCount') != 0:
        raise RuntimeError('Native validation requires a healthy, idle installed worker; do not interrupt user work')
    if info['platform'] != 'windows':
        raise RuntimeError('A connected actual Windows worker is required')
    project = next(p for p in info['projects'] if p['id'] == args.project and p['allowDirectCommands'])
    shared = Path(config['delivery']['root'])
    relative = args.archive.resolve().relative_to(shared.resolve())
    windows_archive = config['delivery']['peerRoot'].rstrip('\\/') + '\\' + str(relative).replace('/', '\\')
    digest = hashlib.sha256(args.archive.read_bytes()).hexdigest()
    with zipfile.ZipFile(args.archive) as archive:
        archive_has_discovery = 'winbridge/apps/worker/tests/discovery.rs' in archive.namelist()
        archive_has_projection = 'winbridge/crates/protocol/tests/projection.rs' in archive.namelist()
        archive_has_operations = 'winbridge/apps/worker/tests/operations.rs' in archive.namelist()
        archive_has_work_scheduling = 'winbridge/apps/worker/tests/work_scheduling.rs' in archive.namelist()
        archive_has_queue_control = 'winbridge/apps/worker/tests/queue_control.rs' in archive.namelist()
        archive_has_resource_scheduling = 'winbridge/apps/worker/tests/resource_scheduling.rs' in archive.namelist()
        archive_checks_work_latency = archive_has_work_scheduling and b'prequeuedStartup' in archive.read('winbridge/apps/worker/tests/work_scheduling.rs')
    run_id = args.verify_readiness_run or time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
    report = shared / config['delivery']['namespace'] / 'workflow-validation' / run_id
    windows_report = config['delivery']['peerRoot'].rstrip('\\/') + '\\' + str(report.relative_to(shared)).replace('/', '\\')
    local = project['workspaceRoot'].rstrip('\\/') + '\\workflow-validation\\' + run_id
    reused = None
    manifest_sha = script_sha = ''
    cache_receipt = None
    if args.task_cas_reuse_build_cache:
        cache_receipt = verify_reusable_build_cache(report.parent, args.task_cas_reuse_build_cache)
        previous = json.loads((ROOT / '.winbridge/workflow-validation' / args.task_cas_reuse_build_cache / 'task-report.json').read_text())['task']
        terminal = call(endpoint, {'method': 'get_task', 'taskId': previous['taskId']})['task']
        if terminal['status'] not in {'succeeded', 'failed'} or terminal.get('exitCode') not in {0, 1} or terminal['submission']['projectId'] != args.project or terminal['submission']['idempotencyKey'] != 'native-workflow-' + args.task_cas_reuse_build_cache:
            raise ValueError('Build cache must belong to a confirmed terminal native task in this project')
    if args.task_cas_reuse_diagnostic:
        reused = verify_reusable_diagnostic(report.parent, args.task_cas_reuse_diagnostic, digest)
        local = project['workspaceRoot'].rstrip('\\/') + '\\workflow-validation\\' + args.task_cas_reuse_diagnostic
        with zipfile.ZipFile(args.archive) as archive:
            manifest_bytes = archive.read('winbridge/winbridge-source-manifest.json')
            manifest_sha = hashlib.sha256(manifest_bytes).hexdigest()
            manifest = json.loads(manifest_bytes)
            if manifest['sourceCommit'] != reused['sourceCommit'] or manifest['dirty']:
                raise ValueError('Reusable diagnostic/archive source identity mismatch')
            script_sha = hashlib.sha256(archive.read('winbridge/scripts/test-windows-workflow.ps1')).hexdigest()
    extra = ''
    profile_argument = ''
    resume_publication = None
    projects_preparation = None
    if args.projects_only:
        spec = importlib.util.spec_from_file_location('projects', ROOT / 'scripts/prepare-projects.py')
        projects = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(projects)
        projects_preparation = projects.prepare(config, local, run_id)
        data = (ROOT / '.winbridge/projects-validation' / run_id / 'prepared.json').read_bytes()
        preparation_sha = hashlib.sha256(data).hexdigest()
        shared_preparation = str(PureWindowsPath(projects_preparation['delivery']['root']) / 'preparation/prepared.json')
        extra = f"$preparationPath = Join-Path $local 'projects-preparation.json'\nCopy-Item -LiteralPath {ps_string(shared_preparation)} -Destination $preparationPath\n"
        profile_argument = ' -ProjectsOnly -ProjectsPreparation $preparationPath -ProjectsPreparationSha256 ' + ps_string(preparation_sha)
    if args.profiles_only:
        profile_argument = ' -ProfilesOnly'
    if args.identity_only:
        profile_argument = ' -IdentityOnly'
    if args.update_preparation_only:
        profile_argument = ' -UpdatePreparationOnly'
        if args.update_preparation_reuse_run:
            old_id = args.update_preparation_reuse_run
            previous_report = report.parent / old_id
            ready = json.loads((previous_report / 'report.ready.json').read_text(encoding='utf-8-sig'))
            names = set()
            for item in ready['files']:
                name = item['path']
                if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name) or name in names or (previous_report / name).is_symlink():
                    raise RuntimeError('Unsafe update preparation cache artifact')
                data = (previous_report / name).read_bytes()
                if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
                    raise RuntimeError('Update preparation cache artifact checksum mismatch')
                names.add(name)
            expected = ['update-source-policy-and-fsm','update-native-preflight','update-legacy-contracts','update-verifier-build']
            prior = json.loads((previous_report / 'report.json').read_text(encoding='utf-8-sig'))
            if (prior['platform'] != 'windows' or prior['sourceDirty'] or not re.fullmatch(r'[0-9a-f]{40}', prior['sourceCommit'])
                    or not re.fullmatch(r'[0-9a-f]{64}', prior['sourceArchiveSha256'])
                    or not {'report.json', *(n+'.log' for n in expected)}.issubset(names)
                    or [g['name'] for g in prior['commands']] != expected or any(g['exitCode'] != 0 for g in prior['commands'])):
                raise RuntimeError('Previous update preparation native groups were not completed')
            saved_root = ROOT / '.winbridge/workflow-validation' / old_id
            saved = json.loads((saved_root / 'task-report.json').read_text())['task']
            original = json.loads((saved_root / 'request.json').read_text())
            actual = call(endpoint, {'method':'get_task','taskId':saved['taskId']})['task']
            if actual['status'] not in {'succeeded','failed'} or actual['submission'] != original or original['projectId'] != args.project or original['idempotencyKey'] != 'native-workflow-'+old_id:
                raise RuntimeError('Update cache owner must be a confirmed terminal native task in this project')
            sha = hashlib.sha256((previous_report / 'report.json').read_bytes()).hexdigest()
            profile_argument += ' -UpdatePreparationCacheRunId ' + ps_string(old_id) + ' -UpdatePreparationCacheSha256 ' + ps_string(sha)
    if args.readiness_only:
        profile_argument = ' -ReadinessOnly'
        if archive_has_queue_control and not args.verify_readiness_run:
            fixture = next(p for p in info['projects'] if p['id'] == 'fixture')
            executable = PureWindowsPath(next(r for r in fixture['recipes'] if r['id'] == 'fixture-success')['command']['executable'])
            if executable.name.lower() != 'winbridge-fixture.exe':
                raise RuntimeError('Known installed fixture path required for previous-worker refusal test')
            if not args.queue_reference_worker and any(c['id'] == 'winbridge.queue-control' for c in info['contractCatalog']['supported']):
                raise RuntimeError('Installed worker already supports queue control; supply the retained schema-12 exe with --queue-reference-worker, not the new runtime')
            profile_argument += ' -QueueReferenceWorker ' + ps_string(args.queue_reference_worker or str(executable.parent / 'winbridge-worker.exe'))
            if archive_has_resource_scheduling:
                if not args.resource_reference_worker and any(c['id'] == 'winbridge.resource-scheduling' for c in info['contractCatalog']['supported']):
                    raise RuntimeError('Supply retained schema-13 exe with --resource-reference-worker; installed worker already supports resources')
                profile_argument += ' -ResourceReferenceWorker ' + ps_string(args.resource_reference_worker or str(executable.parent / 'winbridge-worker.exe'))
    if args.task_fsm_only:
        profile_argument = ' -TaskFsmOnly'
    if args.task_cas_only:
        profile_argument = ' -TaskCasOnly'
        if args.task_cas_runtime_diagnostic:
            profile_argument += ' -TaskCasRuntimeDiagnostic'
        else:
            profile_argument += ' -TaskCasReferenceWorker ' + ps_string(args.task_cas_reference_worker)
    if args.resume_only:
        resume_publication, resume_blob_sha, resume_delivery, resume_measurement = prepare_resume_package(config, shared, run_id)
        profile_argument = ' -ResumeOnly -ResumeDeliveryRoot ' + ps_string(resume_delivery) + ' -ResumePublicationFile ' + ps_string(resume_measurement)
    if args.output_only:
        profile_argument = ' -OutputOnly -OutputReferenceWorker ' + ps_string(args.output_reference_worker)
    if args.delta_only:
        profile_argument = ' -DeltaOnly'
    if args.attention_only:
        profile_argument = ' -AttentionOnly'
    if args.companion_only:
        profile_argument = ' -CompanionOnly'
    if args.workflow_attention_only:
        profile_argument = ' -WorkflowAttentionOnly'
    if args.history_diagnostics_only:
        profile_argument = ' -HistoryDiagnosticsOnly'
    if args.settings_retention_only:
        profile_argument = ' -SettingsRetentionOnly'
    if args.netwatch_profile:
        profile = json.loads(args.netwatch_profile.read_text())
        if not profile.get('readiness', {}).get('netwatch', {}).get('instanceId', '').startswith('winbridge-'):
            raise RuntimeError('Netwatch profile must have an isolated instance')
        arguments = profile['start']['arguments']
        runtime = PureWindowsPath(arguments[arguments.index('--runtime-directory') + 1])
        suffix = PureWindowsPath(profile['readiness']['netwatch']['runtimeDirectory'])
        if runtime.parts[-len(suffix.parts):] != suffix.parts:
            raise RuntimeError('Netwatch runtime mapping mismatch')
        test_root = runtime.parents[len(suffix.parts) - 1]
        owned_relative = test_root.relative_to(PureWindowsPath(project['workspaceRoot']))
        if not owned_relative.parts or owned_relative.parts[0].lower() in {'snapshots', '.winbridge'} or '..' in owned_relative.parts or profile['projectId'] != args.project:
            raise RuntimeError('Isolated Netwatch test root must be a new subdirectory of the registered project')
        PureWindowsPath(profile['start']['executable']).relative_to(test_root)
        encoded = base64.b64encode(json.dumps(profile).encode()).decode()
        extra = f"$profilePath = Join-Path $local 'netwatch.request.json'\n[IO.File]::WriteAllBytes($profilePath, [Convert]::FromBase64String('{encoded}'))\n"
        profile_argument = ' -NetwatchProfile $profilePath'
    if cache_receipt:
        profile_argument += ' -BuildCacheRunId ' + ps_string(cache_receipt['runId']) + ' -BuildCacheReportSha256 ' + ps_string(cache_receipt['reportSha256'])
    script = f"""$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$archive = {ps_string(windows_archive)}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne {ps_string(digest)}) {{ throw 'Archive SHA-256 mismatch' }}
$local = {ps_string(local)}
$reuse = {'$true' if reused else '$false'}
if (-not $reuse -and (Test-Path -LiteralPath $local)) {{ throw 'Refusing to overwrite an existing test run' }}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {{
    foreach ($entry in $zip.Entries) {{
        if ($entry.FullName -notmatch '^winbridge/' -or $entry.FullName -match '(^|/)\\.\\.?(/|$)|[\\\\:]') {{ throw 'Unsafe source archive entry' }}
    }}
}} finally {{ $zip.Dispose() }}
if (-not $reuse) {{
    New-Item -ItemType Directory -Path $local | Out-Null
    Expand-Archive -LiteralPath $archive -DestinationPath $local
}}
$source = Join-Path $local 'winbridge'
if ($reuse) {{
    # Revalidate trusted entrypoints before executing an existing script. The
    # script then validates every source file against the archive-pinned manifest.
    foreach ($path in @($local, $source, (Join-Path $local 'test-build-cache'), (Join-Path $local 'test-temp'))) {{
        $node = Get-Item -LiteralPath $path -ErrorAction Stop
        while ($null -ne $node) {{
            if ($node.Attributes -band [IO.FileAttributes]::ReparsePoint) {{ throw 'Refusing linked diagnostic source/cache' }}
            $node = $node.Parent
        }}
    }}
    if ((Get-FileHash -LiteralPath (Join-Path $source 'winbridge-source-manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant() -ne {ps_string(manifest_sha)}) {{ throw 'Reused manifest differs from archive' }}
    if ((Get-FileHash -LiteralPath (Join-Path $source 'scripts/test-windows-workflow.ps1') -Algorithm SHA256).Hash.ToLowerInvariant() -ne {ps_string(script_sha)}) {{ throw 'Reused entrypoint differs from archive' }}
    $env:WINBRIDGE_PROCESS_TRACE = '1'
    $drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($source))
    Write-Output ('Reusable diagnostic volume free bytes: ' + $drive.AvailableFreeSpace)
    if ($drive.AvailableFreeSpace -lt 8GB) {{ throw 'Less than 8 GiB free; existing validation runs are retained' }}
}}
{extra}
$pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
& $pwsh -NoProfile -File (Join-Path $source 'scripts/test-windows-workflow.ps1') -SourceDirectory $source -ReportDirectory {ps_string(windows_report)} -SourceArchiveSha256 {ps_string(digest)}{profile_argument}
$nativeExit = $LASTEXITCODE
if ($reuse) {{ Write-Output ('Reusable diagnostic volume free bytes after: ' + $drive.AvailableFreeSpace) }}
exit $nativeExit
"""
    request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': args.project,
               'idempotencyKey': 'native-workflow-' + run_id, 'timeoutMs': 1800000,
               'command': {'kind': 'direct', 'executable': 'pwsh.exe',
                           'arguments': ['-NoProfile', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()], 'workingDirectory': '.'}}
    local_report = ROOT / '.winbridge' / 'workflow-validation' / run_id
    if args.verify_readiness_run:
        if local_report.is_symlink() or report.is_symlink():
            raise RuntimeError('Linked saved readiness run refused')
        request = json.loads((local_report / 'request.json').read_text())
        saved = json.loads((local_report / 'task-report.json').read_text())
        task_id = saved['task']['taskId']
        task = call(endpoint, {'method': 'get_task', 'taskId': task_id})['task']
        if task['submission'] != request or task['status'] != 'succeeded' or task['exitCode'] != 0:
            raise RuntimeError('Saved run does not match the terminal accepted task; no submit performed')
        chunks = saved['output']
        print(json.dumps({'readOnlyRecheck': True, 'taskId': task_id, 'reportDirectory': str(report)}), flush=True)
    else:
        local_report.mkdir(parents=True)
        (local_report / 'request.json').write_text(json.dumps(request, indent=2))
        # Preserve the request for reconciliation; stop on an unexpected worker error.
        accepted = call(endpoint, {'method': 'submit', 'request': request})['task']
        task_id = accepted['taskId']
        print(json.dumps({'taskId': task_id, 'reportDirectory': str(report), 'windowsReportDirectory': windows_report, 'sourceSha256': digest, 'reusedDiagnostic': args.task_cas_reuse_diagnostic, 'reusedBuildCache': cache_receipt}), flush=True)
        chunks = []
        sequence = 0
        deadline = time.monotonic() + 1850
        last_status = None
        while time.monotonic() < deadline:
            task = call(endpoint, {'method': 'get_task', 'taskId': task_id})['task']
            if task['status'] != last_status:
                print('native task: ' + task['status'], flush=True)
                last_status = task['status']
            while True:
                page = call(endpoint, {'method': 'read_output', 'taskId': task_id, 'afterSequence': sequence})['chunks']
                if not page:
                    break
                chunks.extend(page)
                sequence = page[-1]['sequence']
            if task['status'] not in {'queued', 'running'}:
                break
            time.sleep(1)
        else:
            raise TimeoutError(f'Native task {task_id} deadline; inspect the retained request and worker task')
        (local_report / 'task-report.json').write_text(json.dumps({'schemaVersion': 1, 'platform': 'windows', 'task': task, 'output': chunks}, indent=2))
    if not (report / 'report.ready.json').exists():
        diagnostic = ''.join(bytes(chunk['bytes']).decode('utf-8', errors='replace') for chunk in chunks)[-8000:]
        raise RuntimeError(f'Native bootstrap failed; task {task_id}; retained report {local_report}: {diagnostic}')
    ready = json.loads((report / 'report.ready.json').read_text(encoding='utf-8-sig'))
    for item in ready['files']:
        name = item['path']
        if Path(name).name != name:
            raise RuntimeError('Unsafe report artifact path')
        data = (report / name).read_bytes()
        if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
            raise RuntimeError('Report artifact checksum mismatch: ' + name)
    result = json.loads((report / 'report.json').read_text(encoding='utf-8-sig'))
    if task['status'] != 'succeeded' or task['exitCode'] != 0 or not result['passed'] or result['platform'] != 'windows' or result['sourceArchiveSha256'] != digest:
        raise RuntimeError({'task': task_id, 'result': result})
    verify_build_cache_usage(result, cache_receipt, local)
    if args.update_preparation_only:
        expected = ['update-source-policy-and-fsm', 'update-native-preflight', 'update-legacy-contracts', 'update-verifier-build']
        scope = result.get('updatePreparation', {})
        proof = json.loads((report / 'update-candidate-proof.json').read_text(encoding='utf-8-sig'))
        build = proof.get('workerBuild', {})
        if ([g['name'] for g in result['commands']] != expected or any(g['exitCode'] != 0 for g in result['commands'])
                or scope.get('passed') is not True or scope.get('runtimeApplied') is not False or scope.get('windowsOpened') is not False
                or proof.get('status') != 'built' or proof.get('runtimeApplied') is not False or proof.get('candidateVerified') is not False
                or build.get('sourceDirty') is not False or build.get('sourceCommit') != result['sourceCommit']
                or build.get('sourceManifestSha256') != scope.get('manifestSha256')
                or {a['name'] for a in proof['artifacts']} != {'winbridge-worker.exe','winbridge-fixture.exe','winbridge-desktop.exe','winbridge-update.exe'}):
            raise RuntimeError('Incomplete native update preparation proof')
        if args.update_preparation_reuse_run:
            expected_cache = str(PureWindowsPath(project['workspaceRoot']) / 'workflow-validation' / args.update_preparation_reuse_run)
            if (proof.get('validationCacheDirectory') != expected_cache
                    or result.get('updatePreparationReuse') != {'runId':args.update_preparation_reuse_run,'reportSha256':sha}):
                raise RuntimeError('Candidate did not reuse the pinned validation cache')
    if bool(result.get('diagnosticOnly', False)) != args.task_cas_runtime_diagnostic:
        raise RuntimeError('Native report diagnostic/full gate scope mismatch')
    if args.task_cas_runtime_diagnostic:
        if [group['name'] for group in result['commands']] != ['process-resume-guard', 'process-runtime-recovery-diagnostic'] or any(group['exitCode'] != 0 for group in result['commands']):
            raise RuntimeError('Incomplete process diagnostic groups')
    if args.task_cas_only and not args.task_cas_runtime_diagnostic:
        expected_groups = ['task-cas-pure-fsm', 'task-cas-process-resume-guard', 'task-cas-fenced-contract', 'task-cas-frozen-contracts', 'task-cas-native-writers', 'task-cas-older-worker-refusal', 'task-cas-desktop-contract-check']
        if [group['name'] for group in result['commands']] != expected_groups or any(group['exitCode'] != 0 for group in result['commands']):
            raise RuntimeError('Incomplete fenced request/CAS native gate groups')
        proof = json.loads((report / 'task-cas-reference-proof.json').read_text(encoding='utf-8-sig'))
        if not proof['passed'] or proof['platform'] != 'windows' or proof['journalSchema'] != 12 or not proof['journalUnchanged'] or proof['exitCode'] != 1:
            raise RuntimeError('Incomplete older-worker schema refusal proof')
        if proof['build']['sourceCommit'] != result['sourceCommit'] or proof['build']['sourceDirty'] or proof['referenceBuild'] != info['build']:
            raise RuntimeError('Candidate/reference worker source identity mismatch')
        if archive_has_discovery:
            catalog_proof = json.loads((report / 'discovery-reference-proof.json').read_text(encoding='utf-8-sig'))
            if not catalog_proof['passed'] or catalog_proof['platform'] != 'windows' or not catalog_proof['catalogAbsent'] or not catalog_proof['isolatedReferenceStopped'] or catalog_proof['candidateBuild'] != proof['build'] or catalog_proof['referenceBuild'] != info['build']:
                raise RuntimeError('Incomplete native legacy discovery/reference proof')
    if args.readiness_only:
        expected_groups = ['readiness-pure-policy', 'readiness-wire-compatibility']
        if archive_has_projection:
            expected_groups.append('readiness-cache-freshness')
        expected_groups += ['readiness-native-info-and-scope']
        if archive_has_operations:
            expected_groups += ['operations-query-contract', 'operations-transaction-snapshot', 'operations-native-ipc']
        if archive_has_work_scheduling:
            expected_groups += ['work-scheduling-policy-contract', 'work-scheduling-durable-capacity', 'work-scheduling-native-pool', 'work-scheduling-workflow-handoff']
        if archive_has_queue_control:
            expected_groups += ['queue-control-contract', 'queue-control-durable-admission', 'queue-control-native-runtime', 'queue-control-previous-worker-refusal']
        if archive_has_resource_scheduling:
            expected_groups += ['resource-scheduling-contract', 'resource-scheduling-atomic-sets', 'resource-scheduling-native-runtime', 'resource-scheduling-previous-worker-refusal']
        expected_groups += ['readiness-desktop-contract-check']
        if not result.get('readinessOnly') or [g['name'] for g in result['commands']] != expected_groups or any(g['exitCode'] != 0 for g in result['commands']):
            raise RuntimeError('Incomplete readiness foundation gate')
        proof = json.loads((report / 'policy-proof.json').read_text(encoding='utf-8-sig'))
        local_schemas = json.loads(subprocess.run([str(ROOT / 'target/debug/winbridge-controller'), '--contract-schemas'], check=True, capture_output=True, text=True, timeout=20).stdout)
        if not proof['passed'] or proof['platform'] != 'windows' or proof['candidateBuild']['sourceCommit'] != result['sourceCommit'] or proof['candidateBuild']['sourceDirty'] or proof['policies']['provider'] != proof['candidateBuild'] or proof['schemas']['provider'] != proof['candidateBuild'] or proof['schemas']['schemas'] != local_schemas['schemas'] or not proof['context']['journalIncarnation']:
            raise RuntimeError('Incomplete native policy/Info proof')
        if archive_has_work_scheduling:
            scheduling = json.loads((report / 'work-scheduling-proof.json').read_text(encoding='utf-8-sig'))
            scenarios = scheduling.get('scenarios', [])
            if not scheduling.get('passed') or scheduling.get('platform') != 'windows' or scheduling.get('build') != proof['candidateBuild'] or [s['maxConcurrentWork'] for s in scenarios] != [1, 2, None] or any(s['admissions'] != 45 or s['cleanupSlots'] != 0 or s['initialSlots'] != (s['maxConcurrentWork'] or 3) or not s['context']['journalIncarnation'] for s in scenarios):
                raise RuntimeError('Incomplete actual native three-project capacity proof')
            if archive_checks_work_latency and any(s.get('prequeuedStartup') is not True or type(s.get('ipcInfoMs')) is not int or not 0 <= s['ipcInfoMs'] < 5000 for s in scenarios):
                raise RuntimeError('Native prequeued startup/loaded IPC responsiveness proof missing')
        if archive_has_queue_control:
            queue = json.loads((report / 'queue-control-proof.json').read_text(encoding='utf-8-sig'))
            expected_checks = ['local-only-envelope', 'root-quota', 'duplicate-full-queue', 'dispatch-pause-no-launch', 'live-owner-survives-pause', 'cancel-output-readable', 'receipt-restart-replay', 'no-automatic-resume', 'zero-cleanup-slots']
            if queue.get('passed') is not True or queue.get('platform') != 'windows' or queue.get('build') != proof['candidateBuild'] or queue.get('checks') != expected_checks or queue['context'] != queue['finalQueue']['context'] or queue['finalQueue']['pendingRoots'] != 0 or queue['finalQueue']['mode'] != 'running':
                raise RuntimeError('Incomplete native durable queue/runtime proof')
        if archive_has_resource_scheduling:
            resources = json.loads((report / 'resource-scheduling-proof.json').read_text(encoding='utf-8-sig'))
            expected = ['three-project-complete-sets', 'shared-cache-blocker', 'fair-next-project', 'native-info-operation-page', 'fenced-cancel', 'duplicate-same-process', 'frozen-request-after-config-edit', 'restart-same-journal', 'zero-resource-cleanup']
            if resources.get('passed') is not True or resources['platform'] != 'windows' or resources['build'] != proof['candidateBuild'] or resources['checks'] != expected or resources['context'] != resources['final']['context'] or resources['final']['owners'] or any(r['usedUnits'] for r in resources['final']['resources']):
                raise RuntimeError('Incomplete native resource set/restart/cleanup proof')
    if args.profiles_only:
        proof = json.loads((report / 'profiles-proof.json').read_text())
        tasks = proof['completed']
        if not proof['passed'] or proof['platform'] != 'windows' or len(tasks) != 2 or tasks[0]['taskId'] == tasks[1]['taskId']:
            raise RuntimeError('Incomplete native two-project profile proof')
        if proof['build']['sourceCommit'] != result['sourceCommit'] or proof['build']['sourceDirty']:
            raise RuntimeError('Profile test worker source identity mismatch')
        for record in tasks:
            if record['status'] != 'succeeded' or record['exitCode'] != 0 or record['recipeOrigin']['profileId'] != 'common':
                raise RuntimeError('Profile command/provenance was not committed')
        if proof['cancelled']['status'] != 'cancelled' or proof['recovered']['status'] != 'indeterminate' or proof['blockedQueued']['status'] != 'failed' or proof['explicitNewProfile']['status'] != 'succeeded':
            raise RuntimeError('Profile cancel/recovery/explicit retry proof incomplete')
    if args.output_only:
        proof = json.loads((report / 'output-proof.json').read_text())
        reference = [case for case in proof['cases'] if case.get('version') == 'reference' and 'measurement' in case]
        current = [case for case in proof['cases'] if case.get('version') == 'current' and 'measurement' in case]
        if not proof['passed'] or proof['platform'] != 'windows' or len(reference) != 3 or len(current) != 3:
            raise RuntimeError('Incomplete matched native output gate')
        for old, new in zip(reference, current):
            if old['build']['sourceCommit'] != info['build']['sourceCommit'] or old['build']['sourceDirty'] or new['build']['sourceCommit'] != result['sourceCommit'] or new['build']['sourceDirty']:
                raise RuntimeError('Output comparison source identity mismatch')
            if old['bodySha256'] != new['bodySha256'] or old['stderrBytes'] != 1048576 or new['stderrBytes'] != 1048576 or new['measurement']['pages'] >= old['measurement']['pages']:
                raise RuntimeError('Output hashes/counts or page reduction did not match')
    if args.resume_only:
        proof = json.loads((report / 'resume-proof.json').read_text())
        baseline = proof['baseline']['import']
        resumed = proof['resumed']['import']
        transfer = resumed['transfer']
        if not proof['passed'] or proof['platform'] != 'windows' or proof['build']['sourceCommit'] != result['sourceCommit'] or proof['build']['sourceDirty']:
            raise RuntimeError('Native resume source identity/proof mismatch')
        if proof['ready'] != resume_publication['ready'] or proof['blobSha256'] != resume_blob_sha:
            raise RuntimeError('Native resume did not import the exact pinned Debian fixture')
        if baseline['transfer']['reusedBytes'] != 0 or baseline['transfer']['sharedBytes'] != transfer['payloadBytes'] or transfer['reusedBytes'] < 4*1024*1024 or transfer['sharedBytes']+transfer['reusedBytes'] != transfer['payloadBytes']:
            raise RuntimeError('Native resume did not reduce verified shared bytes')
        if proof['recovered']['status'] != 'interrupted' or resumed['status'] != 'succeeded' or proof['buildTask']['status'] != 'succeeded' or proof['buildTask']['snapshot']['manifestSha256'] != proof['ready']['manifestSha256']:
            raise RuntimeError('Native resume recovery/build pin mismatch')
    if args.projects_only:
        proof = json.loads((report / 'projects-proof.json').read_text(encoding='utf-8-sig'))
        if not proof['passed'] or proof['platform'] != 'windows' or len(proof['projects']) != 2:
            raise RuntimeError('Incomplete real-project native proof')
        if proof['build']['sourceCommit'] != result['sourceCommit'] or proof['build']['sourceDirty']:
            raise RuntimeError('Project coordinator source identity mismatch')
        sources = {s['projectId']: s for s in projects_preparation['sources']}
        for record in proof['projects']:
            flow, build, app = record['workflow'], record['buildTask'], record['application']
            ready_pin = sources[record['projectId']]['ready']
            if flow['status'] != 'succeeded' or app['status'] != 'stopped' or build['status'] != 'succeeded' or build['exitCode'] != 0:
                raise RuntimeError('Real-project cycle/stop incomplete')
            if build['snapshot']['manifestSha256'] != ready_pin['manifestSha256'] or flow['submission']['snapshotId'] != ready_pin['snapshotId']:
                raise RuntimeError('Project source differs from pinned Debian publication')
            if build['recipeOrigin']['profileId'] != 'rust-service' or record['repeatedWorkflowId'] != flow['workflowId']:
                raise RuntimeError('Common profile provenance/dedup proof missing')
        if proof['initialGeneration'] == proof['restartedGeneration'] or proof['child']['recoveredTree']['status'] != 'indeterminate' or proof['child']['success']['status'] != 'succeeded':
            raise RuntimeError('Real-project restart/recovery proof missing')
    final_info = call(endpoint, {'method':'info'})['info']
    if final_info['generation'] != info['generation'] or final_info.get('journalRecoveryRequired') or final_info.get('maintenanceError') or final_info.get('activeTaskCount') != 0:
        raise RuntimeError('Installed worker health/generation changed unexpectedly; inspect retained task/report')
    proof_name = 'worker-observation-' + uuid.uuid4().hex + '.json' if args.verify_readiness_run else 'worker-proof.json'
    (local_report / proof_name).write_text(json.dumps({'readOnlyRecheck':bool(args.verify_readiness_run),'initial':{k:v for k,v in info.items() if k!='projects'},'final':{k:v for k,v in final_info.items() if k!='projects'}},indent=2))
    print(json.dumps({'passed': True, 'platform': 'windows', 'taskId': task_id, 'reportDirectory': str(report), 'reportSha256': hashlib.sha256((report / 'report.json').read_bytes()).hexdigest(), 'commands': result['commands']}), flush=True)


if __name__ == '__main__':
    main()
