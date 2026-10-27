#!/usr/bin/env python3
"""Independent semantic audit; caller supplies ready-verified bytes and source pins."""
import hashlib
import json
from pathlib import PureWindowsPath


def need(value, message):
    if not value:
        raise ValueError(message)


def verify(files, source, controller_build, runtime):
    def value(name):
        return json.loads(files[name].decode('utf-8-sig'))
    report, proof, descriptor, ready = map(value, ('report.json', 'network-complete.json', 'worker-build-identity.json', 'network-ready.json'))
    need(report['kind'] == 'multi_controller_native_windows' and report['passed'] is True
         and report['error'] is None and report['isolatedWorkerExitConfirmed'] is True
         and type(report['remainingFixtureProcesses']) is int and report['remainingFixtureProcesses'] == 0
         and report['installedRuntimeChanged'] is False and report['windowsOpened'] is False, 'Windows gate/cleanup incomplete')
    need(report['sourceCommit'] == source['sourceCommit'] and report['manifestSha256'] == source['manifestSha256'], 'Report source differs')
    need(type(report['freeBytesBefore']) is int and report['freeBytesBefore'] >= 6 * 1024**3, 'Disk preflight missing')
    need(PureWindowsPath(report['testExecutable']) == PureWindowsPath(runtime) / 'winbridge-worker.exe', 'Fixed EXE path differs')
    sha = hashlib.sha256(files['winbridge-worker.exe']).hexdigest()
    need(sha == report['workerSha256'] and descriptor['role'] == 'worker' and descriptor['targetOs'] == 'windows'
         and descriptor['targetArch'] == 'x86_64', 'Exported worker image differs')
    build = descriptor['build']
    need(build['version'] == '0.2.0' and controller_build['version'] == '0.2.0' and build['sourceCommit'] == source['sourceCommit'] and build['sourceManifestSha256'] == source['manifestSha256']
         and build['sourceDirty'] is False and proof['build'] == build and ready['build'] == build, 'Worker build differs')
    need(proof['source'] == source and proof['controllerBuild'] == controller_build and controller_build['sourceCommit'] == source['sourceCommit']
         and controller_build['sourceDirty'] is False, 'Controller source differs')
    for name in ('source-before.log', 'source-after.log'):
        v = value(name)
        need(v['kind'] == 'source_verified' and v['sourceCommit'] == source['sourceCommit'] and v['manifestSha256'] == source['manifestSha256'], 'Source readback differs')
    commands = report['commands']
    need([c['name'] for c in commands] == ['worker-build', 'worker17-to19']
         and all(type(c['exitCode']) is int and c['exitCode'] == 0 for c in commands), 'Native build/migration incomplete')
    need(commands[0]['arguments'][:9] == ['build', '-p', 'winbridge-worker', '--bins', '--locked', '--offline', '--features', 'bundled-sqlite', '--target-dir'], 'Build feature selection differs')
    need(proof['kind'] == 'multi_controller_native' and proof['passed'] is True and proof['platform'] == 'windows'
         and type(proof['controllers']) is int and proof['controllers'] == 4 and proof['primary'] == 'c3'
         and proof['installedRuntimeChanged'] is False and proof['windowsOpened'] is False, 'Network proof scope differs')
    for key in ('sameRuntime', 'parallelProjects', 'sameProjectSerialized', 'crossControllerDedup', 'disconnectPreservedTask',
                'controllerRestartPreservedCreator', 'crossControllerCancellation', 'sharedOutput'):
        need(proof[key] is True, 'Missing network check: ' + key)
    need(proof['context'] == ready['context'] and proof['generation'] == ready['generation'], 'Ready context differs')
    tasks, parallel, creators, cancels = (proof[k] for k in ('tasks', 'parallel', 'creators', 'cancellations'))
    need(all(len(v) == 4 for v in (tasks, parallel, creators, cancels)), 'Missing controller observations')
    ids = [t['taskId'] for t in tasks]
    need(len(set(ids)) == 4 and [t['taskId'] for t in parallel] == ids and all(t['status'] == 'running' for t in parallel), 'Parallel accepted work missing')
    for i in range(4):
        need(tasks[i]['submission']['projectId'] == f'p{i}' and creators[i]['kind'] == 'controller'
             and creators[i]['controllerId'] == f'c{i}' and creators[i]['workerGeneration'] == proof['generation'], 'Creator differs')
        need(cancels[i]['actor']['kind'] == 'controller' and cancels[i]['actor']['controllerId'] == f'c{(i + 1) % 4}'
             and cancels[i]['actor']['workerGeneration'] == proof['generation'], 'Cancellation actor differs')
    need(len(proof['duplicates']) == 4 and all(t['taskId'] == ids[0] for t in proof['duplicates']), 'Deduplication evidence incomplete')
    need(proof['creatorAfterRestart'] == creators[2] and proof['disconnectedTask']['taskId'] == ids[2]
         and proof['disconnectedTask']['status'] == 'running', 'Disconnect/restart changed accepted work')
    before, lost, after = (proof[k] for k in ('beforeDisconnect', 'lost', 'reconnected'))
    for page in (before, lost, after):
        need(page['context'] == proof['context'] and page['workerGeneration'] == proof['generation'] and page['primary'] == 'c3'
             and page['rpc']['limit'] == 32 and len(page['connections']) == 4
             and {x['id'] for x in page['connections']} == {'c0', 'c1', 'c2', 'c3'}
             and all(x['rpc']['limit'] == 16 for x in page['connections']), 'Topology or budget differs')
    need(all(x['connection']['state'] == 'connected' for p in (before, after) for x in p['connections']), 'Four connections not confirmed')
    need(all((x['connection']['state'] != 'connected') == (x['id'] == 'c2') for x in lost['connections']), 'Disconnect affected another link')
    peer = lambda p: next(x for x in p['connections'] if x['id'] == 'c2')['connection']
    need(peer(before)['controllerGeneration'] != peer(after)['controllerGeneration'], 'Restart identity missing')
    final = {t['taskId']: t for t in proof['finalTasks']}
    need(len(final) == 5 and all(final[t]['status'] == 'cancelled' for t in ids), 'Final task/cancellation state differs')
    queued = proof['queued']
    need(queued['taskId'] not in ids and queued['status'] == 'succeeded' and queued == final[queued['taskId']]
         and queued['submission']['projectId'] == 'p0', 'Serialized queue did not complete')
    text = ''.join(bytes(c['bytes']).decode('utf-8', errors='replace') for c in proof['output']['chunks'])
    need('Привет' in text, 'Cross-controller output missing')
    runs = report['projectRuns']
    need(len(runs) == 4 and {r['project'] for r in runs} == {'p0', 'p1', 'p2', 'p3'}, 'Actual execution counters missing')
    for r in runs:
        expected = 3 if r['project'] == 'p0' else 2
        need(r['expected'] == expected and len(r['lines']) == expected, 'Duplicate or missing actual process execution')
        modes = [line.split(' ')[1] for line in r['lines']]
        need(sorted(modes) == sorted(['tree', 'child'] + (['success'] if expected == 3 else [])), 'Unexpected fixture process modes')
    migration_log = files['worker-migration.log'].decode('utf-8-sig')
    need('actual_seventeen_history_migrates_to_nineteen_without_replay_or_old_worker_write ... ok' in migration_log
         and 'test result: ok. 1 passed; 0 failed;' in migration_log, 'Native migration not selected')
    return {'passed': True, 'workerSha256': sha, 'controllers': 4, 'retainedTasks': 5}


def audit(files, source, controller_build, runtime):
    result = verify(files, source, controller_build, runtime)
    # Mutate observed positive evidence; these are verifier checks, never native runs.
    mutations = [
        ('report.json', lambda v: v.update(remainingFixtureProcesses=1)),
        ('report.json', lambda v: v.update(testExecutable='K:\\random\\winbridge-worker.exe')),
        ('report.json', lambda v: v['projectRuns'][0]['lines'].append('1 tree')),
        ('network-complete.json', lambda v: v['parallel'][0].update(status='queued')),
        ('network-complete.json', lambda v: v['duplicates'][1].update(taskId='new-task')),
        ('network-complete.json', lambda v: v['creators'][0].update(controllerId='c1')),
        ('network-complete.json', lambda v: v['cancellations'][0]['actor'].update(controllerId='c0')),
        ('network-complete.json', lambda v: v.update(controllers=2)),
        ('network-complete.json', lambda v: v.update(creatorAfterRestart=None)),
        ('network-complete.json', lambda v: v['lost']['connections'][0]['connection'].update(state='disconnected')),
        ('network-complete.json', lambda v: v.update(finalTasks=v['finalTasks'][:-1])),
        ('worker-build-identity.json', lambda v: v['build'].update(sourceDirty=True)),
        ('network-ready.json', lambda v: v['context'].update(journalIncarnation='foreign')),
    ]
    for name, change in mutations:
        mutated = dict(files)
        payload = json.loads(files[name].decode('utf-8-sig'))
        change(payload)
        mutated[name] = json.dumps(payload).encode()
        try:
            verify(mutated, source, controller_build, runtime)
        except (ValueError, KeyError, IndexError):
            continue
        raise ValueError('Verifier accepted corrupted evidence: ' + name)
    result['negativeAudits'] = len(mutations)
    return result
