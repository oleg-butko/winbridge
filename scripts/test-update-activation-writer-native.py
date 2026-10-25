#!/usr/bin/env python3
"""Synthetic writer-observer regressions only; never a native execution proof."""
import copy
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
import unittest
import uuid


def load(name, file):
    s = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    m = importlib.util.module_from_spec(s)
    s.loader.exec_module(m)
    return m


gate = load('writer_proof', 'update-activation-writer-proof.py')
prep = load('preparation', 'update-activation-preparation-proof.py')
physical = load('physical', 'update-activation-state-proof.py')


def uid(n):
    return str(uuid.UUID(int=n))


def fixture():
    """The outer verifier separately proves the full actual Main/physical graph."""
    old = dict(incarnation=uid(10), epoch=5, fingerprintSha256='1'*64)
    new = dict(old, epoch=6, fingerprintSha256='2'*64)
    pin = dict(updateId=uid(1), launchId=uid(2), ownerId=uid(5), ownerEpoch=8, backupSha256='3'*64)
    ctx = dict(workerContext=dict(journalIncarnation=uid(10)), startupTimeoutMs=120000,
               workerConfig=dict(sha256='4'*64), supervisor=dict(processId=7))
    owned = dict(process=dict(processId=80), generation=uid(4), state=dict(journalIdentity='fixture-only'))
    record = dict(intent=dict(attemptId=uid(3), admittedAtMs=10000), stateOwned=owned, updatedAtMs=13000)
    sha = lambda domain, value: gate.digest(domain, value, prep)
    binding = dict(updateId=uid(1), launchId=uid(2), updaterOwnerId=uid(5), updaterEpoch=8,
                   activationRevision=6, attemptId=uid(3), physical=owned, expectedAuthority=old,
                   activationRecordSha256=sha(b'winbridge.activation.writer-binding.v1\n', record),
                   backupRecordSha256='3'*64, workerJournalSchema=17, updaterJournalSchema=19, workerConfigSha256='4'*64)
    command = dict(schemaVersion=1, commandId=uid(6), binding=binding, issuedAtMs=14000, deadlineAtMs=130000)
    command_sha = sha(b'winbridge.activation.writer-command.v1\n', command)
    receipt = dict(schemaVersion=1, commandId=uid(6), updateId=uid(1), attemptId=uid(3), commandSha256=command_sha,
                   authority=new, committedAtMs=15000)
    issued = dict(schemaVersion=1, revision=0, command=command, state=dict(phase='issued'), updatedAtMs=14000)
    saved = dict(issued, revision=1, state=dict(phase='confirmed', receipt=receipt), updatedAtMs=16000)
    root = dict(updateId=uid(1), phase='verifying_local')
    physical_view = dict(phase='state_owned', savedOwnership=owned, residentProcess=owned['process'], processAlive=True)
    projection = dict(phase='confirmed', attemptId=uid(3), commandId=uid(6), revision=1, durablePhase='confirmed',
                      savedReceipt=receipt, residentProcess=owned['process'], processAlive=True, reason=None)
    files = dict(phase='bound', heldFiles=5, worker={'sha256':'7'*64}, manifestSha256='8'*64)
    view = dict(activationFiles=files, schemaVersion=16, update=root, driverActive=False, driverError=None, activationWriter=projection,
                activationState=physical_view)
    boundary = dict(input={}, inputEncoded='{}', inputSha256='5'*64)
    build = dict(sourceCommit='fixture-only-not-a-native-build', sourceManifestSha256='9'*64, sourceDirty=False)
    boundary['build'] = build
    marker = dict(schemaVersion=1, kind='activation_writer_before_send', processId=7,
                  **boundary, record=record, recordSha256=sha(b'winbridge.activation.barrier.record.v1\n', record),
                  commandId=uid(6), commandSha256=command_sha, observedAtMs=14500)
    lease = dict(phase='quiesced')
    stop = dict(lease=lease, command='fixture-only')
    rows = dict(sha256='6'*64, tasks=2, output=3)
    p = dict(writerRecord=saved, writerHistory=[issued, saved], actualWorkerReceipt=receipt,
             workerAuthorityBefore=old, workerAuthorityAfter=new, beforeSend=marker, afterRelease=boundary,
             workerLeaseBefore=lease, workerLeaseAfter=lease, acceptedShutdownBefore=stop,
             acceptedShutdownAfter=stop, oldCachedAuthorityRefused=True, preservedRowsBefore=rows,
             preservedRowsAfter=rows, owned=dict(update=root, activationState=physical_view, activationFiles=files),
             confirmed=view, duplicate=view, workerLaunchCount=1, stateHandoffCount=1, savedWorkerCommandCount=1,
             lostWriterAckSentOnce=True, lostHandoffAckSentOnce=True, normalServiceStarted=False,
             publicActivationEnabled=False, childInstanceLockOccupied=True, busyObservations=1)
    for name, version, field in (('legacyState', 15, 'activationState'), ('legacyProcess', 14, 'activationProcess')):
        p[name] = dict(schemaVersion=version, update=root, driverActive=False, driverError=None,
                       **{field: dict(phase='indeterminate', reason='Use internal24', residentProcess=owned['process'], processAlive=True)})
    cold_view = dict(view, activationWriter=dict(projection, phase='not_resident', residentProcess=None, processAlive=None),
                     activationState=dict(physical_view, phase='not_resident', residentProcess=None, processAlive=None),
                     activationFiles=dict(files, heldFiles=0, phase='not_bound', worker=None, manifestSha256=None))
    cold = dict(writerRecord=saved, writerHistory=[issued, saved], actualWorkerReceipt=receipt, workerAuthority=new,
                view=cold_view, preservedRowsAfter=rows, workerLaunchCount=1, stateHandoffCount=1, savedWorkerCommandCount=1,
                automaticColdLaunchCount=0, normalServiceStarted=False, actualRetainedProcessExited=True,
                ownedJobEmpty=True, activationJobEmpty=True, sameInstanceLockAvailable=True, observedState=owned['state'])
    before = dict(finalStateBackup=dict(record=dict(report=dict(sourceAuthority=old))),
                  shutdown=dict(delivery=dict(receipt=stop)))
    # Independent copies model JSON decoding: mutating one observation must not
    # silently mutate other views/history via Python object aliasing.
    return tuple(json.loads(json.dumps(item)) for item in (p, cold, before, pin, ctx, record, build))


class WriterEvidenceTests(unittest.TestCase):
    def check(self, f):
        p, cold, before, pin, ctx, record, build = f
        gate.strict(dict(warm=p, cold=cold, before=before), prep, physical)
        gate.verify_delivery(p, cold, before, pin, ctx, record, build, prep)

    def test_correlated_synthetic_rows_pass_only_the_delivery_component(self):
        self.check(fixture())

    def test_mutations_cannot_claim_receipt_service_or_replay(self):
        edits = [
            ('p/workerAuthorityAfter/epoch', 7),
            ('p/actualWorkerReceipt/commandSha256', '0'*64),
            ('p/writerRecord/command/deadlineAtMs', 250000),
            ('p/writerHistory/0/command/binding/workerJournalSchema', 16),
            ('p/writerHistory/1/command/binding/expectedAuthority/fingerprintSha256', '0'*64),
            ('p/beforeSend/observedAtMs', 13999),
            ('p/beforeSend/commandId', uid(99)),
            ('p/workerLeaseAfter/phase', 'released'),
            ('p/acceptedShutdownAfter/command', 'foreign'),
            ('p/preservedRowsAfter/sha256', '0'*64),
            ('p/oldCachedAuthorityRefused', False),
            ('p/confirmed/activationState/savedOwnership/generation', uid(99)),
            ('p/duplicate/activationFiles/heldFiles', 0),
            ('p/legacyState/activationState/residentProcess/processId', 99),
            ('p/beforeSend/build/sourceCommit', 'foreign'),
            ('p/savedWorkerCommandCount', 2),
            ('p/normalServiceStarted', True),
            ('p/legacyState/activationState/phase', 'state_owned'),
            ('cold/view/activationWriter/processAlive', True),
            ('cold/view/activationWriter/phase', 'confirmed'),
            ('cold/actualRetainedProcessExited', False),
            ('cold/sameInstanceLockAvailable', False),
            ('cold/automaticColdLaunchCount', 1),
            ('cold/view/activationFiles/heldFiles', 5),
            ('cold/writerHistory', []),
        ]
        for path, value in edits:
            with self.subTest(path=path):
                f = fixture()
                nodes = {'p': f[0], 'cold': f[1]}
                keys = path.split('/')
                node = nodes
                for key in keys[:-1]:
                    node = node[int(key)] if isinstance(node, list) else node[key]
                node[int(keys[-1]) if isinstance(node, list) else keys[-1]] = value
                with self.assertRaises((ValueError, KeyError, TypeError)):
                    self.check(f)

    def test_consistent_rewriting_of_next_epoch_is_still_rejected(self):
        f = fixture()
        original = copy.deepcopy(f[0]['workerAuthorityAfter'])
        replacement = dict(original, epoch=8)
        def rewrite(value):
            if value == original:
                return copy.deepcopy(replacement)
            if isinstance(value, dict):
                return {key: rewrite(item) for key, item in value.items()}
            if isinstance(value, list):
                return [rewrite(item) for item in value]
            return value
        f = tuple(rewrite(item) for item in f)
        with self.assertRaises(ValueError):
            self.check(f)

    def test_counter_booleans_unknown_and_private_identity_are_rejected(self):
        for key in ('epoch', 'busyObservations', 'savedWorkerCommandCount', 'tasks', 'output'):
            for value in (True, False, None, -1, '1', 1.0):
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    gate.strict({'nested': {key: value}}, prep, physical)
        for key in ('authorityId', 'authority_id', 'nonce', 'token', 'startupPermit', 'writerOwned', 'serviceReady'):
            with self.subTest(key=key), self.assertRaises(ValueError):
                gate.strict({'cold': [{'nested': {key: False}}]}, prep, physical)

    def test_public_authority_accepts_the_maximum_bounded_receipt_epoch(self):
        gate.authority(dict(incarnation=uid(10), epoch=9007199254740991, fingerprintSha256='1'*64))

    def test_cli_rejects_mixed_writer_modes_before_reading_or_submitting(self):
        script = str(Path(__file__).with_name('validate-update-activation-preparation-native.py'))
        for other in ('--activation-state','--activation-process','--activation-files'):
            result = subprocess.run([sys.executable,script,'--archive','absent.zip','--target-archive','absent-target.zip',
                '--cache-run','absent','--reference-pin','absent.json','--activation-writer',other],capture_output=True,text=True)
            self.assertEqual(result.returncode,2)
            self.assertIn('not allowed with argument',result.stderr)

    def test_unknown_schema_pairs_do_not_extend_legacy_scope(self):
        for worker, updater in ((16, 19), (17, 18), (18, 20), (True, 18)):
            with self.subTest(worker=worker, updater=updater), self.assertRaises(ValueError):
                prep.verify_scope({}, {}, {}, {}, '.', worker_schema=worker, updater_schema=updater)


if __name__ == '__main__':
    unittest.main()
