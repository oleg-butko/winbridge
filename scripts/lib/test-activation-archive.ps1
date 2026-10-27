# Only conserves this test run's inactive files. Never resumes an updater,
# starts processes, opens SQLite, rewrites serialized paths or deletes data.
. (Join-Path $PSScriptRoot 'test-activation-runtime.ps1')
if(-not ('WinBridgeTestArchiveV1' -as [type])){
 Add-Type -Path (Join-Path $PSScriptRoot 'test-activation-archive-native.cs')
}

function Open-TestArchiveDirectoryChain([string]$Path) {
 $paths=[Collections.Generic.List[string]]::new()
 $node=Get-ActivationTestLocalPath $Path
 while($node){$paths.Add($node);$node=[IO.Path]::GetDirectoryName($node)}
 $pins=[Collections.Generic.List[IDisposable]]::new()
 try{
  for($i=$paths.Count-1;$i -ge 0;$i--){$pins.Add([WinBridgeTestArchiveV1]::PinDirectory($paths[$i]))}
  return [pscustomobject]@{pins=$pins;identity=[WinBridgeTestArchiveV1]::Identity($pins[$pins.Count-1])}
 }catch{foreach($pin in $pins){$pin.Dispose()};throw}
}

function Assert-TestArchiveNoProcesses([string]$Root) {
 foreach($process in @(Get-CimInstance Win32_Process)){
  if($process.ExecutablePath){
   $path=[string]$process.ExecutablePath
   if($path.StartsWith('\\?\')){$path=$path.Substring(4)}
   $path=[IO.Path]::GetFullPath($path)
   if($path.StartsWith($Root+'\',[StringComparison]::OrdinalIgnoreCase)){
    throw "Live process $($process.ProcessId) in test runtime; preserve all files"
   }
  }elseif($process.Name -in @('winbridge-worker.exe','winbridge-update.exe','winbridge-desktop.exe','winbridge-fixture.exe')){
   throw 'Cannot inspect an existing WinBridge process; no archive'
  }
 }
}

function Get-TestArchiveSnapshot([string]$Path,[bool]$SharedMarker=$false) {
 $entries=[Collections.Generic.List[object]]::new()
 function Visit([string]$Current,[string]$Relative){
  $item=Get-Item -Force -LiteralPath $Current
  if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Reparse archive entry refused'}
  if($item.PSIsContainer){
   $pin=[WinBridgeTestArchiveV1]::PinDirectory($Current)
   try{
    $entries.Add([ordered]@{path=$Relative;directory=$true;identity=[WinBridgeTestArchiveV1]::Identity($pin)})
    foreach($child in @(Get-ChildItem -Force -LiteralPath $Current|Sort-Object Name)){
     Visit $child.FullName $(if($Relative){$Relative+'/'+$child.Name}else{$child.Name})
    }
   }finally{$pin.Dispose()}
  }else{
   $share=if($SharedMarker){[IO.FileShare]::Read}else{[IO.FileShare]::None}
   $file=[IO.File]::Open($Current,[IO.FileMode]::Open,[IO.FileAccess]::Read,$share)
   try{
    $identity=[WinBridgeTestArchiveV1]::Identity($file.SafeFileHandle)
    $hash=[Security.Cryptography.SHA256]::Create()
    try{$sha=[Convert]::ToHexString($hash.ComputeHash($file)).ToLowerInvariant()}finally{$hash.Dispose()}
    $entries.Add([ordered]@{path=$Relative;directory=$false;identity=$identity;bytes=$file.Length;sha256=$sha})
   }finally{$file.Dispose()}
  }
 }
 Visit $Path ''
 return ,$entries.ToArray()
}

function Assert-TestArchiveSnapshot($Expected,[string]$Path,[bool]$SharedMarker=$false){
 $actual=Get-TestArchiveSnapshot $Path $SharedMarker
 if(($Expected|ConvertTo-Json -Depth 8 -Compress) -cne ($actual|ConvertTo-Json -Depth 8 -Compress)){
  throw 'Archive entry identity/bytes/tree changed; preserve both locations'
 }
}

function Assert-TestArchiveRetirement($Reservation,$Proof){
 if(($Proof.schemaVersion -isnot [int] -and $Proof.schemaVersion -isnot [long]) -or $Proof.schemaVersion -ne 1 -or
  $Proof.kind -cne 'winbridge_activation_runtime_retirement' -or
  $Proof.reservationId -cne $Reservation.binding.reservationId -or
  $Proof.markerSha256 -cne $Reservation.markerSha256 -or
  $Proof.sourceCommit -cne $Reservation.binding.sourceCommit -or
  -not ([string]$Proof.runtimeDirectory).Equals($Reservation.binding.runtimeDirectory,[StringComparison]::OrdinalIgnoreCase) -or
  -not ([string]$Proof.runDirectory).Equals($Reservation.binding.runDirectory,[StringComparison]::OrdinalIgnoreCase) -or
  $Proof.nativeExitConfirmed -isnot [bool] -or -not $Proof.nativeExitConfirmed){throw 'Retirement scope/version/native exit differs'}
 $jobs=@($Proof.jobNames)
 if($Proof.scenario -ceq 'owned-fixture'){
  if($jobs.Count -ne 1 -or $jobs[0] -cne ('WinBridge-archive-fixture-'+$Reservation.binding.reservationId)){throw 'Owned fixture Job differs'}
 }elseif($Proof.scenario -ceq 'native-main'){
  if($jobs.Count -notin @(1,2) -or $jobs[0] -cnotmatch '^WinBridge-shutdown-main-validation-[0-9a-f-]{36}$' -or
   ($jobs.Count -eq 2 -and $jobs[1] -cnotmatch '^Global\\WinBridge-Activation-[0-9a-f]{64}$')){throw 'Actual Main/activation Job names required'}
 }else{throw 'Unknown retirement scenario; no archive'}
 foreach($name in $jobs){if([WinBridgeTestArchiveV1]::ActiveProcesses($name) -ne 0){throw 'Retained named Job still occupied; no archive'}}
 Assert-TestArchiveNoProcesses $Reservation.binding.runtimeDirectory
}

function Archive-TestWorkerActivationRuntime {
 param([Parameter(Mandatory)]$Reservation,[Parameter(Mandatory)][string]$RetirementProof,[Parameter(Mandatory)][string]$RetirementProofSha256,
  [scriptblock]$AfterMoveForTest=$null)
 # The caller must still hold its original FileStream, not reopen from JSON.
 if($Reservation.phase -notin @('image_released','archiving','archive_failed') -or
  $Reservation.runtime.lease.SafeFileHandle.IsClosed){throw 'Original live slot lease required; no cold adoption'}
 $root=Get-ActivationTestLocalPath $Reservation.binding.runtimeDirectory
 $run=Get-ActivationTestLocalPath $Reservation.binding.runDirectory
 $production=Get-ActivationTestLocalPath ([IO.Path]::GetDirectoryName($Reservation.binding.installedWorker))
 if((Test-ActivationPathOverlap $root $run) -or (Test-ActivationPathOverlap $root $production) -or
  (Test-ActivationPathOverlap $run $production)){throw 'Archive scope overlaps production/evidence'}
 if(-not [IO.Path]::GetFullPath($RetirementProof).Equals((Join-Path $run 'runtime-retirement.json'),[StringComparison]::OrdinalIgnoreCase)){
  throw 'Retirement proof must be the fixed file of this run'
 }
 Assert-SessionFile $RetirementProof
 if($RetirementProofSha256 -cnotmatch '^[0-9a-f]{64}$' -or
  (Get-FileHash -LiteralPath $RetirementProof -Algorithm SHA256).Hash.ToLowerInvariant() -cne $RetirementProofSha256){throw 'Retirement proof SHA differs'}
 $proof=Get-Content -Raw -LiteralPath $RetirementProof|ConvertFrom-Json
 Assert-TestArchiveRetirement $Reservation $proof
 $baseMarker=Join-Path $root 'winbridge-test-runtime.json'
 Assert-SessionFile $baseMarker
 $baseStream=[IO.File]::Open($baseMarker,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try{[void][WinBridgeTestArchiveV1]::Identity($baseStream.SafeFileHandle)}finally{$baseStream.Dispose()}
 $owner=Get-Content -Raw -LiteralPath $baseMarker|ConvertFrom-Json
 if(($owner.schemaVersion -isnot [int] -and $owner.schemaVersion -isnot [long]) -or $owner.schemaVersion -ne 1 -or $owner.kind -cne 'winbridge_test_runtime' -or
  -not ([string]$owner.installedWorker).Equals($Reservation.binding.installedWorker,[StringComparison]::OrdinalIgnoreCase)){throw 'Base runtime ownership changed'}
 $rootPin=$null;$archivePin=$null;$runPin=$null
 try{
  $rootPin=Open-TestArchiveDirectoryChain $root
  $runPin=Open-TestArchiveDirectoryChain $run
  if($rootPin.identity.Split(':')[0] -cne $runPin.identity.Split(':')[0]){throw 'Same-volume archive required before creating a plan'}
  if($null -eq $Reservation.archive){
   if($Reservation.ownerPin.SafeFileHandle.IsClosed -or
    (Get-FileHash -LiteralPath $Reservation.marker -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Reservation.markerSha256){throw 'Original declaration pin differs'}
   $files=@('winbridge-worker.exe','winbridge-fixture.exe','winbridge-desktop.exe','winbridge-update.exe',
    'winbridge-worker.local.json','winbridge-update.local.json','winbridge-desktop.local.json','winbridge-installed-set.json','winbridge-activation-runtime.json')
   $dirs=@('update','update-state')
   $skip=@('winbridge-test-runtime.json','.winbridge-test-runtime.lock')
   $items=[Collections.Generic.List[object]]::new()
   foreach($entry in @(Get-ChildItem -Force -LiteralPath $root|Sort-Object Name)){
    if($entry.Name -cin $skip){continue}
    if(($entry.PSIsContainer -and $entry.Name -cnotin $dirs) -or (-not $entry.PSIsContainer -and $entry.Name -cnotin $files)){throw 'Unknown runtime entry preserved; no archive'}
    $items.Add([ordered]@{name=$entry.Name;snapshot=Get-TestArchiveSnapshot $entry.FullName ($entry.Name -ceq 'winbridge-activation-runtime.json')})
   }
   $ordered=@($items|Where-Object {$_.name -cne 'winbridge-activation-runtime.json'})+@($items|Where-Object {$_.name -ceq 'winbridge-activation-runtime.json'})
   $archive=Join-Path $run ('runtime-archive-'+$Reservation.binding.reservationId)
   Assert-SessionDirectory $archive
   if(Test-Path -LiteralPath $archive){throw 'Archive destination already exists; preserved'}
   [IO.Directory]::CreateDirectory($archive)|Out-Null
   $plan=[ordered]@{schemaVersion=1;kind='winbridge_test_runtime_archive';binding=$Reservation.binding;markerSha256=$Reservation.markerSha256;retirementProofSha256=$RetirementProofSha256;runtimeIdentity=$rootPin.identity;archiveDirectory=$archive;entries=$ordered}
   Write-NewJson (Join-Path $archive 'plan.json') $plan
   $planSha=(Get-FileHash -LiteralPath (Join-Path $archive 'plan.json') -Algorithm SHA256).Hash.ToLowerInvariant()
   $Reservation.archive=[pscustomobject]@{plan=$plan;sha256=$planSha}
   Write-NewJson (Join-Path $root 'winbridge-activation-archive.pending.json') @{schemaVersion=1;archiveDirectory=$archive;planSha256=$planSha}
  }
  $saved=$Reservation.archive;$plan=$saved.plan;$archive=[string]$plan.archiveDirectory
  if($plan.retirementProofSha256 -cne $RetirementProofSha256 -or $plan.runtimeIdentity -cne $rootPin.identity -or
   (Get-FileHash -LiteralPath (Join-Path $archive 'plan.json') -Algorithm SHA256).Hash.ToLowerInvariant() -cne $saved.sha256){throw 'Original archive plan/scope changed'}
  if(-not $Reservation.ownerPin.SafeFileHandle.IsClosed){
   if((Get-FileHash -LiteralPath $Reservation.marker -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Reservation.markerSha256){throw 'Original marker changed before moves'}
   $pendingPath=Join-Path $root 'winbridge-activation-archive.pending.json'
   if(-not (Test-Path -LiteralPath $pendingPath)){
    Write-NewJson $pendingPath @{schemaVersion=1;archiveDirectory=$archive;planSha256=$saved.sha256}
   }
   $pendingRecord=Get-Content -Raw -LiteralPath $pendingPath|ConvertFrom-Json
   if($pendingRecord.planSha256 -cne $saved.sha256 -or $pendingRecord.archiveDirectory -cne $archive){throw 'Pending plan changed before moves'}
   $Reservation.phase='archiving'
   $Reservation.ownerPin.Dispose()
  }
  $archivePin=Open-TestArchiveDirectoryChain $archive
  if($rootPin.identity.Split(':')[0] -cne $archivePin.identity.Split(':')[0]){throw 'Same-volume archive required; never copy/delete'}
  $index=0
  foreach($entry in $plan.entries){
   Assert-TestArchiveRetirement $Reservation $proof
   $from=Join-Path $root $entry.name;$to=Join-Path $archive $entry.name
   $issued=Join-Path $archive ("move-$index-issued.json");$done=Join-Path $archive ("move-$index-done.json")
   if(Test-Path -LiteralPath $issued){
    # Reconcile a lost result only by actual identity at destination. A source
    # still present after an issued move is indeterminate, never auto-retried.
    if((Test-Path -LiteralPath $from) -or -not (Test-Path -LiteralPath $to)){throw 'Issued move outcome indeterminate; inspect, do not replay'}
    Assert-TestArchiveSnapshot $entry.snapshot $to
   }else{
    if(Test-Path -LiteralPath $to){throw 'Archive destination occupied; no overwrite'}
    Assert-TestArchiveSnapshot $entry.snapshot $from
    Write-NewJson $issued @{schemaVersion=1;source=$from;destination=$to;planSha256=$saved.sha256}
    [WinBridgeTestArchiveV1]::MoveNew($from,$to)
    if($null -ne $AfterMoveForTest){& $AfterMoveForTest $index}
    Assert-TestArchiveSnapshot $entry.snapshot $to
   }
   if(-not (Test-Path -LiteralPath $done)){Write-NewJson $done @{schemaVersion=1;identityVerified=$true;planSha256=$saved.sha256}}
   $index++
  }
  $pending=Join-Path $root 'winbridge-activation-archive.pending.json'
  $control=Join-Path $archive 'slot-control.json'
  if(Test-Path -LiteralPath $pending){[WinBridgeTestArchiveV1]::MoveNew($pending,$control)}
  $controlRecord=Get-Content -Raw -LiteralPath $control|ConvertFrom-Json
  if(($controlRecord.schemaVersion -isnot [int] -and $controlRecord.schemaVersion -isnot [long]) -or $controlRecord.schemaVersion -ne 1 -or
   $controlRecord.planSha256 -cne $saved.sha256 -or $controlRecord.archiveDirectory -cne $archive){throw 'Slot control locator differs'}
  if(@(Get-ChildItem -LiteralPath $root -Force|Where-Object {$_.Name -cnotin @('winbridge-test-runtime.json','.winbridge-test-runtime.lock')}).Count){throw 'Unexpected remaining slot entries preserved'}
  $complete=Join-Path $archive 'complete.json'
  if(-not (Test-Path -LiteralPath $complete)){
   Write-NewJson $complete @{schemaVersion=1;passed=$true;planSha256=$saved.sha256;entries=$plan.entries.Count;oldPathsNotRewritten=$true}
  }
  $completion=Get-Content -Raw -LiteralPath $complete|ConvertFrom-Json
  if($completion.passed -isnot [bool] -or -not $completion.passed -or $completion.planSha256 -cne $saved.sha256 -or
   $completion.entries -ne $plan.entries.Count -or $completion.oldPathsNotRewritten -isnot [bool] -or -not $completion.oldPathsNotRewritten){throw 'Archive completion differs'}
  $Reservation.phase='archived'
  return [pscustomobject]@{directory=$archive;planSha256=$saved.sha256;entries=$plan.entries.Count}
 }catch{
  if($Reservation.phase -ceq 'archiving'){$Reservation.phase='archive_failed'}
  throw
 }finally{
  foreach($set in @($archivePin,$runPin,$rootPin)){if($null -ne $set){foreach($pin in $set.pins){$pin.Dispose()}}}
 }
}
