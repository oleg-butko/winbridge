<#
.SYNOPSIS
Tests activation fixture paths and real Windows lease inspection. No product Main or UI.
.DESCRIPTION
Run through the installed worker in a fresh exported-source directory. Only compiler
caches are reused from pinned completed native groups. This does not install an update
or prove the same-child Main25 scenario. Source/state/reports stay per-run.
#>
param(
 [Parameter(Mandatory)][string]$SourceDirectory,
 [Parameter(Mandatory)][string]$SourceArchive,
 [Parameter(Mandatory)][string]$SourceArchiveSha256,
 [Parameter(Mandatory)][string]$SourceManifestSha256,
 [Parameter(Mandatory)][string]$CacheDirectory,
 [Parameter(Mandatory)][string]$CacheReceipt,
 [Parameter(Mandatory)][string]$CacheReceiptSha256,
 [Parameter(Mandatory)][string]$ReportDirectory
)
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$false
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/test-activation-runtime.ps1')
if($env:OS -cne 'Windows_NT'){throw 'Native Windows required'}
foreach($sha in @($SourceArchiveSha256,$SourceManifestSha256,$CacheReceiptSha256)){
 if($sha -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact source/cache hashes required'}
}
$source=Get-ActivationTestLocalPath $SourceDirectory
$cache=Get-ActivationTestLocalPath $CacheDirectory
$reportRoot=Get-ActivationTestLocalPath $ReportDirectory
if((Test-ActivationPathOverlap $source $cache) -or (Test-ActivationPathOverlap $source $reportRoot) -or
 (Test-ActivationPathOverlap $cache $reportRoot)){throw 'Separate source/cache/report directories required'}
if(Test-Path -LiteralPath $reportRoot){throw 'Fresh layout report directory required'}
foreach($path in @($SourceArchive,$CacheReceipt)){
 [void](Get-ActivationTestLocalPath ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($path))))
 Assert-SessionFile $path
}
if((Get-FileHash -LiteralPath $SourceArchive -Algorithm SHA256).Hash.ToLowerInvariant() -cne $SourceArchiveSha256 -or
 (Get-FileHash -LiteralPath $CacheReceipt -Algorithm SHA256).Hash.ToLowerInvariant() -cne $CacheReceiptSha256){throw 'Archive/cache receipt differs'}
$prior=Get-Content -Raw -LiteralPath $CacheReceipt|ConvertFrom-Json
$cacheGroups=@('update-source-policy-and-fsm','update-native-preflight','update-legacy-contracts','update-verifier-build')
if($prior.platform -cne 'windows' -or $prior.sourceDirty -isnot [bool] -or $prior.sourceDirty -or
 $prior.passed -isnot [bool] -or $prior.commands.Count -lt 4 -or
 (($prior.commands[0..3].name -join ',') -cne ($cacheGroups -join ',')) -or
 @($prior.commands|Where-Object {$_.exitCode -ne 0}).Count -or
 -not [IO.Path]::GetFullPath([string]$prior.buildCacheDirectory).Equals($cache,[StringComparison]::OrdinalIgnoreCase)){
 throw 'Original compiler cache owner with completed native groups required'
}
$manifestPath=Join-Path $source 'winbridge-source-manifest.json'
Assert-SessionFile $manifestPath
if((Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $SourceManifestSha256){throw 'Source manifest differs'}
$manifest=Get-Content -Raw -LiteralPath $manifestPath|ConvertFrom-Json
if($manifest.dirty -isnot [bool] -or $manifest.dirty){throw 'Clean source export required'}
foreach($f in $manifest.files){
 # Snapshot fixtures deliberately include Unicode names. Constrain path
 # structure, not the language of valid regular filenames.
 if($f.path -isnot [string] -or $f.path -notmatch '^[^/\\:\x00-\x1f]+(?:/[^/\\:\x00-\x1f]+)*$' -or
  $f.path -match '(^|/)\.\.?(/|$)'){throw "Unsafe source manifest path: $($f.path)"}
 $p=Join-Path $source $f.path;Assert-SessionFile $p
 if((Get-Item -LiteralPath $p).Length -ne $f.bytes -or (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() -cne $f.sha256){throw 'Source bytes differ'}
}
[IO.Directory]::CreateDirectory($reportRoot)|Out-Null
$report=[ordered]@{schemaVersion=1;kind='activation_fixture_layout_component';passed=$false;platform='windows';sourceCommit=$manifest.sourceCommit;sourceManifestSha256=$SourceManifestSha256;sourceArchiveSha256=$SourceArchiveSha256;cacheReceiptSha256=$CacheReceiptSha256;buildCacheDirectory=$cache;cacheReportPassed=$prior.passed;cacheOnly=$true;commands=@();runtimeApplied=$false;installedRuntimeApplied=$false;windowsOpened=$false;fullMain25=$false;error=$null;storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
$lease=$null;$oldTarget=$env:CARGO_TARGET_DIR;$oldIncremental=$env:CARGO_INCREMENTAL;$oldTemp=$env:TEMP;$oldTmp=$env:TMP
$oldLocation=Get-Location;$oldEncoding=[Console]::OutputEncoding;$oldPipelineEncoding=$OutputEncoding
try{
 if($report.storage.availableBytesBefore -lt 6GB){throw 'At least 6 GiB free required; no cleanup'}
 Assert-SessionFile (Join-Path $cache '.winbridge-build-cache.lock')
 $lease=[IO.File]::Open((Join-Path $cache '.winbridge-build-cache.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
 $env:CARGO_TARGET_DIR=$cache;$env:CARGO_INCREMENTAL='0'
 $env:TEMP=Join-Path (Split-Path $source -Parent) 'tmp-fixture-layout';$env:TMP=$env:TEMP
 Assert-SessionDirectory $env:TEMP
 if(Test-Path -LiteralPath $env:TEMP){throw 'Fresh test temp directory required'}
 [IO.Directory]::CreateDirectory($env:TEMP)|Out-Null
 [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
 Set-Location -LiteralPath $source
 $cargo=(Get-Command cargo.exe -ErrorAction Stop).Source
 $commands=@(
  @{name='fixture-layout';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--lib','--locked','--offline','update_control::validation::layout','--','--test-threads=1','--nocapture');source='apps/worker/src/update_control/validation/layout_tests.rs';marker='FIXTURE_LAYOUT_BUILD'},
  @{name='fixture-file-links';arguments=@('test','-p','winbridge-platform','--test','file_links','--locked','--offline','--','--test-threads=1','--nocapture');source='crates/platform/tests/file_links.rs';marker='FILE_LINKS_BUILD'}
 )
 foreach($command in $commands){
  $expected=[regex]::Matches([IO.File]::ReadAllText((Join-Path $source $command.source)),'(?m)^#\[test\]\r?\nfn ').Count
  if($expected -lt 1){throw 'Expected native tests missing'}
  $logName=$command.name+'.log';$log=Join-Path $reportRoot $logName
  $free=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  if($free -lt 6GB){throw 'Stop new compilation below 6 GiB; preserve Windows data'}
  Write-Output "Fixture component: $($command.name)"
  $timer=[Diagnostics.Stopwatch]::StartNew()
  $argv=[string[]]$command.arguments
  & $cargo @argv *> $log
  $code=$LASTEXITCODE
  $report.commands+=@{name=$command.name;executable=$cargo;arguments=$command.arguments;log=$logName;exitCode=$code;expectedTests=$expected;durationMs=$timer.ElapsedMilliseconds;availableBytesBefore=$free;availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}
  if($code -ne 0){throw "Fixture command failed: $($command.name); inspect retained log"}
  $text=[IO.File]::ReadAllText($log)
  $summary=[regex]::Matches($text,'test result: ok\. (\d+) passed; 0 failed; 0 ignored;')
  if($summary.Count -ne 1 -or [int]$summary[0].Groups[1].Value -ne $expected){throw 'Native test scope/count differs'}
  $builds=[regex]::Matches($text,('(?m)(?:^|\.\.\. )'+$command.marker+' (\{[^\r\n]*\})\s*$'))
  if($builds.Count -ne 1){throw 'Exactly one compiled source identity required'}
  $build=$builds[0].Groups[1].Value|ConvertFrom-Json
  if($build.sourceDirty -isnot [bool] -or $build.sourceDirty -or $build.sourceCommit -cne $manifest.sourceCommit -or $build.sourceManifestSha256 -cne $SourceManifestSha256){throw 'Compiled source identity differs'}
 }
 $report.passed=$true
}catch{$report.error=$_.Exception.Message;throw}
finally{
 Set-Location $oldLocation
 $env:CARGO_TARGET_DIR=$oldTarget;$env:CARGO_INCREMENTAL=$oldIncremental;$env:TEMP=$oldTemp;$env:TMP=$oldTmp
 [Console]::OutputEncoding=$oldEncoding;$OutputEncoding=$oldPipelineEncoding
 if($null -ne $lease){$lease.Dispose()}
 $report.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
 Write-NewJson (Join-Path $reportRoot 'report.json') $report
 $files=@(Get-ChildItem -LiteralPath $reportRoot -File|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
 Write-NewJson (Join-Path $reportRoot 'report.ready.json') @{schemaVersion=1;passed=$report.passed;files=$files}
}
