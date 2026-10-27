<#
.SYNOPSIS
Verify actual retained updater19 ->20 on a consistent copy; never install or launch a worker.
#>
param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$ReferenceFile,
  [Parameter(Mandatory)][string]$ReferenceSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReportDirectory
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
foreach($path in @($SourceDirectory,$ValidationCacheDirectory,$ReportDirectory)){Assert-SessionDirectory $path}
foreach($sha in @($SourceArchiveSha256,$ReferenceSha256)){if($sha -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact archive/reference SHA required'}}
$source=[IO.Path]::GetFullPath($SourceDirectory)
$work=Split-Path $source -Parent
if(Test-Path -LiteralPath $ReportDirectory){throw 'Existing report preserved'}
New-Item -ItemType Directory -Path $ReportDirectory|Out-Null
$report=[ordered]@{schemaVersion=1;kind='activation_service_migration_native_gate';platform='windows';passed=$false;sourceArchiveSha256=$SourceArchiveSha256;referenceSha256=$ReferenceSha256;runtimeApplied=$false;installedRuntimeApplied=$false;windowsOpened=$false;error=$null;storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
$lease=$null;$location=Get-Location
$oldTarget=$env:CARGO_TARGET_DIR;$oldIncremental=$env:CARGO_INCREMENTAL;$oldInput=$env:WINBRIDGE_SERVICE_MIGRATION_INPUT
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
  if($reference.schemaVersion -ne 1 -or $reference.kind -cne 'activation_schema19_reference'){throw 'Actual19 reference required'}
  $report.sourceCommit=$manifest.sourceCommit
  $inputPath=Join-Path $work 'service-migration-input.json'
  Write-NewJson $inputPath @{root=(Join-Path $work 'service-migration-case');reference=$reference;proof=(Join-Path $ReportDirectory 'activation-service-migration-proof.json')}
  [IO.File]::Copy($ReferenceFile,(Join-Path $ReportDirectory 'reference.json'),$false)
  $env:CARGO_TARGET_DIR=Join-Path $ValidationCacheDirectory 'test-build-cache'
  Assert-WorkflowCachePath $env:CARGO_TARGET_DIR
  if(-not (Test-Path -LiteralPath $env:CARGO_TARGET_DIR -PathType Container)){throw 'Retained native cache required'}
  $lease=[IO.File]::Open((Join-Path $env:CARGO_TARGET_DIR '.winbridge-build-cache.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  $env:CARGO_INCREMENTAL='0';$env:WINBRIDGE_SERVICE_MIGRATION_INPUT=$inputPath
  $env:TEMP=Join-Path $work 'migration-temp';$env:TMP=$env:TEMP
  New-Item -ItemType Directory -Path $env:TEMP|Out-Null
  [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
  Set-Location -LiteralPath $source
  $arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--test','activation_service_migration','--locked','--offline','--','--exact','actual_nineteen_writer_history_survives_twenty_and_old_nineteen_refuses','--ignored','--test-threads=1','--nocapture')
  $started=[DateTimeOffset]::UtcNow
  & cargo @arguments 2>&1|Tee-Object -FilePath (Join-Path $ReportDirectory 'migration.log')|Out-Host
  $exitCode=$LASTEXITCODE
  $report.command=@{executable='cargo';arguments=$arguments;exitCode=$exitCode;startedAt=$started.ToString('o');elapsedMs=[long]([DateTimeOffset]::UtcNow-$started).TotalMilliseconds;log='migration.log'}
  if($exitCode -ne 0){throw "Actual19 migration failed ($exitCode); retained source preserved"}
  Assert-SessionFile (Join-Path $ReportDirectory 'activation-service-migration-proof.json')
  if(-not (Test-Path -LiteralPath (Join-Path $ReportDirectory 'activation-service-migration-proof.json') -PathType Leaf)){throw 'Selected test produced no proof'}
  $report.passed=$true
} catch {$report.error=$_.Exception.Message;throw}
finally {
  Set-Location -LiteralPath $location
  if($lease){$lease.Dispose()}
  $env:CARGO_TARGET_DIR=$oldTarget;$env:CARGO_INCREMENTAL=$oldIncremental;$env:WINBRIDGE_SERVICE_MIGRATION_INPUT=$oldInput
  $env:TEMP=$oldTemp;$env:TMP=$oldTmp
  [Console]::OutputEncoding=$oldEncoding;$OutputEncoding=$oldPipelineEncoding
  $report.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  Write-NewJson (Join-Path $ReportDirectory 'report.json') $report
  $files=@(Get-ChildItem -LiteralPath $ReportDirectory -File|Sort-Object Name|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $ReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
