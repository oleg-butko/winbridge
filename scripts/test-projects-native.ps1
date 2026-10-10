<#
.SYNOPSIS
Runs Netwatch and WinBridge snapshots through the same profile and isolated worker.
.DESCRIPTION
No desktop, WebSocket or installed runtime changes. All configs, processes and
SQLite journals belong to this new test run. Source commits and adapter patches
are pinned by the preparation JSON. Called only by test-windows-workflow.ps1.
#>
param([Parameter(Mandatory)][string]$ReportDirectory,
      [Parameter(Mandatory)][string]$PreparationPath)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot 'worker-cli.ps1')
$prepared = Get-Content -LiteralPath $PreparationPath -Raw | ConvertFrom-Json
$root = $prepared.windowsRoot
if (Test-Path -LiteralPath $root) { throw 'Refusing to reuse a project validation runtime' }
New-Item -ItemType Directory -Path $root | Out-Null
foreach ($project in $prepared.projects) { New-Item -ItemType Directory -Path $project.workspaceRoot | Out-Null }
$configPath = Join-Path $root 'coordinator.local.json'
$worker = Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-worker.exe'
$config = [ordered]@{ schemaVersion = 1; instanceId = 'projects-gate-' + [guid]::NewGuid().ToString('N')
    nodeId = 'projects-validation'; stateDirectory = (Join-Path $root 'coordinator-state')
    worker = @{ controllerUrl = $null; keepRunningOnUiExit = $true }
    delivery = $prepared.delivery; projects = $prepared.projects; recipeProfiles = $prepared.recipeProfiles }
$config | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $configPath -Encoding utf8
$process = $null
$childRestart = $null
$result = [ordered]@{ schemaVersion = 1; platform = 'windows'; passed = $false; build = $null
    initialGeneration = $null; restartedGeneration = $null; projects = @(); child = $null; error = $null }
function Invoke-Coordinator([string[]]$Arguments) {
    Invoke-WorkerJson -Executable $worker -ConfigPath $configPath -Arguments $Arguments
}
function Start-Coordinator([string]$Label) {
    $started = Start-Process -FilePath $worker -ArgumentList @('--config', ('"' + $configPath + '"'), 'serve') -NoNewWindow -PassThru `
        -RedirectStandardOutput (Join-Path $ReportDirectory ($Label + '.stdout.log')) -RedirectStandardError (Join-Path $ReportDirectory ($Label + '.stderr.log'))
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        if ($started.HasExited) { throw 'Owned coordinator exited before IPC was ready' }
        try { $null = Invoke-Coordinator @('info'); return $started } catch {
            if ([DateTime]::UtcNow -ge $deadline) { Stop-Process -Id $started.Id -Force; $started.WaitForExit(); throw }
            Start-Sleep -Milliseconds 100
        }
    } while ($true)
}
function Read-Output([string]$Executable, [string]$ConfigPath, [string]$TaskId) {
    $chunks = @(); $sequence = '0'
    do {
        $page = Invoke-WorkerJson -Executable $Executable -ConfigPath $ConfigPath -Arguments @('output', $TaskId, $sequence)
        if ($page.chunks.Count -eq 0) { break }
        $chunks += @($page.chunks); $sequence = [string]$page.chunks[-1].sequence
    } while ($true)
    return ,$chunks
}
function Wait-ChildTask([string]$Executable, [string]$ConfigPath, [string]$TaskId, [string]$Status) {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $task = (Invoke-WorkerJson -Executable $Executable -ConfigPath $ConfigPath -Arguments @('get', $TaskId)).task
        # Running is durable launch intent; PID may still be absent until
        # process creation is committed. A process assertion must wait for both.
        if ($task.status -eq $Status -and ($Status -ne 'running' -or $null -ne $task.processId)) { return $task }
        if ($task.status -notin @('queued', 'running') -or [DateTime]::UtcNow -ge $deadline) {
            throw "Child task did not reach ${Status}: $($task.reason)"
        }
        Start-Sleep -Milliseconds 100
    } while ($true)
}
try {
    $process = Start-Coordinator 'projects-worker'
    $info = (Invoke-Coordinator @('info')).info
    $result.build = $info.build; $result.initialGeneration = $info.generation
    if ($info.projects.Count -ne 2 -or $info.commandHandlers.id -notcontains 'recipe') { throw 'Common handler metadata missing' }
    $accepted = @()
    foreach ($request in $prepared.workflows) {
        $path = Join-Path $root ($request.projectId + '.request.json')
        $request | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $path -Encoding utf8
        $flow = (Invoke-Coordinator @('workflow', $path)).workflow
        $repeat = (Invoke-Coordinator @('workflow', $path)).workflow
        if ($repeat.workflowId -ne $flow.workflowId) { throw 'Same key created another workflow before completion' }
        $accepted += @{ path = $path; flow = $flow }
        Write-Output "Accepted $($request.projectId): $($flow.workflowId)"
    }
    $deadline = [DateTime]::UtcNow.AddMinutes(16)
    foreach ($entry in $accepted) {
        do {
            $flow = (Invoke-Coordinator @('get-workflow', $entry.flow.workflowId)).workflow
            if ($flow.status -notin @('queued', 'running')) { break }
            if ([DateTime]::UtcNow -ge $deadline) { throw 'Project workflow deadline exceeded' }
            Start-Sleep -Milliseconds 500
        } while ($true)
        $record = [ordered]@{ projectId = $flow.submission.projectId; workflow = $flow; buildTask = $null
            startTask = $null; application = $null; log = $null; repeatedWorkflowId = $null }
        $result.projects += $record
        foreach ($step in $flow.steps | Where-Object { $_.name -in @('build', 'start') -and $_.operationId }) {
            $task = (Invoke-Coordinator @('get', $step.operationId)).task
            $record[$step.name + 'Task'] = $task
            $chunks = Read-Output $worker $configPath $task.taskId
            [ordered]@{ task = $task; output = $chunks } | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath (Join-Path $ReportDirectory ($record.projectId + '-' + $step.name + '.json')) -Encoding utf8
        }
        if ($flow.status -ne 'succeeded') { throw "$($record.projectId) workflow failed: $($flow.reason)" }
        $source = $prepared.sources | Where-Object { $_.projectId -eq $record.projectId }
        if ($record.buildTask.status -ne 'succeeded' -or $record.buildTask.exitCode -ne 0 -or
            $record.buildTask.snapshot.manifestSha256 -ne $source.ready.manifestSha256 -or
            $record.buildTask.recipeOrigin.profileId -ne 'rust-service' -or
            $record.startTask.recipeOrigin.profileId -or $flow.recipePins.Count -ne 2) { throw 'Frozen command/source/origin mismatch' }
        foreach ($artifact in $flow.artifacts) {
            if ($artifact.sourceCommit -ne $source.validationCommit -or $artifact.manifestSha256 -ne $source.ready.manifestSha256 -or
                (Get-FileHash -LiteralPath $artifact.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $artifact.sha256) { throw 'Artifact/source checksum mismatch' }
        }
        $record.application = (Invoke-Coordinator @('get-application', $flow.applicationId)).application
        if ($record.application.status -ne 'ready') { throw 'Application not ready' }
        $record.log = (Invoke-Coordinator @('log', $flow.applicationId)).page
        $text = [Text.Encoding]::UTF8.GetString([byte[]]$record.log.bytes)
        $expected = if ($record.projectId -eq 'netwatch-agent') { 'IPC listener ready' } else { 'WinBridge worker ready:' }
        if (-not $text.Contains($expected)) { throw 'Actual application log missing' }
        $record.repeatedWorkflowId = (Invoke-Coordinator @('workflow', $entry.path)).workflow.workflowId
        if ($record.repeatedWorkflowId -ne $flow.workflowId) { throw 'Completed workflow was replayed' }
        Write-Output "Ready $($record.projectId): build $($record.buildTask.taskId), application $($flow.applicationId)"
    }
    # The second real application serves its own actual IPC, with direct commands
    # disabled. Its requests use the unchanged task service and durable journal.
    $childProject = $prepared.projects | Where-Object { $_.id -eq 'winbridge-worker' }
    $childConfigPath = Join-Path $childProject.workspaceRoot 'application/runtime/worker.local.json'
    $childConfig = Get-Content -LiteralPath $childConfigPath -Raw | ConvertFrom-Json
    $childWorker = Join-Path $childProject.workspaceRoot 'build-cache/debug/winbridge-worker.exe'
    $childInfo = (Invoke-WorkerJson -Executable $childWorker -ConfigPath $childConfigPath -Arguments @('info')).info
    if ($childConfig.worker.controllerUrl -or $childInfo.instanceId -ne $childConfig.instanceId -or
        $childInfo.generation -eq $info.generation -or $childInfo.activeTaskCount -ne 0) { throw 'Child worker isolation mismatch' }
    $workerSource = $prepared.sources | Where-Object { $_.projectId -eq 'winbridge-worker' }
    if ($childInfo.build.sourceCommit -ne $workerSource.baseCommit -or -not $childInfo.build.sourceDirty -or
        $childInfo.build.sourceManifestSha256 -ne $workerSource.buildIdentityManifestSha256) { throw 'Validation edits were hidden in child build identity' }
    $child = [ordered]@{ info = $childInfo; success = $null; tree = $null; treeChildPid = $null }
    $result.child = $child
    foreach ($mode in @('success', 'tree')) {
        $request = [ordered]@{ protocolVersion = 1; requestId = $prepared.runId + '-' + $mode; projectId = 'fixture'
            idempotencyKey = $prepared.runId + '-' + $mode; timeoutMs = 120000
            command = @{ kind = 'recipe'; recipeId = $mode; arguments = @() } }
        $path = Join-Path $root ('child-' + $mode + '.request.json')
        $request | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $path -Encoding utf8
        $task = (Invoke-WorkerJson -Executable $childWorker -ConfigPath $childConfigPath -Arguments @('submit', $path)).task
        $task = Wait-ChildTask $childWorker $childConfigPath $task.taskId $(if ($mode -eq 'success') { 'succeeded' } else { 'running' })
        $repeat = (Invoke-WorkerJson -Executable $childWorker -ConfigPath $childConfigPath -Arguments @('submit', $path)).task
        if ($repeat.taskId -ne $task.taskId) { throw 'Child task was replayed' }
        $child[$mode] = $task
    }
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $chunks = Read-Output $childWorker $childConfigPath $child.tree.taskId
        $text = [Text.Encoding]::UTF8.GetString([byte[]]@($chunks | Where-Object { $_.stream -eq 'stdout' } | ForEach-Object { $_.bytes }))
        if ($text -match 'child pid=(\d+)') { $child.treeChildPid = [int]$Matches[1]; break }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Managed grandchild PID missing' }
        Start-Sleep -Milliseconds 100
    } while ($true)
    foreach ($record in $result.projects) {
        $application = $record.application
        $null = Invoke-Coordinator @('stop-application', $application.applicationId)
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        do {
            $stopped = (Invoke-Coordinator @('get-application', $application.applicationId)).application
            if ($stopped.status -eq 'stopped') { break }
            if ([DateTime]::UtcNow -ge $deadline) { throw 'Managed application stop deadline exceeded' }
            Start-Sleep -Milliseconds 100
        } while ($true)
        if (Get-Process -Id $application.processId -ErrorAction SilentlyContinue) { throw 'Application process survived stop' }
        $record.application = $stopped
    }
    foreach ($ownedPid in @($child.tree.processId, $child.treeChildPid)) {
        if (Get-Process -Id $ownedPid -ErrorAction SilentlyContinue) { throw 'Managed nested fixture tree survived application stop' }
    }
    $childReachable = $false
    try { $null = Invoke-WorkerJson -Executable $childWorker -ConfigPath $childConfigPath -Arguments @('info'); $childReachable = $true } catch { }
    if ($childReachable) { throw 'Child worker IPC survived application stop' }
    # Explicitly restart this test application to inspect its own crash journal.
    # Its interrupted external task is reconciled by key, never relaunched.
    $childRestart = Start-Process -FilePath $childWorker -ArgumentList @('--config', ('"' + $childConfigPath + '"'), 'serve') -NoNewWindow -PassThru `
        -RedirectStandardOutput (Join-Path $ReportDirectory 'child-worker-restarted.stdout.log') -RedirectStandardError (Join-Path $ReportDirectory 'child-worker-restarted.stderr.log')
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        if ($childRestart.HasExited) { throw 'Explicitly restarted child worker exited' }
        try { $restored = (Invoke-WorkerJson -Executable $childWorker -ConfigPath $childConfigPath -Arguments @('info')).info; break } catch {
            if ([DateTime]::UtcNow -ge $deadline) { throw }; Start-Sleep -Milliseconds 100
        }
    } while ($true)
    $recovered = (Invoke-WorkerJson -Executable $childWorker -ConfigPath $childConfigPath -Arguments @('submit', (Join-Path $root 'child-tree.request.json'))).task
    $oldSuccess = (Invoke-WorkerJson -Executable $childWorker -ConfigPath $childConfigPath -Arguments @('submit', (Join-Path $root 'child-success.request.json'))).task
    if ($restored.generation -eq $childInfo.generation -or $restored.activeTaskCount -ne 0 -or
        $recovered.taskId -ne $child.tree.taskId -or $recovered.status -ne 'indeterminate' -or
        $oldSuccess.taskId -ne $child.success.taskId -or $oldSuccess.status -ne 'succeeded') { throw 'Child journal recovery replayed a command or changed a result' }
    $child['restartedInfo'] = $restored; $child['recoveredTree'] = $recovered
    Stop-Process -Id $childRestart.Id -Force; $childRestart.WaitForExit(); $childRestart.Dispose(); $childRestart = $null
    # Restart only the owned coordinator after both applications stopped. Nothing
    # is submitted automatically; old explicit keys must still return old IDs.
    Stop-Process -Id $process.Id -Force; $process.WaitForExit(); $process.Dispose(); $process = $null
    $process = Start-Coordinator 'projects-worker-restarted'
    $restarted = (Invoke-Coordinator @('info')).info
    $result.restartedGeneration = $restarted.generation
    if ($restarted.generation -eq $result.initialGeneration -or $restarted.activeTaskCount -ne 0 -or
        $restarted.journalRecoveryRequired -or $restarted.maintenanceError) { throw 'Coordinator restart did not restore an idle healthy journal' }
    foreach ($record in $result.projects) {
        $path = ($accepted | Where-Object { $_.flow.submission.projectId -eq $record.projectId }).path
        $old = (Invoke-Coordinator @('workflow', $path)).workflow
        if ($old.workflowId -ne $record.workflow.workflowId -or $old.status -ne 'succeeded' -or
            (Invoke-Coordinator @('get-application', $old.applicationId)).application.status -ne 'stopped') { throw 'Restart changed startup history or replayed an application' }
        $build = (Invoke-Coordinator @('get', $record.buildTask.taskId)).task
        if (($build | ConvertTo-Json -Depth 40 -Compress) -ne ($record.buildTask | ConvertTo-Json -Depth 40 -Compress)) { throw 'Historical build command/provenance changed after restart' }
    }
    $result.passed = $true
} catch {
    $result.error = $_.Exception.Message
    $result['errorPosition'] = $_.InvocationInfo.PositionMessage
    $result['errorStack'] = $_.ScriptStackTrace
    throw
} finally {
    # Only this script's coordinator PID is stopped; its Job Objects own all test
    # applications. The installed Windows worker is never signalled or replaced.
    if ($process) {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
        $process.Dispose()
    }
    if ($childRestart) {
        if (-not $childRestart.HasExited) { Stop-Process -Id $childRestart.Id -Force; $childRestart.WaitForExit() }
        $childRestart.Dispose()
    }
    $result | ConvertTo-Json -Depth 60 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'projects-proof.json') -Encoding utf8
}
