param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$RuntimeDirectory,
  [Parameter(Mandatory)][string]$ExpectedWorkerSha256,
  [Parameter(Mandatory)][string]$ExpectedSourceManifestSha256,
  [Parameter(Mandatory)][string]$ReportPath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows file-sharing test required' }
$RuntimeDirectory = [IO.Path]::GetFullPath($RuntimeDirectory)
$SourceDirectory = [IO.Path]::GetFullPath($SourceDirectory)
$scratch = Split-Path $RuntimeDirectory -Parent
$run = Split-Path $scratch -Parent
if ((Split-Path $RuntimeDirectory -Leaf) -cne 'runtime' -or (Split-Path $scratch -Leaf) -cne 'runtime-startup-test' -or (Split-Path $run -Leaf) -notmatch '^\d{8}-\d{6}-[0-9a-f]{8}$' -or (Split-Path (Split-Path $run -Parent) -Leaf) -cne 'workflow-validation' -or -not $SourceDirectory.Equals((Join-Path $run 'winbridge'),[StringComparison]::OrdinalIgnoreCase)) { throw 'Only the isolated startup fixture of this validation run is allowed' }
$sourceManifestPath = Join-Path $SourceDirectory 'winbridge-source-manifest.json'
if ($ExpectedSourceManifestSha256 -notmatch '^[0-9a-f]{64}$' -or (Get-Item -LiteralPath $sourceManifestPath).Length -gt 4194304 -or (Get-FileHash -LiteralPath $sourceManifestPath).Hash.ToLowerInvariant() -cne $ExpectedSourceManifestSha256) { throw 'Pinned source manifest required before loading test helpers' }
$sourceManifest = Get-Content -LiteralPath $sourceManifestPath -Raw | ConvertFrom-Json
foreach ($relative in @('scripts/session-runtime.ps1','scripts/worker-cli.ps1')) {
  $entry = @($sourceManifest.files | Where-Object { $_.path -ceq $relative })
  $path = Join-Path $SourceDirectory $relative
  if ($entry.Count -ne 1 -or (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $entry[0].sha256) { throw 'Test helper differs from verified source' }
}
. (Join-Path $SourceDirectory 'scripts/session-runtime.ps1')
. (Join-Path $SourceDirectory 'scripts/worker-cli.ps1')
Assert-SessionDirectory $RuntimeDirectory
$worker = Join-Path $RuntimeDirectory 'winbridge-worker.exe'
$configPath = Join-Path $RuntimeDirectory 'winbridge-worker.local.json'
$manifestPath = Join-Path $RuntimeDirectory 'winbridge-installed-set.json'
foreach ($path in @($worker,$configPath,$manifestPath)) { Assert-SessionFile $path }
if ($ExpectedWorkerSha256 -notmatch '^[0-9a-f]{64}$' -or (Get-FileHash -LiteralPath $worker).Hash.ToLowerInvariant() -cne $ExpectedWorkerSha256) { throw 'Expected actual candidate worker image required' }
$configBytes = [IO.File]::ReadAllBytes($configPath)
$config = [Text.Encoding]::UTF8.GetString($configBytes) | ConvertFrom-Json
$state = Join-Path $scratch 'worker-state'
if (-not ([IO.Path]::GetFullPath($config.stateDirectory)).Equals($state,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $state) -or $config.projects.Count -ne 1 -or -not ([IO.Path]::GetFullPath($config.projects[0].workspaceRoot)).Equals($SourceDirectory,[StringComparison]::OrdinalIgnoreCase) -or $config.projects[0].recipes.Count -ne 0 -or $config.worker.PSObject.Properties['controllerUrl']) { throw 'Expected unused, local-only generated startup config' }
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.artifacts[0].descriptor.build.sourceManifestSha256 -cne $ExpectedSourceManifestSha256) { throw 'Known runtime belongs to another source generation' }
$journal = Join-Path $RuntimeDirectory 'update-state/updates.sqlite'
$journalSha = (Get-FileHash -LiteralPath $journal).Hash
$hashes = [ordered]@{}
foreach ($name in @('winbridge-installed-set.json','winbridge-worker.exe','winbridge-fixture.exe','winbridge-desktop.exe','winbridge-update.exe')) {
  $path = Join-Path $RuntimeDirectory $name
  Assert-SessionFile $path
  $hashes[$name] = (Get-FileHash -LiteralPath $path).Hash
}
# Generated test config only. Keep the original as a create-only fixture artifact.
$configBackup = Join-Path $scratch 'worker-config-before-lifetime.json'
if (Test-Path -LiteralPath $configBackup) { throw 'Lifetime fixture was already attempted; preserved' }
$stream = [IO.File]::Open($configBackup,[IO.FileMode]::CreateNew)
try { $stream.Write($configBytes,0,$configBytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
$config.instanceId = 'runtime-life-'+[Guid]::NewGuid().ToString('N')
$config.projects[0].allowDirectCommands = $false
[IO.File]::WriteAllText($configPath,($config | ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))
$start = [Diagnostics.ProcessStartInfo]::new($worker)
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
$start.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
foreach ($argValue in @('--config',$configPath,'serve')) { $start.ArgumentList.Add($argValue) }
$process = [Diagnostics.Process]::new()
$process.StartInfo = $start
$stdout = $null; $stderr = $null; $started = $false; $outcome = $null
$timer = [Diagnostics.Stopwatch]::StartNew()
try {
  $null = $process.Start(); $started = $true
  $ownedPid = $process.Id
  $stdout = $process.StandardOutput.ReadToEndAsync()
  $stderr = $process.StandardError.ReadToEndAsync()
  $info = $null; $lastError = ''
  while ($timer.Elapsed.TotalSeconds -lt 120) {
    if ($process.HasExited) { throw 'Isolated worker exited before readiness' }
    try { $info = (Invoke-WorkerJson $worker $configPath @('info')).info; break } catch { $lastError = $_.Exception.Message }
    Start-Sleep -Milliseconds 100
  }
  if (-not $info -or $info.instanceId -cne $config.instanceId -or $info.build.sourceCommit -cne $manifest.artifacts[0].descriptor.build.sourceCommit -or $info.build.buildId -cne $manifest.artifacts[0].descriptor.build.buildId -or $info.activeTaskCount -ne 0) { throw "Actual isolated worker readiness mismatch: $lastError" }
  $denied = @()
  foreach ($name in $hashes.Keys) {
    $handle = $null; $rejected = $false
    try { $handle = [IO.File]::Open((Join-Path $RuntimeDirectory $name),[IO.FileMode]::Open,[IO.FileAccess]::Write,[IO.FileShare]::ReadWrite) } catch [IO.IOException] { $rejected = $true } finally { if ($handle) { $handle.Dispose() } }
    if (-not $rejected) { throw "Known-runtime handle was not retained: $name" }
    $denied += $name
  }
  # This tests lifetime only. No task was accepted; forced stop of this exact
  # owned test process is not a graceful update shutdown/activation proof.
  $process.Kill($true)
  if (-not $process.WaitForExit(15000)) { throw 'Owned fixture exit unconfirmed' }
  $released = @()
  foreach ($name in $hashes.Keys) {
    $handle = [IO.File]::Open((Join-Path $RuntimeDirectory $name),[IO.FileMode]::Open,[IO.FileAccess]::Write,[IO.FileShare]::ReadWrite)
    $handle.Dispose()
    if ((Get-FileHash -LiteralPath (Join-Path $RuntimeDirectory $name)).Hash -cne $hashes[$name]) { throw "Lifetime test changed bytes: $name" }
    $released += $name
  }
  if ((Get-FileHash -LiteralPath $journal).Hash -cne $journalSha) { throw 'Worker startup changed updater DB' }
  $outcome = @{schemaVersion=1;kind='worker_runtime_lifetime_proof';passed=$true;platform='windows';actualWorkerMain=$true;processId=$ownedPid;instanceId=$config.instanceId;generation=$info.generation;build=$info.build;imageSha256=$ExpectedWorkerSha256;deniedWhileRunning=$denied;releasedAfterExit=$released;ownedProcessStopped=$true;forcedFixtureStop=$true;workerJournalCreated=$true;updaterJournalChanged=$false;acceptedTaskCount=0;gracefulShutdownVerified=$false;activationVerified=$false;runtimeApplied=$false;windowsOpened=$false;elapsedMs=$timer.ElapsedMilliseconds}
} catch {
  $outcome = @{schemaVersion=1;kind='worker_runtime_lifetime_failure';passed=$false;error=$_.Exception.Message;runtimeApplied=$false;windowsOpened=$false}
  throw
} finally {
  if ($started -and -not $process.HasExited) { $process.Kill($true); if (-not $process.WaitForExit(15000)) { throw 'Owned fixture cleanup unconfirmed' } }
  if ($stdout) { $outcome.stdout = $stdout.GetAwaiter().GetResult() }
  if ($stderr) { $outcome.stderr = $stderr.GetAwaiter().GetResult() }
  $process.Dispose()
  if ($outcome) { Write-NewJson $ReportPath $outcome }
  if ($outcome) { Write-NewJson ($ReportPath+'.ready.json') @{schemaVersion=1;kind='worker_runtime_lifetime_ready';passed=$outcome.passed;files=@(@{path=(Split-Path $ReportPath -Leaf);bytes=(Get-Item -LiteralPath $ReportPath).Length;sha256=(Get-FileHash -LiteralPath $ReportPath).Hash.ToLowerInvariant()})} }
}
Write-Output 'Actual worker serve retained all five pins; exact owned idle fixture stopped, handles released, bytes preserved'
