#!/usr/bin/env python3
"""Synthetic observer regressions. These records are not actual migration proof."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
import uuid


def load(name, file):
    s=importlib.util.spec_from_file_location(name,Path(__file__).with_name(file))
    m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m


proof=load('migration_proof','update-activation-writer-migration-proof.py')
checks=load('checks','update-activation-preparation-proof.py')
physical=load('physical','update-activation-state-proof.py')
references=load('references','update-activation-writer-reference.py')


def uid(n):
    return str(uuid.UUID(int=n))


def fixtures():
    source=dict(sourceCommit='a'*40,manifestSha256='b'*64)
    build=dict(sourceCommit=source['sourceCommit'],sourceManifestSha256=source['manifestSha256'],sourceDirty=False)
    image=dict(path='K:/retained/winbridge-worker.exe',fileIdentity='old-worker',sha256='c'*64,bytes=100,querySupported=True)
    fixture=dict(image,path='K:/retained/winbridge-fixture.exe',fileIdentity='old-fixture',sha256='d'*64)
    updater_image=dict(image,path='K:/retained/winbridge-update.exe',fileIdentity='old-updater',sha256='e'*64)
    root='K:/isolated-run'
    worker_image=dict(image,path=root+'/worker-journal-migration-case/old16-bin/winbridge-worker.exe',fileIdentity='new-worker')
    fixture_image=dict(fixture,path=root+'/worker-journal-migration-case/old16-bin/winbridge-fixture.exe',fileIdentity='new-fixture')
    authority=dict(incarnation=uid(8),epoch=4,fingerprintSha256='f'*64)
    accepted=dict(updateId=uid(1),lease=dict(phase='quiesced'))
    activation=dict(revision=6,state=dict(phase='state_owned'),intent=dict(source=dict(updateId=uid(1),ownerEpoch=4)))
    saved_root=dict(updateId=uid(1),phase='verifying_local')
    sha=lambda value:hashlib.sha256(checks.canonical(value)).hexdigest()
    common=dict(referenceRun='20261025-100933-d8a50807',proofSha256='1'*64,reportSha256='2'*64)
    r=dict(common,image=updater_image,manifestSha256='3'*64,updateId=uid(1),activationSha256=sha(activation),rootSha256=sha(saved_root))
    rw=dict(common,image=image,fixtureImage=fixture,descriptor=dict(role='worker',build=build),authority=authority,acceptedShutdown=accepted)
    reference=dict(updater=r,worker=rw)
    before=dict(schemaVersion=18,ownerEpoch=4,installationId=uid(7),scopeSha256='4'*64,activeUpdateId=uid(1),lastSequence=12)
    updater=dict(schemaVersion=1,kind='activation_writer_schema_migration_proof',passed=True,platform='windows',build=build,
                 runtimeApplied=False,installedRuntimeApplied=False,windowsOpened=False,
                 referenceProofSha256=r['proofSha256'],referenceReportSha256=r['reportSha256'],referenceImage=updater_image,
                 referenceManifestSha256=r['manifestSha256'],before=before,after=dict(before,schemaVersion=19,ownerEpoch=5),backupInfo=before,
                 retainedActivation=activation,retainedRoot=saved_root,gateRows=0,rowsBefore='5'*64,rowsAfter='5'*64,backupRows='5'*64)
    for name in ('oldEighteenInitRefused','oldEighteenStatusRefused','sourceBytesUnchanged','copyBytesUnchangedByOld','noFreshReplacementPermit','existingReplacementObserved','migrationBackupVerified'):
        updater[name]=True
    task=dict(taskId=uid(9),status='succeeded',exitCode=0,submission=dict(projectId='source',idempotencyKey='old16-history',
              command=dict(kind='direct',arguments=['success'],executable=fixture_image['path'])))
    worker=dict(schemaVersion=1,kind='worker_journal_activation_migration_proof',passed=True,platform='windows',build=build,
                runtimeApplied=False,installedRuntimeApplied=False,windowsOpened=False,**common)
    worker.pop('proofSha256');worker.pop('reportSha256')
    worker.update(referenceProofSha256=rw['proofSha256'],referenceReportSha256=rw['reportSha256'],referenceImage=image,referenceDescriptor=rw['descriptor'],
                  fromSchema=16,toSchema=17,authorityBefore=authority,authorityAfter=authority,rowsBefore='6'*64,rowsAfter='6'*64,backupRows='6'*64,
                  activeAuthority=authority,activeAcceptedShutdown=accepted,activeRowsBefore='7'*64,activeRowsAfter='7'*64,activeSchemaAfter=16,
                  activeRefusal='Unfinished worker update blocks migration',copiedOldImage=worker_image,copiedFixtureImage=fixture_image,
                  task=task,duplicateTaskId=task['taskId'],oldProcess=dict(processId=80,creationTime=134000000000000000,jobName='Global\\WinBridge-worker-migration-fixture'),
                  outputChunks=[dict(sequence=1,stream='stdout',offset=0,bytes=list('success Привет ✓\n'.encode())),dict(sequence=2,stream='stderr',offset=0,bytes=list(b'diagnostic\n'))])
    for name in ('oldSixteenRefusedSeventeen','activeSixteenRefusedBeforeWrites','migrationBackupVerified','oldWorkerJobEmpty','sourceBytesUnchanged','noActivationInvented'):
        worker[name]=True
    return tuple(json.loads(json.dumps(value)) for value in (updater,worker,source,reference,root))


class MigrationEvidenceTests(unittest.TestCase):
    def test_correlated_components_and_negative_audit(self):
        a,b,source,ref,root=fixtures()
        self.assertEqual(proof.audit(a,b,source,ref,checks,physical,root),18)

    def test_boolean_exit_code_and_typed_output_vectors_are_not_accepted(self):
        for change in ('exitCode','bytes','offset','sequence','unknown'):
            a,b,source,ref,root=fixtures()
            if change=='exitCode': b['task']['exitCode']=False
            elif change=='unknown': b['outputChunks'][0]['token']=False
            else: b['outputChunks'][0][change]=[True] if change=='bytes' else True
            with self.subTest(change=change),self.assertRaises((ValueError,TypeError)):
                proof.verify_worker(b,source,ref,checks,physical,root)

    def test_reference_builder_rejects_old_schema_relabeling_before_derivation(self):
        pin=dict(target={})
        for worker,updater in ((17,18),(16,19)):
            fake=dict(cases=[dict(activationState=dict(record=dict(intent=dict(context=dict(workerJournalSchema=worker,updaterJournalSchema=updater)),revision=6,state=dict(phase='state_owned'))),
                                  replacement=dict(actualManifest=dict(artifacts=[]),actualInstalledFiles=[]),beforeReplacement={})])
            with self.subTest(worker=worker,updater=updater),self.assertRaises(ValueError):
                references.derive(pin,fake,checks)

    def test_coordinated_authority_rewrite_cannot_change_the_verified_old_reference(self):
        a,b,source,ref,root=fixtures()
        b['authorityBefore']['epoch']=5;b['authorityAfter']['epoch']=5
        b['activeAuthority']['epoch']=5
        with self.assertRaises(ValueError):
            proof.verify_worker(b,source,ref,checks,physical,root)


if __name__=='__main__':
    unittest.main()
