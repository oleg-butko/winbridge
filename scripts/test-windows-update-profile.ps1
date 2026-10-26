<#
.SYNOPSIS
Plan-only by default. -Execute benchmarks two test profiles in an isolated source export.
.DESCRIPTION
Needs only the installed worker, no desktop or test windows. Never installs updater.
SourceDirectory is a clean exported winbridge directory; WorkDirectory is its NEW sibling.
Retains all logs/binaries; uses the normal native gate's exclusive Cargo cache lease.
#>
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$ManifestSha256,
    [Parameter(Mandatory)][string]$WorkDirectory,
    [Parameter(Mandatory)][string]$CacheDirectory,
    [switch]$Execute
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference = $false
$spec = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'experiments/update-profile-v1.json') | ConvertFrom-Json
function Get-BuildArguments([string]$Variant) {
    $args = @()
    foreach ($profile in @('dev', 'test')) {
        foreach ($setting in @('opt-level=0', 'debug=2', 'debug-assertions=true', 'overflow-checks=true', 'incremental=false')) {
            $args += @('--config', "profile.$profile.$setting")
        }
        foreach ($package in $spec.packages) {
            $args += @('--config', "profile.$profile.package.$package.opt-level=$($spec.variants.$Variant)")
            foreach ($setting in @('debug=2', 'debug-assertions=true', 'overflow-checks=true', 'incremental=false')) {
                $args += @('--config', "profile.$profile.package.$package.$setting")
            }
        }
    }
    return $args + @('test', '-p', 'winbridge-update', '--lib', '--features', ($spec.features -join ','),
        '--locked', '--offline', '--no-run', '--message-format=json', '--timings', '--target-dir', $CacheDirectory)
}
if (-not $Execute) {
    [ordered]@{ kind='update_profile_plan'; execute=$false; installedRuntimeChanged=$false;
        spec=$spec; source=$SourceDirectory; manifestSha256=$ManifestSha256; work=$WorkDirectory;
        buildA=@(Get-BuildArguments 'A'); buildB=@(Get-BuildArguments 'B');
        note='No builds or tests started. Read docs/plans/run-speed/README.md before -Execute.'
    } | ConvertTo-Json -Depth 12
    return
}
if ($env:OS -ne 'Windows_NT' -or $PSVersionTable.PSVersion.Major -lt 7) { throw 'Requires Windows PowerShell 7' }
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
$source = [IO.Path]::GetFullPath($SourceDirectory)
$work = [IO.Path]::GetFullPath($WorkDirectory)
$CacheDirectory = [IO.Path]::GetFullPath($CacheDirectory)
$runRoot = Split-Path $source -Parent
$validationRoot = Split-Path $runRoot -Parent
if ((Split-Path $source -Leaf) -cne 'winbridge' -or (Split-Path $runRoot -Leaf) -notmatch '^\d{8}-\d{6}-[a-f0-9]{8}$' -or
    (Split-Path $validationRoot -Leaf) -cne 'workflow-validation' -or
    (Split-Path $work -Parent) -ine $runRoot -or (Split-Path $work -Leaf) -notmatch '^profile-v1-\d{8}-\d{6}-[a-f0-9]{8}$' -or
    (Split-Path $CacheDirectory -Leaf) -cne 'test-build-cache' -or
    (Split-Path (Split-Path $CacheDirectory -Parent) -Parent) -ine $validationRoot) { throw 'Expected isolated source/work and retained workflow-validation cache' }
Assert-SessionDirectory $source
Assert-SessionDirectory $work
Assert-WorkflowCachePath $CacheDirectory
if ((Test-Path -LiteralPath $work) -or -not (Test-Path -LiteralPath $CacheDirectory -PathType Container)) { throw 'Work must be NEW; retained cache must exist' }
$tainted = @(Get-ChildItem Env: | Where-Object {
    $_.Name -match '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_PROFILE_.*|WINBRIDGE_.*)$'
})
if ($tainted.Count) { throw ('Refusing experiment environment overrides: ' + ($tainted.Name -join ', ')) }
if (@(Get-Process cargo -ErrorAction SilentlyContinue).Count) { throw 'Another Cargo process is active; do not compete with it' }
$manifestPath = Join-Path $source 'winbridge-source-manifest.json'
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($manifest.dirty -isnot [bool] -or $manifest.dirty -or $manifest.sourceCommit -notmatch '^[a-f0-9]{40,64}$') { throw 'Requires clean exported source' }
function Assert-Source {
    if ((Get-FileHash -LiteralPath $manifestPath).Hash.ToLowerInvariant() -cne $ManifestSha256) { throw 'Manifest changed' }
    $expected = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    [void]$expected.Add('winbridge-source-manifest.json')
    foreach ($file in $manifest.files) {
        if ($file.path -match '(^/|\\|:|(^|/)\.\.?(/|$))' -or -not $expected.Add($file.path)) { throw 'Unsafe/duplicate source path' }
        $path = Join-Path $source $file.path
        Assert-SessionDirectory (Split-Path $path -Parent)
        Assert-SessionFile $path
        if ((Get-Item -LiteralPath $path).Length -ne $file.bytes -or (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $file.sha256) { throw "Source changed: $($file.path)" }
    }
    foreach ($item in Get-ChildItem -LiteralPath $source -Force -Recurse) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked source input' }
        if (-not $item.PSIsContainer -and -not $expected.Contains([IO.Path]::GetRelativePath($source, $item.FullName).Replace('\','/'))) { throw 'Unmanifested source input' }
    }
}
function Get-FreeSpace {
    $measurements = @()
    foreach ($path in @($source, $work, $CacheDirectory)) {
        $drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($path))
        $free = $drive.AvailableFreeSpace
        if ($drive.DriveType -ne [IO.DriveType]::Fixed -or $free -lt 6GB) { throw "Needs local fixed disk with >=6GiB: $path" }
        $measurements += @{ path=$path; freeBytes=$free; measuredAt=[DateTime]::UtcNow.ToString('o') }
    }
    return $measurements
}
Assert-Source
$initialSpace = @(Get-FreeSpace)
$leasePath = Join-Path $CacheDirectory '.winbridge-build-cache.lock'
Assert-SessionFile $leasePath
$lease = [IO.File]::Open($leasePath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
$pins = @()
$oldLocation = Get-Location
$savedEnv = @{}
foreach ($name in @('CARGO_INCREMENTAL','CARGO_TERM_COLOR','TEMP','TMP')) { $savedEnv[$name] = [Environment]::GetEnvironmentVariable($name) }
$report = [ordered]@{ schemaVersion=1; kind='update_profile_pilot'; passed=$false; installedRuntimeChanged=$false;
    sourceCommit=$manifest.sourceCommit; sourceDirty=$false; sourceManifestSha256=$ManifestSha256;
    specSha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'experiments/update-profile-v1.json')).Hash.ToLowerInvariant();
    platform='windows'; startedAt=[DateTime]::UtcNow.ToString('o'); initialSpace=$initialSpace;
    builds=@(); binaries=@(); commands=@(); error=$null }
function Invoke-Logged([string]$Name, [string]$Executable, [string[]]$Arguments) {
    $log = "$Name.log"
    Write-Host "WinBridge profile pilot: $Name"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    & $Executable @Arguments 2>&1 | ForEach-Object { $_.ToString() } | Set-Content -LiteralPath (Join-Path $work $log) -Encoding utf8
    $code = $LASTEXITCODE
    $watch.Stop()
    $entry = [ordered]@{ name=$Name; executable=$Executable; arguments=$Arguments; log=$log; exitCode=$code; elapsedMs=$watch.ElapsedMilliseconds }
    $report.commands += $entry
    if ($code -ne 0) { throw "Command failed ($code): $Name; see $log" }
    return $entry
}
try {
    New-Item -ItemType Directory -Path $work | Out-Null
    $env:CARGO_INCREMENTAL = '0'
    $env:CARGO_TERM_COLOR = 'never'
    $env:TEMP = Join-Path $work 'test-temp'
    $env:TMP = $env:TEMP
    New-Item -ItemType Directory -Path $env:TEMP | Out-Null
    Set-Location $source
    $report.cargo = (& cargo.exe -V | Out-String).Trim()
    $report.rustc = (& rustc.exe -vV | Out-String).Trim()
    $report.logicalProcessors = [Environment]::ProcessorCount
    $lists = @{}
    foreach ($variant in @('A','B')) {
        foreach ($phase in @('setup','warm')) {
            Assert-Source
            $space = @(Get-FreeSpace)
            $args = @(Get-BuildArguments $variant)
            $entry = Invoke-Logged "build-$variant-$phase" 'cargo.exe' $args
            $messages = @(Get-Content -LiteralPath (Join-Path $work $entry.log) | Where-Object { $_.StartsWith('{') } | ForEach-Object { ConvertFrom-Json -InputObject $_ })
            $artifacts = @($messages | Where-Object { $_.reason -eq 'compiler-artifact' })
            $root = @($artifacts | Where-Object { [IO.Path]::GetFullPath($_.manifest_path) -ieq [IO.Path]::GetFullPath((Join-Path $source 'apps/update/Cargo.toml')) -and $_.profile.test -and $null -ne $_.executable -and 'lib' -in $_.target.kind })
            if ($root.Count -ne 1) { throw 'Missing unique updater libtest Cargo artifact' }
            $root = $root[0]
            foreach ($package in $spec.packages) {
                $packageArtifacts = @($artifacts | Where-Object { $_.target.name -ceq $package.Replace('-','_') -and 'lib' -in $_.target.kind })
                if (-not $packageArtifacts.Count) { throw "Missing profile evidence: $package" }
                foreach ($artifact in $packageArtifacts) {
                    if ($artifact.profile.opt_level -cne [string]$spec.variants.$variant -or $artifact.profile.debuginfo -ne 2 -or
                        $artifact.profile.debug_assertions -ne $true -or $artifact.profile.overflow_checks -ne $true) { throw "Unexpected compiled profile: $package" }
                }
            }
            if (@(Compare-Object @($root.features | Sort-Object) @($spec.features | Sort-Object)).Count) { throw 'Unexpected test features' }
            $identity = @($messages | Where-Object { $_.reason -eq 'build-script-executed' } | ForEach-Object { $_.env } | Where-Object { $_[0] -like 'WINBRIDGE_*' })
            $identityMap = @{}
            foreach ($pair in $identity) { $identityMap[$pair[0]] = $pair[1] }
            if ($identityMap['WINBRIDGE_SOURCE_COMMIT'] -cne $manifest.sourceCommit -or $identityMap['WINBRIDGE_SOURCE_DIRTY'] -cne 'false' -or
                $identityMap['WINBRIDGE_SOURCE_MANIFEST_SHA256'] -cne $ManifestSha256) { throw 'Cargo build identity differs from source pin' }
            $timing = Join-Path $CacheDirectory 'cargo-timings/cargo-timing.html'
            if (-not (Test-Path -LiteralPath $timing -PathType Leaf)) { throw 'Cargo timing HTML missing' }
            Copy-Item -LiteralPath $timing -Destination (Join-Path $work "build-$variant-$phase.html")
            $report.builds += @{ variant=$variant; phase=$phase; command=$entry.name; sourceIdentity=$identityMap; artifact=$root; space=$space }
            Assert-Source
        }
        Assert-SessionFile $root.executable
        if (-not ([IO.Path]::GetFullPath($root.executable)).StartsWith($CacheDirectory + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Artifact outside leased cache' }
        $binary = Join-Path $work "tests-$variant.exe"
        Copy-Item -LiteralPath $root.executable -Destination $binary
        $pins += [IO.File]::Open($binary, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        $report.binaries += @{ variant=$variant; file="tests-$variant.exe"; bytes=(Get-Item -LiteralPath $binary).Length; sha256=(Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant(); profile=$root.profile; features=$root.features }
        Set-Location (Join-Path $source 'apps/update')
        $list = Invoke-Logged "list-$variant" $binary @($spec.filter, '--list', '--color=never')
        $names = @(Get-Content -LiteralPath (Join-Path $work $list.log) | Where-Object { $_ -match '^(.+): test$' } | ForEach-Object { $_ -replace ': test$', '' } | Sort-Object)
        if ($names.Count -lt $spec.minimumTests -or @($names | Sort-Object -Unique).Count -ne $names.Count) { throw 'Incomplete/duplicate test set' }
        foreach ($name in $spec.requiredTests) { if ($name -cnotin $names) { throw "Required test missing: $name" } }
        $lists[$variant] = $names
        $policy = Invoke-Logged "policy-$variant" $binary @($spec.policyTest, '--exact', '--ignored', '--test-threads=1', '--nocapture', '--color=never')
        if ((Get-Content -Raw -LiteralPath (Join-Path $work $policy.log)) -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;') { throw 'Compiled safety probe did not pass' }
        Set-Location $source
    }
    if (@(Compare-Object $lists.A $lists.B).Count) { throw 'A/B test lists differ' }
    $report.testNames = $lists.A
    Set-Location (Join-Path $source 'apps/update')
    foreach ($stage in @('warmup', 'sample')) {
        $order = if ($stage -eq 'warmup') { $spec.warmupOrder } else { $spec.sampleOrder }
        for ($i=0; $i -lt $order.Count; $i++) {
            $variant = $order[$i]
            $binary = Join-Path $work "tests-$variant.exe"
            $entry = Invoke-Logged "$stage-$i-$variant" $binary @($spec.filter, '--test-threads=1', '--nocapture', '--color=never')
            $result = Get-Content -Raw -LiteralPath (Join-Path $work $entry.log)
            if ($result -notmatch 'test result: ok\. (\d+) passed; 0 failed; 0 ignored;.*?finished in ([0-9.]+)s' -or [int]$Matches[1] -ne $lists.A.Count) { throw 'Test set did not complete successfully' }
            $entry.libtestMs = [double]::Parse($Matches[2], [Globalization.CultureInfo]::InvariantCulture) * 1000
        }
    }
    Assert-Source
    foreach ($binary in $report.binaries) {
        if ((Get-FileHash -LiteralPath (Join-Path $work $binary.file)).Hash.ToLowerInvariant() -cne $binary.sha256) { throw 'Frozen binary changed' }
    }
    $report.finalSpace = @(Get-FreeSpace)
    $report.passed = $true
} catch {
    $report.error = $_.ToString()
    Write-Warning $report.error
} finally {
    Set-Location $oldLocation
    foreach ($name in $savedEnv.Keys) { [Environment]::SetEnvironmentVariable($name, $savedEnv[$name]) }
    foreach ($pin in $pins) { $pin.Dispose() }
    $lease.Dispose()
    if (Test-Path -LiteralPath $work -PathType Container) {
        $report.completedAt = [DateTime]::UtcNow.ToString('o')
        Write-NewJson (Join-Path $work 'report.json') $report
        $files = @(Get-ChildItem -LiteralPath $work -File | ForEach-Object { @{ path=$_.Name; bytes=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant() } })
        Write-NewJson (Join-Path $work 'report.ready.json') @{ schemaVersion=1; files=$files }
    }
}
if (-not $report.passed) { throw "Profile pilot failed; preserve $work and reconcile before retry" }
Write-Host "Profile pilot complete: $work. Analyze report separately; this is not a full native gate. Leave installed worker running; desktop may be closed."
