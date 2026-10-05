param(
  [string]$SettingsPath = '',
  [switch]$SkipBuild,
  [switch]$PrepareOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$RepoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ($RepoRoot.StartsWith('K:\VMs\shared\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Extract sources to a local Windows folder before building; do not build in shared' }
if (-not $SettingsPath) { $SettingsPath = Join-Path $RepoRoot 'target\debug\winbridge-session.local.json' }
$SettingsPath = [IO.Path]::GetFullPath($SettingsPath)
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
function Invoke-Checked([string]$Executable, [string[]]$Arguments) {
  & $Executable @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$Executable exited with code $LASTEXITCODE" }
}
if (-not (Test-Path -LiteralPath $SettingsPath)) {
  $seed = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $RepoRoot 'examples\windows-session.config.json') | ConvertFrom-Json
  Write-NewJson $SettingsPath $seed
}
$settings = Get-Content -Raw -Encoding UTF8 -LiteralPath $SettingsPath | ConvertFrom-Json
$fields = @($settings.PSObject.Properties.Name)
$required = @('schemaVersion', 'controllerUrl', 'instanceId', 'nodeId', 'reportDirectory', 'fixtureDirectory')
$expected = $required + @('runtimeDirectory')
foreach ($field in $required) { if ($field -notin $fields) { throw "Missing $field in $SettingsPath" } }
if (@($fields | Where-Object { $_ -notin $expected }).Count) { throw "Unknown settings fields in $SettingsPath" }
if ($settings.schemaVersion -ne 1) { throw 'Unsupported session settings schemaVersion; expected 1' }
foreach ($id in @($settings.instanceId, $settings.nodeId)) { if ($id -notmatch '^[A-Za-z0-9_-]{1,80}$') { throw 'instanceId/nodeId must use 1-80 letters, digits, dash or underscore' } }
if ($settings.controllerUrl -notmatch '^ws://[0-9.]+:[0-9]+/?$') { throw 'controllerUrl must use ws://LOCAL_IPV4:PORT' }
$uri = [Uri]$settings.controllerUrl
$ip = [Net.IPAddress]::Parse($uri.Host)
$parts = $ip.GetAddressBytes()
$localIp = $parts.Length -eq 4 -and ($parts[0] -eq 10 -or $parts[0] -eq 127 -or ($parts[0] -eq 192 -and $parts[1] -eq 168) -or ($parts[0] -eq 172 -and $parts[1] -ge 16 -and $parts[1] -le 31))
if (-not $localIp -or $uri.Port -lt 1 -or $uri.Port -gt 65535) { throw 'controllerUrl requires a private/loopback IPv4 address and port 1-65535' }
if ($settings.reportDirectory -notmatch '^[A-Za-z]:[\\/]') { throw 'reportDirectory must be an absolute Windows path' }
$fixtureRoot = [string]$settings.fixtureDirectory
if ($fixtureRoot -match '^[\\/]' -or $fixtureRoot -match '^[A-Za-z]:(?![\\/])') { throw 'fixtureDirectory cannot use UNC/root-relative/drive-relative paths' }
if (-not [IO.Path]::IsPathRooted($fixtureRoot)) { $fixtureRoot = Join-Path $RepoRoot $fixtureRoot }
$fixtureRoot = [IO.Path]::GetFullPath($fixtureRoot)
$sharedRoot = [IO.Path]::GetFullPath('K:\VMs\shared').TrimEnd('\')
if ($fixtureRoot -eq $sharedRoot -or $fixtureRoot.StartsWith($sharedRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Fixtures/state/build must stay on a local disk outside shared' }
$runtimeRoot = 'K:\_proj-26\winbridge\runtime'
if ('runtimeDirectory' -in $fields) {
  if (-not ($settings.runtimeDirectory -is [string]) -or -not $settings.runtimeDirectory.Trim()) { throw 'runtimeDirectory must be a nonempty path' }
  $runtimeRoot = [string]$settings.runtimeDirectory
}
if ($runtimeRoot -match '^[\\/]' -or $runtimeRoot -match '^[A-Za-z]:(?![\\/])') { throw 'runtimeDirectory cannot use UNC/root-relative/drive-relative paths' }
if (-not [IO.Path]::IsPathRooted($runtimeRoot)) { $runtimeRoot = Join-Path $RepoRoot $runtimeRoot }
$runtimeRoot = [IO.Path]::GetFullPath($runtimeRoot)
if ($runtimeRoot -eq $sharedRoot -or $runtimeRoot.StartsWith($sharedRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Runtime/config/state must stay on a local disk outside shared' }
$runId = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
$layout = Get-SessionLayout $fixtureRoot $runtimeRoot $runId
$runRoot = $layout.runRoot
if (Test-Path -LiteralPath $runRoot) { throw "Run directory already exists; files preserved: $runRoot" }
$runtime = $layout.runtimeDirectory
[IO.Directory]::CreateDirectory($runRoot) | Out-Null
$reportRoot = Join-Path $settings.reportDirectory $runId
$workerConfig = $layout.workerConfigPath
$report = [ordered]@{ schemaVersion = 1; platform = 'windows'; preparationStatus = 'preparing'; runId = $runId; runRoot = $runRoot; reportRoot = $reportRoot; settingsPath = $SettingsPath; controllerUrl = $settings.controllerUrl; instanceId = $settings.instanceId; nodeId = $settings.nodeId; workerConfigPath = $workerConfig; runtimeDirectory = $runtime; error = $null }
Push-Location $RepoRoot
try {
  if (-not $SkipBuild) {
    Invoke-Checked 'cargo' @('build', '--workspace', '--locked', '--features', 'winbridge-worker/bundled-sqlite')
    Invoke-Checked 'npm.cmd' @('ci')
    Invoke-Checked 'npm.cmd' @('run', 'build:desktop')
  }
  $active = @(Get-Process -Name 'winbridge-worker','winbridge-desktop','winbridge-fixture' -ErrorAction SilentlyContinue | ForEach-Object { if ($_.Path) { $_.Path } })
  $sources = [ordered]@{
    'winbridge-worker.exe' = Join-Path $RepoRoot 'target\debug\winbridge-worker.exe'
    'winbridge-fixture.exe' = Join-Path $RepoRoot 'target\debug\winbridge-fixture.exe'
    'winbridge-desktop.exe' = Join-Path $RepoRoot 'apps\desktop\src-tauri\target\release\winbridge-desktop.exe'
  }
  $report.binaries = @(Copy-SessionBinaries $sources $runtime (Join-Path $runRoot 'staged-binaries') $active)
  $existingConfig = Test-Path -LiteralPath $workerConfig
  $actual = Initialize-SessionConfiguration $layout $settings
  $report.workerConfigPreserved = $existingConfig
  $report.controllerUrl = $actual.worker.controllerUrl
  $report.instanceId = $actual.instanceId
  $report.nodeId = $actual.nodeId
  $report.stateDirectory = $actual.stateDirectory
  $report.projectDirectories = @($actual.projects | ForEach-Object { $_.workspaceRoot })
  if ($existingConfig) { Write-Host "Existing worker config preserved: $workerConfig; its settings take precedence over creation defaults" }
  Invoke-Checked (Join-Path $runtime 'winbridge-worker.exe') @('--config', $workerConfig, 'validate')
  if (-not $PrepareOnly) {
    $existing = Get-SessionWorkerInfo (Join-Path $runtime 'winbridge-worker.exe') $workerConfig
    if ($existing) { throw "Worker instance $($existing.instanceId) is already running (generation $($existing.generation)); keep using it or stop it before preparing another launch. Configs/state preserved" }
    if (@($active | Where-Object { $_.Equals((Join-Path $runtime 'winbridge-worker.exe'), [StringComparison]::OrdinalIgnoreCase) }).Count) {
      throw 'The fixed worker executable is already running but local IPC could not confirm it; stop it before preparing another launch'
    }
  }
  $report.preparationStatus = 'prepared'
} catch {
  $report.preparationStatus = 'failed'
  $report.error = $_.Exception.Message
  throw
} finally {
  Pop-Location
  Write-NewJson (Join-Path $runRoot 'setup-report.json') $report
  try { Write-NewJson (Join-Path $reportRoot 'setup-report.json') $report }
  catch { Write-Warning "Shared report unavailable: $($_.Exception.Message). Local report: $(Join-Path $runRoot 'setup-report.json')" }
}
Write-Host "Session prepared: $runRoot"
Write-Host "Shared report: $reportRoot"
Write-Host "Settings preserved: $SettingsPath"
Write-Host "Fixed runtime: $runtime (runtimeDirectory in settings; changing it changes the firewall application path)"
Write-Host "Worker start: & '$(Join-Path $runtime 'winbridge-worker.exe')' --config '$workerConfig' serve"
Write-Host "Desktop: $(Join-Path $runtime 'winbridge-desktop.exe')"
if (-not $PrepareOnly) {
  Start-Process -FilePath (Join-Path $runtime 'winbridge-desktop.exe')
  & (Join-Path $runtime 'winbridge-worker.exe') --config $workerConfig serve
  if ($LASTEXITCODE -ne 0) { throw "Worker exited with code $LASTEXITCODE" }
}
