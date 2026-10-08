# File helpers shared by Windows preparation and portable filesystem checks.
function Write-NewJson([string]$Path, $Value) {
  [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path)) | Out-Null
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 30))
  $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try { $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
}
function Assert-SessionDirectory([string]$Path) {
  $current = [IO.Path]::GetFullPath($Path)
  while ($current) {
    $item = Get-Item -Force -LiteralPath $current -ErrorAction SilentlyContinue
    if ($item) {
      if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Session directory cannot traverse a file/junction/symlink; preserved: $current"
      }
    }
    $current = [IO.Path]::GetDirectoryName($current)
  }
}
function Assert-SessionFile([string]$Path) {
  $item = Get-Item -Force -LiteralPath $Path -ErrorAction SilentlyContinue
  if ($item -and ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint))) {
    throw "Session file cannot be a directory/junction/symlink; preserved: $Path"
  }
}
function Get-SessionLayout([string]$FixtureRoot, [string]$RuntimeRoot, [string]$RunId) {
  if ($RunId -notmatch '^[A-Za-z0-9_-]{1,80}$') { throw 'Invalid session run ID' }
  Assert-SessionDirectory $FixtureRoot
  Assert-SessionDirectory $RuntimeRoot
  return @{
    runRoot = Join-Path $FixtureRoot $RunId
    runtimeDirectory = $RuntimeRoot
    projectDirectory = Join-Path $RuntimeRoot 'project'
    stateDirectory = Join-Path $RuntimeRoot 'state'
    workerConfigPath = Join-Path $RuntimeRoot 'winbridge-worker.local.json'
    desktopConfigPath = Join-Path $RuntimeRoot 'winbridge-desktop.local.json'
  }
}
function Initialize-SessionConfiguration($Layout, $Settings) {
  Assert-SessionDirectory $Layout.projectDirectory
  Assert-SessionDirectory $Layout.stateDirectory
  [IO.Directory]::CreateDirectory($Layout.runtimeDirectory) | Out-Null
  [IO.Directory]::CreateDirectory($Layout.projectDirectory) | Out-Null
  Assert-SessionFile $Layout.workerConfigPath
  Assert-SessionFile $Layout.desktopConfigPath
  if (-not (Test-Path -LiteralPath $Layout.workerConfigPath)) {
    Write-NewJson $Layout.workerConfigPath @{
      schemaVersion = 1; instanceId = $Settings.instanceId; nodeId = $Settings.nodeId
      stateDirectory = $Layout.stateDirectory
      worker = @{ controllerUrl = $Settings.controllerUrl; keepRunningOnUiExit = $true }
      projects = @(@{ id = 'fixture'; workspaceRoot = $Layout.projectDirectory; allowDirectCommands = $true; maxConcurrentMutations = 1
        recipes = @(@{ id = 'fixture-success'; executable = (Join-Path $Layout.runtimeDirectory 'winbridge-fixture.exe'); arguments = @('success'); workingDirectory = '.'; timeoutMs = 10000 }) })
    }
  }
  if (-not (Test-Path -LiteralPath $Layout.desktopConfigPath)) {
    $reportDirectory = if ($env:OS -eq 'Windows_NT') { 'K:\VMs\shared\WinBridge\diagnostics' } else { Join-Path $Layout.runtimeDirectory 'diagnostics' }
    Write-NewJson $Layout.desktopConfigPath @{ schemaVersion = 1; workerConfigPath = 'winbridge-worker.local.json'; reportDirectory = $reportDirectory }
  }
  $desktop = Get-Content -Raw -Encoding UTF8 -LiteralPath $Layout.desktopConfigPath | ConvertFrom-Json
  $fields = @($desktop.PSObject.Properties.Name)
  if (@($fields | Where-Object { $_ -notin @('schemaVersion','workerConfigPath','reportDirectory') }).Count -or 'schemaVersion' -notin $fields -or 'workerConfigPath' -notin $fields -or $desktop.schemaVersion -ne 1 -or -not ($desktop.workerConfigPath -is [string]) -or -not $desktop.workerConfigPath.Trim()) {
    throw "Invalid existing desktop config; file preserved: $($Layout.desktopConfigPath)"
  }
  if ('reportDirectory' -in $fields -and (-not ($desktop.reportDirectory -is [string]) -or -not [IO.Path]::IsPathFullyQualified($desktop.reportDirectory) -or $desktop.reportDirectory -match '(^|[\\/])\.\.([\\/]|$)')) {
    throw "Invalid desktop reportDirectory; config preserved: $($Layout.desktopConfigPath)"
  }
  $configured = [string]$desktop.workerConfigPath
  if ($configured -match '^[A-Za-z]:(?![\\/])' -or ($configured -match '^[\\/]' -and $configured -notmatch '^[\\/]{2}' -and $env:OS -eq 'Windows_NT')) {
    throw "Ambiguous desktop workerConfigPath; file preserved: $($Layout.desktopConfigPath)"
  }
  if (-not [IO.Path]::IsPathRooted($configured)) { $configured = Join-Path $Layout.runtimeDirectory $configured }
  if (-not [IO.Path]::GetFullPath($configured).Equals([IO.Path]::GetFullPath($Layout.workerConfigPath), [StringComparison]::OrdinalIgnoreCase)) {
    throw "Desktop points to another worker config; file preserved: $($Layout.desktopConfigPath). Expected $($Layout.workerConfigPath)"
  }
  return Get-Content -Raw -Encoding UTF8 -LiteralPath $Layout.workerConfigPath | ConvertFrom-Json
}
function Copy-SessionBinaries($Sources, [string]$RuntimeRoot, [string]$StagingRoot, [string[]]$ActiveExecutablePaths = @()) {
  Assert-SessionDirectory $RuntimeRoot
  Assert-SessionDirectory $StagingRoot
  [IO.Directory]::CreateDirectory($StagingRoot) | Out-Null
  $plan = @()
  foreach ($name in $Sources.Keys) {
    if ($name -notin @('winbridge-worker.exe', 'winbridge-fixture.exe', 'winbridge-desktop.exe')) { throw "Unexpected runtime binary name: $name" }
    Assert-SessionFile $Sources[$name]
    if (-not (Test-Path -LiteralPath $Sources[$name] -PathType Leaf)) { throw "Build artifact not found: $($Sources[$name])" }
    $staged = Join-Path $StagingRoot $name
    if (Test-Path -LiteralPath $staged) { throw "Staging file already exists; preserved: $staged" }
    Copy-Item -LiteralPath $Sources[$name] -Destination $staged
    $target = Join-Path $RuntimeRoot $name
    Assert-SessionFile $target
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $staged).Hash
    $changed = -not (Test-Path -LiteralPath $target) -or (Get-FileHash -Algorithm SHA256 -LiteralPath $target).Hash -ne $hash
    $plan += @{ path = $target; staged = $staged; sha256 = $hash; changed = $changed }
  }
  if (@($plan | Where-Object { $_.changed }).Count) {
    foreach ($active in $ActiveExecutablePaths) {
      if (@($plan | Where-Object { [IO.Path]::GetFullPath($_.path).Equals([IO.Path]::GetFullPath($active), [StringComparison]::OrdinalIgnoreCase) }).Count) {
        throw "Close worker/desktop before updating runtime; binaries/configs/state preserved. Active executable: $active"
      }
    }
  }
  [IO.Directory]::CreateDirectory($RuntimeRoot) | Out-Null
  foreach ($entry in $plan) {
    if ($entry.changed) { Copy-Item -LiteralPath $entry.staged -Destination $entry.path }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $entry.path).Hash -ne $entry.sha256) { throw "Runtime binary verification failed: $($entry.path)" }
  }
  return @($plan | ForEach-Object { @{ path = $_.path; sha256 = $_.sha256; updated = $_.changed } })
}
function Get-SessionWorkerInfo([string]$Executable, [string]$ConfigPath) {
  # A missing local pipe is normal here; capture native stderr without changing
  # the caller's preferences. This probe never submits/cancels commands.
  $ErrorActionPreference = 'Continue'
  $PSNativeCommandUseErrorActionPreference = $false
  $output = & $Executable --config $ConfigPath info 2>&1
  if ($LASTEXITCODE -ne 0) { return $null }
  $response = ($output -join "`n") | ConvertFrom-Json -ErrorAction Stop
  if ($response.kind -ne 'info') { throw 'Unexpected response from existing worker' }
  return $response.info
}
