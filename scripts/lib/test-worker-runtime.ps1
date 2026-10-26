# One reusable executable path for firewall rules. State/config/report stay per-run.
function Open-TestWorkerRuntime {
 param(
  [Parameter(Mandatory)][string]$Directory,
  [Parameter(Mandatory)][string]$InstalledWorker,
  [Parameter(Mandatory)][string]$Image,
  [Parameter(Mandatory)][string]$Sha256
 )
 if($Directory -notmatch '^[A-Za-z]:[\\/]' -or $Directory -match '(^|[\\/])\.\.([\\/]|$)'){throw 'Test runtime needs an absolute local path without traversal'}
 $root=[IO.Path]::GetFullPath($Directory).TrimEnd('\')
 $production=[IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($InstalledWorker)).TrimEnd('\')
 if($root -notmatch '^[A-Za-z]:[\\/]' -or $root.Equals($production,[StringComparison]::OrdinalIgnoreCase) -or
  $root.StartsWith($production+'\',[StringComparison]::OrdinalIgnoreCase) -or
  $production.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Test runtime must be separate from production runtime'}
 Assert-SessionDirectory $root
 Assert-WorkflowCachePath $root
 if([IO.DriveInfo]::new([IO.Path]::GetPathRoot($root)).DriveType -ne [IO.DriveType]::Fixed){throw 'Test runtime requires a local fixed disk'}
 $marker=Join-Path $root 'winbridge-test-runtime.json'
 if((Test-Path -LiteralPath $root) -and -not (Test-Path -LiteralPath $marker) -and
  @(Get-ChildItem -LiteralPath $root -Force).Count){throw 'Unmarked nonempty test directory preserved'}
 [IO.Directory]::CreateDirectory($root)|Out-Null
 $lease=$null;$pin=$null;$source=$null
 try{
  $leasePath=Join-Path $root '.winbridge-test-runtime.lock'
  Assert-SessionFile $leasePath
  $lease=[IO.File]::Open($leasePath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  if(Test-Path -LiteralPath $marker){
   Assert-SessionFile $marker
   $owner=Get-Content -Raw -LiteralPath $marker|ConvertFrom-Json
   if(($owner.schemaVersion -isnot [long] -and $owner.schemaVersion -isnot [int]) -or
    $owner.schemaVersion -ne 1 -or $owner.kind -cne 'winbridge_test_runtime' -or
    -not ([string]$owner.installedWorker).Equals([IO.Path]::GetFullPath($InstalledWorker),[StringComparison]::OrdinalIgnoreCase)){
    throw 'Test runtime ownership marker differs; preserved'
   }
  }else{
   Write-NewJson $marker @{schemaVersion=1;kind='winbridge_test_runtime';installedWorker=[IO.Path]::GetFullPath($InstalledWorker)}
  }
  $allowed=@('winbridge-test-runtime.json','.winbridge-test-runtime.lock','winbridge-worker.exe')
  if(@(Get-ChildItem -LiteralPath $root -Force|Where-Object {$_.Name -notin $allowed -or $_.PSIsContainer}).Count){throw 'Unexpected test runtime entries preserved'}
  $target=Join-Path $root 'winbridge-worker.exe'
  Assert-SessionFile $target
  foreach($process in @(Get-CimInstance Win32_Process -Filter "Name='winbridge-worker.exe'")){
   if(-not $process.ExecutablePath){throw 'Cannot inspect an existing worker; no replacement'}
   if($process.ExecutablePath.Equals($target,[StringComparison]::OrdinalIgnoreCase)){throw 'Test worker is still running; do not replace or adopt it'}
  }
  Assert-SessionFile $Image
  $source=[IO.File]::Open($Image,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
  $hash=[Security.Cryptography.SHA256]::Create()
  try{$actual=[Convert]::ToHexString($hash.ComputeHash($source)).ToLowerInvariant()}finally{$hash.Dispose()}
  if($actual -cne $Sha256){throw 'Pinned test source differs'}
  # This owned slot contains reproducible binary bytes only. No state/config is
  # copied here. A failed copy cannot authorize execution without the readback.
  Assert-SessionDirectory $root
  Assert-SessionFile $target
  [IO.File]::Copy($Image,$target,$true)
  $pin=[IO.File]::Open($target,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
  $hash=[Security.Cryptography.SHA256]::Create()
  try{$actual=[Convert]::ToHexString($hash.ComputeHash($pin)).ToLowerInvariant()}finally{$hash.Dispose()}
  if($actual -cne $Sha256){throw 'Test executable copy differs; no launch'}
  return [pscustomobject]@{directory=$root;executable=$target;sha256=$actual;lease=$lease;pin=$pin}
 }catch{
  if($null -ne $pin){$pin.Dispose()};if($null -ne $lease){$lease.Dispose()};throw
 }finally{if($null -ne $source){$source.Dispose()}}
}

function Close-TestWorkerRuntime($Runtime){
 if($null -ne $Runtime){$Runtime.pin.Dispose();$Runtime.lease.Dispose()}
}
