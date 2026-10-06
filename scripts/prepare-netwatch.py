#!/usr/bin/env python3
"""Publish a committed isolated Netwatch clone and prepare its Windows workflow.

Never edits the original Netwatch repository or submits a command implicitly.
The validation-only commit adds instance/runtime/log arguments to the agent.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def replace_once(path, old, new):
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f'Netwatch source contract changed: {path.name}; patch not applied')
    path.write_text(text.replace(old, new))


def isolated_clone(original, source):
    original_commit = command('git', '-C', str(original), 'rev-parse', 'HEAD')
    subprocess.run(['git', 'clone', '--quiet', '--no-hardlinks', str(original), str(source)], check=True)
    subprocess.run(['git', '-C', str(source), 'checkout', '--quiet', original_commit], check=True)
    replace_once(source / 'agent/src/ipc.rs', '''        windows_pipe::current_user_sid()?
''', '''        match std::env::var("NETWATCH_INSTANCE_ID") {
            Ok(id) if id.starts_with("winbridge-") && id.len() <= 64
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') =>
                format!("{}-{id}", windows_pipe::current_user_sid()?),
            Ok(_) => bail!("validation instance ID must be winbridge-* (up to 64 safe bytes)"),
            Err(std::env::VarError::NotPresent) => windows_pipe::current_user_sid()?,
            Err(error) => return Err(error.into()),
        }
''')
    replace_once(source / 'agent/src/main.rs', '    let directory = ipc::runtime_directory()?;', '''    let args: Vec<String> = std::env::args().collect();
    let argument = |name: &str| -> Result<Option<String>> {
        match args.iter().position(|value| value == name) {
            Some(index) => Ok(Some(args.get(index + 1).context("missing validation argument value")?.clone())),
            None => Ok(None),
        }
    };
    let instance = argument("--instance-id")?;
    if let Some(id) = &instance {
        anyhow::ensure!(id.starts_with("winbridge-") && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'), "invalid validation instance ID");
        std::env::set_var("NETWATCH_INSTANCE_ID", id);
    }
    let directory = match argument("--runtime-directory")? {
        Some(path) => std::path::PathBuf::from(path),
        None => ipc::runtime_directory()?,
    };''')
    replace_once(source / 'agent/src/main.rs', '    let log_path = logging::init(&log_directory,', '''    let log_directory = argument("--log-directory")?.map(std::path::PathBuf::from).unwrap_or(log_directory);
    let log_path = logging::init(&log_directory,''')
    (source / 'winbridge-validation.json').write_text(json.dumps({'nodeId': 'winbridge-netwatch-validation', 'role': 'desktop', 'connectionMode': 'direct-ip', 'listenPort': 1113, 'peers': []}, indent=2))
    shutil.copyfile(ROOT / 'fixtures/netwatch/validation-build.ps1', source / 'winbridge-validation-build.ps1')
    subprocess.run(['git', '-C', str(source), 'add', 'agent/src/main.rs', 'agent/src/ipc.rs', 'winbridge-validation.json', 'winbridge-validation-build.ps1'], check=True)
    subprocess.run(['git', '-C', str(source), '-c', 'user.name=WinBridge validation', '-c', 'user.email=validation@local', 'commit', '--quiet', '-m', 'test(agent): isolate WinBridge validation instance'], check=True)
    return original_commit, command('git', '-C', str(source), 'rev-parse', 'HEAD')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path, required=True)
    parser.add_argument('--netwatch-source', type=Path, default=Path('/home/user1/netwatch'))
    parser.add_argument('--project', default='snapshot-fixture', help='Already registered project; all Windows writes stay in its root')
    parser.add_argument('--workspace-root', help='Override only for a separately registered isolated test worker')
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('rpc', ROOT / 'scripts/validate-workflow-native.py')
    rpc = importlib.util.module_from_spec(spec); spec.loader.exec_module(rpc)
    settings = json.loads(args.controller_config.read_text())
    endpoint = Path(settings['stateDirectory']) / ('controller-' + settings['instanceId'] + '.sock')
    info = rpc.call(endpoint, {'method': 'info'})['info']
    project = next(p for p in info['projects'] if p['id'] == args.project)
    if info['platform'] != 'windows' or not project['allowDirectCommands']:
        raise RuntimeError('This profile requires Windows and a registered project with direct commands')
    label = time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8]
    run = ROOT / '.winbridge/netwatch-validation' / label
    run.mkdir(parents=True)
    source = run / 'source'
    original, commit = isolated_clone(args.netwatch_source, source)
    config = {**settings, 'projects': [{'id': args.project, 'sourceRoot': str(source), 'sourceMode': 'commit'}]}
    config_path = run / 'controller.local.json'; config_path.write_text(json.dumps(config, indent=2))
    ready = json.loads(command(str(ROOT / 'target/debug/winbridge-controller'), '--config', str(config_path), 'publish', args.project, 'HEAD'))
    root = (args.workspace_root or project['workspaceRoot']).rstrip('\\/')
    path = lambda *parts: '\\'.join([root, *parts])
    relative = f'netwatch-validation/{label}'
    runtime = path('netwatch-validation', label, 'runtime')
    logs = path('netwatch-validation', label, 'logs')
    cache = path('netwatch-build-cache')
    request = {'protocolVersion': 1, 'requestId': str(uuid.uuid4()), 'idempotencyKey': str(uuid.uuid4()),
        'name': 'Netwatch · isolated development cycle', 'projectId': args.project, 'snapshotId': ready['snapshotId'], 'manifestSha256': ready['manifestSha256'], 'timeoutMs': 900000,
        'build': {'kind': 'direct', 'executable': 'pwsh.exe', 'arguments': ['-NoProfile', '-File', './winbridge-validation-build.ps1', '-RuntimeDirectory', runtime, '-LogDirectory', logs, '-TargetDirectory', cache], 'workingDirectory': '.'},
        'start': {'kind': 'direct', 'executable': path('netwatch-build-cache', 'debug', 'netwatch-agent.exe'), 'arguments': ['--managed', '--instance-id', 'winbridge-' + label, '--runtime-directory', runtime, '--log-directory', logs], 'workingDirectory': '.'},
        'readiness': {'netwatch': {'instanceId': 'winbridge-' + label, 'runtimeDirectory': relative + '/runtime'}, 'timeoutMs': 15000},
        'logPath': relative + '/logs/netwatch-agent.log', 'artifacts': [{'name': 'Netwatch agent', 'path': 'netwatch-build-cache/debug/netwatch-agent.exe'}]}
    request_path = run / 'workflow.request.json'; request_path.write_text(json.dumps(request, ensure_ascii=False, indent=2))
    (run / 'isolation.patch').write_text(command('git', '-C', str(source), 'diff', original, commit))
    result = {'runId': label, 'runDirectory': str(run), 'requestPath': str(request_path), 'originalCommit': original, 'validationCommit': commit, 'snapshotId': ready['snapshotId'], 'manifestSha256': ready['manifestSha256'], 'workspaceRoot': root,
        'purpose': 'Separate Netwatch clone/config/logs/IPC, no peers or listener; preparation does not run Windows commands',
        'workerFeatures': info.get('workflowFeatures', [])}
    (run / 'prepared.json').write_text(json.dumps(result, ensure_ascii=False, indent=2))
    print(json.dumps(result, ensure_ascii=False))


if __name__ == '__main__':
    main()
