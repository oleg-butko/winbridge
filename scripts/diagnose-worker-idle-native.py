#!/usr/bin/env python3
"""Read CPU snapshots in short tasks, leaving the worker idle between samples.

Requests are saved before submission; an uncertain submission is never replayed.
No worker is started, stopped or reconfigured. Requires the native controller.
"""
import argparse
import base64
import importlib.util
import json
from pathlib import Path
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('native', ROOT / 'scripts/validate-workflow-native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)

SCRIPT = r"""$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$samples=@(Get-Process -Name winbridge-worker | ForEach-Object {
  $p=$_
  $threads=@($p.Threads | ForEach-Object {
    [ordered]@{id=$_.Id; cpuMs=$_.TotalProcessorTime.TotalMilliseconds;
      userMs=$_.UserProcessorTime.TotalMilliseconds; kernelMs=$_.PrivilegedProcessorTime.TotalMilliseconds}
  })
  $p.Refresh()
  [ordered]@{pid=$p.Id; path=$p.Path; startedUtc=$p.StartTime.ToUniversalTime().ToString('o');
    utc=[DateTime]::UtcNow.ToString('o'); cpuMs=$p.TotalProcessorTime.TotalMilliseconds;
    userMs=$p.UserProcessorTime.TotalMilliseconds; kernelMs=$p.PrivilegedProcessorTime.TotalMilliseconds;
    workingSetBytes=$p.WorkingSet64; handles=$p.HandleCount; threads=$threads;
    freeBytes=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($p.Path)).AvailableFreeSpace}
})
ConvertTo-Json -Depth 6 -Compress -InputObject $samples
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--controller-config', type=Path, required=True)
    parser.add_argument('--samples', type=int, default=3)
    parser.add_argument('--interval', type=float, default=15)
    args = parser.parse_args()
    if not 2 <= args.samples <= 10 or not 5 <= args.interval <= 60:
        parser.error('Use 2..10 samples and a 5..60 second idle interval')
    config = json.loads(args.controller_config.read_text())
    endpoint = Path(config['stateDirectory']) / ('controller-' + config['instanceId'] + '.sock')
    info = native.call(endpoint, {'method': 'info'})['info']
    if info['platform'] != 'windows' or info['activeTaskCount'] or info.get('journalRecoveryRequired'):
        raise ValueError('An idle healthy Windows worker is required')
    run = time.strftime('%Y%m%d-%H%M%S', time.gmtime()) + '-' + uuid.uuid4().hex[:8]
    directory = ROOT / '.winbridge/idle-diagnostics' / run
    directory.mkdir(parents=True)
    (directory / 'worker.json').write_text(json.dumps({k: info.get(k) for k in
        ('build', 'generation', 'activeTaskCount')}, indent=2))
    print('Diagnostics: ' + str(directory), flush=True)
    samples = []
    for index in range(args.samples):
        before = native.call(endpoint, {'method': 'info'})['info']
        if before['activeTaskCount']:
            raise ValueError('Concurrent work detected; idle measurement stopped')
        request = dict(protocolVersion=1, requestId=str(uuid.uuid4()), projectId='snapshot-fixture',
            idempotencyKey='idle-diagnostic-' + run + '-' + str(index), timeoutMs=60000,
            command=dict(kind='direct', executable='pwsh.exe', workingDirectory='.',
                arguments=['-NoProfile', '-NonInteractive', '-EncodedCommand',
                           base64.b64encode(SCRIPT.encode('utf-16le')).decode()]))
        saved = directory / f'{index}-submission.json'
        meta = dict(request=request, taskId=None)
        saved.write_text(json.dumps(meta, indent=2))
        task = native.call(endpoint, dict(method='submit', request=request))['task']
        meta['taskId'] = task['taskId']
        saved.write_text(json.dumps(meta, indent=2))
        deadline = time.monotonic() + 90
        while task['status'] in {'queued', 'running'}:
            if time.monotonic() > deadline:
                raise TimeoutError('Observe saved task; do not resubmit: ' + str(saved))
            time.sleep(.25)
            task = native.call(endpoint, dict(method='get_task', taskId=task['taskId']))['task']
        (directory / f'{index}-terminal.json').write_text(json.dumps(task, indent=2))
        chunks, sequence = [], 0
        while True:
            page = native.call(endpoint, dict(method='read_output', taskId=task['taskId'],
                                             afterSequence=sequence))['chunks']
            if not page:
                break
            chunks.extend(page)
            sequence = page[-1]['sequence']
        output = b''.join(bytes(c['bytes']) for c in chunks if c['stream'] == 'stdout').decode('utf-8-sig')
        (directory / f'{index}-output.json').write_text(output)
        if task['status'] != 'succeeded' or task['exitCode'] != 0:
            raise RuntimeError('Snapshot failed; inspect saved output and task: ' + str(saved))
        snapshot = json.loads(output)
        if len(snapshot) != 1:
            raise ValueError('Exactly one worker required; inspect saved process identities')
        samples.append(snapshot[0])
        print(f'Sample {index + 1}: pid={snapshot[0]["pid"]}, CPU={snapshot[0]["cpuMs"]}ms', flush=True)
        if index + 1 < args.samples:
            time.sleep(args.interval)
    from datetime import datetime
    intervals = []
    for first, last in zip(samples, samples[1:]):
        if (first['pid'], first['startedUtc'], first['path']) != (last['pid'], last['startedUtc'], last['path']):
            raise ValueError('Worker identity changed during measurement')
        elapsed = (datetime.fromisoformat(last['utc']) - datetime.fromisoformat(first['utc'])).total_seconds()
        previous = {t['id']: t for t in first['threads']}
        hot = sorted((dict(id=t['id'], cpuMs=t['cpuMs'] - previous[t['id']]['cpuMs'])
                      for t in last['threads'] if t['id'] in previous), key=lambda t: -t['cpuMs'])[:5]
        intervals.append(dict(elapsedSeconds=elapsed, cpuMs=last['cpuMs'] - first['cpuMs'],
            percentOfOneCore=(last['cpuMs'] - first['cpuMs']) / (elapsed * 10),
            userMs=last['userMs'] - first['userMs'], kernelMs=last['kernelMs'] - first['kernelMs'],
            hottestThreads=hot))
    report = dict(worker=samples[0]['path'], pid=samples[0]['pid'], samples=samples, intervals=intervals,
        caveat='Intervals include two short diagnostic task boundaries; intervening idle waits have no diagnostic task.')
    (directory / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(dict(report=str(directory / 'report.json'), intervals=intervals), indent=2), flush=True)


if __name__ == '__main__':
    main()
