<# Fresh-source release compiler A/B; managed --artifact-info queries only, no UI/install. #>
param(
    [Parameter(Mandatory)][string]$SourceDirectory,
    [Parameter(Mandatory)][string]$ManifestSha256,
    [Parameter(Mandatory)][string]$CacheDirectory,
    [Parameter(Mandatory)][string]$WorkDirectory,
    [Parameter(Mandatory)][string]$SourceArchivePath,
    [Parameter(Mandatory)][string]$ArchiveSha256,
    [Parameter(Mandatory)][string]$VerifierExecutable,
    [Parameter(Mandatory)][string]$VerifierSha256
)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$PSNativeCommandUseErrorActionPreference=$false
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
$source=[IO.Path]::GetFullPath($SourceDirectory)
$cache=[IO.Path]::GetFullPath($CacheDirectory)
$work=[IO.Path]::GetFullPath($WorkDirectory)
foreach ($path in @($source,$cache,$work)) { Assert-SessionDirectory $path }
Assert-WorkflowCachePath $cache
if ($env:OS -cne 'Windows_NT' -or (Split-Path $work -Parent) -ine (Split-Path $source -Parent) -or
    (Split-Path $work -Leaf) -notmatch '^desktop-release-\d{8}-\d{6}-[a-f0-9]{8}$' -or
    (Split-Path $cache -Parent) -ine (Split-Path (Split-Path $source -Parent) -Parent) -or
    (Test-Path -LiteralPath $work)) { throw 'Requires NEW isolated sibling report and retained native compiler cache' }
$target=Join-Path $cache 'winbridge/apps/desktop/src-tauri/target'
Assert-WorkflowCachePath $target
if (-not (Test-Path -LiteralPath $target -PathType Container)) { throw 'Retained desktop target required' }
$tainted=@(Get-ChildItem Env:|Where-Object {$_.Name -match '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_PROFILE_.*|WINBRIDGE_.*)$'})
if ($tainted.Count -or @(Get-Process cargo -ErrorAction SilentlyContinue).Count) { throw 'Requires unmodified compiler environment and no active Cargo' }
foreach ($file in @($SourceArchivePath,$VerifierExecutable)) { Assert-SessionFile $file }
if ((Get-FileHash -LiteralPath $SourceArchivePath).Hash.ToLowerInvariant() -cne $ArchiveSha256 -or
    (Get-FileHash -LiteralPath $VerifierExecutable).Hash.ToLowerInvariant() -cne $VerifierSha256) { throw 'Archive/helper pin differs' }
$leasePath=Join-Path $cache 'update-candidate-cache.lock'
Assert-SessionFile $leasePath
$lease=[IO.File]::Open($leasePath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$helperPin=$null
$saved=@{}
foreach ($name in @('CARGO_INCREMENTAL','CARGO_TERM_COLOR','TEMP','TMP')) { $saved[$name]=[Environment]::GetEnvironmentVariable($name) }
$oldLocation=Get-Location
$packages=[ordered]@{'winbridge-desktop'='apps/desktop/src-tauri/Cargo.toml';'winbridge-core'='crates/core/Cargo.toml';
    'winbridge-protocol'='crates/protocol/Cargo.toml';'winbridge-platform'='crates/platform/Cargo.toml';'winbridge-transport'='crates/transport/Cargo.toml'}
$plan=@(@{variant='A';warmup=$true},@{variant='B';warmup=$true},@{variant='A';warmup=$false},
    @{variant='B';warmup=$false},@{variant='B';warmup=$false},@{variant='A';warmup=$false})
$report=[ordered]@{schemaVersion=1;kind='desktop_release_profile_pilot';passed=$false;testsExecuted=$false;windowsOpened=$false;
    installedRuntimeChanged=$false;controlUsesDefault=$true;validationOnlyCandidate=$true;candidateOptLevel=0;
    archiveSha256=$ArchiveSha256;sourceManifestSha256=$ManifestSha256;
    verifierSha256=$VerifierSha256;packages=@($packages.Keys);trials=@();error=$null}
$controlProfiles=@{}
function Assert-TrialSource([string]$Root) {
    $manifestPath=Join-Path $Root 'winbridge-source-manifest.json'
    if ((Get-FileHash -LiteralPath $manifestPath).Hash.ToLowerInvariant() -cne $ManifestSha256) { throw 'Trial manifest changed' }
    $manifest=Get-Content -Raw -LiteralPath $manifestPath|ConvertFrom-Json
    if ($manifest.dirty -isnot [bool] -or $manifest.dirty -or -not $manifest.includesBuiltUi) { throw 'Clean snapshot with built UI required' }
    foreach ($item in Get-ChildItem -LiteralPath $Root -Recurse -Force) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked trial source' }
    }
    foreach ($file in $manifest.files) {
        if ($file.path -match '(^/|\\|:|(^|/)\.\.?(/|$))') { throw 'Unsafe manifest path' }
        $path=Join-Path $Root $file.path
        if ((Get-Item -LiteralPath $path).Length -ne $file.bytes -or (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $file.sha256) { throw ('Trial input changed: '+$file.path) }
    }
    return $manifest.sourceCommit
}
try {
    New-Item -ItemType Directory -Path $work|Out-Null
    $helperPin=[IO.File]::Open($VerifierExecutable,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    if ((Get-FileHash -LiteralPath $VerifierExecutable).Hash.ToLowerInvariant() -cne $VerifierSha256) { throw 'Helper changed before pin' }
    $localArchive=Join-Path $work 'source.zip'
    [IO.File]::Copy($SourceArchivePath,$localArchive,$false)
    if ((Get-FileHash -LiteralPath $localArchive).Hash.ToLowerInvariant() -cne $ArchiveSha256) { throw 'Copied ZIP differs' }
    $env:CARGO_INCREMENTAL='0';$env:CARGO_TERM_COLOR='never'
    $env:TEMP=Join-Path $work 'test-temp';$env:TMP=$env:TEMP
    New-Item -ItemType Directory -Path $env:TEMP|Out-Null
    $report.toolchain=(& rustc.exe -vV|Out-String).Trim()
    for ($i=0; $i -lt $plan.Count; $i++) {
        $variant=$plan[$i].variant
        $level=if ($variant -ceq 'A') {3} else {0}
        $key="trial-$i-$variant"
        $trialRoot=Join-Path $work $key
        $drive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($target))
        $freeBefore=$drive.AvailableFreeSpace
        if ($drive.DriveType -ne [IO.DriveType]::Fixed -or $freeBefore -lt 6GB) { throw 'Need local disk >=6GiB before trial' }
        Write-Host "WinBridge desktop build: $key opt$level; warmup=$($plan[$i].warmup)"
        $setup=[Diagnostics.Stopwatch]::StartNew()
        Expand-Archive -LiteralPath $localArchive -DestinationPath $trialRoot
        $trialSource=Join-Path $trialRoot 'winbridge'
        $commit=Assert-TrialSource $trialSource
        $setup.Stop()
        Set-Location $trialSource
        $args=@()
        if ($variant -ceq 'B') {
            foreach ($package in $packages.Keys) { $args+=@('--config',"profile.release.package.$package.opt-level=$level") }
        }
        $args+=@('build','--manifest-path','apps/desktop/src-tauri/Cargo.toml','--features','custom-protocol','--release',
            '--locked','--offline','--message-format=json','--timings','--target-dir',$target)
        $watch=[Diagnostics.Stopwatch]::StartNew()
        & cargo.exe @args 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $work "$key.log") -Encoding utf8
        $code=$LASTEXITCODE;$watch.Stop()
        if ($code -ne 0) { throw "Desktop compile failed: $key ($code)" }
        $messages=@(Get-Content -LiteralPath (Join-Path $work "$key.log")|Where-Object {$_.StartsWith('{')}|ForEach-Object {ConvertFrom-Json -InputObject $_})
        $artifacts=@($messages|Where-Object {$_.reason -ceq 'compiler-artifact'})
        $locals=@()
        foreach ($package in $packages.Keys) {
            $expected=[IO.Path]::GetFullPath((Join-Path $trialSource $packages[$package]))
            $found=@($artifacts|Where-Object {[IO.Path]::GetFullPath($_.manifest_path) -ieq $expected -and ('lib' -in $_.target.kind -or 'bin' -in $_.target.kind)})
            if ($found.Count -ne 1 -or $found[0].fresh -ne $false -or $found[0].profile.opt_level -cne [string]$level) { throw "Expected freshly compiled local package/profile: $package" }
            $normalized=$found[0].profile|ConvertTo-Json -Compress|ConvertFrom-Json
            $normalized.opt_level='same'
            $policy=$normalized|ConvertTo-Json -Compress
            if ($i -eq 0) { $controlProfiles[$package]=$policy }
            elseif ($policy -cne $controlProfiles[$package]) { throw "Non-opt profile flags changed: $package" }
            $locals+=$found[0]
        }
        $root=@($locals|Where-Object {$null -ne $_.executable -and 'bin' -in $_.target.kind})
        if ($root.Count -ne 1 -or 'custom-protocol' -cnotin $root[0].features) { throw 'Missing unique custom-protocol desktop executable' }
        $root=$root[0]
        $image=Join-Path $work "$key.exe"
        [IO.File]::Copy($root.executable,$image,$false)
        $imageSha=(Get-FileHash -LiteralPath $image).Hash.ToLowerInvariant()
        $query=@(& $VerifierExecutable artifact-query $image desktop 2>&1|ForEach-Object {$_.ToString()})
        $queryCode=$LASTEXITCODE
        $query|Set-Content -LiteralPath (Join-Path $work "$key-identity.log") -Encoding utf8
        if ($queryCode -ne 0) { throw 'Managed desktop identity query failed' }
        $identity=($query -join "`n")|ConvertFrom-Json
        if ($identity.kind -cne 'artifact_query' -or $identity.readOnly -ne $true -or $identity.cleanupVerified -ne $true -or
            $identity.candidateVerified -ne $false -or $identity.descriptor.role -cne 'desktop' -or
            $identity.descriptor.build.sourceDirty -ne $false -or $identity.descriptor.build.sourceCommit -cne $commit -or
            $identity.descriptor.build.sourceManifestSha256 -cne $ManifestSha256 -or $identity.image.sha256 -cne $imageSha) { throw 'Identity/source/cleanup evidence differs' }
        Copy-Item -LiteralPath (Join-Path $target 'cargo-timings/cargo-timing.html') -Destination (Join-Path $work "$key.html")
        if ((Assert-TrialSource $trialSource) -cne $commit) { throw 'Source changed during compiler/query' }
        $report.trials+=@{key=$key;variant=$variant;warmup=$plan[$i].warmup;optLevel=$level;setupMs=$setup.ElapsedMilliseconds;
            elapsedMs=$watch.ElapsedMilliseconds;exitCode=$code;arguments=$args;log="$key.log";sourceDirectory=$trialSource;
            sourceCommit=$commit;localArtifacts=$locals;compiledArtifacts=@($artifacts|Where-Object {$_.fresh -eq $false});
            executable="$key.exe";bytes=(Get-Item -LiteralPath $image).Length;sha256=$imageSha;identityLog="$key-identity.log";freeBytesBefore=$freeBefore;freeBytes=$drive.AvailableFreeSpace}
        Write-Host "WinBridge desktop build: $key completed $($watch.ElapsedMilliseconds)ms; identity/cleanup/source passed"
    }
    foreach ($trial in $report.trials) {
        if ((Get-FileHash -LiteralPath (Join-Path $work $trial.executable)).Hash.ToLowerInvariant() -cne $trial.sha256) { throw 'Frozen image changed' }
    }
    $report.passed=$true
} catch {
    $report.error=$_.ToString();Write-Warning $report.error
} finally {
    Set-Location $oldLocation
    foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name,$saved[$name]) }
    if ($helperPin) { $helperPin.Dispose() }
    $lease.Dispose()
    if (Test-Path -LiteralPath $work -PathType Container) {
        Write-NewJson (Join-Path $work 'report.json') $report
        $files=@(Get-ChildItem -LiteralPath $work -File|Where-Object {$_.Name -cne 'source.zip'}|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}})
        Write-NewJson (Join-Path $work 'report.ready.json') @{schemaVersion=1;files=$files}
    }
}
if (-not $report.passed) { throw 'Desktop build pilot failed; preserve results and reconcile' }
