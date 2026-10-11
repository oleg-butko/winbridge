param(
  [string]$RepoRoot = (Split-Path $PSScriptRoot -Parent),
  [string]$SettingsPath = '',
  [switch]$SkipBuild
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$RepoRoot = [IO.Path]::GetFullPath($RepoRoot)
$runId = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
$runRoot = $null
$reportRoot = $null
$configPath = $null
$desktopConfigs = [Collections.Generic.List[object]]::new()
$workerProcess = $null
$ipcAclVerified = $false
$results = [Collections.Generic.List[object]]::new()
$utf8 = [Text.UTF8Encoding]::new($false)

function Write-Json($Path, $Value) {
  [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 30), $utf8)
}
function Assert-That([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw $Message }
}
function Resolve-Directory([string]$Value, [string]$Base, [string]$Field) {
  Assert-That (-not [string]::IsNullOrWhiteSpace($Value)) "$Field must contain a directory path"
  Assert-That ($Value -notmatch '^[A-Za-z]:($|[^\\/])') "$Field must not use a drive-relative path"
  if ([IO.Path]::IsPathRooted($Value)) { return [IO.Path]::GetFullPath($Value) }
  return [IO.Path]::GetFullPath((Join-Path $Base $Value))
}
function Contains-Directory([string]$Parent, [string]$Child) {
  $parentPath = $Parent.TrimEnd([char[]]@('\', '/'))
  $childPath = $Child.TrimEnd([char[]]@('\', '/'))
  return $childPath.Equals($parentPath, [StringComparison]::OrdinalIgnoreCase) -or $childPath.StartsWith(($parentPath + [IO.Path]::DirectorySeparatorChar), [StringComparison]::OrdinalIgnoreCase)
}
function Initialize-Validation {
  if (-not $script:SettingsPath) { $script:SettingsPath = Join-Path $script:RepoRoot 'target\debug\winbridge-validation.local.json' }
  $script:SettingsPath = [IO.Path]::GetFullPath($script:SettingsPath)
  if (-not (Test-Path -LiteralPath $script:SettingsPath)) {
    $parent = Split-Path $script:SettingsPath -Parent
    $null = New-Item -ItemType Directory -Path $parent -Force
    $defaults = Get-Content -LiteralPath (Join-Path $script:RepoRoot 'examples\windows-validation.config.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    Write-Json $script:SettingsPath $defaults
  }
  $settings = Get-Content -LiteralPath $script:SettingsPath -Raw -Encoding UTF8 | ConvertFrom-Json
  $fields = @($settings.PSObject.Properties.Name)
  foreach ($field in @('schemaVersion', 'reportDirectory', 'fixtureDirectory')) { Assert-That ($fields -contains $field) "Missing $field in $script:SettingsPath" }
  Assert-That (@($fields | Where-Object { $_ -notin @('schemaVersion', 'reportDirectory', 'fixtureDirectory') }).Count -eq 0) "Unknown fields in $script:SettingsPath"
  Assert-That ($settings.schemaVersion -eq 1) "Unsupported schemaVersion in $script:SettingsPath; expected 1"
  Assert-That ($settings.reportDirectory -is [string] -and $settings.fixtureDirectory -is [string]) 'reportDirectory and fixtureDirectory must be strings'
  $reports = Resolve-Directory $settings.reportDirectory $script:RepoRoot 'reportDirectory'
  $fixtures = Resolve-Directory $settings.fixtureDirectory $script:RepoRoot 'fixtureDirectory'
  Assert-That (-not $fixtures.StartsWith('\\')) 'fixtureDirectory must be on a local fixed disk'
  Assert-That (-not ((Contains-Directory $reports $fixtures) -or (Contains-Directory $fixtures $reports))) 'reportDirectory and fixtureDirectory must be separate directories; SQLite and execution stay outside shared reports'
  $script:runRoot = Join-Path $fixtures $script:runId
  $script:reportRoot = Join-Path $reports $script:runId
  $script:projectRoot = Join-Path $script:runRoot 'project'
  $script:configPath = Join-Path $script:runRoot 'worker.local.json'
  $null = New-Item -ItemType Directory -Path $script:projectRoot
  $null = New-Item -ItemType Directory -Path $script:reportRoot
  Write-Host "Validation settings: $script:SettingsPath"
  Write-Host "Shared reports: $script:reportRoot"
  Write-Host "Local execution and journal: $script:runRoot"
}
function Prepare-DesktopConfig([string]$Directory) {
  $null = New-Item -ItemType Directory -Path $Directory -Force
  $path = Join-Path $Directory 'winbridge-desktop.local.json'
  $created = -not (Test-Path -LiteralPath $path)
  if ($created) {
    Write-Json $path @{ schemaVersion = 1; workerConfigPath = $script:configPath }
  } else {
    Write-Host "Existing desktop config kept: $path. For this run, set workerConfigPath to '$script:configPath'."
  }
  $script:desktopConfigs.Add(@{ path = $path; created = $created; suggestedWorkerConfigPath = $script:configPath })
}
function Invoke-Worker([string[]]$Operation, [switch]$ExpectError) {
  $start = [Diagnostics.ProcessStartInfo]::new()
  $start.FileName = $script:workerExe
  # These are CLI operation names, UUIDs, numeric offsets and existing JSON file
  # paths, never user executable/argv. File paths cannot contain a quote on Windows.
  $start.Arguments = ((@('--config', $script:configPath) + $Operation) | ForEach-Object { '"' + $_ + '"' }) -join ' '
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $start.StandardOutputEncoding = $script:utf8
  $start.StandardErrorEncoding = $script:utf8
  $client = [Diagnostics.Process]::new()
  $client.StartInfo = $start
  try {
    Assert-That ($client.Start()) 'Could not start worker client'
    $stdout = $client.StandardOutput.ReadToEndAsync()
    $stderr = $client.StandardError.ReadToEndAsync()
    if (-not $client.WaitForExit(15000)) { $client.Kill(); $client.WaitForExit(); throw 'Worker client exceeded its IPC deadline' }
    $text = $stdout.GetAwaiter().GetResult()
    $errorText = $stderr.GetAwaiter().GetResult()
    [IO.File]::WriteAllText((Join-Path $script:reportRoot 'client-stderr.log'), $errorText, $script:utf8)
    $exit = $client.ExitCode
  } finally { $client.Dispose() }
  if ($exit -ne 0 -and -not $ExpectError) { throw "worker client failed ($exit): $text; see client-stderr.log" }
  if (-not $text) { throw "worker returned no JSON; see client-stderr.log" }
  $response = ($text -join "`n") | ConvertFrom-Json
  if ($ExpectError) { Assert-That ($response.kind -eq 'error') 'Expected a policy error' }
  elseif ($response.kind -eq 'error') { throw ('Worker error: ' + $response.message) }
  return $response
}
function Start-Worker {
  $script:workerProcess = Start-Process -FilePath $script:workerExe -ArgumentList @('--config', ('"' + $script:configPath + '"'), 'serve') -PassThru -RedirectStandardOutput (Join-Path $script:reportRoot 'worker-stdout.log') -RedirectStandardError (Join-Path $script:reportRoot 'worker-stderr.log')
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  do {
    if ($script:workerProcess.HasExited) { throw 'Worker exited before IPC became ready; see worker-stderr.log' }
    try { $info = Invoke-Worker -Operation @('info'); return $info.info } catch { Start-Sleep -Milliseconds 100 }
  } while ([DateTime]::UtcNow -lt $deadline)
  throw 'Worker startup deadline exceeded'
}
function New-Request([string]$Key, [string]$Mode, [int]$Timeout = 10000) {
  return [ordered]@{ protocolVersion = 1; requestId = 'request-' + $Key; projectId = 'fixture'; idempotencyKey = $Key; timeoutMs = $Timeout;
    command = [ordered]@{ kind = 'direct'; executable = $script:fixtureExe; arguments = @($Mode); workingDirectory = '.' } }
}
function Submit($Request, [switch]$ExpectError) {
  $path = Join-Path $script:reportRoot ('request-' + [Guid]::NewGuid().ToString('N') + '.json')
  Write-Json $path $Request
  $response = Invoke-Worker -Operation @('submit', $path) -ExpectError:$ExpectError
  if ($ExpectError) { return $response }
  return $response.task
}
function Wait-Task([string]$Id, [string]$Expected) {
  $deadline = [DateTime]::UtcNow.AddSeconds(25)
  do {
    $task = (Invoke-Worker -Operation @('get', $Id)).task
    if ($task.status -notin @('queued', 'running')) {
      Assert-That ($task.status -eq $Expected) ("Task $Id expected $Expected, got " + ($task | ConvertTo-Json -Depth 10 -Compress))
      return $task
    }
    Start-Sleep -Milliseconds 50
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "Task $Id did not finish within 25 seconds"
}
function Read-Output([string]$Id) {
  $chunks = [Collections.Generic.List[object]]::new()
  $bytes = @{ stdout = [Collections.Generic.List[byte]]::new(); stderr = [Collections.Generic.List[byte]]::new() }
  $after = 0
  do {
    $page = @((Invoke-Worker -Operation @('output', $Id, [string]$after)).chunks)
    foreach ($chunk in $page) { $chunks.Add($chunk); $bytes[$chunk.stream].AddRange([byte[]]$chunk.bytes); $after = $chunk.sequence }
  } while ($page.Count -gt 0)
  return @{ chunks = $chunks.ToArray(); stdout = [Text.Encoding]::UTF8.GetString($bytes.stdout.ToArray()); stderr = [Text.Encoding]::UTF8.GetString($bytes.stderr.ToArray()) }
}
function Save-TaskReport($Task, [string]$Name) {
  $output = Read-Output $Task.taskId
  Write-Json (Join-Path $script:reportRoot ($Name + '.task-report.json')) @{ schemaVersion = 1; platform = 'windows'; task = $Task; output = @($output.chunks); outputComplete = $true }
  return $output
}
function Assert-ProcessGone([int]$ProcessId) {
  $deadline = [DateTime]::UtcNow.AddSeconds(5)
  while ((Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
  Assert-That ($null -eq (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) "Managed PID $ProcessId survived tree termination"
}
function Wait-Tree($Task) {
  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  do {
    $current = (Invoke-Worker -Operation @('get', $Task.taskId)).task
    $output = Read-Output $Task.taskId
    if ($output.stdout -match 'child pid=(\d+)' -and $current.processId) { return @{ task = $current; childId = [int]$Matches[1] } }
    Start-Sleep -Milliseconds 50
  } while ([DateTime]::UtcNow -lt $deadline)
  throw 'Fixture process tree was not observed before deadline'
}

function Assert-PipeAcl($Info) {
  if (-not ('WinBridgeValidationNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class WinBridgeValidationNative {
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode, ExactSpelling=true)]
  static extern uint GetNamedSecurityInfoW(string name, int type, uint info, out IntPtr owner, out IntPtr group, out IntPtr dacl, out IntPtr sacl, out IntPtr descriptor);
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode, ExactSpelling=true, SetLastError=true)]
  static extern bool ConvertSecurityDescriptorToStringSecurityDescriptorW(IntPtr descriptor, uint revision, uint info, out IntPtr text, out uint size);
  [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr pointer);
  public static string PipeSddl(string name) {
    IntPtr owner, group, dacl, sacl, descriptor;
    uint error = GetNamedSecurityInfoW(name, 1, 4, out owner, out group, out dacl, out sacl, out descriptor);
    if (error != 0) throw new Win32Exception((int)error);
    try {
      IntPtr text; uint size;
      if (!ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, 1, 4, out text, out size)) throw new Win32Exception(Marshal.GetLastWin32Error());
      try { return Marshal.PtrToStringUni(text); } finally { LocalFree(text); }
    } finally { LocalFree(descriptor); }
  }
}
'@
  }
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  $pipe = '\\.\pipe\WinBridge-' + $sid + '-' + $Info.instanceId
  $sddl = [WinBridgeValidationNative]::PipeSddl($pipe)
  $descriptor = [Security.AccessControl.RawSecurityDescriptor]::new($sddl)
  Assert-That ($descriptor.DiscretionaryAcl.Count -eq 1) 'Named pipe grants access to more than one principal'
  $ace = $descriptor.DiscretionaryAcl[0]
  Assert-That ($ace.SecurityIdentifier.Value -eq $sid) 'Named pipe DACL does not target the current user SID'
  Assert-That (($descriptor.ControlFlags -band [Security.AccessControl.ControlFlags]::DiscretionaryAclProtected) -ne 0) 'Named pipe DACL inherits permissions'
  Assert-That (($ace.AccessMask -band 0x10000000) -ne 0 -or ($ace.AccessMask -band 0x1F01FF) -eq 0x1F01FF) 'Named pipe does not grant the expected user access'
  $script:ipcAclVerified = $true
  return $sddl
}

try {
  Initialize-Validation
  if (-not $SkipBuild) {
    Push-Location $RepoRoot
    try {
      & cargo build --workspace --locked --features winbridge-worker/bundled-sqlite
      Assert-That ($LASTEXITCODE -eq 0) 'Rust build failed'
      & cargo test --workspace --locked --features winbridge-worker/bundled-sqlite
      Assert-That ($LASTEXITCODE -eq 0) 'Rust tests failed on Windows'
    } finally { Pop-Location }
  }
  $workerExe = Join-Path $RepoRoot 'target\debug\winbridge-worker.exe'
  $fixtureExe = Join-Path $RepoRoot 'target\debug\winbridge-fixture.exe'
  Assert-That (Test-Path $workerExe) "Worker executable not found: $workerExe"
  Assert-That (Test-Path $fixtureExe) "Fixture executable not found: $fixtureExe"
  Write-Json $configPath @{ schemaVersion = 1; instanceId = 'validation-' + [Guid]::NewGuid().ToString('N'); nodeId = 'windows-validation'; stateDirectory = (Join-Path $runRoot 'state'); maxOutputBytesPerTask = 4096;
    worker = @{ keepRunningOnUiExit = $true }; projects = @(@{ id = 'fixture'; workspaceRoot = $projectRoot; allowDirectCommands = $true; maxConcurrentMutations = 1;
      recipes = @(@{ id = 'fixture-success'; executable = $fixtureExe; arguments = @('success'); workingDirectory = '.'; timeoutMs = 10000 }) }) }
  Write-Json (Join-Path $runRoot 'winbridge-desktop.local.json') @{ schemaVersion = 1; workerConfigPath = $configPath }
  Write-Json (Join-Path $reportRoot 'worker-config.report.json') (Get-Content -LiteralPath $configPath -Raw -Encoding UTF8 | ConvertFrom-Json)
  Prepare-DesktopConfig (Join-Path $RepoRoot 'apps\desktop\src-tauri\target\release')
  Prepare-DesktopConfig (Join-Path $RepoRoot 'apps\desktop\src-tauri\target\debug')
  $info = Start-Worker
  Assert-That ($info.platform -eq 'windows') 'Native Windows worker is required'
  $results.Add(@{ check = 'local-ipc'; passed = ($info.platform -eq 'windows'); info = $info })
  $acl = Assert-PipeAcl $info
  $second = Start-Process -FilePath $workerExe -ArgumentList @('--config', ('"' + $configPath + '"'), 'serve') -PassThru -RedirectStandardOutput (Join-Path $reportRoot 'second-worker-stdout.log') -RedirectStandardError (Join-Path $reportRoot 'second-worker-stderr.log')
  if (-not $second.WaitForExit(5000)) { Stop-Process -Id $second.Id -Force; throw 'A second worker acquired the same stateDirectory' }
  Assert-That ($second.ExitCode -ne 0) 'A second worker unexpectedly succeeded'
  $results.Add(@{ check = 'user-sid-pipe-dacl-exclusive-worker'; passed = $true; sddl = $acl })

  $request = New-Request 'success' 'success'
  $accepted = Submit $request
  $request.requestId = 'reconnect-success'
  $duplicate = Submit $request
  Assert-That ($accepted.taskId -eq $duplicate.taskId) 'Duplicate request created a different task'
  $success = Wait-Task $accepted.taskId 'succeeded'
  Assert-That ($success.exitCode -eq 0) 'Success exit code is incorrect'
  $output = Save-TaskReport $success 'success'
  Assert-That ($output.stdout.Contains('stdout: success')) 'stdout was not saved'
  Assert-That ($output.stderr.Contains('stderr: diagnostic')) 'stderr was not saved'
  Assert-That (@(Get-Content (Join-Path $projectRoot 'runs.log')).Count -eq 1) 'Duplicate launched a second process'
  $conflict = New-Request 'success' 'failure'
  $null = Submit $conflict -ExpectError
  $results.Add(@{ check = 'success-output-dedup-payload-conflict'; passed = $true; taskId = $success.taskId })

  $literal = New-Request 'arguments' 'arguments'
  $literal.command.arguments = @('arguments', 'a & b', '$value', 'a path with spaces', 'embedded"quote', 'C:\path\', ('UTF8-' + [char]0x2713))
  $literalTask = Wait-Task (Submit $literal).taskId 'succeeded'
  $literalOutput = Save-TaskReport $literalTask 'arguments'
  $line = @($literalOutput.stdout -split "`n" | Where-Object { $_.StartsWith('[') })[0]
  $actualArgs = @($line | ConvertFrom-Json)
  for ($i = 0; $i -lt $actualArgs.Count; $i++) { Assert-That ($actualArgs[$i] -ceq $literal.command.arguments[$i + 1]) "Windows argument $i changed during execution" }
  Assert-That ($actualArgs.Count -eq ($literal.command.arguments.Count - 1)) 'Windows argument count changed'
  $results.Add(@{ check = 'literal-windows-argv-unicode'; passed = $true })

  $failure = Wait-Task (Submit (New-Request 'failure' 'failure')).taskId 'failed'
  Assert-That ($failure.exitCode -eq 7) 'Failure exit code was not preserved'
  $null = Save-TaskReport $failure 'failure'
  $recipe = New-Request 'recipe' 'success'; $recipe.command = @{ kind = 'recipe'; recipeId = 'fixture-success'; arguments = @() }
  $null = Wait-Task (Submit $recipe).taskId 'succeeded'
  $results.Add(@{ check = 'failure-and-recipe'; passed = $true })

  $flood = Wait-Task (Submit (New-Request 'flood' 'flood')).taskId 'succeeded'
  Assert-That ($flood.outputTruncated -and $flood.outputBytes -eq 4096) 'Output quota was not recorded honestly'
  $results.Add(@{ check = 'both-pipes-flood-without-ui'; passed = $true })

  $timeout = Wait-Task (Submit (New-Request 'timeout' 'tree' 1500)).taskId 'failed'
  Assert-That ($timeout.reason -match 'timeout') 'Timeout reason missing'
  Assert-ProcessGone $timeout.processId
  $timeoutOutput = Read-Output $timeout.taskId
  if ($timeoutOutput.stdout -match 'child pid=(\d+)') { Assert-ProcessGone ([int]$Matches[1]) }
  $results.Add(@{ check = 'timeout-tree'; passed = $true })

  $tree = Wait-Tree (Submit (New-Request 'cancel' 'tree' 60000))
  $queued = Submit (New-Request 'queued' 'success')
  Assert-That ($queued.status -eq 'queued') 'Project queue allowed overlapping commands'
  $queuedCancel = (Invoke-Worker -Operation @('cancel', $queued.taskId)).task
  Assert-That ($queuedCancel.status -eq 'cancelled') 'Queued cancellation failed'
  $cancel = (Invoke-Worker -Operation @('cancel', $tree.task.taskId)).task
  Assert-That ($cancel.cancelRequested -or $cancel.status -eq 'cancelled') 'Cancel acceptance missing'
  $cancelled = Wait-Task $tree.task.taskId 'cancelled'
  Assert-ProcessGone $tree.task.processId
  Assert-ProcessGone $tree.childId
  $null = Save-TaskReport $cancelled 'cancelled'
  $results.Add(@{ check = 'project-queue-and-tree-cancel'; passed = $true })

  foreach ($cwd in @('..', 'NUL.txt', 'C:\Windows', '\\server\share')) {
    $bad = New-Request ('bad-' + [Guid]::NewGuid().ToString('N')) 'success'; $bad.command.workingDirectory = $cwd
    $null = Submit $bad -ExpectError
  }
  $outside = Join-Path $runRoot 'outside'; New-Item -ItemType Directory -Path $outside | Out-Null
  New-Item -ItemType Junction -Path (Join-Path $projectRoot 'escape') -Target $outside | Out-Null
  $junction = New-Request 'junction' 'success'; $junction.command.workingDirectory = 'escape'
  $null = Submit $junction -ExpectError
  $wrongVersion = New-Request 'wrong-version' 'success'; $wrongVersion.protocolVersion = 999
  $null = Submit $wrongVersion -ExpectError
  $results.Add(@{ check = 'traversal-reserved-unc-junction-wire-version'; passed = $true })

  $crashRequest = New-Request 'crash' 'tree' 60000
  $crash = Wait-Tree (Submit $crashRequest)
  Stop-Process -Id $workerProcess.Id -Force
  $workerProcess.WaitForExit()
  Assert-ProcessGone $crash.task.processId
  Assert-ProcessGone $crash.childId
  $before = @(Get-Content (Join-Path $projectRoot 'runs.log')).Count
  $null = Start-Worker
  $recovered = (Invoke-Worker -Operation @('get', $crash.task.taskId)).task
  Assert-That ($recovered.status -eq 'indeterminate') 'Crash recovery invented a known outcome'
  $crashRequest.requestId = 'reconnected-crash'
  $same = Submit $crashRequest
  Assert-That ($same.taskId -eq $recovered.taskId) 'Recovery duplicate created a new task'
  Start-Sleep -Milliseconds 400
  Assert-That (@(Get-Content (Join-Path $projectRoot 'runs.log')).Count -eq $before) 'Indeterminate command was silently relaunched'
  $null = Save-TaskReport $recovered 'indeterminate'
  Assert-That ((Invoke-Worker -Operation @('get', $success.taskId)).task.status -eq 'succeeded') 'Terminal result did not survive restart'
  $results.Add(@{ check = 'worker-crash-job-cleanup-recovery-no-retry'; passed = $true })

  Write-Host "Runtime checks passed. Shared reports: $reportRoot"
  Write-Host "For the UI check, start worker with: & '$workerExe' --config '$configPath' serve"
  Write-Host "Desktop reads winbridge-desktop.local.json beside its exe. Run npm run build:desktop; see docs/validation/windows-validation.md."
} catch {
  $results.Add(@{ check = 'exception'; passed = $false; message = $_.Exception.Message; detail = ($_ | Out-String) })
  Write-Host "Validation failed: $($_.Exception.Message). Report directory: $reportRoot"
  throw
} finally {
  if ($workerProcess -and -not $workerProcess.HasExited) { Stop-Process -Id $workerProcess.Id -Force; $workerProcess.WaitForExit() }
  if ($runRoot -and (Test-Path -LiteralPath $runRoot)) {
    $report = @{ schemaVersion = 1; platform = 'windows'; occurredAt = [DateTime]::UtcNow.ToString('o'); checks = $results.ToArray(); uiVerified = $false; ipcAclVerified = $ipcAclVerified; runRoot = $runRoot; reportRoot = $reportRoot; settingsPath = $SettingsPath; workerConfigPath = $configPath; desktopConfigs = $desktopConfigs.ToArray() }
    # Keep a local diagnostic if shared disappears during the run.
    Write-Json (Join-Path $runRoot 'validation-report.json') $report
    try { Write-Json (Join-Path $reportRoot 'validation-report.json') $report }
    catch {
      $report['reportPublicationError'] = $_.Exception.Message
      Write-Json (Join-Path $runRoot 'validation-report.json') $report
      Write-Warning "Cannot publish report to shared: $($_.Exception.Message). Local report: $runRoot\validation-report.json"
      throw
    }
  }
}
