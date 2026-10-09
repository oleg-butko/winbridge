#!/usr/bin/env python3
"""Alternate old/new publishers on identical commits; verify exact existing ready.

Debian only: no Windows command and no production runtime update. Existing shared
packages are verified, never overwritten. Both versions recapture ZIP before reuse;
only capturePackMs is compared, not reused publication to fresh publication.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('performance', ROOT / 'scripts/measure-performance.py')
performance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(performance)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-report', type=Path, required=True)
    parser.add_argument('--reference-controller', type=Path, required=True)
    parser.add_argument('--controller', type=Path, default=ROOT / 'target/debug/winbridge-controller')
    parser.add_argument('--repeats', type=int, default=3)
    args = parser.parse_args()
    if not 1 <= args.repeats <= 10:
        parser.error('repeats must be 1–10')
    baseline = json.loads(args.baseline_report.read_text())
    if baseline['passed'] is not True:
        raise RuntimeError('A successful original baseline is required')
    local_source = args.baseline_report.resolve().parent
    run_id = time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()) + '-' + uuid.uuid4().hex[:8]
    local = ROOT / '.winbridge/performance' / run_id
    local.mkdir(parents=True)
    rows = [c for c in baseline['cases'] if c['sample'].startswith('many-files-')]
    if not rows:
        raise RuntimeError('Baseline has no many-files samples')
    # The generated configs belong to the original benchmark. Read, never edit.
    config = json.loads((local_source / (rows[0]['sample'] + '.controller.local.json')).read_text())
    shared = Path(config['delivery']['root']) / config['delivery']['namespace'] / 'performance' / run_id
    shared.mkdir()
    report = {'schemaVersion': 1, 'runId': run_id, 'baselineRunId': baseline['runId'], 'passed': False,
              'error': None, 'samples': [], 'repeats': args.repeats,
              'limitations': ['Same commits and packages; OS caches are not flushed.',
                              'Alternating order limits systematic warm-cache bias; only capture+pack is compared.',
                              'Reused shared packages are verified; no fresh transfer or Windows execution in this comparison.']}
    try:
        report['builds'] = {name: json.loads(subprocess.check_output([str(binary.resolve()), '--describe']))
                            for name, binary in [('before', args.reference_controller), ('after', args.controller)]}
        for repeat in range(args.repeats):
            for row in rows:
                sample = {'sample': row['sample'], 'commit': row['commit'], 'repeat': repeat + 1}
                report['samples'].append(sample)
                order = [('before', args.reference_controller), ('after', args.controller)]
                if repeat % 2:
                    order.reverse()
                for name, binary in order:
                    result = json.loads(subprocess.check_output([str(binary.resolve()), '--config',
                        str(local_source / (row['sample'] + '.controller.local.json')), 'measure-publish',
                        config['projects'][0]['id'], row['commit']]))
                    sample[name] = result
                    if result['reusedPackage'] is not True or result['ready'] != row['publication']['ready']:
                        raise RuntimeError(f'ZIP/manifest/ready differs on the same committed source: {name} {row["sample"]}')
                before = sample['before']['timings']['capturePackMs']
                after = sample['after']['timings']['capturePackMs']
                sample['speedup'] = before / after
                print(f'{row["sample"]} pair {repeat + 1}: {before:.1f} → {after:.1f} ms; ready identical', flush=True)
        report['captureSummary'] = {name: performance.stats([s[name]['timings']['capturePackMs'] for s in report['samples']])
                                     for name in ('before', 'after')}
        before = report['captureSummary']['before']['medianMs']
        after = report['captureSummary']['after']['medianMs']
        report['medianSpeedup'] = before / after
        report['medianReductionPercent'] = 100 * (1 - after / before)
        if after >= before:
            raise RuntimeError('No measured capture improvement; do not report this optimization as successful')
        report['passed'] = True
    except BaseException as exc:
        report['error'] = f'{type(exc).__name__}: {exc}'
        raise
    finally:
        metadata = performance.write_new(local / 'capture-comparison.json', report)
        if performance.write_new(shared / 'capture-comparison.json', report) != metadata:
            raise RuntimeError('Local/shared comparison report differs')
        performance.write_new(shared / 'ready.json', {'schemaVersion': 1, 'runId': run_id,
            'passed': report['passed'], 'report': metadata})
        print(f'Report: {shared}/capture-comparison.json; passed={report["passed"]}; SHA-256={metadata["sha256"]}', flush=True)


if __name__ == '__main__':
    main()
