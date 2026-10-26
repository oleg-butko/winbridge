<# Probe the exact native Cargo feature variants twice; no tests or UI are executed. #>
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [Parameter(Mandatory)][string]$ManifestSha256,
    [Parameter(Mandatory)][string]$CacheDirectory,
    [Parameter(Mandatory)][string]$WorkDirectory,
    [switch]$InventoryOnly
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
$source = [IO.Path]::GetFullPath($SourceDirectory)
$cache = [IO.Path]::GetFullPath($CacheDirectory)
$work = [IO.Path]::GetFullPath($WorkDirectory)
Assert-SessionDirectory $source
Assert-SessionDirectory $work
Assert-WorkflowCachePath $cache
if ($env:OS -cne 'Windows_NT' -or (Split-Path $work -Parent) -ine (Split-Path $source -Parent) -or
    (Split-Path $source -Leaf) -cne 'winbridge' -or (Split-Path (Split-Path $source -Parent) -Leaf) -notmatch '^\d{8}-\d{6}-[a-f0-9]{8}$' -or
    (Split-Path $cache -Leaf) -cne 'test-build-cache' -or
    (Split-Path (Split-Path $cache -Parent) -Parent) -ine (Split-Path (Split-Path $source -Parent) -Parent) -or
    (Split-Path $work -Leaf) -notmatch '^build-matrix-\d{8}-\d{6}-[a-f0-9]{8}$' -or
    (Test-Path -LiteralPath $work) -or -not (Test-Path -LiteralPath $cache -PathType Container)) { throw 'Expected new isolated sibling work directory and retained local cache' }
$tainted = @(Get-ChildItem Env: | Where-Object { $_.Name -match '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_PROFILE_.*|WINBRIDGE_.*)$' })
if ($tainted.Count) { throw ('Refusing environment overrides: '+($tainted.Name -join ', ')) }
if (@(Get-Process cargo -ErrorAction SilentlyContinue).Count) { throw 'Another Cargo process is active' }
$manifestPath = Join-Path $source 'winbridge-source-manifest.json'
Assert-SessionFile $manifestPath
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($manifest.dirty -isnot [bool] -or $manifest.dirty) { throw 'Clean immutable source required' }
if ($InventoryOnly) {
    if ((Get-FileHash -LiteralPath $manifestPath).Hash.ToLowerInvariant() -cne $ManifestSha256) { throw 'Manifest changed' }
    $names=@($manifest.files.path)+@('winbridge-source-manifest.json')
    $extra=@(Get-ChildItem -LiteralPath $source -Recurse -File -Force | ForEach-Object {
        $relative=[IO.Path]::GetRelativePath($source,$_.FullName).Replace('\','/')
        if ($relative -cnotin $names) { $relative }
    })
    @{kind='source_inventory';extraFiles=$extra;cargoInvoked=$false;freeBytes=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace} | ConvertTo-Json -Depth 5
    return
}
function Assert-Source {
    if ((Get-FileHash -LiteralPath $manifestPath).Hash.ToLowerInvariant() -cne $ManifestSha256) { throw 'Manifest changed' }
    $expected=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $generated=@()
    $allowedGenerated=@('apps/desktop/src-tauri/gen/schemas/acl-manifests.json','apps/desktop/src-tauri/gen/schemas/capabilities.json',
        'apps/desktop/src-tauri/gen/schemas/desktop-schema.json','apps/desktop/src-tauri/gen/schemas/windows-schema.json')
    [void]$expected.Add('winbridge-source-manifest.json')
    foreach ($file in $manifest.files) {
        if ($file.path -match '(^/|\\|:|(^|/)\.\.?(/|$))' -or -not $expected.Add($file.path)) { throw 'Unsafe/duplicate source path' }
        $path = Join-Path $source $file.path
        Assert-SessionFile $path
        if ((Get-Item -LiteralPath $path).Length -ne $file.bytes -or (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $file.sha256) { throw 'Source input changed' }
    }
    foreach ($item in Get-ChildItem -LiteralPath $source -Recurse -Force) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked source input' }
        if (-not $item.PSIsContainer) {
            $relative=[IO.Path]::GetRelativePath($source,$item.FullName).Replace('\','/')
            if (-not $expected.Contains($relative)) {
                if ($relative -cnotin $allowedGenerated) { throw ('Unmanifested source input: '+$relative) }
                $generated += [ordered]@{path=$relative;bytes=$item.Length;sha256=(Get-FileHash -LiteralPath $item.FullName).Hash.ToLowerInvariant()}
            }
        }
    }
    return @($generated|Sort-Object {$_.path})
}
$generatedBefore=@(Assert-Source)
$leasePath = Join-Path $cache '.winbridge-build-cache.lock'
Assert-SessionFile $leasePath
$lease = [IO.File]::Open($leasePath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$saved = @{}
foreach ($key in @('CARGO_INCREMENTAL','CARGO_TERM_COLOR','CARGO_LOG','TEMP','TMP')) { $saved[$key] = [Environment]::GetEnvironmentVariable($key) }
$oldLocation = Get-Location
$report = [ordered]@{schemaVersion=1;kind='cargo_build_matrix_probe';passed=$false;testsExecuted=$false;installedRuntimeChanged=$false;
    sourceCommit=$manifest.sourceCommit;sourceManifestSha256=$ManifestSha256;generatedTauriSchemas=$generatedBefore;commands=@();error=$null}
$matrix = @(
    @{key='update-base';arguments=@('test','-p','winbridge-update','--lib','--features','bundled-sqlite')},
    @{key='worker-validation';arguments=@('test','-p','winbridge-worker','--lib','--features','bundled-sqlite,update-validation')},
    @{key='update-validation';arguments=@('test','-p','winbridge-update','--lib','--features','bundled-sqlite,update-validation')},
    @{key='protocol-contract';arguments=@('test','-p','winbridge-protocol','--test','update_activation_writer_view')}
)
try {
    New-Item -ItemType Directory -Path $work | Out-Null
    $env:CARGO_INCREMENTAL='0'
    $env:CARGO_TERM_COLOR='never'
    $env:CARGO_LOG='cargo::core::compiler::fingerprint=info'
    $env:TEMP=Join-Path $work 'test-temp';$env:TMP=$env:TEMP
    New-Item -ItemType Directory -Path $env:TEMP | Out-Null
    Set-Location $source
    $report.toolchain = (& rustc.exe -vV | Out-String).Trim()
    for ($cycle=0; $cycle -lt 2; $cycle++) {
        foreach ($step in $matrix) {
            $drive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($cache))
            $freeBefore=$drive.AvailableFreeSpace
            if ($drive.DriveType -ne [IO.DriveType]::Fixed -or $freeBefore -lt 6GB) { throw 'Need local fixed disk with >=6GiB' }
            $name="cycle-$cycle-$($step.key)"
            $args=$step.arguments+@('--locked','--offline','--no-run','--message-format=json','--timings','--target-dir',$cache)
            Write-Host "WinBridge Cargo matrix: $name"
            $watch=[Diagnostics.Stopwatch]::StartNew()
            & cargo.exe @args 2>&1 | ForEach-Object { $_.ToString() } | Set-Content -LiteralPath (Join-Path $work "$name.log") -Encoding utf8
            $code=$LASTEXITCODE;$watch.Stop()
            $messages=@(Get-Content -LiteralPath (Join-Path $work "$name.log") | Where-Object {$_.StartsWith('{')} | ForEach-Object {ConvertFrom-Json -InputObject $_})
            $artifacts=@($messages|Where-Object {$_.reason -ceq 'compiler-artifact'})
            $compiled=@($artifacts|Where-Object {$_.fresh -eq $false})
            $report.commands+=@{key=$step.key;cycle=$cycle;arguments=$args;log="$name.log";exitCode=$code;elapsedMs=$watch.ElapsedMilliseconds;
                freshArtifacts=@($artifacts|Where-Object {$_.fresh -eq $true}).Count;compiled=$compiled;freeBytesBefore=$freeBefore;freeBytes=$drive.AvailableFreeSpace}
            if ($code -ne 0) { throw "Cargo matrix failed: $name" }
            Copy-Item -LiteralPath (Join-Path $cache 'cargo-timings/cargo-timing.html') -Destination (Join-Path $work "$name.html")
            Write-Host "WinBridge Cargo matrix: $name completed $($watch.ElapsedMilliseconds)ms, $($compiled.Count) compiled units"
        }
    }
    $generatedAfter=@(Assert-Source)
    if (($generatedBefore|ConvertTo-Json -Depth 5 -Compress) -cne ($generatedAfter|ConvertTo-Json -Depth 5 -Compress)) { throw 'Generated Tauri schemas changed during Cargo-only probe' }
    $report.passed=$true
} catch {
    $report.error=$_.ToString();Write-Warning $report.error
} finally {
    Set-Location $oldLocation
    foreach ($key in $saved.Keys) { [Environment]::SetEnvironmentVariable($key,$saved[$key]) }
    $lease.Dispose()
    if (Test-Path -LiteralPath $work -PathType Container) {
        Write-NewJson (Join-Path $work 'report.json') $report
        $files=@(Get-ChildItem -LiteralPath $work -File|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}})
        Write-NewJson (Join-Path $work 'report.ready.json') @{schemaVersion=1;files=$files}
    }
}
if (-not $report.passed) { throw 'Build matrix failed; preserve all files and reconcile' }
