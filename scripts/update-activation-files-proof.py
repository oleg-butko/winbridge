"""Live read binding21 proof. A cold readable manifest is never custody or startup."""
import copy


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def verify(proof, source, target, root, preparation, health):
    need(proof['kind'] == 'activation_files_main_native_proof', 'Wrong file binding proof kind')
    preparation.verify(dict(proof,kind='activation_preparation_main_native_proof'),source,target,root,health)
    case,=proof['cases']
    f,cold=case['activationFiles'],case['coldActivationFiles']
    r=case['activationPreparation']['record']
    pin=r['intent']['source']
    replacement=case['replacement']
    expected_set=dict(schemaVersion=1,kind='installed_set_observation',readOnly=True,runtimeDirectory=pin['runtimeDirectory'],
                      runtimeIdentity=pin['runtimeIdentity'],manifestFile=replacement['actualInstalledFiles'][4],
                      manifest=replacement['actualManifest'],images=replacement['actualInstalledFiles'][:4])
    need(f['actualSet']==expected_set and f['record']==cold['record']==r,'Strict installed set or Prepared identity differs')
    base=dict(phase='not_bound',attemptId=r['intent']['attemptId'],setId=pin['setId'],heldFiles=0,
              worker=None,manifestSha256=None,originalStateHeld=True,reason=None)
    bound=dict(base,phase='bound',heldFiles=5,worker=pin['worker'],manifestSha256=pin['manifest']['sha256'])
    for name,expected,active in [('before',base,False),('admitted',dict(base,phase='binding'),True),
                                 ('view',bound,False),('duplicate',bound,False)]:
        v=f[name]
        need(v['schemaVersion']==13 and v['update']==replacement['view']['update']
             and v['activationFiles']==expected and v['driverActive'] is active and v['driverError'] is None,
             'Binding scope/progress/duplicate differs: '+name)
    duplicate=f['duplicateActive']
    need(duplicate['schemaVersion']==13 and duplicate['update']==f['view']['update'] and duplicate['driverError'] is None
         and (duplicate['activationFiles']==dict(base,phase='binding') and duplicate['driverActive'] is True
              or duplicate['activationFiles']==bound and duplicate['driverActive'] is False), 'Active duplicate created/replayed another binding')
    v=cold['view']
    need(v['schemaVersion']==13 and v['activationFiles']==dict(base,originalStateHeld=False)
         and v['update']==f['view']['update'] and v['driverActive'] is False and v['driverError'] is None,
         'Cold data regained live file/state resources')
    need(f['workerBytesBefore']==f['workerBytesAfter']==replacement['workerBytesBefore']
         and all(f[k] is True for k in ('strictPinBeforeRefused','strictPinAfterSucceeded','workerWriteDenied','workerDeleteDenied',
                                      'originalInstanceLockHeld','legacyFieldAbsent'))
         and cold['ownedJobEmpty'] is True and cold['workerBytesUnchanged'] is True
         and f['workerLaunchCount']==f['stateHandoffCount']==cold['workerLaunchCount']==cold['stateHandoffCount']==0,
         'Binding opened state, launched worker, released lock or changed files')


def audit(proof,source,target,root,preparation,health):
    verify(proof,source,target,root,preparation,health)
    prefix='cases/0/activationFiles/'
    edits=[(prefix+'view/activationFiles/heldFiles',4),(prefix+'admitted/activationFiles/phase','bound'),
           (prefix+'view/activationFiles/originalStateHeld',False),(prefix+'actualSet/manifest/setId','foreign'),
           (prefix+'actualSet/images/0/fileIdentity','foreign'),(prefix+'workerBytesAfter',[]),
           (prefix+'strictPinBeforeRefused',False),(prefix+'strictPinAfterSucceeded',False),
           (prefix+'workerWriteDenied',False),(prefix+'workerDeleteDenied',False),(prefix+'originalInstanceLockHeld',False),
           (prefix+'workerLaunchCount',1),(prefix+'stateHandoffCount',1),(prefix+'legacyFieldAbsent',False),
           ('cases/0/coldActivationFiles/view/activationFiles/heldFiles',5),
           ('cases/0/coldActivationFiles/view/activationFiles/originalStateHeld',True),
           ('cases/0/coldActivationFiles/view/activationFiles/heldFiles',False),
           ('cases/0/coldActivationFiles/view/activationFiles/originalStateHeld',0),
           (prefix+'actualSet/readOnly',1)]
    for path,value in edits:
        changed=copy.deepcopy(proof);obj=changed;parts=path.split('/')
        for part in parts[:-1]:obj=obj[int(part)] if isinstance(obj,list) else obj[part]
        obj[parts[-1]]=value
        try:verify(changed,source,target,root,preparation,health)
        except (ValueError,KeyError,TypeError,IndexError):continue
        raise ValueError('Accepted changed file binding proof: '+path)
    return len(edits)
