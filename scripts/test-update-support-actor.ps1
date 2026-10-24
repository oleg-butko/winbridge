<#
.SYNOPSIS
Run the isolated v18 support actor against actual Windows Main; never install.
#>
param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$TargetArchivePath,
  [Parameter(Mandatory)][string]$TargetArchiveSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReportDirectory,
  [switch]$CompatibleDrainGate,
  [string]$MigrationReferenceFile,
  [string]$MigrationReferenceSha256
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
foreach($p in @($SourceDirectory,$ValidationCacheDirectory,$ReportDirectory)){Assert-SessionDirectory $p}
foreach($sha in @($SourceArchiveSha256,$TargetArchiveSha256)){if($sha -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact SHA256 required'}}
$source=[IO.Path]::GetFullPath($SourceDirectory)
$root=Split-Path $source -Parent
$cache=[IO.Path]::GetFullPath($ValidationCacheDirectory)
$manifestPath=Join-Path $source 'winbridge-source-manifest.json'
$manifest=Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$report=[ordered]@{schemaVersion=1;kind='candidate_support_actor_native_gate';platform='windows';passed=$false;sourceCommit=$manifest.sourceCommit;sourceArchiveSha256=$SourceArchiveSha256;commands=@();runtimeApplied=$false;installedRuntimeApplied=$false;windowsOpened=$false;productionDrainEnforced=$false;error=$null;storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
if($CompatibleDrainGate){$report.kind='compatible_drain_main_native_gate';$report.productionDrainEnforced=$true}
$oldTarget=$env:CARGO_TARGET_DIR;$oldIncremental=$env:CARGO_INCREMENTAL;$oldInput=$env:WINBRIDGE_SHUTDOWN_GATE_INPUT
$oldMigrationInput=$env:WINBRIDGE_COMPATIBLE_DRAIN_MIGRATION_INPUT
$oldTemp=$env:TEMP;$oldTmp=$env:TMP;$oldLocation=Get-Location
$oldConsoleEncoding=[Console]::OutputEncoding;$oldPipelineEncoding=$OutputEncoding
$lease=$null
try {
  if($env:OS -cne 'Windows_NT' -or $report.storage.availableBytesBefore -lt 6GB){throw 'Native Windows with at least 6 GiB free required; no cleanup'}
  if($manifest.dirty -or -not $manifest.includesBuiltUi){throw 'Clean exported source with built UI required'}
  foreach($f in $manifest.files){$p=Join-Path $source $f.path;Assert-SessionFile $p;if((Get-Item -LiteralPath $p).Length -ne $f.bytes -or (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() -cne $f.sha256){throw "Source manifest differs: $($f.path)"}}
  Assert-SessionFile $TargetArchivePath
  if((Get-FileHash -LiteralPath $TargetArchivePath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $TargetArchiveSha256){throw 'Target ZIP differs'}
  $zip=[IO.Compression.ZipFile]::OpenRead($TargetArchivePath)
  try {
    $entry=$zip.GetEntry('winbridge/winbridge-source-manifest.json')
    if(-not $entry -or $entry.Length -gt 4MB){throw 'Bounded target manifest required'}
    $stream=$entry.Open();$memory=[IO.MemoryStream]::new()
    try{$stream.CopyTo($memory);$bytes=$memory.ToArray()}finally{$stream.Dispose();$memory.Dispose()}
  } finally {$zip.Dispose()}
  $targetManifest=[Text.Encoding]::UTF8.GetString($bytes)|ConvertFrom-Json
  if($targetManifest.dirty -or -not $targetManifest.includesBuiltUi -or $targetManifest.sourceCommit -ceq $manifest.sourceCommit){throw 'Distinct clean target required'}
  $pin=@{archiveName=[IO.Path]::GetFileName($TargetArchivePath);archiveSha256=$TargetArchiveSha256;manifestSha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant();sourceCommit=$targetManifest.sourceCommit}
  $report.target=$pin
  if($CompatibleDrainGate){
    Assert-SessionFile $MigrationReferenceFile
    if($MigrationReferenceSha256 -cnotmatch '^[0-9a-f]{64}$' -or (Get-FileHash -LiteralPath $MigrationReferenceFile -Algorithm SHA256).Hash.ToLowerInvariant() -cne $MigrationReferenceSha256){throw 'Exact16 migration reference required'}
    $reference=Get-Content -LiteralPath $MigrationReferenceFile -Raw|ConvertFrom-Json
    if($reference.kind -cne 'compatible_drain_schema16_reference' -or $reference.schemaVersion -ne 1){throw 'Unexpected migration reference'}
    $report.migrationReferenceSha256=$MigrationReferenceSha256
  }
  $env:CARGO_TARGET_DIR=Join-Path $cache 'test-build-cache'
  Assert-WorkflowCachePath $env:CARGO_TARGET_DIR
  if(-not (Test-Path -LiteralPath $env:CARGO_TARGET_DIR -PathType Container)){throw 'Retained native cache required'}
  $lock=Join-Path $env:CARGO_TARGET_DIR '.winbridge-build-cache.lock';Assert-SessionFile $lock
  $lease=[IO.File]::Open($lock,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  $env:CARGO_INCREMENTAL='0'
  $env:TEMP=Join-Path $root 'test-temp';$env:TMP=$env:TEMP
  New-Item -ItemType Directory -Path $env:TEMP|Out-Null
  [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
  Set-Location -LiteralPath $source
  $pwsh=(Get-Command pwsh.exe -ErrorAction Stop).Source
  $caseRoot=Join-Path $root 'support-main-cases'
  New-Item -ItemType Directory -Path $caseRoot|Out-Null
  $oldStage=Join-Path $root 'support-old-set'
  $inputPath=Join-Path $root 'support-main-input.json'
  Write-NewJson $inputPath @{root=$caseRoot;target=$pin;archive=[IO.Path]::GetFullPath($TargetArchivePath);oldImages=(Join-Path $oldStage 'payload');powershell=$pwsh;validationCache=$cache;candidateSupport=$true}
  if($CompatibleDrainGate){
    $gateInput=Get-Content -LiteralPath $inputPath -Raw|ConvertFrom-Json -AsHashtable
    $gateInput.compatibleDrain=$true
    # The original create-only input is still unpublished; build a separate file.
    $inputPath=Join-Path $root 'compatible-main-input.json'
    Write-NewJson $inputPath $gateInput
    $migrationInput=Join-Path $root 'compatible-migration-input.json'
    Write-NewJson $migrationInput @{root=(Join-Path $root 'compatible-migration-case');reference=$reference;proof=(Join-Path $ReportDirectory 'compatible-migration-proof.json')}
    $env:WINBRIDGE_COMPATIBLE_DRAIN_MIGRATION_INPUT=$migrationInput
  }
  $env:WINBRIDGE_SHUTDOWN_GATE_INPUT=$inputPath
  $commands=@(
    @{name='support-supervisor-build';arguments=@('build','-p','winbridge-update','--features','bundled-sqlite','--locked','--offline')},
    @{name='support-model-ledger';minimum=12;arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','candidate_support','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='support-contract';minimum=2;arguments=@('test','-p','winbridge-protocol','--test','update_candidate_support','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='support-policy';minimum=6;arguments=@('test','-p','winbridge-core','--lib','updater_compatibility','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='support-frozen-views';minimum=1;arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','replacement_view_v9_does_not_add_fields_to_frozen_views','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='support-worker-mailbox';minimum=1;arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--lib','support_v18_observes_without_effects_and_preserves_legacy_views','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='support-old-real-set';executable=$pwsh;arguments=@('-NoProfile','-NonInteractive','-File',(Join-Path $source 'scripts/build-windows-update.ps1'),'-SourceDirectory',$source,'-ManifestSha256',(Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant(),'-StagingDirectory',$oldStage,'-VerifierExecutable',(Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-update.exe'),'-ValidationCacheDirectory',$cache)},
    @{name='support-main-pipeline';minimum=1;arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--test','update_shutdown','--locked','--offline','--','--exact','candidate_support::actual_main_queries_candidate_support_without_draining_and_retains_lost_ack','--ignored','--test-threads=1','--nocapture')}
  )
  if($CompatibleDrainGate){
    $commands=$commands[0..5]+@(
      @{name='compatible-model-ledger';minimum=7;arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','compatible_drain','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='compatible-contract';minimum=1;arguments=@('test','-p','winbridge-protocol','--test','update_compatible_drain','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='compatible-mailbox';minimum=1;arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--lib','production_drain_v19_observes_and_rejects_missing_support_without_effects','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='compatible-actual-migration';minimum=1;arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--test','compatible_drain_migration','--locked','--offline','--','--exact','actual_sixteen_support_history_survives_seventeen_and_old_sixteen_refuses','--ignored','--test-threads=1','--nocapture')}
    )+$commands[6..7]
    $commands[-1].arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--test','update_shutdown','--locked','--offline','--','--exact','candidate_support::actual_main_blocks_incompatible_production_drain_before_worker_effects','--ignored','--test-threads=1','--nocapture')
  }
  foreach($command in $commands){
    Write-Output "Candidate support actor: $($command.name); installed worker remains active"
    $log=$command.name+'.log'
    $exe=if($command.ContainsKey('executable')){$command.executable}else{'cargo.exe'}
    $arguments=$command.arguments
    & $exe @arguments 2>&1 | Tee-Object -FilePath (Join-Path $ReportDirectory $log)
    $code=$LASTEXITCODE
    $report.commands+=@{name=$command.name;exitCode=$code;log=$log;arguments=$command.arguments}
    if($code -ne 0){throw "Support actor command failed: $($command.name)"}
    if($command.ContainsKey('minimum')){
      $passed=0
      foreach($m in [regex]::Matches((Get-Content -LiteralPath (Join-Path $ReportDirectory $log) -Raw),'test result: ok\. ([0-9]+) passed; 0 failed;')){$passed+=[int]$m.Groups[1].Value}
      if($passed -lt $command.minimum){throw "Tests were not selected: $($command.name)"}
    }
  }
  [IO.File]::Copy((Join-Path $caseRoot 'proof.json'),(Join-Path $ReportDirectory 'candidate-support-actor-proof.json'),$false)
  $report.passed=$true
} catch {$report.error=$_.Exception.Message;Write-Output ('Candidate support actor gate failed: '+$report.error)} finally {
  if(Get-Variable caseRoot -ErrorAction SilentlyContinue){
    $case=Join-Path $caseRoot 'None'
    foreach($name in @('01-baseline.json','02-preimages.json','03-source.json','04-build.json','05-artifacts.json','06-health.json','07-support.json','support-ready.json','08-support-cleanup.json','validation-error.json','stdout.log','stderr.log')){
      $path=Join-Path $case $name
      if(Test-Path -LiteralPath $path -PathType Leaf){[IO.File]::Copy($path,(Join-Path $ReportDirectory ('main-'+$name)),$false)}
    }
  }
  $report.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  if($lease){$lease.Dispose()}
  $env:CARGO_TARGET_DIR=$oldTarget;$env:CARGO_INCREMENTAL=$oldIncremental;$env:WINBRIDGE_SHUTDOWN_GATE_INPUT=$oldInput
  $env:WINBRIDGE_COMPATIBLE_DRAIN_MIGRATION_INPUT=$oldMigrationInput
  $env:TEMP=$oldTemp;$env:TMP=$oldTmp
  [Console]::OutputEncoding=$oldConsoleEncoding;$OutputEncoding=$oldPipelineEncoding
  Set-Location -LiteralPath $oldLocation
  Write-NewJson (Join-Path $ReportDirectory 'report.json') $report
  $files=@(Get-ChildItem -LiteralPath $ReportDirectory -File|Sort-Object Name|ForEach-Object{@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $ReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
if(-not $report.passed){exit 1}
