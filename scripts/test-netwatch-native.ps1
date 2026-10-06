<#
.SYNOPSIS
Runs the prepared, isolated Netwatch snapshot through a separately built test worker.
.DESCRIPTION
Called by test-windows-workflow.ps1. Does not replace the installed runtime, connect
to Debian, modify Netwatch production files, or request a firewall permission.
#>
param([Parameter(Mandatory)][string]$SourceDirectory, [Parameter(Mandatory)][string]$ReportDirectory,
      [Parameter(Mandatory)][string]$ProfilePath)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$request = Get-Content -LiteralPath $ProfilePath -Raw | ConvertFrom-Json
$instance = $request.readiness.netwatch.instanceId
if ($instance -notmatch '^winbridge-[A-Za-z0-9-]{1,50}$') { throw 'Netwatch validation requires an isolated instance ID' }
$runtimeArg = [array]::IndexOf($request.start.arguments, '--runtime-directory')
if ($runtimeArg -lt 0) { throw 'Isolated runtime argument missing' }
$runtimePath = $request.start.arguments[$runtimeArg + 1]
$relative = $request.readiness.netwatch.runtimeDirectory.Replace('/', '\')
if (-not $runtimePath.EndsWith('\' + $relative)) { throw 'Runtime/project mapping mismatch' }
$root = $runtimePath.Substring(0, $runtimePath.Length - $relative.Length - 1)
if (Test-Path -LiteralPath $root) { throw 'Refusing to overwrite an existing Netwatch validation workspace' }
New-Item -ItemType Directory -Path $root | Out-Null
$configPath = Join-Path $root 'worker.local.json'
$worker = Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-worker.exe'
if (-not (Test-Path -LiteralPath $worker)) { throw 'Test worker binary was not built' }
$config = [ordered]@{ schemaVersion = 1; instanceId = 'netwatch-gate-' + [guid]::NewGuid().ToString('N'); nodeId = 'netwatch-test'; stateDirectory = (Join-Path $root 'state');
    worker = @{ controllerUrl = $null; keepRunningOnUiExit = $true }; delivery = @{ adapter = 'shared-folder'; root = 'K:\VMs\shared'; namespace = 'WinBridge' };
    projects = @(@{ id = $request.projectId; workspaceRoot = $root; allowDirectCommands = $true; maxConcurrentMutations = 1; recipes = @() }) }
$config | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $configPath -Encoding utf8
$process = $null
$application = $null
$result = [ordered]@{ platform = 'windows'; passed = $false; instanceId = $instance; workflow = $null; application = $null; build = $null; log = $null; error = $null }
function Invoke-TestWorker([string[]]$Arguments) {
    $text = & $worker --config $configPath @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Test worker CLI failed: $text" }
    return ($text | ConvertFrom-Json)
}
try {
    # Quotes are required because Start-Process builds one native command-line string.
    $process = Start-Process -FilePath $worker -ArgumentList @('--config', ('"' + $configPath + '"'), 'serve') -NoNewWindow -PassThru -RedirectStandardOutput (Join-Path $ReportDirectory 'netwatch-worker.stdout.log') -RedirectStandardError (Join-Path $ReportDirectory 'netwatch-worker.stderr.log')
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        if ($process.HasExited) { throw 'Isolated worker exited before its IPC was ready' }
        try { $info = Invoke-TestWorker -Arguments @('info'); break } catch { if ([DateTime]::UtcNow -ge $deadline) { throw }; Start-Sleep -Milliseconds 100 }
    } while ($true)
    if ($info.info.workflowFeatures -notcontains 'netwatch_health' -or $info.info.workflowFeatures -notcontains 'artifacts') { throw 'Test worker lacks the new workflow features' }
    $result.build = $info.info.build
    $accepted = Invoke-TestWorker -Arguments @('workflow', $ProfilePath)
    $flowId = $accepted.workflow.workflowId
    $deadline = [DateTime]::UtcNow.AddMinutes(16)
    do {
        $flow = (Invoke-TestWorker -Arguments @('get-workflow', $flowId)).workflow
        if ($flow.status -notin @('queued', 'running')) { break }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Netwatch workflow deadline exceeded' }
        Start-Sleep -Milliseconds 500
    } while ($true)
    $result.workflow = $flow
    foreach ($step in $flow.steps | Where-Object { $_.name -in @('build', 'start') -and $_.operationId }) {
        $task = Invoke-TestWorker -Arguments @('get', $step.operationId)
        $task | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath (Join-Path $ReportDirectory ('netwatch-' + $step.name + '-task.json')) -Encoding utf8
        & $worker --config $configPath output $step.operationId | Set-Content -LiteralPath (Join-Path $ReportDirectory ('netwatch-' + $step.name + '-output.json')) -Encoding utf8
        if ($LASTEXITCODE -ne 0) { throw 'Could not capture Netwatch output' }
    }
    if ($flow.status -ne 'succeeded') { throw "Netwatch workflow failed: $($flow.reason)" }
    if ($flow.artifacts.Count -ne 1 -or $flow.artifacts[0].sourceCommit -ne $flow.submission.snapshotId.Substring(2)) { throw 'Artifact/source association mismatch' }
    $artifact = $flow.artifacts[0]
    if ((Get-FileHash -LiteralPath $artifact.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $artifact.sha256) { throw 'Netwatch artifact checksum mismatch' }
    $application = (Invoke-TestWorker -Arguments @('get-application', $flow.applicationId)).application
    if ($application.status -ne 'ready' -or $application.reason -notlike '*owned PID*') { throw 'Netwatch IPC readiness was not confirmed' }
    $pidValue = $application.processId
    $log = (Invoke-TestWorker -Arguments @('log', $application.applicationId)).page
    $decoded = [Text.Encoding]::UTF8.GetString([byte[]]$log.bytes)
    if ($decoded -notlike '*Configuration loaded: node=winbridge-netwatch-validation*' -or $decoded -notlike '*IPC listener ready*') { throw 'Actual isolated Netwatch log not found' }
    $result.log = $log
    $repeated = Invoke-TestWorker -Arguments @('workflow', $ProfilePath)
    if ($repeated.workflow.workflowId -ne $flowId) { throw 'Duplicate Netwatch workflow launched another instance' }
    $null = Invoke-TestWorker -Arguments @('stop-application', $application.applicationId)
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $application = (Invoke-TestWorker -Arguments @('get-application', $application.applicationId)).application
        if ($application.status -eq 'stopped') { break }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Netwatch managed process stop deadline exceeded' }
        Start-Sleep -Milliseconds 100
    } while ($true)
    if (Get-Process -Id $pidValue -ErrorAction SilentlyContinue) { throw 'Owned Netwatch process survived stop' }
    if ((Invoke-TestWorker -Arguments @('get-workflow', $flowId)).workflow.status -ne 'succeeded') { throw 'Application stop rewrote startup history' }
    $result.application = $application
    $result.passed = $true
} catch {
    $result.error = $_.Exception.Message
    throw
} finally {
    if ($application -and $application.status -notin @('stopped', 'exited', 'failed', 'indeterminate')) {
        try { $null = Invoke-TestWorker -Arguments @('stop-application', $application.applicationId) } catch { Write-Warning $_ }
    }
    # This PID belongs only to this script; Windows Job closure stops its owned tree.
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
    $result | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'netwatch-report.json') -Encoding utf8
}
