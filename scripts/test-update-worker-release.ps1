<#
.SYNOPSIS
Run the same-Store release and lifetime/config-pin unit groups on Windows.
Synthetic runtime fixtures: no V8 end-to-end proof, worker install or UI launch.
#>
param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReportDirectory
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
foreach($path in @($SourceDirectory,$ValidationCacheDirectory,$ReportDirectory)){Assert-SessionDirectory $path}
if($SourceArchiveSha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact source archive SHA required'}
if(Test-Path -LiteralPath $ReportDirectory){throw 'Existing report preserved'}
New-Item -ItemType Directory -Path $ReportDirectory|Out-Null
$source=[IO.Path]::GetFullPath($SourceDirectory)
$work=Split-Path $source -Parent
$report=[ordered]@{schemaVersion=1;kind='worker_finalization_release_unit_native_gate';platform='windows';passed=$false;sourceArchiveSha256=$SourceArchiveSha256;runtimeApplied=$false;installedRuntimeApplied=$false;windowsOpened=$false;physicalV8Proved=$false;error=$null;commands=@();storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
$lease=$null;$location=Get-Location
$oldTarget=$env:CARGO_TARGET_DIR;$oldIncremental=$env:CARGO_INCREMENTAL
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
  foreach($pin in @(@{path='apps/worker/src/store.rs';name='JOURNAL_SCHEMA_VERSION';value=19},@{path='apps/update/src/journal.rs';name='UPDATE_SCHEMA_VERSION';value=23})){
    $schemaMatches=[regex]::Matches([IO.File]::ReadAllText((Join-Path $source $pin.path)),("(?m)^pub const "+$pin.name+": u32 = ([0-9]+);\r?$"))
    if($schemaMatches.Count -ne 1 -or [int]$schemaMatches[0].Groups[1].Value -ne $pin.value){throw 'Fixed schema profile differs; no build'}
  }
  $report.sourceCommit=$manifest.sourceCommit
  $report.sourceManifestSha256=(Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
  $env:CARGO_TARGET_DIR=Join-Path $ValidationCacheDirectory 'test-build-cache'
  Assert-WorkflowCachePath $env:CARGO_TARGET_DIR
  if(-not (Test-Path -LiteralPath $env:CARGO_TARGET_DIR -PathType Container)){throw 'Retained native build cache required'}
  $lease=[IO.File]::Open((Join-Path $env:CARGO_TARGET_DIR '.winbridge-build-cache.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  $env:CARGO_INCREMENTAL='0';$env:TEMP=Join-Path $work 'worker-release-temp';$env:TMP=$env:TEMP
  New-Item -ItemType Directory -Path $env:TEMP|Out-Null
  [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
  Set-Location -LiteralPath $source
  $arguments=@('test','-p','winbridge-worker','--lib','--locked','--offline','--features','bundled-sqlite,update-validation','--no-run','--message-format=json-render-diagnostics')
  $started=[DateTimeOffset]::UtcNow
  & cargo @arguments 1>(Join-Path $ReportDirectory 'build.jsonl') 2>(Join-Path $ReportDirectory 'build-stderr.log')
  $code=$LASTEXITCODE
  $report.commands+=@{kind='build';executable='cargo';arguments=$arguments;exitCode=$code;elapsedMs=[long]([DateTimeOffset]::UtcNow-$started).TotalMilliseconds;stdout='build.jsonl';stderr='build-stderr.log'}
  if($code -ne 0){throw "Worker library build failed ($code); see build logs"}
  $artifacts=@(Get-Content -LiteralPath (Join-Path $ReportDirectory 'build.jsonl')|ForEach-Object {
    $m=$_|ConvertFrom-Json
    if($m.reason -ceq 'compiler-artifact' -and $m.target.name -ceq 'winbridge_worker' -and $m.profile.test -and $null -ne $m.executable){$m.executable}
  })
  if($artifacts.Count -ne 1){throw 'One actual Cargo worker test artifact required'}
  $exe=[IO.Path]::GetFullPath($artifacts[0]);Assert-SessionFile $exe
  if(-not $exe.StartsWith(([IO.Path]::GetFullPath($env:CARGO_TARGET_DIR).TrimEnd('\')+'\'),[StringComparison]::OrdinalIgnoreCase)){throw 'Test artifact escaped owned cache'}
  $exeSha=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
  $report.testArtifact=@{path=$exe;sha256=$exeSha;bytes=(Get-Item -LiteralPath $exe).Length}
  foreach($case in @(@{filter='store::activation_finalization::tests::';count=6;log='sql-release.log'},@{filter='local_service::lifetime::tests::';count=10;log='lifetime.log'})){
    $arguments=@($case.filter,'--test-threads=1','--nocapture')
    $started=[DateTimeOffset]::UtcNow
    & $exe @arguments 2>&1|Tee-Object -FilePath (Join-Path $ReportDirectory $case.log)|Out-Host
    $code=$LASTEXITCODE
    $report.commands+=@{kind='test';executable=$exe;arguments=$arguments;expectedTests=$case.count;exitCode=$code;elapsedMs=[long]([DateTimeOffset]::UtcNow-$started).TotalMilliseconds;log=$case.log}
    $summary=Get-Content -LiteralPath (Join-Path $ReportDirectory $case.log) -Raw
    if($code -ne 0 -or $summary -cnotmatch ("test result: ok\. "+$case.count+" passed; 0 failed; 0 ignored;")){throw "Fixed native group failed or did not execute expected tests: $($case.filter)"}
  }
  if((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant() -cne $exeSha){throw 'Native artifact changed across execution'}
  $report.passed=$true
} catch {$report.error=$_.Exception.Message;throw}
finally {
  Set-Location -LiteralPath $location
  if($lease){$lease.Dispose()}
  $env:CARGO_TARGET_DIR=$oldTarget;$env:CARGO_INCREMENTAL=$oldIncremental
  $env:TEMP=$oldTemp;$env:TMP=$oldTmp
  [Console]::OutputEncoding=$oldEncoding;$OutputEncoding=$oldPipelineEncoding
  $report.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  Write-NewJson (Join-Path $ReportDirectory 'report.json') $report
  $files=@(Get-ChildItem -LiteralPath $ReportDirectory -File|Sort-Object Name|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $ReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
