"""Independent checks for expected failures in the owned Main15 runtime."""
import hashlib
import json
from pathlib import PureWindowsPath
import uuid


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def verify(proof, source, target, before_verifier, preimages_bytes, batch_at):
    expected_faults = ['existing_directory', 'locked_old_file', 'receipt_write_denied', 'hold_before_receipt']
    need(type(proof.get('schemaVersion')) is int and proof['schemaVersion'] == 1
         and proof.get('kind') == 'runtime_replacement_faults_native_proof'
         and proof.get('passed') is True and proof.get('platform') == 'windows'
         and 'runtimeApplied' in proof and proof['runtimeApplied'] is None and proof.get('installedRuntimeApplied') is False
         and proof.get('windowsOpened') is False and len(proof.get('cases', [])) == 4,
         'Replacement fault proof scope differs')
    ids = set()
    for case, fault in zip(proof['cases'], expected_faults):
        before, p, after = (case[k] for k in ('beforeReplacement', 'replacement', 'afterOwnedExit'))
        before_verifier(dict(proof, kind='runtime_preparation_main_native_proof', runtimeApplied=False,
                             cases=[before]), source, target)
        preparation = before['runtimePreparation']['record']
        plan = preparation['plan']
        r, view = p['record'], p['view']
        intent = r['intent']
        held = fault == 'hold_before_receipt'
        retired = fault in ('receipt_write_denied', 'hold_before_receipt')
        prefix = 3 if retired else 0
        revision = prefix if held else prefix+1
        root = PureWindowsPath(plan['runtimeDirectory'])
        directory = root/'update'/plan['updateId']/'displaced-runtime'
        need(p.get('expectedFault') == fault and plan['updateId'] not in ids,
             'Missing/repeated ordered fault case')
        ids.add(plan['updateId'])
        need(set(intent) == {'schemaVersion', 'attemptId', 'admittedAtMs', 'preparation'}
             and type(intent['schemaVersion']) is int and intent['schemaVersion'] == 1
             and intent['preparation'] == preparation
             and str(uuid.UUID(intent['attemptId'])) == intent['attemptId']
             and intent['attemptId'] not in [plan[k] for k in ('updateId', 'attemptId', 'ownerId', 'launchId')]
             and intent['attemptId'] != plan['manifest']['setId']
             and type(intent['admittedAtMs']) is int and plan['admittedAtMs'] <= intent['admittedAtMs'] <= 9007199254740991,
             'Fault intent is not bound to original Prepared scope')
        state = r['state'] if held else r['state']['previous']
        directory_id = state.get('directoryIdentity') if retired else None
        need(state == ({'phase': 'replacing', 'directoryIdentity': directory_id} if retired else {'phase': 'planned'})
             and (not retired or isinstance(directory_id, str) and directory_id)
             and r['revision'] == revision and len(p['history']) == revision+1
             and p['history'][-1] == r, 'Fault did not preserve exact interrupted prefix')
        reason = None if held else r['state']['reason']
        if not held:
            expected_reason = {'existing_directory': 'Displaced runtime directory already exists',
                               'locked_old_file': 'os error 32',
                               'receipt_write_denied': 'Required update write was not applied'}[fault]
            need(r['state']['phase'] == 'indeterminate' and isinstance(reason, str)
                 and expected_reason in reason and len(reason) <= 900,
                 'Fault reason/outcome differs from expected injected cause')
        for number, entry in enumerate(p['history']):
            regular_state = ({'phase': 'planned'} if number == 0 else {'phase': 'directory_issued'}
                             if number == 1 else {'phase': 'replacing', 'directoryIdentity': directory_id})
            expected_state = ({'phase': 'indeterminate', 'previous': state, 'reason': reason}
                              if not held and number == revision else regular_state)
            batch = batch_at(preparation, min(number, prefix))
            need(type(entry.get('schemaVersion')) is int and type(entry.get('revision')) is int
                 and type(entry['batch'].get('schemaVersion')) is int
                 and all(type(f.get('schemaVersion')) is int and type(f.get('revision')) is int for f in entry['batch']['files'])
                 and entry == dict(schemaVersion=1, intent=intent, revision=number,
                                   state=expected_state, batch=batch),
                 'Fault history skipped/invented receipt or changed immutable data')
        expected_view = dict(phase='moving_files' if held else 'indeterminate', attemptId=intent['attemptId'],
            revision=revision, directory=str(directory), retiredFiles=0, installedFiles=0, totalFiles=5,
            issuedFile='winbridge-worker.exe' if retired else None, issuedMove='retire' if retired else None,
            physicalCustodyHeld=True, reason=reason)
        need(view['schemaVersion'] == 9 and view['update'] == before['root']
             and view['update']['phase'] == 'replacing' and view['driverActive'] is held
             and (view['driverError'] is None if held else isinstance(view['driverError'], str) and view['driverError'])
             and view['runtimeReplacement'] == expected_view
             and p['duplicate']['runtimeReplacement'] == expected_view
             and p['duplicate']['update'] == view['update']
             and p['duplicate']['driverActive'] is held,
             'Fault view/duplicate changed scope, custody, causal prefix or root')
        initial = p['beforeAdmission']
        need(initial['schemaVersion'] == 9 and initial['update'] == before['root']
             and initial['driverActive'] is False and initial['driverError'] is None
             and initial['candidateShutdown']['physicalCustodyHeld'] is True
             and initial['runtimeReplacement'] == dict(expected_view, phase='not_admitted', attemptId=None,
                  revision=None, directory=None, issuedFile=None, issuedMove=None,
                  physicalCustodyHeld=False, reason=None), 'Fault before-admission view invented permission')
        admitted = p['admitted']
        need(admitted['schemaVersion'] == 9 and admitted['driverActive'] is True
             and admitted['driverError'] is None and admitted['update'] == before['root']
             and admitted['runtimeReplacement'] == dict(expected_view, phase='acquiring', revision=0,
                issuedFile=None, issuedMove=None, physicalCustodyHeld=False, reason=None),
             'Fault was not admitted durably before effects')
        old = []
        for i, spec in enumerate(plan['files']):
            image = dict(spec['old'])
            if retired and i == 0:
                image['path'] = str(directory/'winbridge-worker.exe')
            old.append(image)
        need(p['actualOldFiles'] == old and p['actualPreparedFiles'] == before['runtimePreparation']['actualFiles'],
             'Fault actual old/prepared images were lost, replayed or overwritten')
        preservation = p['preservation']
        need(preservation['sourceFiles'] == before['runtimePreparation']['sourceFiles']
             and hashlib.sha256(preimages_bytes(preservation['preimages'])).hexdigest() == plan['proofs']['preimagesSha256']
             and preservation['backupFile'] == before['finalStateBackup']['record']['report']['backup']
             and p['workerBytesBefore'] == p['workerBytesAfter'],
             'Fault mutated source/preimages/final backup/worker DB')
        need(all(p.get(k) is True for k in ('duplicateNoNewAttempt', 'originalInstanceLockHeld', 'preparedDirectoryComplete'))
             and p.get('causeRemovedBeforeDuplicate') is (fault in ('locked_old_file', 'receipt_write_denied'))
             and p.get('activationStarted') is False and p.get('installedRuntimeApplied') is False,
             'Fault did not retain exclusion or retried after cause removal')
        if fault == 'existing_directory':
            sentinel = p['sentinel']
            need(PureWindowsPath(sentinel['path']) == directory/'preserve-sentinel.txt'
                 and sentinel['bytes'] == len(b'preserve unknown displaced directory')
                 and sentinel['sha256'] == hashlib.sha256(b'preserve unknown displaced directory').hexdigest()
                 and sentinel['querySupported'] is False and sentinel['fileIdentity'],
                 'Existing displaced directory sentinel was changed/adopted')
        else:
            need(p['sentinel'] is None, 'Unexpected sentinel in another fault case')
        if held:
            raw, marker = p['validationInputJson'], p['heldMarker']
            control = dict(schemaVersion=1, kind='winbridge_runtime_replacement_validation',
                updateId=plan['updateId'], launchId=plan['launchId'], ownerId=plan['ownerId'], ownerEpoch=plan['ownerEpoch'],
                configSha256=plan['configSha256'], runtimeIdentity=plan['runtimeIdentity'], preparationAttemptId=plan['attemptId'])
            need(isinstance(raw, str) and len(raw.encode()) <= 8192 and json.loads(raw) == control
                 and marker.get('kind') == 'runtime_replacement_before_receipt' and marker['schemaVersion'] == 1
                 and marker['record'] == r and marker['inputSha256'] == hashlib.sha256(raw.encode()).hexdigest()
                 and marker['processId'] == before['beforeMainExit']['supervisor']['process']['processId']
                 and marker['build'].get('sourceCommit') == source['sourceCommit']
                 and marker['build'].get('sourceManifestSha256') == source['manifestSha256']
                 and marker['build'].get('sourceDirty') is False
                 and marker['afterEffect'] == dict(schemaVersion=1, intent=intent, revision=4,
                     state={'phase': 'replacing', 'directoryIdentity': directory_id}, batch=batch_at(preparation, 4)),
                 'Held native move receipt was invented, committed or rebound')
        else:
            need(p['heldMarker'] is None and p['validationInputJson'] is None, 'Unexpected crash barrier')
        cold = dict(expected_view, phase='indeterminate', physicalCustodyHeld=False,
                    reason=('Сохранена незавершённая замена; активного исполнителя нет. Автоматического повтора нет.' if held else reason))
        need(after['record'] == r and after['view']['schemaVersion'] == 9
             and after['sentinel'] == p['sentinel']
             and after['view']['update'] == view['update'] and after['view']['runtimeReplacement'] == cold
             and after['view']['driverActive'] is False and after['view']['driverError'] is None
             and all(after.get(k) is True for k in ('normalStartupRefused', 'workerBytesUnchanged', 'ownedJobEmpty'))
             and after.get('activationStarted') is False, 'Fault cold observation granted replay or kernel custody')
