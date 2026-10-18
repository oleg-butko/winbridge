param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$CandidateDirectory,
  [Parameter(Mandatory)][string]$ScratchRoot,
  [Parameter(Mandatory)][string]$ReportPath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/update-stage.ps1')
# Actual product executables, isolated create-only runtime. No serve success,
# scheduler, controller connection, GUI, installed runtime or journal changes.
foreach ($path in @($SourceDirectory,$CandidateDirectory,$ScratchRoot)) { Assert-SessionDirectory $path }
if (Test-Path -LiteralPath $ScratchRoot) { throw 'Startup scratch already exists; preserved' }
[IO.Directory]::CreateDirectory($ScratchRoot) | Out-Null
$runtime = Join-Path $ScratchRoot 'runtime'
$sources = [ordered]@{}
foreach ($name in @('winbridge-worker.exe','winbridge-fixture.exe','winbridge-desktop.exe','winbridge-update.exe')) { $sources[$name] = Join-Path $CandidateDirectory "payload/$name" }
$copies = @(New-UpdatePayload $sources $runtime)
$worker = Join-Path $runtime 'winbridge-worker.exe'
$updater = Join-Path $runtime 'winbridge-update.exe'
$workerConfig = Join-Path $runtime 'winbridge-worker.local.json'
$updaterConfig = Join-Path $runtime 'winbridge-update.local.json'
$state = Join-Path $ScratchRoot 'worker-state'
Write-NewJson $workerConfig @{schemaVersion=1;instanceId='update-startup-test';nodeId='test';stateDirectory=$state;maxOutputBytesPerTask=4096;worker=@{keepRunningOnUiExit=$true};projects=@(@{id='fixture';workspaceRoot=$SourceDirectory;allowDirectCommands=$true;maxConcurrentMutations=1;recipes=@()})}
Write-NewJson $updaterConfig @{schemaVersion=1;sourceDirectory=$SourceDirectory;runtimeDirectory=$runtime;workerConfigPath=$workerConfig}
$commands = [Collections.Generic.List[object]]::new()
function Invoke-StartupProcess([string]$Executable,[string[]]$Arguments) {
  $start = [Diagnostics.ProcessStartInfo]::new($Executable)
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $start.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
  $start.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
  foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
  $process = [Diagnostics.Process]::new()
  $process.StartInfo = $start
  $timer = [Diagnostics.Stopwatch]::StartNew()
  try {
    $null = $process.Start()
    $pidValue = $process.Id
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(120000)) {
      $process.Kill($true); $process.WaitForExit()
      throw 'Isolated startup verification timed out; owned process tree stopped'
    }
    $result = @{executable=$Executable;arguments=$Arguments;processId=$pidValue;exitCode=$process.ExitCode;elapsedMs=$timer.ElapsedMilliseconds;stdout=$stdout.GetAwaiter().GetResult();stderr=$stderr.GetAwaiter().GetResult()}
    $commands.Add($result)
    return $result
  } finally { $process.Dispose() }
}
$startupFailure = $null
try {
  $init = Invoke-StartupProcess $updater @('--config',$updaterConfig,'journal','init')
  if ($init.exitCode -ne 0) { throw "Isolated updater init failed: $($init.stderr)" }
  $info = ($init.stdout | ConvertFrom-Json).info
  $journal = Join-Path $runtime 'update-state/updates.sqlite'
  $journalSha = (Get-FileHash -LiteralPath $journal).Hash
  function Assert-Readonly {
    if ((Test-Path -LiteralPath $state) -or (Get-FileHash -LiteralPath $journal).Hash -cne $journalSha) { throw 'Startup opened worker state or changed updater DB' }
  }
  function Assert-Rejected([string]$Operation,[string]$Reason,[string]$Config=$workerConfig) {
    $r = Invoke-StartupProcess $worker @('--config',$Config,$Operation)
    if ($r.exitCode -eq 0 -or -not $r.stderr.Contains($Reason)) { throw "Expected startup rejection '$Reason': $($r.stderr)" }
    Assert-Readonly
  }
  Assert-Rejected 'verify-runtime' 'Bootstrap required'
  $candidate = Get-Content -LiteralPath (Join-Path $CandidateDirectory 'candidate-report.json') -Raw | ConvertFrom-Json
  $artifacts = @()
  foreach ($copy in $copies) {
    $query = @($candidate.artifactIdentities | Where-Object { $_.name -ceq $copy.name })
    if ($query.Count -ne 1 -or $query[0].query.image.sha256 -cne $copy.sha256) { throw 'Startup fixture differs from actual candidate query' }
    $artifacts += @{name=$copy.name;bytes=$copy.bytes;sha256=$copy.sha256;descriptor=$query[0].query.descriptor}
  }
  $manifest = @{schemaVersion=1;kind='winbridge_installed_set';setId=[Guid]::NewGuid().ToString();installationId=$info.installationId;scopeSha256=$info.scopeSha256;artifacts=$artifacts}
  $manifestPath = Join-Path $runtime 'winbridge-installed-set.json'
  Write-NewJson $manifestPath $manifest
  $manifestBytes = [IO.File]::ReadAllBytes($manifestPath)
  $verified = Invoke-StartupProcess $worker @('--config',$workerConfig,'verify-runtime')
  if ($verified.exitCode -ne 0) { throw "Actual worker runtime verification failed: $($verified.stderr)" }
  $proof = $verified.stdout | ConvertFrom-Json
  if ($proof.kind -cne 'worker_runtime_verification' -or $proof.readOnly -ne $true -or $proof.workerJournalOpened -ne $false -or $proof.schedulerStarted -ne $false -or $proof.updateAuthorityEnabled -ne $false -or $proof.identity.process.processId -ne $verified.processId -or $proof.identity.setId -cne $manifest.setId -or $proof.identity.image.sha256 -cne $copies[0].sha256 -or $proof.identity.manifestSha256 -cne (Get-FileHash -LiteralPath $manifestPath).Hash.ToLowerInvariant()) { throw 'Actual loaded-worker identity/readonly proof incomplete' }
  Assert-Readonly
  [IO.File]::WriteAllText($manifestPath,'{}')
  Assert-Rejected 'serve' 'Installed-set manifest JSON rejected'
  [IO.File]::WriteAllBytes($manifestPath,$manifestBytes)
  $foreign = [Text.Encoding]::UTF8.GetString($manifestBytes) | ConvertFrom-Json
  $foreign.installationId = [Guid]::NewGuid().ToString()
  [IO.File]::WriteAllText($manifestPath,($foreign | ConvertTo-Json -Depth 20))
  Assert-Rejected 'serve' 'Installed-set manifest version/installation/scope rejected'
  [IO.File]::WriteAllBytes($manifestPath,$manifestBytes)
  $fixturePath = Join-Path $runtime 'winbridge-fixture.exe'
  $fixtureBytes = [IO.File]::ReadAllBytes($fixturePath)
  [IO.File]::WriteAllText($fixturePath,'tampered test image')
  Assert-Rejected 'serve' 'Installed runtime differs from its known manifest'
  [IO.File]::WriteAllBytes($fixturePath,$fixtureBytes)
  $foreign = [Text.Encoding]::UTF8.GetString($manifestBytes) | ConvertFrom-Json
  $foreign.artifacts[0].descriptor.build.buildId = 'fabricated-worker'
  [IO.File]::WriteAllText($manifestPath,($foreign | ConvertTo-Json -Depth 20))
  Assert-Rejected 'serve' 'Loaded worker path/build does not match'
  [IO.File]::WriteAllBytes($manifestPath,$manifestBytes)
  $alternate = Join-Path $runtime 'alternate-worker.local.json'
  [IO.File]::Copy($workerConfig,$alternate,$false)
  Assert-Rejected 'serve' 'Known installed runtime points to another' $alternate
  Write-NewJson $ReportPath @{schemaVersion=1;kind='worker_runtime_startup_proof';passed=$true;platform=$(if ($env:OS -eq 'Windows_NT') {'windows'} else {'linux'});identity=$proof.identity;actualWorkerMain=$true;knownFixtureManifest=$true;workerJournalOpened=$false;updaterJournalChanged=$false;invalidSetRejectedBeforeJournal=$true;descriptorFabricationRejected=$true;foreignConfigRejected=$true;runtimeApplied=$false;candidateVerified=$false;windowsOpened=$false;commands=$commands.ToArray()}
  Write-Output 'Actual worker startup: loaded identity verified, invalid sets/config rejected before journal, no scheduler/network/UI'
} catch {
  $startupFailure = $_.Exception.Message
  throw
} finally {
  if ($startupFailure) {
    Write-NewJson ($ReportPath+'.failure.json') @{schemaVersion=1;kind='worker_runtime_startup_failure';passed=$false;error=$startupFailure;scratchRoot=$ScratchRoot;workerStateExists=(Test-Path -LiteralPath $state);commands=$commands.ToArray();runtimeApplied=$false;windowsOpened=$false}
  }
}
