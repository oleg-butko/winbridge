#!/usr/bin/env python3
"""Summarize verified terminal native logs; never submit, build or replay work."""
import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import re


def verified_files(directory):
    marker = directory/'report.ready.json'
    raw = marker.read_bytes()
    ready = json.loads(raw.decode('utf-8-sig'))
    if type(ready.get('schemaVersion')) is not int or ready['schemaVersion'] != 1:
        raise ValueError('Unsupported ready schema')
    files = {}
    for item in ready['files']:
        name = item['path']
        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name) or name in files or (directory/name).is_symlink():
            raise ValueError('Unsafe or duplicate report filename')
        data = (directory/name).read_bytes()
        if type(item['bytes']) is not int or len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
            raise ValueError('Report bytes changed: '+name)
        files[name] = data
    if marker.read_bytes() != raw or any((directory/name).read_bytes() != data for name, data in files.items()):
        raise ValueError('Ready/report bytes changed on second read')
    return files


def summarize(directory):
    files = verified_files(directory)
    report = json.loads(files['report.json'].decode('utf-8-sig'))
    commands = []
    for command in report['commands']:
        log = files[command['log']].decode('utf-8-sig', errors='replace')
        builds = re.findall(r'Finished [^\r\n]*? in (?:(\d+)m )?([0-9.]+)s', log)
        bodies = re.findall(r'test result: [^\r\n]*?finished in ([0-9.]+)s', log)
        commands.append(dict(name=command['name'], exitCode=command['exitCode'],
                             elapsedMs=command.get('elapsedMs'),
                             cargoReportedBuildMs=round(sum(60*int(m or 0)+float(s) for m, s in builds)*1000),
                             reportedTestBodyMs=round(sum(map(float, bodies))*1000)))
    edges = defaultdict(lambda: dict(count=0, totalStepMs=0, maxStepMs=0, artifactHashBytes=None, artifactHashMs=None))
    spans = []
    for name, data in files.items():
        # The supervisor exports receiver timing once. Other case output files
        # can contain the same captured diagnostics; don't add those again.
        if name != 'shutdown-main-pipeline.log' and not name.endswith('-supervisor-stderr.log'):
            continue
        for line in data.decode('utf-8-sig', errors='replace').splitlines():
            if not line.startswith('WinBridge activation timing '):
                continue
            value = json.loads(line[len('WinBridge activation timing '):])
            key = (name, value.get('processId'), value['operation'], value['previousPhase'], value['phase'])
            if any(type(value[k]) is not int or value[k] < 0 for k in ('stepMs', 'elapsedMs')):
                raise ValueError('Invalid timing counter')
            edge = edges[key]
            edge['count'] += 1
            edge['totalStepMs'] += value['stepMs']
            edge['maxStepMs'] = max(edge['maxStepMs'], value['stepMs'])
            for output, field in [('artifactHashBytes', 'processArtifactHashBytesDelta'),
                                  ('artifactHashMs', 'processArtifactHashMsDelta')]:
                if field in value:
                    if type(value[field]) is not int or value[field] < 0:
                        raise ValueError('Invalid artifact timing counter')
                    edge[output] = (edge[output] or 0) + value[field]
            if value['phase'] == 'trace_end':
                spans.append(dict(log=name, processId=value.get('processId'), operation=value['operation'],
                                  elapsedMs=value['elapsedMs'], remainingMs=value['remainingMs']))
    ranked = [dict(log=k[0], processId=k[1], operation=k[2], previousPhase=k[3], phase=k[4], **v)
              for k, v in sorted(edges.items(), key=lambda item: item[1]['totalStepMs'], reverse=True)]
    nested_builds = []
    proof_bytes = files.get('candidate-health-actor-proof.json')
    if proof_bytes:
        proof = json.loads(proof_bytes.decode('utf-8-sig'))
        for case in proof.get('cases', []):
            build = case.get('beforeReplacement', {}).get('beforeMainExit', {}).get('duplicate', {}).get('build')
            if build:
                accepted, completed = build['spec']['acceptedAtMs'], build['updatedAtMs']
                if any(type(v) is not int or v < 0 for v in (accepted, completed)) or completed < accepted:
                    raise ValueError('Invalid durable compiler interval')
                nested_builds.append(dict(attemptId=build['spec']['attemptId'], phase=build['phase'],
                                          outcome=build['outcome'], acceptedToLastUpdateMs=completed-accepted))
    return dict(run=directory.name, passed=report['passed'], filesVerifiedTwice=len(files),
                reportSha256=hashlib.sha256(files['report.json']).hexdigest(), commands=commands,
                completedSpans=spans, slowestEdges=ranked[:25], nestedCompilerIntervals=nested_builds,
                interpretation='Command elapsed is wall time; reported build/test times and nested spans overlap. '
                               'Do not add different timing operations or build and test-body totals as disjoint time. '
                               'Artifact counters cover artifact::pin in that process, not every platform hash. '
                               'Nested compiler intervals are durable wall-clock admission-to-completion observations, '
                               'including coordination/cleanup; they are inside the Main test body.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report_directory', type=Path)
    args = parser.parse_args()
    print(json.dumps(summarize(args.report_directory), ensure_ascii=False, indent=2))
