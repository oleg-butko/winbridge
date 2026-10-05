param([string]$SettingsPath = '', [switch]$SkipBuild, [switch]$PrepareOnly)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$RepoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
if (-not $SettingsPath) { $SettingsPath = Join-Path $RepoRoot 'target\debug\winbridge-snapshot.local.json' }
$SettingsPath = [IO.Path]::GetFullPath($SettingsPath)
Assert-SessionFile $SettingsPath
if (-not (Test-Path -LiteralPath $SettingsPath)) {
  Write-NewJson $SettingsPath (Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $RepoRoot 'examples\windows-snapshot.config.json') | ConvertFrom-Json)
}
$settings = Get-Content -Raw -Encoding UTF8 -LiteralPath $SettingsPath | ConvertFrom-Json
$fields = @($settings.PSObject.Properties.Name)
$expected = @('schemaVersion','runtimeDirectory','workspaceDirectory','deliveryRoot','namespace','peerRoot','reportDirectory')
if ($settings.schemaVersion -ne 1 -or $fields.Count -ne $expected.Count -or @($fields | Where-Object { $_ -notin $expected }).Count) { throw "Invalid snapshot settings; preserved: $SettingsPath" }
foreach ($field in @('runtimeDirectory','workspaceDirectory','deliveryRoot','reportDirectory')) {
  if (-not ($settings.$field -is [string]) -or $settings.$field -notmatch '^[A-Za-z]:[\\/]') { throw "$field requires a local absolute Windows path" }
  Assert-SessionDirectory $settings.$field
}
if ($settings.namespace -notmatch '^[A-Za-z0-9_-]{1,80}$' -or -not ($settings.peerRoot -is [string])) { throw 'Invalid namespace/peerRoot' }
$runtime = [IO.Path]::GetFullPath($settings.runtimeDirectory)
$workspace = [IO.Path]::GetFullPath($settings.workspaceDirectory)
$shared = [IO.Path]::GetFullPath($settings.deliveryRoot).TrimEnd('\')
foreach ($path in @($runtime,$workspace)) {
  if ($path.Equals($shared,[StringComparison]::OrdinalIgnoreCase) -or $path.StartsWith($shared+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Runtime/workspace must stay outside shared' }
}
# The existing preparation script builds and updates only the fixed runtime. It
# preserves all existing settings/configs/state and refuses active binary updates.
$sessionSettings = Join-Path $RepoRoot 'target\debug\winbridge-session.local.json'
if (-not (Test-Path -LiteralPath $sessionSettings)) {
  $seed = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $RepoRoot 'examples\windows-session.config.json') | ConvertFrom-Json
  $seed | Add-Member -Force -NotePropertyName runtimeDirectory -NotePropertyValue $runtime
  Write-NewJson $sessionSettings $seed
}
if (Test-Path -LiteralPath $sessionSettings) {
  $session = Get-Content -Raw -Encoding UTF8 -LiteralPath $sessionSettings | ConvertFrom-Json
  $sessionRuntime = 'K:\_proj-26\winbridge\runtime'
  if ('runtimeDirectory' -in @($session.PSObject.Properties.Name)) { $sessionRuntime = $session.runtimeDirectory }
  if (-not [IO.Path]::IsPathRooted($sessionRuntime)) { $sessionRuntime = Join-Path $RepoRoot $sessionRuntime }
  if (-not [IO.Path]::GetFullPath($sessionRuntime).Equals($runtime,[StringComparison]::OrdinalIgnoreCase)) { throw 'Snapshot/runtime settings disagree; both local configs are preserved' }
}
& (Join-Path $PSScriptRoot 'prepare-windows-session.ps1') -SettingsPath $sessionSettings -SkipBuild:$SkipBuild -PrepareOnly
$basePath = Join-Path $runtime 'winbridge-worker.local.json'
$base = Get-Content -Raw -Encoding UTF8 -LiteralPath $basePath | ConvertFrom-Json
$workerPath = Join-Path $runtime 'winbridge-worker.snapshot.local.json'
Assert-SessionFile $workerPath
[IO.Directory]::CreateDirectory($workspace) | Out-Null
if (-not (Test-Path -LiteralPath $workerPath)) {
  # Additional config, not a rewrite or backup of the existing config. Same IPC
  # identity/state allows the existing desktop to connect to the selected worker.
  $candidate = Get-Content -Raw -Encoding UTF8 -LiteralPath $basePath | ConvertFrom-Json
  if (@($candidate.projects | Where-Object { $_.id -eq 'snapshot-fixture' }).Count) { throw 'Base config already registers snapshot-fixture; review its delivery settings instead' }
  $candidate | Add-Member -Force -NotePropertyName delivery -NotePropertyValue @{ adapter='shared-folder'; root=$settings.deliveryRoot; namespace=$settings.namespace; peerRoot=$settings.peerRoot }
  $candidate.projects = @($candidate.projects) + @(@{ id='snapshot-fixture'; workspaceRoot=$workspace; allowDirectCommands=$true; maxConcurrentMutations=1; recipes=@(@{id='snapshot-build'; executable='cargo.exe'; arguments=@('build','--locked','--offline','--target-dir',(Join-Path $workspace 'build-cache')); workingDirectory='.'; timeoutMs=180000}) })
  Write-NewJson $workerPath $candidate
}
$actual = Get-Content -Raw -Encoding UTF8 -LiteralPath $workerPath | ConvertFrom-Json
if ($actual.instanceId -ne $base.instanceId -or $actual.stateDirectory -ne $base.stateDirectory) { throw 'Selected snapshot config must keep desktop IPC identity/state; configs preserved' }
$workerExe = Join-Path $runtime 'winbridge-worker.exe'
& $workerExe --config $workerPath validate
if ($LASTEXITCODE -ne 0) { throw 'Snapshot worker configuration validation failed' }
$runId = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$localReport = Join-Path $RepoRoot ('.winbridge\snapshot-validation\'+$runId+'\setup-report.json')
$report = @{schemaVersion=1;platform='windows';preparationStatus='prepared';runId=$runId;settingsPath=$SettingsPath;workerConfigPath=$workerPath;runtimeDirectory=$runtime;stateDirectory=$actual.stateDirectory;controllerUrl=$actual.worker.controllerUrl;projects=@($actual.projects);delivery=$actual.delivery}
Write-NewJson $localReport $report
try { Write-NewJson (Join-Path $settings.reportDirectory ($runId+'\setup-report.json')) $report }
catch { Write-Warning "Shared report unavailable. Local report: $localReport. $($_.Exception.Message)" }
Write-Host "Snapshot worker config: $workerPath (original worker/desktop configs preserved)"
Write-Host "Shared report: $(Join-Path $settings.reportDirectory $runId)"
Write-Host "Start: & '$workerExe' --config '$workerPath' serve"
if (-not $PrepareOnly) {
  if (Get-SessionWorkerInfo $workerExe $workerPath) { throw 'An instance is already running; stop it before selecting the snapshot configuration' }
  $active = @(Get-Process -Name 'winbridge-worker' -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.Equals($workerExe,[StringComparison]::OrdinalIgnoreCase) })
  if ($active.Count) { throw 'Fixed worker is already running but IPC is unavailable; no second instance started' }
  if (-not @(Get-Process -Name 'winbridge-desktop' -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.Equals((Join-Path $runtime 'winbridge-desktop.exe'),[StringComparison]::OrdinalIgnoreCase) }).Count) { Start-Process -FilePath (Join-Path $runtime 'winbridge-desktop.exe') }
  & $workerExe --config $workerPath serve
  if ($LASTEXITCODE -ne 0) { throw "Worker exited with code $LASTEXITCODE" }
}
