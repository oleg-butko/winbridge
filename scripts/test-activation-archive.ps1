param([Parameter(Mandatory)][string]$WorkDirectory,[Parameter(Mandatory)][string]$Fixture)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/test-activation-runtime.ps1')
[void](Get-ActivationTestLocalPath $WorkDirectory)
Assert-SessionFile $Fixture
if(Test-Path -LiteralPath $WorkDirectory){throw 'Fresh archive-test directory required'}
[IO.Directory]::CreateDirectory($WorkDirectory)|Out-Null
$oldTemp=$env:TEMP;$oldTmp=$env:TMP
$env:TEMP=Join-Path $WorkDirectory 'compiler-temp';$env:TMP=$env:TEMP
[IO.Directory]::CreateDirectory($env:TEMP)|Out-Null
$scopes=[Collections.Generic.List[object]]::new();$child=$null;$locked=$null
$checks=[Collections.Generic.List[string]]::new()
function Refuses([scriptblock]$Action){
 $rejected=$false;try{& $Action|Out-Null}catch{$rejected=$true}
 if(-not $rejected){throw "Archive policy accepted unsafe action: $Action"}
}
try{
 . (Join-Path $PSScriptRoot 'lib/test-activation-archive.ps1')
 $installed=Join-Path $WorkDirectory 'production/winbridge-worker.exe'
 $sha=(Get-FileHash -LiteralPath $Fixture -Algorithm SHA256).Hash.ToLowerInvariant()
 function New-ArchiveCase([string]$Name){
  $base=Join-Path $WorkDirectory $Name;$run=Join-Path $base 'run';$runtime=Join-Path $base 'runtime'
  [IO.Directory]::CreateDirectory($run)|Out-Null
  $reservation=Open-TestWorkerActivationRuntime -Directory $runtime -InstalledWorker $installed -Image $Fixture -Sha256 $sha -RunDirectory $run -SourceCommit ('a'*40) -SourceManifestSha256 ('b'*64)
  Release-TestWorkerActivationImagePin $reservation
  $jobName='WinBridge-archive-fixture-'+$reservation.binding.reservationId
  $job=[WinBridgeTestArchiveV1+FixtureJob]::new($jobName)
  # The flag alone is deliberately insufficient; live Job/process tests below
  # must refuse it until the actual retained fixture has exited.
  $proof=Join-Path $run 'runtime-retirement.json'
  Write-NewJson $proof @{schemaVersion=1;kind='winbridge_activation_runtime_retirement';scenario='owned-fixture';reservationId=$reservation.binding.reservationId;markerSha256=$reservation.markerSha256;sourceCommit=$reservation.binding.sourceCommit;runtimeDirectory=$runtime;runDirectory=$run;nativeExitConfirmed=$true;jobNames=@($jobName)}
  $case=[pscustomobject]@{reservation=$reservation;root=$runtime;run=$run;proof=$proof;proofSha256=(Get-FileHash -LiteralPath $proof -Algorithm SHA256).Hash.ToLowerInvariant();job=$job;jobName=$jobName}
  $scopes.Add($case)
  return $case
 }
 function Run-Archive($Case,[scriptblock]$After=$null){
  Archive-TestWorkerActivationRuntime $Case.reservation $Case.proof $Case.proofSha256 -AfterMoveForTest $After
 }
 $bad=New-ArchiveCase 'unknown'
 [IO.File]::WriteAllText((Join-Path $bad.root 'manual.txt'),'preserve unknown file')
 Refuses {Run-Archive $bad}
 if([IO.File]::ReadAllText((Join-Path $bad.root 'manual.txt')) -cne 'preserve unknown file'){throw 'Unknown data changed'}
 $checks.Add('unknown-entry-preserved')

 $linked=New-ArchiveCase 'hardlink'
 $p=Join-Path $linked.root 'winbridge-worker.local.json';[IO.File]::WriteAllText($p,'opaque fixture')
 New-Item -ItemType HardLink -Path (Join-Path $linked.run 'alias') -Target $p|Out-Null
 Refuses {Run-Archive $linked}
 if([IO.File]::ReadAllText($p) -cne 'opaque fixture'){throw 'Linked data changed'}
 $checks.Add('hardlinked-data-preserved')

 $reparse=New-ArchiveCase 'junction'
 $outside=Join-Path $reparse.run 'outside';[IO.Directory]::CreateDirectory($outside)|Out-Null
 [IO.File]::WriteAllText((Join-Path $outside 'keep.txt'),'outside unchanged')
 New-Item -ItemType Junction -Path (Join-Path $reparse.root 'update') -Target $outside|Out-Null
 Refuses {Run-Archive $reparse}
 if([IO.File]::ReadAllText((Join-Path $outside 'keep.txt')) -cne 'outside unchanged'){throw 'Junction target changed'}
 $checks.Add('junction-target-preserved')

 $collisionSource=Join-Path $WorkDirectory 'move-source.txt'
 $collisionTarget=Join-Path $WorkDirectory 'move-target.txt'
 [IO.File]::WriteAllText($collisionSource,'source preserved')
 [IO.File]::WriteAllText($collisionTarget,'target preserved')
 Refuses {[WinBridgeTestArchiveV1]::MoveNew($collisionSource,$collisionTarget)}
 if([IO.File]::ReadAllText($collisionSource) -cne 'source preserved' -or [IO.File]::ReadAllText($collisionTarget) -cne 'target preserved'){throw 'Native move overwrote/deleted collision data'}
 $checks.Add('native-move-never-overwrites-collision')
 $occupied=New-ArchiveCase 'occupied'
 $occupiedArchive=Join-Path $occupied.run ('runtime-archive-'+$occupied.reservation.binding.reservationId)
 [IO.Directory]::CreateDirectory($occupiedArchive)|Out-Null
 [IO.File]::WriteAllText((Join-Path $occupiedArchive 'keep.txt'),'existing archive preserved')
 Refuses {Run-Archive $occupied}
 if([IO.File]::ReadAllText((Join-Path $occupiedArchive 'keep.txt')) -cne 'existing archive preserved' -or -not (Test-Path -LiteralPath $occupied.reservation.marker)){throw 'Occupied archive changed'}
 $checks.Add('occupied-archive-and-source-preserved')

 $case=New-ArchiveCase 'successful'
 $config=Join-Path $case.root 'winbridge-worker.local.json'
 Write-NewJson $config @{originalRuntimePath=$case.root;fixture='no secret or production config'}
 $configBytes=[IO.File]::ReadAllBytes($config)
 $state=Join-Path $case.root 'update-state';[IO.Directory]::CreateDirectory($state)|Out-Null
 # Opaque bytes test conservation, not SQLite validity or a migration proof.
 [IO.File]::WriteAllBytes((Join-Path $state 'opaque-state.bin'),[byte[]]@(0,1,2,255,4))
 $source=Join-Path $case.root 'update/поколение/Привет ✓.txt'
 [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($source))|Out-Null
 [IO.File]::WriteAllText($source,'immutable fixture Привет ✓',[Text.UTF8Encoding]::new($false))
 $snapshot=Get-TestArchiveSnapshot (Join-Path $case.root 'update')
 $workerIdentity=$null
 $stream=[IO.File]::OpenRead($case.reservation.runtime.executable)
 try{$workerIdentity=[WinBridgeTestArchiveV1]::Identity($stream.SafeFileHandle)}finally{$stream.Dispose()}
 Refuses {Archive-TestWorkerActivationRuntime $case.reservation $case.proof ('0'*64)}
 $checks.Add('wrong-retirement-hash-refused')
 $locked=[IO.File]::Open($config,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 Refuses {Run-Archive $case}
 $locked.Dispose();$locked=$null
 if($case.reservation.phase -cne 'image_released'){throw 'Refused inventory consumed the original reservation'}
 $checks.Add('open-data-handle-refused-before-moves')

 $si=[Diagnostics.ProcessStartInfo]::new($case.reservation.runtime.executable)
 $si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.WorkingDirectory=$case.run;$si.ArgumentList.Add('long')
 $child=[Diagnostics.Process]::Start($si);$case.job.Assign($child.Handle)
 Refuses {Run-Archive $case}
 if([WinBridgeTestArchiveV1]::ActiveProcesses($case.jobName) -eq 0){throw 'Actual fixture Job not occupied'}
 $checks.Add('actual-live-job-overrides-retirement-flag')
 $child.Kill();if(-not $child.WaitForExit(15000)){throw 'Fixture exit unconfirmed'};$child.Dispose();$child=$null
 $until=[DateTime]::UtcNow.AddSeconds(10)
 while([WinBridgeTestArchiveV1]::ActiveProcesses($case.jobName) -ne 0){if([DateTime]::UtcNow -gt $until){throw 'Fixture Job exit unconfirmed'};Start-Sleep -Milliseconds 25}
 Refuses {Run-Archive $case {param($Index) if($Index -eq 0){throw 'Injected lost move result after the actual OS rename'}}}
 if($case.reservation.phase -cne 'archive_failed' -or -not (Test-Path -LiteralPath (Join-Path $case.root 'winbridge-activation-archive.pending.json'))){throw 'Interrupted archive did not preserve the plan/blocker'}
 Refuses {Open-TestWorkerRuntime $case.root $installed $Fixture $sha}
 $checks.Add('lost-result-keeps-plan-and-blocks-new-run')
 $archived=Run-Archive $case
 Assert-TestArchiveSnapshot $snapshot (Join-Path $archived.directory 'update')
 if([Convert]::ToBase64String($configBytes) -cne [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $archived.directory 'winbridge-worker.local.json')))){throw 'Serialized paths/config bytes changed'}
 $stream=[IO.File]::OpenRead((Join-Path $archived.directory 'winbridge-worker.exe'))
 try{if([WinBridgeTestArchiveV1]::Identity($stream.SafeFileHandle) -cne $workerIdentity){throw 'Archive copied instead of preserving inode'}}finally{$stream.Dispose()}
 if(@(Get-ChildItem -Force -LiteralPath $case.root|Where-Object {$_.Name -cnotin @('winbridge-test-runtime.json','.winbridge-test-runtime.lock')}).Count){throw 'Archive left slot data behind'}
 $checks.Add('all-identities-bytes-and-serialized-paths-preserved')
 Close-TestWorkerActivationRuntime $case.reservation
 Refuses {Run-Archive $case}
 $checks.Add('closed-reservation-cannot-adopt-archive')
 $nextRun=Join-Path (Split-Path $case.run -Parent) 'next-run';[IO.Directory]::CreateDirectory($nextRun)|Out-Null
 $next=Open-TestWorkerActivationRuntime -Directory $case.root -InstalledWorker $installed -Image $Fixture -Sha256 $sha -RunDirectory $nextRun -SourceCommit ('a'*40) -SourceManifestSha256 ('b'*64)
 try{
  if($next.runtime.executable -cne $case.reservation.runtime.executable -or $next.binding.reservationId -ceq $case.reservation.binding.reservationId){throw 'Stable path/new generation scope changed'}
 }finally{Close-TestWorkerActivationRuntime $next}
 $checks.Add('same-executable-path-fresh-reservation-after-archive')
 Write-Output ('ACTIVATION_ARCHIVE_POLICY '+(@{schemaVersion=1;passed=$true;checks=@($checks.ToArray());checksPassed=$checks.Count;archiveDirectory=$archived.directory;planSha256=$archived.planSha256;actualNamedJob=$true;processStopped=$true;oldPathsNotRewritten=$true;installedRuntimeApplied=$false;windowsOpened=$false;fullMain25=$false}|ConvertTo-Json -Compress -Depth 8))
}finally{
 if($null -ne $locked){$locked.Dispose()}
 if($null -ne $child){if(-not $child.HasExited){$child.Kill()};[void]$child.WaitForExit(15000);$child.Dispose()}
 foreach($scope in $scopes){$scope.job.Dispose();Close-TestWorkerActivationRuntime $scope.reservation}
 $env:TEMP=$oldTemp;$env:TMP=$oldTmp
}
