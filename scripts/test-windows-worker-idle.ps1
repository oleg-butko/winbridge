<# Isolated worker regression and CPU experiment; installed runtime is never changed. #>
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [Parameter(Mandatory)][string]$ManifestSha256,
    [Parameter(Mandatory)][string]$CacheDirectory,
    [Parameter(Mandatory)][string]$WorkDirectory,
    [Parameter(Mandatory)][string]$SourceArchivePath,
    [Parameter(Mandatory)][string]$ArchiveSha256,
    [Parameter(Mandatory)][string]$BaselineExecutable,
    [Parameter(Mandatory)][string]$BaselineSha256
)
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$false
$ProgressPreference='SilentlyContinue'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
$work=[IO.Path]::GetFullPath($WorkDirectory)
$cache=[IO.Path]::GetFullPath($CacheDirectory)
Assert-SessionDirectory $work
Assert-WorkflowCachePath $cache
if ($env:OS -cne 'Windows_NT' -or (Test-Path -LiteralPath $work) -or
    (Split-Path $work -Leaf) -notmatch '^tmp-worker-idle-\d{8}-\d{6}-[a-f0-9]{8}$' -or
    (Split-Path $work -Parent) -ine (Split-Path ([IO.Path]::GetFullPath($SourceDirectory)) -Parent) -or
    (Split-Path $cache -Leaf) -cne 'test-build-cache' -or -not (Test-Path -LiteralPath $cache -PathType Container)) {
    throw 'Requires NEW isolated report directory and retained native compiler cache'
}
$tainted=@(Get-ChildItem Env: | Where-Object {$_.Name -match '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_PROFILE_.*|WINBRIDGE_.*)$'})
if ($tainted.Count -or @(Get-Process cargo -ErrorAction SilentlyContinue).Count) { throw 'Requires unmodified environment and no active Cargo' }
foreach ($file in @($SourceArchivePath,$BaselineExecutable)) {Assert-SessionFile $file}
if ((Get-FileHash -LiteralPath $SourceArchivePath).Hash.ToLowerInvariant() -cne $ArchiveSha256 -or
    (Get-FileHash -LiteralPath $BaselineExecutable).Hash.ToLowerInvariant() -cne $BaselineSha256) {throw 'Pinned input differs'}
$leasePath=Join-Path $cache '.winbridge-build-cache.lock'
Assert-SessionFile $leasePath
$lease=[IO.File]::Open($leasePath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$saved=@{}
foreach ($name in @('CARGO_INCREMENTAL','CARGO_TERM_COLOR','TEMP','TMP','WINBRIDGE_IDLE_BASELINE_WORKER')) {$saved[$name]=[Environment]::GetEnvironmentVariable($name)}
$oldLocation=Get-Location
$report=[ordered]@{schemaVersion=1;kind='worker_idle_cpu';passed=$false;installedRuntimeChanged=$false;
    windowsOpened=$false;archiveSha256=$ArchiveSha256;manifestSha256=$ManifestSha256;
    baselineSha256=$BaselineSha256;commands=@();cpu=@();error=$null}
$source=Join-Path $work 'source/winbridge'
function Assert-Source {
    $path=Join-Path $source 'winbridge-source-manifest.json'
    if ((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $ManifestSha256) {throw 'Manifest changed'}
    $manifest=Get-Content -Raw -LiteralPath $path|ConvertFrom-Json
    if ($manifest.dirty -isnot [bool] -or $manifest.dirty) {throw 'Clean candidate source required'}
    $names=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    [void]$names.Add('winbridge-source-manifest.json')
    foreach ($file in $manifest.files) {
        if ($file.path -match '(^/|\\|:|(^|/)\.\.?(/|$))' -or -not $names.Add($file.path)) {throw 'Unsafe source name'}
        $item=Join-Path $source $file.path
        Assert-SessionFile $item
        if ((Get-Item -LiteralPath $item).Length -ne $file.bytes -or
            (Get-FileHash -LiteralPath $item).Hash.ToLowerInvariant() -cne $file.sha256) {throw 'Source changed'}
    }
    foreach ($item in Get-ChildItem -LiteralPath $source -Recurse -Force) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {throw 'Linked source'}
        if (-not $item.PSIsContainer -and -not $names.Contains([IO.Path]::GetRelativePath($source,$item.FullName).Replace('\','/'))) {throw 'Unmanifested source'}
    }
    return $manifest.sourceCommit
}
function Invoke-Check([string]$Name,[string[]]$CargoArguments) {
    $drive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($cache))
    $free=$drive.AvailableFreeSpace
    if ($drive.DriveType -ne [IO.DriveType]::Fixed -or $free -lt 6GB) {throw 'Need local fixed disk >=6GiB'}
    $arguments=@('test','-p','winbridge-worker','--locked','--offline','--features','bundled-sqlite','--target-dir',$cache)+$CargoArguments
    Write-Host "WinBridge isolated worker: $Name"
    $watch=[Diagnostics.Stopwatch]::StartNew()
    & cargo.exe @arguments 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $work "$Name.log") -Encoding utf8
    $code=$LASTEXITCODE;$watch.Stop()
    $report.commands+=@([ordered]@{name=$Name;arguments=$arguments;exitCode=$code;wallMs=$watch.ElapsedMilliseconds;freeBytesBefore=$free})
    if ($code -ne 0) {throw "Worker check failed: $Name ($code)"}
}
try {
    New-Item -ItemType Directory -Path $work|Out-Null
    $zip=Join-Path $work 'source.zip'
    [IO.File]::Copy($SourceArchivePath,$zip,$false)
    if ((Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant() -cne $ArchiveSha256) {throw 'Copied ZIP differs'}
    Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $work 'source')
    $report.sourceCommit=Assert-Source
    $report.rootCargoSha256=(Get-FileHash -LiteralPath (Join-Path $source 'Cargo.toml')).Hash.ToLowerInvariant()
    if ((Get-FileHash -LiteralPath (Join-Path $SourceDirectory 'Cargo.toml')).Hash.ToLowerInvariant() -cne $report.rootCargoSha256) {throw 'Baseline/candidate Cargo profiles differ'}
    $baseline=Join-Path $work 'baseline-worker.exe'
    [IO.File]::Copy($BaselineExecutable,$baseline,$false)
    if ((Get-FileHash -LiteralPath $baseline).Hash.ToLowerInvariant() -cne $BaselineSha256) {throw 'Copied baseline differs'}
    $env:CARGO_INCREMENTAL='0';$env:CARGO_TERM_COLOR='never'
    $env:TEMP=Join-Path $work 'tmp-test-temp';$env:TMP=$env:TEMP
    New-Item -ItemType Directory -Path $env:TEMP|Out-Null
    Set-Location $source
    $report.toolchain=(& rustc.exe -vV|Out-String).Trim()
    Invoke-Check 'index-and-states' @('--lib','store::idle_tests','--','--nocapture')
    Invoke-Check 'history-and-live-task' @('--test','idle_scheduler','--','--nocapture')
    Invoke-Check 'scheduler-regressions' @('--test','work_scheduling','--test','resource_scheduling','--test','workflows','--test','queue_control','--test','task_fsm','--test','leases')
    $env:WINBRIDGE_IDLE_BASELINE_WORKER=$baseline
    Invoke-Check 'idle-cpu' @('--test','idle_scheduler','native_idle_cpu_baseline_and_candidate','--','--ignored','--nocapture')
    $report.cpu=@(Get-Content -LiteralPath (Join-Path $work 'idle-cpu.log')|Where-Object {$_.StartsWith('IDLE_CPU ')}|ForEach-Object {$_.Substring(9)|ConvertFrom-Json})
    if ($report.cpu.Count -ne 2 -or $report.cpu[0].variant -cne 'baseline' -or $report.cpu[1].variant -cne 'candidate' -or
        $report.cpu[1].percentOfOneCore -ge $report.cpu[0].percentOfOneCore*0.5) {throw 'CPU acceptance bound failed'}
    $report.candidateSha256=(Get-FileHash -LiteralPath (Join-Path $cache 'debug/winbridge-worker.exe')).Hash.ToLowerInvariant()
    if ((Get-FileHash -LiteralPath $baseline).Hash.ToLowerInvariant() -cne $BaselineSha256 -or
        (Get-FileHash -LiteralPath $BaselineExecutable).Hash.ToLowerInvariant() -cne $BaselineSha256) {throw 'Baseline changed'}
    if ((Assert-Source) -cne $report.sourceCommit) {throw 'Source identity changed'}
    $report.passed=$true
} catch {$report.error=$_.Exception.Message;Write-Host "Worker idle probe failed: $($report.error)"}
finally {
    Set-Location $oldLocation
    foreach ($name in $saved.Keys) {[Environment]::SetEnvironmentVariable($name,$saved[$name])}
    $lease.Dispose()
    if (Test-Path -LiteralPath $work) {
        $report.freeBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($cache)).AvailableFreeSpace
        $report|ConvertTo-Json -Depth 12|Set-Content -LiteralPath (Join-Path $work 'report.json') -Encoding utf8
        $files=@(Get-ChildItem -LiteralPath $work -File|Where-Object {$_.Name -eq 'report.json' -or $_.Extension -eq '.log'}|ForEach-Object {
            [ordered]@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}
        })
        @{schemaVersion=1;files=$files}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $work 'report.ready.json') -Encoding utf8
    }
}
if (-not $report.passed) {exit 1}
