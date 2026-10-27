param([Parameter(Mandatory)][string]$WorkDirectory,[Parameter(Mandatory)][string]$Fixture)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/test-activation-runtime.ps1')
Assert-SessionDirectory $WorkDirectory
Assert-SessionFile $Fixture
if($env:OS -cne 'Windows_NT' -or (Test-Path -LiteralPath $WorkDirectory)){throw 'New local Windows policy-test directory required'}
[IO.Directory]::CreateDirectory($WorkDirectory)|Out-Null
$installed=Join-Path $WorkDirectory 'production/winbridge-worker.exe'
$root=Join-Path $WorkDirectory 'test-runtime'
$run=Join-Path $WorkDirectory 'run'
[IO.Directory]::CreateDirectory($run)|Out-Null
$sha=(Get-FileHash -LiteralPath $Fixture -Algorithm SHA256).Hash.ToLowerInvariant()
$sourceCommit='a'*40;$manifestSha='b'*64
$checks=[Collections.Generic.List[string]]::new()
$slot=$null
function Refuses([scriptblock]$Action){
 $refused=$false;$value=$null;try{$value=& $Action}catch{$refused=$true}
 if(-not $refused){
  if($null -ne $value -and $null -ne $value.PSObject.Properties['phase']){Close-TestWorkerActivationRuntime $value}
  throw "Unsafe activation test runtime operation accepted: $Action"
 }
}
function Claim([string]$SlotPath,[string]$RunPath=$run,[string]$Commit=$sourceCommit){
 Open-TestWorkerActivationRuntime -Directory $SlotPath -InstalledWorker $installed -Image $Fixture -Sha256 $sha -RunDirectory $RunPath -SourceCommit $Commit -SourceManifestSha256 $manifestSha
}
try{
 Refuses {Claim $root $run 'bad'}
 if(Test-Path -LiteralPath $root){throw 'Invalid source pin created a runtime'}
 $checks.Add('bad-source-pin-no-side-effects')
 foreach($unsafe in @((Split-Path $installed -Parent),$run,(Join-Path $run 'child'),(Split-Path $run -Parent))){Refuses {Claim $unsafe}}
 $checks.Add('production-and-evidence-overlap-refused')
 foreach($unsafe in @('K:relative','\\host\share\runtime','K:\safe\..\runtime','K:\runtime:stream','K:\')){Refuses {Get-ActivationTestLocalPath $unsafe}}
 $checks.Add('ambiguous-network-traversal-stream-drive-root-refused')
 $unknown=Join-Path $WorkDirectory 'unknown'
 [IO.Directory]::CreateDirectory($unknown)|Out-Null
 [IO.File]::WriteAllText((Join-Path $unknown 'manual.txt'),'preserve')
 Refuses {Claim $unknown}
 if([IO.File]::ReadAllText((Join-Path $unknown 'manual.txt')) -cne 'preserve'){throw 'Manual data changed'}
 $checks.Add('unknown-directory-preserved')
 $slot=Claim $root
 if($slot.binding.runtimeDirectory -cne [IO.Path]::GetFullPath($root) -or $slot.binding.runDirectory -cne [IO.Path]::GetFullPath($run) -or
  $slot.binding.sourceCommit -cne $sourceCommit -or $slot.binding.sourceManifestSha256 -cne $manifestSha){throw 'Reservation scope changed'}
 $checks.Add('exact-stable-path-and-source-bound')
 Refuses {Claim $root}
 Refuses {Open-TestWorkerRuntime $root $installed $Fixture $sha}
 $checks.Add('activation-and-legacy-contenders-refused')
 $path=$slot.runtime.executable
 Refuses {$f=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Write);$f.Dispose()}
 $checks.Add('initial-image-pin-held')
 Refuses {$f=[IO.File]::Open($slot.marker,[IO.FileMode]::Open,[IO.FileAccess]::Write);$f.Dispose()}
 $checks.Add('declaration-pin-held')
 Release-TestWorkerActivationImagePin $slot
 $f=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Write);$f.Dispose()
 if($slot.phase -cne 'image_released' -or $slot.runtime.lease.SafeFileHandle.IsClosed){throw 'Image release lost the exclusive runtime lease'}
 Refuses {Open-TestWorkerRuntime $root $installed $Fixture $sha}
 Refuses {Release-TestWorkerActivationImagePin $slot}
 $checks.Add('image-pin-released-once-lease-retained')
 $config=Join-Path $root 'winbridge-worker.local.json'
 Write-NewJson $config @{fixture='preserve configuration'}
 $configSha=(Get-FileHash -LiteralPath $config -Algorithm SHA256).Hash
 $ownerSha=$slot.markerSha256
 Close-TestWorkerActivationRuntime $slot
 Close-TestWorkerActivationRuntime $slot
 Refuses {Release-TestWorkerActivationImagePin $slot}
 $checks.Add('close-idempotent-never-creates-grant')
 Refuses {Claim $root}
 Refuses {Open-TestWorkerRuntime $root $installed $Fixture $sha}
 if((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $sha -or
  (Get-FileHash -LiteralPath $config -Algorithm SHA256).Hash -cne $configSha -or
  (Get-FileHash -LiteralPath $slot.marker -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ownerSha){throw 'Abandoned reservation/config/image changed'}
 $checks.Add('cold-reservation-and-config-preserved-no-reuse')
 Write-Output ('ACTIVATION_RUNTIME_POLICY '+(@{schemaVersion=1;passed=$true;checks=@($checks.ToArray());checksPassed=$checks.Count;stablePath=$true;networkOpened=$false;processStarted=$false;installedRuntimeApplied=$false;windowsOpened=$false}|ConvertTo-Json -Compress -Depth 5))
}finally{Close-TestWorkerActivationRuntime $slot}
