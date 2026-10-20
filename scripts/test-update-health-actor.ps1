<#
.SYNOPSIS
Run the isolated full-candidate health actor gate; never install or open desktop.
#>
param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$TargetArchivePath,
  [Parameter(Mandatory)][string]$TargetArchiveSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReportDirectory
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
foreach($path in @($SourceDirectory,$ValidationCacheDirectory,$ReportDirectory)){Assert-SessionDirectory $path}
foreach($sha in @($SourceArchiveSha256,$TargetArchiveSha256)){if($sha -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact source archive SHA required'}}
$source=[IO.Path]::GetFullPath($SourceDirectory)
$cache=[IO.Path]::GetFullPath($ValidationCacheDirectory)
$manifestPath=Join-Path $source 'winbridge-source-manifest.json'
$manifest=Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if($manifest.dirty -or -not $manifest.includesBuiltUi){throw 'Clean exported source with built UI required'}
New-Item -ItemType Directory -Path $ReportDirectory -Force | Out-Null
$report=[ordered]@{schemaVersion=1;kind='candidate_health_native_gate';platform='windows';passed=$false;sourceCommit=$manifest.sourceCommit;sourceArchiveSha256=$SourceArchiveSha256;commands=@();runtimeApplied=$false;windowsOpened=$false;error=$null;storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
$oldTarget=$env:CARGO_TARGET_DIR;$oldIncremental=$env:CARGO_INCREMENTAL;$oldInput=$env:WINBRIDGE_HEALTH_GATE_INPUT;$oldProof=$env:WINBRIDGE_HEALTH_GATE_PROOF
$oldConsoleEncoding=[Console]::OutputEncoding;$oldPipelineEncoding=$OutputEncoding
$oldTemp=$env:TEMP;$oldTmp=$env:TMP;$oldLocation=Get-Location
$lease=$null
try {
  if($env:OS -cne 'Windows_NT' -or $report.storage.availableBytesBefore -lt 6GB){throw 'Windows local disk with at least 6 GiB required; no cleanup'}
  [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
  foreach($f in $manifest.files){$p=Join-Path $source $f.path;Assert-SessionFile $p;if((Get-Item -LiteralPath $p).Length -ne $f.bytes -or (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() -cne $f.sha256){throw "Source manifest mismatch: $($f.path)"}}
  Assert-SessionFile $TargetArchivePath
  if((Get-FileHash -LiteralPath $TargetArchivePath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $TargetArchiveSha256){throw 'Target archive SHA mismatch'}
  $zip=[IO.Compression.ZipFile]::OpenRead($TargetArchivePath)
  try {
    $entry=$zip.GetEntry('winbridge/winbridge-source-manifest.json');if(-not $entry -or $entry.Length -gt 4MB){throw 'Bounded target source manifest required'}
    $stream=$entry.Open();$memory=[IO.MemoryStream]::new()
    try {$stream.CopyTo($memory);$bytes=$memory.ToArray()} finally {$stream.Dispose();$memory.Dispose()}
  } finally {$zip.Dispose()}
  $targetManifest=[Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json
  if($targetManifest.dirty -or -not $targetManifest.includesBuiltUi -or $targetManifest.sourceCommit -ceq $manifest.sourceCommit){throw 'Distinct clean target generation with built UI required'}
  $pin=@{archiveName=[IO.Path]::GetFileName($TargetArchivePath);archiveSha256=$TargetArchiveSha256;manifestSha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant();sourceCommit=$targetManifest.sourceCommit}
  $pwsh=(Get-Command pwsh.exe -ErrorAction Stop).Source
  $inputPath=Join-Path (Split-Path $source -Parent) 'health-input.json'
  Write-NewJson $inputPath @{schemaVersion=1;target=$pin;archive=[IO.Path]::GetFullPath($TargetArchivePath);powershell=$pwsh;validationCache=$cache}
  $report.target=$pin
  $env:CARGO_TARGET_DIR=Join-Path $cache 'test-build-cache'
  Assert-WorkflowCachePath $env:CARGO_TARGET_DIR
  if(-not (Test-Path -LiteralPath $env:CARGO_TARGET_DIR -PathType Container)){throw 'Retained native test cache required'}
  foreach($p in @((Join-Path $cache 'winbridge/target'),(Join-Path $cache 'winbridge/apps/desktop/src-tauri/target'))){Assert-SessionDirectory $p;if(-not (Test-Path -LiteralPath $p -PathType Container)){throw 'Retained candidate compiler caches required'}}
  $lock=Join-Path $env:CARGO_TARGET_DIR '.winbridge-build-cache.lock';Assert-SessionFile $lock
  $lease=[IO.File]::Open($lock,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  $env:CARGO_INCREMENTAL='0';$env:WINBRIDGE_HEALTH_GATE_INPUT=$inputPath;$env:WINBRIDGE_HEALTH_GATE_PROOF=Join-Path $ReportDirectory 'candidate-health-actor-proof.json'
  $env:TEMP=Join-Path (Split-Path $source -Parent) 'test-temp';$env:TMP=$env:TEMP;New-Item -ItemType Directory -Path $env:TEMP | Out-Null
  Set-Location -LiteralPath $source
  $commands=@(
    @{name='health-transition-fsm';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','driver_transition_proof','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='health-supervisor-build';arguments=@('build','-p','winbridge-update','--features','bundled-sqlite','--locked','--offline')},
    @{name='health-full-pipeline';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','update_control::tests::health::health_actor_full_candidate_roundtrip','--locked','--offline','--','--exact','--ignored','--test-threads=1','--nocapture')}
  )
  foreach($command in $commands){
    $log=$command.name+'.log';$arguments=$command.arguments
    Write-Output "Health native gate: $($command.name); installed runtime remains active"
    & cargo.exe @arguments 2>&1 | Tee-Object -FilePath (Join-Path $ReportDirectory $log)
    $code=$LASTEXITCODE;$report.commands+=@{name=$command.name;arguments=$arguments;exitCode=$code;log=$log}
    if($code -ne 0){throw "Health native command failed: $($command.name); see $log"}
  }
  $proof=Get-Content -LiteralPath $env:WINBRIDGE_HEALTH_GATE_PROOF -Raw | ConvertFrom-Json
  if($proof.kind -cne 'candidate_health_actor_proof' -or $proof.passed -ne $true -or $proof.runtimeApplied -ne $false -or $proof.windowsOpened -ne $false -or $proof.cases.Count -ne 4){throw 'Actual health actor proof incomplete'}
  $report.passed=$true
} catch {$report.error=$_.Exception.Message;Write-Output ("Health gate failed: "+$report.error)} finally {
  $report.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  if($lease){$lease.Dispose()}
  $env:CARGO_TARGET_DIR=$oldTarget;$env:CARGO_INCREMENTAL=$oldIncremental;$env:WINBRIDGE_HEALTH_GATE_INPUT=$oldInput;$env:WINBRIDGE_HEALTH_GATE_PROOF=$oldProof;$env:TEMP=$oldTemp;$env:TMP=$oldTmp
  [Console]::OutputEncoding=$oldConsoleEncoding;$OutputEncoding=$oldPipelineEncoding
  Set-Location -LiteralPath $oldLocation
  Write-NewJson (Join-Path $ReportDirectory 'report.json') $report
  $files=@(Get-ChildItem -LiteralPath $ReportDirectory -File | Sort-Object Name | ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $ReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
if(-not $report.passed){exit 1}
