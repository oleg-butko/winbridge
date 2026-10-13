#!/usr/bin/env python3
"""Run isolated, real Windows regression tests through the already approved worker."""
import argparse
import base64
import hashlib
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import socket
import struct
import subprocess
import time
import uuid

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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path, required=True)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--project', default='snapshot-fixture')
    parser.add_argument('--delta-only', action='store_true', help='Check pinned-base delta on actual HGFS with isolated journal/build, without desktop windows')
    parser.add_argument('--profiles-only', action='store_true', help='Check two registered projects with a reusable recipe profile, real IPC, cancel and crash/restart; no desktop windows')
    parser.add_argument('--projects-only', action='store_true', help='Publish separate committed Netwatch/WinBridge clones and run both through a common build profile; no desktop or network from test instances')
    parser.add_argument('--identity-only', action='store_true', help='Check protocol/build identity on actual Windows/Git without desktop or test workers')
    parser.add_argument('--task-fsm-only', action='store_true', help='Check Task FSM, frozen v1 baseline, real launch/PID commit faults, cancel/tree/timeout/recovery and workflow regressions; no desktop windows')
    parser.add_argument('--task-cas-only', action='store_true', help='Check task revisions, schema-6 migration, stale writes and atomic output/retention with an older reference worker; no desktop windows')
    parser.add_argument('--task-cas-reference-worker', help='Read-only Windows path to a schema-6 worker exe, used only with NEW isolated state to prove schema-7 refusal')
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
    if sum(bool(v) for v in (args.attention_only, args.companion_only, args.workflow_attention_only, args.history_diagnostics_only, args.settings_retention_only, args.netwatch_profile, args.delta_only, args.output_only, args.resume_only, args.profiles_only, args.projects_only, args.identity_only, args.task_fsm_only, args.task_cas_only)) > 1:
        parser.error('Choose exactly one isolated Windows scenario; see --help')
    if args.output_only and not args.output_reference_worker:
        parser.error('--output-only requires --output-reference-worker')
    if args.task_cas_only and not args.task_cas_reference_worker:
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
    run_id = time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
    report = shared / config['delivery']['namespace'] / 'workflow-validation' / run_id
    windows_report = config['delivery']['peerRoot'].rstrip('\\/') + '\\' + str(report.relative_to(shared)).replace('/', '\\')
    local = project['workspaceRoot'].rstrip('\\/') + '\\workflow-validation\\' + run_id
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
    if args.task_fsm_only:
        profile_argument = ' -TaskFsmOnly'
    if args.task_cas_only:
        profile_argument = ' -TaskCasOnly -TaskCasReferenceWorker ' + ps_string(args.task_cas_reference_worker)
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
    script = f"""$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$archive = {ps_string(windows_archive)}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne {ps_string(digest)}) {{ throw 'Archive SHA-256 mismatch' }}
$local = {ps_string(local)}
if (Test-Path -LiteralPath $local) {{ throw 'Refusing to overwrite an existing test run' }}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {{
    foreach ($entry in $zip.Entries) {{
        if ($entry.FullName -notmatch '^winbridge/' -or $entry.FullName -match '(^|/)\\.\\.?(/|$)|[\\\\:]') {{ throw 'Unsafe source archive entry' }}
    }}
}} finally {{ $zip.Dispose() }}
New-Item -ItemType Directory -Path $local | Out-Null
Expand-Archive -LiteralPath $archive -DestinationPath $local
$source = Join-Path $local 'winbridge'
{extra}
$pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
& $pwsh -NoProfile -File (Join-Path $source 'scripts/test-windows-workflow.ps1') -SourceDirectory $source -ReportDirectory {ps_string(windows_report)} -SourceArchiveSha256 {ps_string(digest)}{profile_argument}
exit $LASTEXITCODE
"""
    request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'projectId': args.project,
               'idempotencyKey': 'native-workflow-' + run_id, 'timeoutMs': 1800000,
               'command': {'kind': 'direct', 'executable': 'pwsh.exe',
                           'arguments': ['-NoProfile', '-EncodedCommand', base64.b64encode(script.encode('utf-16le')).decode()], 'workingDirectory': '.'}}
    local_report = ROOT / '.winbridge' / 'workflow-validation' / run_id
    local_report.mkdir(parents=True)
    (local_report / 'request.json').write_text(json.dumps(request, indent=2))
    # Preserve the request for reconciliation; stop on an unexpected worker error.
    accepted = call(endpoint, {'method': 'submit', 'request': request})['task']
    task_id = accepted['taskId']
    print(json.dumps({'taskId': task_id, 'reportDirectory': str(report), 'windowsReportDirectory': windows_report, 'sourceSha256': digest}), flush=True)
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
    if args.task_cas_only:
        proof = json.loads((report / 'task-cas-reference-proof.json').read_text(encoding='utf-8-sig'))
        if not proof['passed'] or proof['platform'] != 'windows' or proof['journalSchema'] != 7 or not proof['journalUnchanged'] or proof['exitCode'] != 1:
            raise RuntimeError('Incomplete older-worker schema refusal proof')
        if proof['build']['sourceCommit'] != result['sourceCommit'] or proof['build']['sourceDirty'] or proof['referenceBuild'] != info['build']:
            raise RuntimeError('Candidate/reference worker source identity mismatch')
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
    (local_report / 'worker-proof.json').write_text(json.dumps({'initial':{k:v for k,v in info.items() if k!='projects'},'final':{k:v for k,v in final_info.items() if k!='projects'}},indent=2))
    print(json.dumps({'passed': True, 'platform': 'windows', 'taskId': task_id, 'reportDirectory': str(report), 'reportSha256': hashlib.sha256((report / 'report.json').read_bytes()).hexdigest(), 'commands': result['commands']}), flush=True)


if __name__ == '__main__':
    main()
