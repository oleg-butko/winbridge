# Reservation for the future full activation gate. This is test infrastructure,
# not an installation/recovery API. JSON below is evidence, never process custody.
# Intentionally no reuse/archive/delete: an unfinished reservation is preserved
# until native process/Job exit and an explicit archive protocol are implemented.
. (Join-Path $PSScriptRoot 'test-worker-runtime.ps1')

function Get-ActivationTestLocalPath([string]$Path) {
 if(($Path -notmatch '^[A-Za-z]:[\\/]') -or ($Path -match '(^|[\\/])\.\.([\\/]|$)') -or
  ($Path.Length -gt 2 -and $Path.Substring(2).Contains(':'))){throw 'Activation fixture requires an absolute local path without traversal or streams'}
 $full=[IO.Path]::GetFullPath($Path).TrimEnd('\','/')
 if($full.Length -le 2){throw 'Activation fixture cannot own a drive root'}
 Assert-SessionDirectory $full
 Assert-WorkflowCachePath $full
 if([IO.DriveInfo]::new([IO.Path]::GetPathRoot($full)).DriveType -ne [IO.DriveType]::Fixed){throw 'Activation fixture requires a local fixed disk'}
 return $full
}

function Test-ActivationPathOverlap([string]$A,[string]$B) {
 return $A.Equals($B,[StringComparison]::OrdinalIgnoreCase) -or
  $A.StartsWith($B+'\',[StringComparison]::OrdinalIgnoreCase) -or
  $B.StartsWith($A+'\',[StringComparison]::OrdinalIgnoreCase)
}

function Open-TestWorkerActivationRuntime {
 param(
  [Parameter(Mandatory)][string]$Directory,
  [Parameter(Mandatory)][string]$InstalledWorker,
  [Parameter(Mandatory)][string]$Image,
  [Parameter(Mandatory)][string]$Sha256,
  [Parameter(Mandatory)][string]$RunDirectory,
  [Parameter(Mandatory)][string]$SourceCommit,
  [Parameter(Mandatory)][string]$SourceManifestSha256
 )
 # Validate the entire declaration before copying a binary or claiming a slot.
 if($SourceCommit -cnotmatch '^[0-9a-f]{40}$' -or $SourceManifestSha256 -cnotmatch '^[0-9a-f]{64}$' -or $Sha256 -cnotmatch '^[0-9a-f]{64}$'){
  throw 'Exact activation fixture source/image pins required'
 }
 $root=Get-ActivationTestLocalPath $Directory
 $run=Get-ActivationTestLocalPath $RunDirectory
 if(($InstalledWorker -notmatch '^[A-Za-z]:[\\/]') -or ($InstalledWorker.Substring(2).Contains(':'))){throw 'Absolute installed worker path required'}
 Assert-SessionFile $InstalledWorker
 $production=Get-ActivationTestLocalPath ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($InstalledWorker)))
 if((Test-ActivationPathOverlap $root $run) -or (Test-ActivationPathOverlap $root $production) -or
  (Test-ActivationPathOverlap $run $production)){throw 'Activation slot, run evidence and production must be disjoint'}
 if(-not (Test-Path -LiteralPath $run -PathType Container)){throw 'Existing run evidence directory required'}
 $marker=Join-Path $root 'winbridge-activation-runtime.json'
 Assert-SessionFile $marker
 # Even an otherwise empty abandoned reservation is not silently adopted.
 if(Test-Path -LiteralPath $marker){throw 'Previous activation reservation preserved; explicit native exit/archive proof required'}
 $runtime=$null;$ownerPin=$null
 try{
  $runtime=Open-TestWorkerRuntime -Directory $root -InstalledWorker $InstalledWorker -Image $Image -Sha256 $Sha256
  $binding=[ordered]@{
   schemaVersion=1;kind='winbridge_activation_test_reservation';reservationId=[guid]::NewGuid().ToString()
   runtimeDirectory=$root;runDirectory=$run;installedWorker=[IO.Path]::GetFullPath($InstalledWorker)
   sourceCommit=$SourceCommit;sourceManifestSha256=$SourceManifestSha256;initialWorkerSha256=$Sha256
   createdAtUtc=[DateTime]::UtcNow.ToString('o')
  }
  Write-NewJson $marker $binding
  $ownerPin=[IO.File]::Open($marker,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
  $ownerSha=(Get-FileHash -LiteralPath $marker -Algorithm SHA256).Hash.ToLowerInvariant()
  return [pscustomobject]@{runtime=$runtime;binding=$binding;marker=$marker;markerSha256=$ownerSha;ownerPin=$ownerPin;phase='pinned'}
 }catch{
  if($null -ne $ownerPin){$ownerPin.Dispose()}
  if($null -ne $runtime){Close-TestWorkerRuntime $runtime}
  throw
 }
}

function Release-TestWorkerActivationImagePin($Reservation) {
 # Keep the exclusive slot lease and immutable declaration, only relinquish
 # the temporary script's image pin BEFORE normal Main/preparation/admission.
 # Actual replacement later requires the original native updater guard.
 if($null -eq $Reservation -or $Reservation.phase -cne 'pinned' -or
  $Reservation.runtime.lease.SafeFileHandle.IsClosed -or $Reservation.ownerPin.SafeFileHandle.IsClosed -or
  (Get-FileHash -LiteralPath $Reservation.marker -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Reservation.markerSha256){
  throw 'Original pinned activation reservation required; no reopen or adoption'
 }
 $Reservation.runtime.pin.Dispose()
 $Reservation.runtime.pin=$null
 $Reservation.phase='image_released'
}

function Close-TestWorkerActivationRuntime($Reservation) {
 if($null -ne $Reservation -and $Reservation.phase -cne 'closed'){
  if($null -ne $Reservation.runtime.pin){$Reservation.runtime.pin.Dispose()}
  $Reservation.ownerPin.Dispose()
  $Reservation.runtime.lease.Dispose()
  $Reservation.phase='closed'
  # Preserve every byte including marker on success/failure. Closing a script
  # lease is not a process/Job exit proof and never authorizes the next run.
 }
}
