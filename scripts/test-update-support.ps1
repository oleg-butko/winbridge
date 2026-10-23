<#
.SYNOPSIS
Probe standard/validation updater support in isolated directories, without installation.
#>
param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReferenceFile,
  [Parameter(Mandatory)][string]$ReferenceSha256,
  [Parameter(Mandatory)][string]$ReportDirectory
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
foreach($p in @($SourceDirectory,$ValidationCacheDirectory,$ReportDirectory)){Assert-SessionDirectory $p}
foreach($sha in @($SourceArchiveSha256,$ReferenceSha256)){if($sha -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact lowercase SHA256 required'}}
$source=[IO.Path]::GetFullPath($SourceDirectory)
$root=Split-Path $source -Parent
$manifest=Get-Content -LiteralPath (Join-Path $source 'winbridge-source-manifest.json') -Raw | ConvertFrom-Json
$report=[ordered]@{schemaVersion=1;kind='updater_runtime_support_native_gate';platform='windows';passed=$false;sourceCommit=$manifest.sourceCommit;sourceArchiveSha256=$SourceArchiveSha256;commands=@();images=@{};referenceSha256=$ReferenceSha256;runtimeApplied=$false;installedRuntimeApplied=$false;windowsOpened=$false;error=$null;storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
$lease=$null
$oldTarget=$env:CARGO_TARGET_DIR;$oldIncremental=$env:CARGO_INCREMENTAL;$oldInput=$env:WINBRIDGE_SUPPORT_GATE_INPUT
$oldTemp=$env:TEMP;$oldTmp=$env:TMP;$oldLocation=Get-Location
$oldConsoleEncoding=[Console]::OutputEncoding;$oldPipelineEncoding=$OutputEncoding
try {
  if($env:OS -cne 'Windows_NT' -or $report.storage.availableBytesBefore -lt 6GB){throw 'Native Windows disk with at least 6 GiB required; no cleanup'}
  if($manifest.dirty -or -not $manifest.includesBuiltUi){throw 'Clean source export with built UI required'}
  foreach($f in $manifest.files){$p=Join-Path $source $f.path;Assert-SessionFile $p;if((Get-Item -LiteralPath $p).Length -ne $f.bytes -or (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() -cne $f.sha256){throw "Source manifest differs: $($f.path)"}}
  Assert-SessionFile $ReferenceFile
  if((Get-FileHash -LiteralPath $ReferenceFile -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ReferenceSha256){throw 'Reference receipt changed'}
  $reference=Get-Content -LiteralPath $ReferenceFile -Raw | ConvertFrom-Json
  if($reference.schemaVersion -ne 1 -or $reference.kind -cne 'retained_updater_support_reference'){throw 'Unsupported reference receipt'}
  Assert-SessionFile $reference.image.path
  if((Get-Item -LiteralPath $reference.image.path).Length -ne $reference.image.bytes -or (Get-FileHash -LiteralPath $reference.image.path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $reference.image.sha256){throw 'Retained old updater changed; preserve reference'}
  $env:CARGO_TARGET_DIR=Join-Path $ValidationCacheDirectory 'test-build-cache'
  Assert-WorkflowCachePath $env:CARGO_TARGET_DIR
  if(-not (Test-Path -LiteralPath $env:CARGO_TARGET_DIR -PathType Container)){throw 'Existing native cache required'}
  $lock=Join-Path $env:CARGO_TARGET_DIR '.winbridge-build-cache.lock';Assert-SessionFile $lock
  $lease=[IO.File]::Open($lock,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
  $env:CARGO_INCREMENTAL='0'
  $env:TEMP=Join-Path $root 'test-temp';$env:TMP=$env:TEMP
  New-Item -ItemType Directory -Path $env:TEMP | Out-Null
  Set-Location -LiteralPath $source
  function Invoke-SupportCargo {
    param([string]$Name,[string[]]$CargoArguments)
    Write-Output "Updater support gate: $Name; installed worker remains active"
    $log=$Name+'.log'
    & cargo.exe @CargoArguments 2>&1 | Tee-Object -FilePath (Join-Path $ReportDirectory $log)
    $code=$LASTEXITCODE
    $report.commands+=@{name=$Name;arguments=$CargoArguments;exitCode=$code;log=$log}
    if($code -ne 0){throw "Support command failed: $Name; see $log"}
  }
  foreach($flavor in @('standard','validation')){
    $features=if($flavor -ceq 'standard'){'bundled-sqlite'}else{'bundled-sqlite,update-validation'}
    Invoke-SupportCargo ($flavor+'-build') @('build','-p','winbridge-update','--features',$features,'--locked','--offline')
    $directory=Join-Path $root ('support-images/'+$flavor)
    New-Item -ItemType Directory -Path $directory | Out-Null
    $image=Join-Path $directory 'winbridge-update.exe'
    [IO.File]::Copy((Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-update.exe'),$image,$false)
    [IO.File]::WriteAllText((Join-Path $directory 'winbridge-update.local.json'),'invalid configuration: do not load',[Text.UTF8Encoding]::new($false))
    $report.images[$flavor]=@{path=$image;bytes=(Get-Item -LiteralPath $image).Length;sha256=(Get-FileHash -LiteralPath $image -Algorithm SHA256).Hash.ToLowerInvariant()}
    Invoke-SupportCargo ($flavor+'-cli') @('test','-p','winbridge-update','--features',$features,'--test','updater_support','--locked','--offline','--','--test-threads=1','--nocapture')
  }
  Invoke-SupportCargo 'support-binding' @('test','-p','winbridge-update','--features','bundled-sqlite,update-validation','--lib','updater_support::tests::probe_binding_rejects_stale_process_wrong_build_and_zero_contract_versions','--locked','--offline','--','--exact','--test-threads=1','--nocapture')
  $inputPath=Join-Path $root 'support-input.json'
  Write-NewJson $inputPath @{schemaVersion=1;standard=$report.images.standard.path;standardSha256=$report.images.standard.sha256;validation=$report.images.validation.path;validationSha256=$report.images.validation.sha256;oldImage=$reference.image;oldDescriptor=$reference.descriptor;proof=(Join-Path $ReportDirectory 'updater-support-proof.json')}
  $env:WINBRIDGE_SUPPORT_GATE_INPUT=$inputPath
  Invoke-SupportCargo 'support-actual' @('test','-p','winbridge-update','--features','bundled-sqlite,update-validation','--test','updater_support_native','--locked','--offline','--','--exact','actual_support_queries_are_image_bound_and_old_marker_is_not_executed','--ignored','--test-threads=1','--nocapture')
  foreach($name in @('standard-cli','validation-cli','support-binding','support-actual')){
    $minimum=if($name -like '*-cli'){3}else{1}
    $counts=[regex]::Matches((Get-Content -LiteralPath (Join-Path $ReportDirectory ($name+'.log')) -Raw),'test result: ok\. ([0-9]+) passed; 0 failed;')
    $passed=0;foreach($count in $counts){$passed+=[int]$count.Groups[1].Value}
    if($passed -lt $minimum){throw "Expected support tests not selected: $name"}
  }
  $report.passed=$true
} catch {$report.error=$_.Exception.Message;Write-Output ('Updater support gate failed: '+$report.error)} finally {
  $report.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  if($lease){$lease.Dispose()}
  $env:CARGO_TARGET_DIR=$oldTarget;$env:CARGO_INCREMENTAL=$oldIncremental;$env:WINBRIDGE_SUPPORT_GATE_INPUT=$oldInput;$env:TEMP=$oldTemp;$env:TMP=$oldTmp
  [Console]::OutputEncoding=$oldConsoleEncoding;$OutputEncoding=$oldPipelineEncoding
  Set-Location -LiteralPath $oldLocation
  Write-NewJson (Join-Path $ReportDirectory 'report.json') $report
  $files=@(Get-ChildItem -LiteralPath $ReportDirectory -File | Sort-Object Name | ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $ReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
if(-not $report.passed){exit 1}
