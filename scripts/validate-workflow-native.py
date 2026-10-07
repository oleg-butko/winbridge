#!/usr/bin/env python3
"""Run isolated, real Windows regression tests through the already approved worker."""
import argparse
import base64
import hashlib
import json
from pathlib import Path, PureWindowsPath
import socket
import struct
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


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
    parser.add_argument('--attention-only', action='store_true', help='Run the stage-5 attention tests without repeating the completed workflow gate')
    parser.add_argument('--companion-only', action='store_true', help='Build and exercise an isolated native tray/popup desktop and worker')
    parser.add_argument('--netwatch-profile', type=Path, help='Prepared isolated Netwatch workflow; run using the newly built test worker')
    args = parser.parse_args()
    if sum(bool(v) for v in (args.attention_only, args.companion_only, args.netwatch_profile)) > 1:
        parser.error('Choose one isolated scenario: attention, companion or Netwatch')
    config = json.loads(args.controller_config.read_text())
    endpoint = Path(config['stateDirectory']) / ('controller-' + config['instanceId'] + '.sock')
    info = call(endpoint, {'method': 'info'})['info']
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
    if args.attention_only:
        profile_argument = ' -AttentionOnly'
    if args.companion_only:
        profile_argument = ' -CompanionOnly'
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
    print(json.dumps({'passed': True, 'platform': 'windows', 'taskId': task_id, 'reportDirectory': str(report), 'reportSha256': hashlib.sha256((report / 'report.json').read_bytes()).hexdigest(), 'commands': result['commands']}), flush=True)


if __name__ == '__main__':
    main()
