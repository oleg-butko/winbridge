<#
.SYNOPSIS
Build a pinned source tree into a new update candidate; never install or stop runtime.
.DESCRIPTION
The source tree must already contain winbridge-source-manifest.json and pass the
installed updater's readonly verification. Use a new staging directory outside src.
Worker may remain running; no desktop windows are started. Source apply and binary
replacement are later updater phases, not operations performed by this script.
#>
param(
  [string]$SourceDirectory = '',
  [Parameter(Mandatory)][string]$ManifestSha256,
  [Parameter(Mandatory)][string]$StagingDirectory,
  [string]$VerifierExecutable = 'K:\_proj-26\winbridge\runtime\winbridge-update.exe',
  [string]$ValidationCacheDirectory = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows candidate build requires Windows' }
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot 'lib/update-stage.ps1')
if (-not $SourceDirectory) { $SourceDirectory = Join-Path $PSScriptRoot '..' }
$source = [IO.Path]::GetFullPath($SourceDirectory).TrimEnd('\')
$stage = [IO.Path]::GetFullPath($StagingDirectory).TrimEnd('\')
foreach ($path in @($source,$stage)) {
  if ($path -notmatch '^[A-Za-z]:[\\/]') { throw 'Source and staging require local absolute drive paths' }
  Assert-SessionDirectory $path
  if ([IO.DriveInfo]::new([IO.Path]::GetPathRoot($path)).DriveType -ne [IO.DriveType]::Fixed) { throw 'Build/staging must use a local fixed disk' }
}
if ($stage.Equals($source,[StringComparison]::OrdinalIgnoreCase) -or $stage.StartsWith($source+'\',[StringComparison]::OrdinalIgnoreCase) -or $source.StartsWith($stage+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Staging and source roots must not overlap' }
if (Test-Path -LiteralPath $stage) { throw "Staging already exists; preserved: $stage" }
if ($ManifestSha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'ManifestSha256 must use 64 lowercase hex characters' }
Assert-SessionFile $VerifierExecutable
if (-not (Test-Path -LiteralPath $VerifierExecutable -PathType Leaf)) { throw 'Installed verifier unavailable; bootstrap updater is required' }
Assert-SessionDirectory ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($VerifierExecutable)))
$drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($source))
$stageDrive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($stage))
$workspaceTarget = Join-Path $source 'target'
$desktopTarget = Join-Path $source 'apps/desktop/src-tauri/target'
$validationCacheLease = $null
if ($ValidationCacheDirectory) {
  $ValidationCacheDirectory = [IO.Path]::GetFullPath($ValidationCacheDirectory)
  Assert-SessionDirectory $ValidationCacheDirectory
  $workspaceTarget = Join-Path $ValidationCacheDirectory 'winbridge/target'
  $desktopTarget = Join-Path $ValidationCacheDirectory 'winbridge/apps/desktop/src-tauri/target'
  foreach ($cachePath in @($workspaceTarget,$desktopTarget)) {
    Assert-SessionDirectory $cachePath
    if (-not (Test-Path -LiteralPath $cachePath -PathType Container) -or [IO.DriveInfo]::new([IO.Path]::GetPathRoot($cachePath)).DriveType -ne [IO.DriveType]::Fixed) { throw 'Validation reuse requires existing local compiler caches' }
  }
}
Assert-UpdateBuildPaths -Source $source -WorkspaceTarget $workspaceTarget -DesktopTarget $desktopTarget
$report = [ordered]@{schemaVersion=1;kind='winbridge-update-candidate';status='preparing';runtimeApplied=$false;candidateVerified=$false;sourceDirectory=$source;manifestSha256=$ManifestSha256;storage=@{sourceAvailableBytesBefore=$drive.AvailableFreeSpace;stagingAvailableBytesBefore=$stageDrive.AvailableFreeSpace};commands=@();artifacts=@();error=$null}
[IO.Directory]::CreateDirectory($stage) | Out-Null
$oldIncremental = $env:CARGO_INCREMENTAL
$oldUpdateConsoleEncoding = [Console]::OutputEncoding
$oldUpdatePipelineEncoding = $OutputEncoding
Push-Location $source
try {
  [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
  $OutputEncoding = [Text.UTF8Encoding]::new($false)
  if ($ValidationCacheDirectory) {
    $cacheDrive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($ValidationCacheDirectory))
    $report.storage.validationCacheAvailableBytesBefore = $cacheDrive.AvailableFreeSpace
    if ($cacheDrive.AvailableFreeSpace -lt 6GB) { throw 'Less than 6 GiB free in validation compiler cache; no build started' }
    $leasePath = Join-Path $ValidationCacheDirectory 'update-candidate-cache.lock'
    Assert-SessionFile $leasePath
    $validationCacheLease = [IO.File]::Open($leasePath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    $report.validationCacheDirectory = $ValidationCacheDirectory
  }
  if ($drive.AvailableFreeSpace -lt 6GB -or $stageDrive.AvailableFreeSpace -lt 6GB) { throw 'Less than 6 GiB free; no build started and no cache deleted' }
  $verifiedText = & $VerifierExecutable verify-source $source $ManifestSha256
  if ($LASTEXITCODE -ne 0) { throw 'Pinned source verification failed; no build started' }
  $verified = ($verifiedText -join "`n") | ConvertFrom-Json
  if ($verified.kind -ne 'source_verified' -or $verified.manifestSha256 -cne $ManifestSha256) { throw 'Unexpected source verification response' }
  $report.source = $verified
  $env:CARGO_INCREMENTAL = '0'
  $commands = @(@{name='rust';executable='cargo.exe';arguments=@('build','--workspace','--locked','--features','winbridge-worker/bundled-sqlite','--target-dir',$workspaceTarget)})
  if (-not $verified.includesBuiltUi) {
    $commands += @{name='npm-ci';executable='npm.cmd';arguments=@('ci')}
    $commands += @{name='frontend';executable='npm.cmd';arguments=@('run','build:ui')}
  }
  . (Join-Path $PSScriptRoot 'lib/update-build-profile.ps1')
  $desktopArguments=@(Get-UpdateDesktopBuildArguments -TargetDirectory $desktopTarget -NativeValidation:([bool]$ValidationCacheDirectory))
  if ($ValidationCacheDirectory) { Write-Host 'WinBridge native validation: desktop local crates opt0; release/custom-protocol/check flags preserved; no production profile change' }
  $commands += @{name='desktop';executable='cargo.exe';arguments=$desktopArguments}
  foreach ($command in $commands) {
    Write-Host "WinBridge candidate: $($command.name); installed runtime remains active"
    $exe = $command.executable
    $arguments = $command.arguments
    $log = Join-Path $stage ($command.name+'.log')
    & $exe @arguments 2>&1 | Tee-Object -FilePath $log | ForEach-Object { Write-Output $_ }
    $code = $LASTEXITCODE
    $report.commands += @{name=$command.name;executable=$exe;arguments=$arguments;exitCode=$code;log=([IO.Path]::GetFileName($log))}
    if ($code -ne 0) { throw "Candidate build failed: $($command.name), exit $code" }
  }
  $readback = & $VerifierExecutable verify-source $source $ManifestSha256
  if ($LASTEXITCODE -ne 0) { throw 'Source changed during build; candidate not accepted' }
  $after = ($readback -join "`n") | ConvertFrom-Json
  if ($after.manifestSha256 -cne $ManifestSha256) { throw 'Source manifest changed during build' }
  $workerIdentityText = & (Join-Path $workspaceTarget 'debug/winbridge-worker.exe') --describe
  if ($LASTEXITCODE -ne 0) { throw 'Built worker identity could not be read' }
  $identity = (($workerIdentityText -join "`n") | ConvertFrom-Json).build
  if ($identity.sourceDirty -or $identity.sourceCommit -cne $verified.sourceCommit -or $identity.sourceManifestSha256 -cne $ManifestSha256) { throw 'Built worker does not identify the pinned clean source; stale/mismatched artifact refused' }
  $report.workerBuild = $identity
  $artifacts = [ordered]@{
    'winbridge-worker.exe' = Join-Path $workspaceTarget 'debug/winbridge-worker.exe'
    'winbridge-fixture.exe' = Join-Path $workspaceTarget 'debug/winbridge-fixture.exe'
    'winbridge-update.exe' = Join-Path $workspaceTarget 'debug/winbridge-update.exe'
    'winbridge-desktop.exe' = Join-Path $desktopTarget 'release/winbridge-desktop.exe'
  }
  # Queries remain children of the managed compiler (or the isolated native
  # validation Job). Their helper adds its own bounded Job and stdio capture.
  # Unsupported old desktop files are rejected by the marker before execution.
  # Cargo exports may hard-link its deps cache. Probe only independent,
  # create-only payload copies; keep the verifier's hard-link rejection intact.
  $payloadDirectory = Join-Path $stage 'payload'
  $report.artifacts = @(New-UpdatePayload $artifacts $payloadDirectory)
  $queryHelper = Join-Path $payloadDirectory 'winbridge-update.exe'
  $roles = [ordered]@{'winbridge-worker.exe'='worker';'winbridge-fixture.exe'='fixture';'winbridge-desktop.exe'='desktop';'winbridge-update.exe'='updater'}
  $report.artifactIdentities = @()
  foreach ($name in $roles.Keys) {
    $role = $roles[$name]
    Write-Host "WinBridge candidate: readonly identity $role; no UI/configuration"
    $logName = 'identity-'+$role+'.log'
    $image = Join-Path $payloadDirectory $name
    $queryText = @(& $queryHelper artifact-query $image $role 2>&1 | Tee-Object -FilePath (Join-Path $stage $logName))
    $queryCode = $LASTEXITCODE
    $report.commands += @{name=('identity-'+$role);executable=$queryHelper;arguments=@('artifact-query',$image,$role);exitCode=$queryCode;log=$logName}
    if ($queryCode -ne 0) { throw "Artifact identity query failed: $role; see $logName" }
    $query = ($queryText -join "`n") | ConvertFrom-Json
    $d = $query.descriptor
    if ($query.kind -ne 'artifact_query' -or $query.readOnly -ne $true -or $query.candidateVerified -ne $false -or $query.cleanupVerified -ne $true -or $query.image.querySupported -ne $true -or $d.schemaVersion -ne 1 -or $d.queryContract -ne 1 -or $d.role -cne $role -or $d.targetOs -cne 'windows' -or $d.build.sourceDirty -ne $false -or $d.build.sourceCommit -cne $verified.sourceCommit -or $d.build.sourceManifestSha256 -cne $ManifestSha256 -or $d.build.version -cne $identity.version) { throw "Artifact identity source/role/contract mismatch: $name" }
    $report.artifactIdentities += @{name=$name;query=$query}
  }
  foreach ($artifact in $report.artifacts) {
    $query = @($report.artifactIdentities | Where-Object { $_.name -ceq $artifact.name })[0].query
    if ($artifact.sha256 -cne $query.image.sha256 -or $artifact.bytes -ne $query.image.bytes) { throw "Artifact changed between identity query and payload staging: $($artifact.name)" }
  }
  $report.artifactIdentityChecked = $true
  $report.status = 'built'
} catch {
  $report.status = 'failed'
  $report.error = $_.Exception.Message
  throw
} finally {
  [Console]::OutputEncoding = $oldUpdateConsoleEncoding
  $OutputEncoding = $oldUpdatePipelineEncoding
  if ($validationCacheLease) { $validationCacheLease.Dispose() }
  $env:CARGO_INCREMENTAL = $oldIncremental
  Pop-Location
  $report.storage.sourceAvailableBytesAfter = $drive.AvailableFreeSpace
  $report.storage.stagingAvailableBytesAfter = $stageDrive.AvailableFreeSpace
  if ($ValidationCacheDirectory) { $report.storage.validationCacheAvailableBytesAfter = $cacheDrive.AvailableFreeSpace }
  Write-NewJson (Join-Path $stage 'candidate-report.json') $report
}
# Validate the published exact report against the actual four payload files. This
# is standalone diagnostics; only the managed compiler journal can bind its result.
try {
  [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
  $OutputEncoding = [Text.UTF8Encoding]::new($false)
  $reportSha = (Get-FileHash -LiteralPath (Join-Path $stage 'candidate-report.json') -Algorithm SHA256).Hash.ToLowerInvariant()
  $fileArguments = @('inspect-candidate-files',$source,$ManifestSha256,$stage,$reportSha)
  $fileText = & $queryHelper @fileArguments 2> (Join-Path $stage 'candidate-files-stderr.log')
  $fileCode = $LASTEXITCODE
  Write-NewJson (Join-Path $stage 'candidate-files-command.json') @{executable=$queryHelper;arguments=$fileArguments;exitCode=$fileCode}
  if ($fileCode -ne 0) { throw 'Candidate exact-file inspection failed; see candidate-files-stderr.log; no install' }
  $fileProof = ($fileText -join "`n") | ConvertFrom-Json
  if ($fileProof.kind -ne 'candidate_file_inspection' -or $fileProof.readOnly -ne $true -or $fileProof.journalBound -ne $false -or $fileProof.runtimeApplied -or $fileProof.candidateVerified -or $fileProof.report.sha256 -cne $reportSha -or $fileProof.source.manifestSha256 -cne $ManifestSha256 -or $fileProof.artifacts.Count -ne 4) { throw 'Candidate exact-file proof incomplete; no install' }
  Write-NewJson (Join-Path $stage 'candidate-files-proof.json') $fileProof
  Write-Host 'Candidate built; executable identities and exact payload files checked. Worker health verification and installation have not run.'
} finally {
  [Console]::OutputEncoding = $oldUpdateConsoleEncoding
  $OutputEncoding = $oldUpdatePipelineEncoding
}
