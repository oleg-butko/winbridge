#!/usr/bin/env python3
"""Audit a real saved native proof with deliberate, independently rejected mutations."""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path

spec = importlib.util.spec_from_file_location('health', Path(__file__).with_name('validate-update-health-actor-native.py'))
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)


def set_value(proof, path, value):
    parts = path.split('/')
    at = proof
    for part in parts[:-1]:
        at = at[int(part)] if isinstance(at, list) else at[part]
    key = int(parts[-1]) if isinstance(at, list) else parts[-1]
    at[key] = value


def all_records(proof, change):
    r = proof['cases'][0]['replacement']
    for record in [r['record']] + r['history']:
        change(record)


def audit(proof, source, target):
    health.verify_runtime_replacement_proof(proof, source, target)
    prefix = 'cases/0/replacement/'
    paths = [
        ('kind', 'wrong-proof'), ('schemaVersion', True), ('runtimeApplied', False),
        ('installedRuntimeApplied', True), ('windowsOpened', True), ('build/sourceDirty', True),
        (prefix+'record/revision', 21), (prefix+'record/state/phase', 'committed'),
        (prefix+'record/intent/attemptId', proof['cases'][0]['beforeReplacement']['root']['updateId']),
        (prefix+'record/intent/preparation/files/0/image/sha256', '0'*64),
        (prefix+'actualRetainedFiles/0/fileIdentity', 'different-file'),
        (prefix+'actualInstalledFiles/0/sha256', '0'*64),
        (prefix+'actualInstalledFiles/4/path', 'K:\\other\\manifest.json'),
        (prefix+'actualManifest/setId', '00000000-0000-0000-0000-000000000000'),
        (prefix+'view/runtimeReplacement/physicalCustodyHeld', False),
        (prefix+'view/runtimeReplacement/installedFiles', 4),
        (prefix+'view/update/phase', 'committed'),
        (prefix+'view/update/revision', proof['cases'][0]['replacement']['view']['update']['revision']+1),
        (prefix+'admitted/runtimeReplacement/revision', 1),
        (prefix+'admitted/driverActive', False),
        (prefix+'duplicateActive/runtimeReplacement/attemptId', 'different-attempt'),
        (prefix+'duplicate/runtimeReplacement/revision', 21),
        (prefix+'preservation/sourceFiles/0/sha256', '0'*64),
        (prefix+'preservation/preimages/files/0/sha256', '0'*64),
        (prefix+'preservation/backupFile/sha256', '0'*64),
        (prefix+'workerBytesAfter/0/1', '0'*64),
        (prefix+'wrongVersionRefused', False), (prefix+'intentBeforeAck', False),
        (prefix+'originalInstanceLockHeld', False), (prefix+'oldViewsOmitReplacement', False),
        (prefix+'preparedDirectoryEmpty', False), (prefix+'activationStarted', True),
        ('cases/0/afterOwnedExit/view/runtimeReplacement/physicalCustodyHeld', True),
        ('cases/0/afterOwnedExit/normalStartupRefused', False),
        ('cases/0/afterOwnedExit/workerBytesUnchanged', False),
        ('cases/0/afterOwnedExit/ownedJobEmpty', False),
        ('cases/0/afterOwnedExit/activationStarted', True),
        ('cases/0/beforeReplacement/runtimePreparation/record/plan/proofs/preimagesSha256', '0'*64),
    ]
    changes = [(path, lambda p, path=path, value=value: set_value(p, path, value)) for path, value in paths]
    changes += [
        ('missing-causal-step', lambda p: p['cases'][0]['replacement']['history'].pop(3)),
        ('counter-bool-in-file-issue', lambda p: p['cases'][0]['replacement']['history'][3]['batch']['files'][0].update(revision=True)),
        ('intent-unknown-field-everywhere', lambda p: all_records(p, lambda r: r['intent'].update(permission='granted'))),
        ('intent-schema-bool-everywhere', lambda p: all_records(p, lambda r: r['intent'].update(schemaVersion=True))),
        ('record-schema-bool-everywhere', lambda p: all_records(p, lambda r: r.update(schemaVersion=True))),
        ('batch-schema-bool-everywhere', lambda p: all_records(p, lambda r: r['batch'].update(schemaVersion=True))),
    ]
    rejected = []
    for name, change in changes:
        bad = copy.deepcopy(proof)
        change(bad)
        try:
            health.verify_runtime_replacement_proof(bad, source, target)
        except (ValueError, KeyError, TypeError, IndexError):
            rejected.append(name)
        else:
            raise ValueError('Verifier accepted mutated actual proof: '+name)
    return rejected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--proof', type=Path, required=True)
    parser.add_argument('--pin', type=Path, required=True, help='Saved native source/target pins')
    args = parser.parse_args()
    raw = args.proof.read_bytes()
    pin = json.loads(args.pin.read_text())
    names = audit(json.loads(raw.decode('utf-8-sig')), pin['source'], pin['target'])
    print(json.dumps({'passed': True, 'proofSha256': hashlib.sha256(raw).hexdigest(),
                      'negativeMutationsRejected': len(names), 'mutations': names}))


if __name__ == '__main__':
    main()
