<#
.SYNOPSIS
Verify one fixed-profile retained journal migration on a copy; never install or launch a worker.
#>
param(
  [Parameter(Mandatory)][ValidateSet('Service19To20','Controller20To21','Lifetime21To22','Finalization22To23','Keeper23To24')][string]$Profile,
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$ReferenceFile,
  [Parameter(Mandatory)][string]$ReferenceSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReportDirectory
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
$case=switch -Exact ($Profile) {
  'Service19To20' {@{profile='Service19To20';schema=20;referenceKind='activation_schema19_reference';reportKind='activation_service_migration_native_gate';target='activation_service_migration';test='actual_nineteen_writer_history_survives_twenty_and_old_nineteen_refuses';environment='WINBRIDGE_SERVICE_MIGRATION_INPUT';prefix='service';proof='activation-service-migration-proof.json'}}
  'Controller20To21' {@{profile='Controller20To21';schema=21;referenceKind='activation_schema20_reference';reportKind='activation_controller_migration_native_gate';target='activation_controller_migration';test='actual_twenty_local_service_history_survives_twenty_one_and_old_twenty_refuses';environment='WINBRIDGE_CONTROLLER_MIGRATION_INPUT';prefix='controller';proof='activation-controller-migration-proof.json'}}
  'Lifetime21To22' {@{profile='Lifetime21To22';schema=22;referenceKind='activation_schema21_reference';reportKind='activation_lifetime_migration_native_gate';target='activation_lifetime_migration';test='actual_twenty_one_controller_history_survives_twenty_two_and_old_twenty_one_refuses';environment='WINBRIDGE_LIFETIME_MIGRATION_INPUT';prefix='lifetime';proof='activation-lifetime-migration-proof.json'}}
  'Finalization22To23' {@{profile='Finalization22To23';schema=23;referenceKind='activation_schema22_reference';reportKind='activation_finalization_migration_native_gate';target='activation_finalization_migration';test='actual_twenty_two_lifetime_history_survives_twenty_three_and_old_twenty_two_refuses';environment='WINBRIDGE_FINALIZATION_MIGRATION_INPUT';prefix='finalization';proof='activation-finalization-migration-proof.json'}}
  'Keeper23To24' {@{profile='Keeper23To24';schema=24;referenceKind='activation_schema23_reference';reportKind='activation_keeper_migration_native_gate';target='activation_keeper_migration';test='actual_twenty_three_finalization_survives_twenty_four_and_old_twenty_three_refuses';environment='WINBRIDGE_KEEPER_MIGRATION_INPUT';prefix='keeper';proof='activation-keeper-migration-proof.json'}}
  default {throw 'Unknown fixed migration profile'}
}
$Profile=$case.profile

. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
foreach($path in @($SourceDirectory,$ValidationCacheDirectory,$ReportDirectory)){Assert-SessionDirectory $path}
foreach($sha in @($SourceArchiveSha256,$ReferenceSha256)){if($sha -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact archive/reference SHA required'}}
$source=[IO.Path]::GetFullPath($SourceDirectory)
$work=Split-Path $source -Parent
if(Test-Path -LiteralPath $ReportDirectory){throw 'Existing report preserved'}
New-Item -ItemType Directory -Path $ReportDirectory|Out-Null
$report=[ordered]@{schemaVersion=1;kind=$case.reportKind;profile=$Profile;platform='windows';passed=$false;sourceArchiveSha256=$SourceArchiveSha256;referenceSha256=$ReferenceSha256;runtimeApplied=$false;installedRuntimeApplied=$false;windowsOpened=$false;error=$null;storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
$lease=$null;$location=Get-Location
$oldTarget=$env:CARGO_TARGET_DIR;$oldIncremental=$env:CARGO_INCREMENTAL;$oldInput=[Environment]::GetEnvironmentVariable($case.environment)
$oldTemp=$env:TEMP;$oldTmp=$env:TMP
$oldEncoding=[Console]::OutputEncoding;$oldPipelineEncoding=$OutputEncoding
try {
  if($env:OS -cne 'Windows_NT' -or $report.storage.availableBytesBefore -lt 6GB){throw 'Windows and 6 GiB free required; no cleanup'}
  $manifestPath=Join-Path $source 'winbridge-source-manifest.json'
  Assert-SessionFile $manifestPath
  $manifest=Get-Content -LiteralPath $manifestPath -Raw|ConvertFrom-Json
  if($manifest.schemaVersion -ne 1 -or $manifest.dirty){throw 'Clean source manifest required'}
  foreach($f in $manifest.files){
    $p=Join-Path $source $f.path;Assert-SessionFile $p
    if((Get-Item -LiteralPath $p).Length -ne $f.bytes -or (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() -cne $f.sha256){throw "Source manifest mismatch: $($f.path)"}
  }
  Assert-SessionFile $ReferenceFile
  if((Get-FileHash -LiteralPath $ReferenceFile -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ReferenceSha256){throw 'Reference changed'}
  $reference=Get-Content -LiteralPath $ReferenceFile -Raw|ConvertFrom-Json
  if($reference.schemaVersion -ne 1 -or $reference.kind -cne $case.referenceKind){throw 'Fixed-profile actual retained reference required'}
  $schemaText=[IO.File]::ReadAllText((Join-Path $source 'apps/update/src/journal.rs'))
  $schemaMatch=[regex]::Matches($schemaText,'(?m)^pub const UPDATE_SCHEMA_VERSION: u32 = ([0-9]+);\r?$')
  if($schemaMatch.Count -ne 1 -or [int]$schemaMatch[0].Groups[1].Value -ne $case.schema){throw 'Pinned source SDK differs from the fixed migration profile; no build'}
  $report.sourceCommit=$manifest.sourceCommit
  $inputPath=Join-Path $work ($case.prefix+'-migration-input.json')
  Write-NewJson $inputPath @{root=(Join-Path $work ($case.prefix+'-migration-case'));reference=$reference;proof=(Join-Path $ReportDirectory $case.proof)}
  [IO.File]::Copy($ReferenceFile,(Join-Path $ReportDirectory 'reference.json'),$false)
  $env:CARGO_TARGET_DIR=Join-Path $ValidationCacheDirectory 'test-build-cache'
  Assert-WorkflowCachePath $env:CARGO_TARGET_DIR
  if(-not (Test-Path -LiteralPath $env:CARGO_TARGET_DIR -PathType Container)){throw 'Retained native cache required'}
  $lease=[IO.File]::Open((Join-Path $env:CARGO_TARGET_DIR '.winbridge-build-cache.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  $env:CARGO_INCREMENTAL='0';[Environment]::SetEnvironmentVariable($case.environment,$inputPath,[EnvironmentVariableTarget]::Process)
  $env:TEMP=Join-Path $work 'migration-temp';$env:TMP=$env:TEMP
  New-Item -ItemType Directory -Path $env:TEMP|Out-Null
  [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
  Set-Location -LiteralPath $source
  $arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--test',$case.target,'--locked','--offline','--','--exact',$case.test,'--ignored','--test-threads=1','--nocapture')
  $started=[DateTimeOffset]::UtcNow
  & cargo @arguments 2>&1|Tee-Object -FilePath (Join-Path $ReportDirectory 'migration.log')|Out-Host
  $exitCode=$LASTEXITCODE
  $report.command=@{executable='cargo';arguments=$arguments;exitCode=$exitCode;startedAt=$started.ToString('o');elapsedMs=[long]([DateTimeOffset]::UtcNow-$started).TotalMilliseconds;log='migration.log'}
  if($exitCode -ne 0){throw "Actual journal migration failed ($exitCode); retained source preserved"}
  Assert-SessionFile (Join-Path $ReportDirectory $case.proof)
  if(-not (Test-Path -LiteralPath (Join-Path $ReportDirectory $case.proof) -PathType Leaf)){throw 'Selected test produced no proof'}
  $report.passed=$true
} catch {$report.error=$_.Exception.Message;throw}
finally {
  Set-Location -LiteralPath $location
  if($lease){$lease.Dispose()}
  $env:CARGO_TARGET_DIR=$oldTarget;$env:CARGO_INCREMENTAL=$oldIncremental;[Environment]::SetEnvironmentVariable($case.environment,$oldInput,[EnvironmentVariableTarget]::Process)
  $env:TEMP=$oldTemp;$env:TMP=$oldTmp
  [Console]::OutputEncoding=$oldEncoding;$OutputEncoding=$oldPipelineEncoding
  $report.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  Write-NewJson (Join-Path $ReportDirectory 'report.json') $report
  $files=@(Get-ChildItem -LiteralPath $ReportDirectory -File|Sort-Object Name|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $ReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
