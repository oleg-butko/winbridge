"""Independent verifier for the isolated Main15 replacement proof, not permission."""
from pathlib import PureWindowsPath
import hashlib
import uuid


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def expected_batch(preparation, revision):
    """Derive the only legal causal prefix from immutable before-images."""
    plan = preparation['plan']
    root = PureWindowsPath(plan['runtimeDirectory'])
    displaced = root/'update'/plan['updateId']/'displaced-runtime'
    files = []
    for i, (spec, prepared) in enumerate(zip(plan['files'], preparation['files'])):
        name = PureWindowsPath(spec['old']['path']).name
        step = max(0, min(4, revision-2-i*4))
        retained = dict(spec['old'], path=str(displaced/name))
        installed = dict(prepared['image'], path=str(root/name))
        state = [{'phase': 'prepared'}, {'phase': 'retire_issued'},
                 {'phase': 'retired', 'retained': retained},
                 {'phase': 'place_issued', 'retained': retained},
                 {'phase': 'installed', 'retained': retained, 'installed': installed}][step]
        files.append({'schemaVersion': 1, 'revision': step, 'state': state,
                      'spec': {'updateId': plan['updateId'], 'runtimeDirectory': plan['runtimeDirectory'],
                               'file': spec['file'], 'old': spec['old'], 'prepared': prepared['image']}})
    return {'schemaVersion': 1, 'files': files}


def verify(proof, source, target, verify_preparation, preimages_bytes):
    require(proof.get('schemaVersion') == 1 and proof.get('kind') == 'runtime_replacement_main_native_proof'
            and proof.get('passed') is True and proof.get('platform') == 'windows'
            and proof.get('runtimeApplied') is True and proof.get('installedRuntimeApplied') is False
            and proof.get('windowsOpened') is False and len(proof.get('cases', [])) == 1,
            'Replacement native proof scope differs')
    case, = proof['cases']
    before, replacement, after = (case[k] for k in ('beforeReplacement', 'replacement', 'afterOwnedExit'))
    # This component describes the BEFORE stage. Do not relabel the final runtime
    # as unchanged: the top-level proof above explicitly requires actual effects.
    verify_preparation(dict(proof, kind='runtime_preparation_main_native_proof',
                            runtimeApplied=False, cases=[before]), source, target)
    preparation = before['runtimePreparation']['record']
    r, done = replacement['record'], replacement['view']
    intent, state = r['intent'], r['state']
    plan = preparation['plan']
    root = PureWindowsPath(plan['runtimeDirectory'])
    directory = root/'update'/plan['updateId']/'displaced-runtime'
    attempt = intent['attemptId']
    require(str(uuid.UUID(attempt)) == attempt and attempt not in [plan[k] for k in
            ('updateId', 'attemptId', 'ownerId', 'launchId')] + [plan['manifest']['setId']]
            and intent.get('schemaVersion') == 1 and intent['preparation'] == preparation
            and type(intent['admittedAtMs']) is int
            and plan['admittedAtMs'] <= intent['admittedAtMs'] <= 9007199254740991,
            'Replacement intent differs from original Prepared graph')
    require(state.get('phase') == 'files_moved' and isinstance(state.get('directoryIdentity'), str)
            and state['directoryIdentity'] and r['revision'] == 22,
            'Replacement final receipt incomplete')
    directory_id = state['directoryIdentity']
    history = replacement['history']
    require(len(history) == 23 and history[-1] == r, 'Missing exact replacement causal history')
    for revision, entry in enumerate(history):
        expected_state = ({'phase': 'planned'} if revision == 0 else
                          {'phase': 'directory_issued'} if revision == 1 else
                          {'phase': 'files_moved' if revision == 22 else 'replacing',
                           'directoryIdentity': directory_id})
        require(entry == {'schemaVersion': 1, 'intent': intent, 'revision': revision,
                          'state': expected_state, 'batch': expected_batch(preparation, revision)},
                'Replacement receipt changed/skipped/reordered causal prefix')
    files = r['batch']['files']
    old, new = replacement['actualRetainedFiles'], replacement['actualInstalledFiles']
    require(len(old) == len(new) == 5 and old == [f['state']['retained'] for f in files]
            and new == [f['state']['installed'] for f in files]
            and directory_id not in [i['fileIdentity'] for i in old+new]
            and directory_id != preparation['directoryIdentity'],
            'Actual moved File IDs/paths/hash/bytes differ from receipts')
    require(replacement['actualManifest'] == plan['manifest'], 'Installed manifest differs')
    initial, admitted = replacement['beforeAdmission'], replacement['admitted']
    require(initial['schemaVersion'] == 9 and initial['update'] == before['root']
            and initial['driverActive'] is False and initial['driverError'] is None
            and initial['candidateShutdown']['physicalCustodyHeld'] is True
            and initial['runtimeReplacement'] == dict(phase='not_admitted', attemptId=None,
                revision=None, directory=None, retiredFiles=0, installedFiles=0, totalFiles=5,
                issuedFile=None, issuedMove=None, physicalCustodyHeld=False, reason=None),
            'Replacement initial view invented custody or admission')
    require(admitted['schemaVersion'] == 9 and admitted['update'] == initial['update']
            and admitted['driverActive'] is True and admitted['driverError'] is None
            and admitted['runtimeReplacement'] == dict(phase='acquiring', attemptId=attempt,
                revision=0, directory=str(directory), retiredFiles=0, installedFiles=0, totalFiles=5,
                issuedFile=None, issuedMove=None, physicalCustodyHeld=False, reason=None),
            'Replacement admission/ACK scope differs')
    expected_view = dict(phase='files_moved', attemptId=attempt, revision=22,
        directory=str(directory), retiredFiles=5, installedFiles=5, totalFiles=5,
        issuedFile=None, issuedMove=None, physicalCustodyHeld=True, reason=None)
    expected_root = dict(before['root'], phase='verifying_local', revision=before['root']['revision']+1,
                         updatedAtMs=done['update']['updatedAtMs'],
                         reason='Runtime replacement: exact five moved files verified; activation pending')
    require(done['schemaVersion'] == 9 and done['update'] == expected_root
            and done['update']['updatedAtMs'] >= before['root']['updatedAtMs']
            and done['driverActive'] is False and done['driverError'] is None
            and done['runtimeReplacement'] == expected_view
            and done['candidateShutdown']['physicalCustodyHeld'] is False
            and replacement['duplicate']['runtimeReplacement'] == expected_view
            and replacement['duplicate']['update'] == done['update']
            and replacement['duplicate']['driverActive'] is False
            and replacement['duplicate']['driverError'] is None,
            'Replacement final/duplicate/root view claims activation or changes scope')
    duplicate = replacement['duplicateActive']['runtimeReplacement']
    require(duplicate['attemptId'] == attempt and type(duplicate['revision']) is int
            and 0 <= duplicate['revision'] <= 22, 'Duplicate created a new attempt')
    revisions = replacement['observedRevisions']
    require(revisions and revisions == sorted(set(revisions)) and revisions[-1] == 22
            and all(type(n) is int and 0 <= n <= 22 for n in revisions),
            'Observed progress regressed or changed causal scope')
    preservation = replacement['preservation']
    require(preservation['sourceFiles'] == before['runtimePreparation']['sourceFiles']
            and hashlib.sha256(preimages_bytes(preservation['preimages'])).hexdigest() == plan['proofs']['preimagesSha256']
            and preservation['backupFile'] == before['finalStateBackup']['record']['report']['backup']
            and replacement['workerBytesBefore'] == replacement['workerBytesAfter'],
            'Source/preimage/final backup/worker DB bytes changed')
    bytes_before = replacement['workerBytesBefore']
    require(len(bytes_before) == 2 and [a[0] for a in bytes_before] == ['journal.sqlite', 'journal.sqlite-wal']
            and all(a[1] is None or (isinstance(a[1], str) and len(a[1]) == 64) for a in bytes_before)
            and bytes_before[0][1] is not None, 'Worker DB/WAL fingerprint missing')
    require(all(replacement.get(k) is True for k in ('wrongVersionRefused', 'intentBeforeAck',
            'originalInstanceLockHeld', 'oldViewsOmitReplacement', 'preparedDirectoryEmpty',
            'sourcePayloadUnchanged', 'preimagesUnchanged', 'finalBackupUnchanged'))
            and replacement.get('activationStarted') is False
            and replacement.get('installedRuntimeApplied') is False,
            'Replacement effect/ownership/isolation evidence missing')
    require(after['record'] == r and after['view']['schemaVersion'] == 9
            and after['view']['runtimeReplacement'] == dict(expected_view, physicalCustodyHeld=False)
            and after['view']['update'] == done['update']
            and after['view']['driverActive'] is False and after['view']['driverError'] is None
            and all(after.get(k) is True for k in ('normalStartupRefused', 'workerBytesUnchanged', 'ownedJobEmpty'))
            and after.get('activationStarted') is False,
            'Cold history adopted custody/replayed effects or started ordinary worker')
