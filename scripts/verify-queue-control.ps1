# Installed exe only. Every mutation below targets a new isolated instance.
# The caller supplies $wbQueueSettings; working config/state are never opened.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
function wbAssert([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function wbSameBuild($Actual, $Expected) {
    foreach ($name in @('version','sourceCommit','sourceDirty','sourceManifestSha256','buildId')) {
        wbAssert ($Actual.$name -ceq $Expected.$name) "Executable identity changed: $name"
    }
}
function wbSameScope($Actual, $Expected) { return ($Actual.instanceId -ceq $Expected.instanceId -and $Actual.journalIncarnation -ceq $Expected.journalIncarnation) }
function wbReadExact($Stream, [int]$Count) {
    $data = [byte[]]::new($Count); $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.ReadAsync($data, $offset, $Count - $offset)
        wbAssert ($read.Wait(10000)) 'Local pipe read deadline'
        wbAssert ($read.Result -gt 0) 'Local pipe closed early'
        $offset += $read.Result
    }
    return ,$data
}
function wbRpc([string]$Instance, $Request) {
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $pipe = [IO.Pipes.NamedPipeClientStream]::new('.', ('WinBridge-' + $sid + '-' + $Instance), [IO.Pipes.PipeDirection]::InOut, [IO.Pipes.PipeOptions]::Asynchronous)
    try {
        $pipe.Connect(3000)
        $body = [Text.Encoding]::UTF8.GetBytes(($Request | ConvertTo-Json -Depth 60 -Compress))
        $pipe.Write([BitConverter]::GetBytes([uint32]$body.Length), 0, 4); $pipe.Write($body, 0, $body.Length); $pipe.Flush()
        $length = [BitConverter]::ToUInt32((wbReadExact $pipe 4), 0)
        wbAssert ($length -gt 0 -and $length -le 1048576) 'Invalid local IPC frame size'
        return ([Text.Encoding]::UTF8.GetString((wbReadExact $pipe $length)) | ConvertFrom-Json -Depth 60)
    } finally { $pipe.Dispose() }
}
function wbInfo([string]$Instance) {
    $response = wbRpc $Instance @{method='info'}
    wbAssert ($response.kind -eq 'info') 'Info unavailable'
    wbSameBuild $response.info.build $wbQueueSettings.provider
    return $response.info
}
function wbView($Info) {
    $view = $Info.contractCatalog.queueControl
    wbAssert ($view.schemaVersion -eq 1 -and -not $view.readOnly) 'Queue metadata unavailable/readonly'
    wbSameBuild $view.provider $wbQueueSettings.provider
    wbAssert (wbSameScope $view.context $Info.fencedRequests.context) 'Foreign queue scope'
    return $view
}
function wbFencedReply($Info, $Request) {
    $reply = wbRpc $Info.instanceId @{requestContractVersion=2;context=$Info.fencedRequests.context;request=$Request}
    wbAssert (wbSameScope $reply.context $Info.fencedRequests.context) 'Foreign fenced reply'
    return $reply
}
function wbFenced($Info, $Request) {
    $reply=wbFencedReply $Info $Request
    wbAssert ($reply.outcome -eq 'responded' -and $reply.response.kind -ne 'error') 'Fenced result unconfirmed'
    return $reply.response
}
function wbSet($Info, [string]$Id, [string]$Revision, [string]$Mode) {
    $request=@{queueControlVersion=1;context=$Info.fencedRequests.context;command=@{kind='set';commandId=$Id;expectedRevision=$Revision;mode=$Mode}}
    $response=wbRpc $Info.instanceId $request
    wbAssert ($response.queueControlVersion -eq 1 -and (wbSameScope $response.context $Info.fencedRequests.context)) 'Queue reply scope/version changed'
    wbAssert ($response.result.kind -eq 'applied') ('Queue command unconfirmed: ' + ($response | ConvertTo-Json -Depth 30 -Compress))
    $r=$response.result.receipt
    wbAssert ($r.commandId -ceq $Id -and $r.fromRevision -ceq $Revision -and $r.mode -ceq $Mode -and (wbSameScope $r.context $Info.fencedRequests.context)) 'Foreign receipt'
    return $response
}
function wbWait($Info, [string]$TaskId, [string]$Status) {
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {
        $task=(wbFenced $Info @{method='get_task';taskId=$TaskId}).task
        if ($task.status -eq $Status -and ($Status -ne 'running' -or $null -ne $task.processId)) { return $task }
        wbAssert ([DateTime]::UtcNow -lt $deadline) "Task $TaskId did not reach $Status"
        Start-Sleep -Milliseconds 25
    } while ($true)
}
function wbSubmit([string]$Project, [string]$Key, [string]$Mode) {
    return @{method='submit';request=@{protocolVersion=1;requestId=$Key;idempotencyKey=$Key;projectId=$Project;timeoutMs=120000;command=@{kind='direct';executable=$wbQueueSettings.fixtureExecutable;arguments=@($Mode);workingDirectory='.'}}}
}
function wbSave([string]$Path, $Value) {
    $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 70))
    $file=[IO.File]::Open($Path,[IO.FileMode]::CreateNew)
    try { $file.Write($bytes,0,$bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
    return @{bytes=$bytes.Length;sha256=(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}
}
function wbStart([string]$ConfigPath, [string]$Instance) {
    $start=[Diagnostics.ProcessStartInfo]::new($wbQueueSettings.workerExecutable)
    $start.UseShellExecute=$false; $start.CreateNoWindow=$true; $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
    $start.StandardOutputEncoding=[Text.UTF8Encoding]::new($false); $start.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
    foreach ($arg in @('--config',$ConfigPath,'serve')) { $start.ArgumentList.Add($arg) }
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$start
    $null=$process.Start()
    # Retain ownership before probing IPC, including when startup fails.
    $script:owned=@{process=$process;stdout=$process.StandardOutput.ReadToEndAsync();stderr=$process.StandardError.ReadToEndAsync()}
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {
        try { return (wbInfo $Instance) } catch {
            if ($process.HasExited -or [DateTime]::UtcNow -ge $deadline) { throw }
            Start-Sleep -Milliseconds 50
        }
    } while ($true)
}
function wbStop([string]$LogName) {
    if ($null -eq $script:owned) { return }
    $p=$script:owned.process
    try {
        if (-not $p.HasExited) { $p.Kill($true) }
        wbAssert ($p.WaitForExit(10000)) 'Owned test worker cleanup unconfirmed'
        $null=$script:owned.stdout.GetAwaiter().GetResult()
        [IO.File]::WriteAllText((Join-Path $wbQueueSettings.workDirectory $LogName),$script:owned.stderr.GetAwaiter().GetResult(),[Text.UTF8Encoding]::new($false))
    } finally { $p.Dispose(); $script:owned=$null }
}
$script:owned=$null
$proof=[ordered]@{schemaVersion=1;platform='windows';profile='installed-queue-control-v1';passed=$false;provider=$wbQueueSettings.provider;binarySha256=$null;storage=$null;before=$null;after=$null;observations=@();receipt=$null;replay=$null;cancelled=$null;succeeded=$null;isolatedStopped=$false;checks=@();error=$null}
try {
    wbAssert ($env:OS -eq 'Windows_NT') 'Windows required'
    New-Item -ItemType Directory -Path $wbQueueSettings.workDirectory -ErrorAction Stop | Out-Null
    New-Item -ItemType Directory -Path $wbQueueSettings.reportDirectory -ErrorAction Stop | Out-Null
    $drive=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($wbQueueSettings.workDirectory))
    $proof.storage=@{availableBytesBefore=$drive.AvailableFreeSpace;availableBytesAfter=$null}
    wbAssert ($drive.AvailableFreeSpace -ge 6GB) 'Less than 6 GiB free; no existing directories are deleted'
    $proof.binarySha256=(Get-FileHash -LiteralPath $wbQueueSettings.workerExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    $before=wbInfo $wbQueueSettings.context.instanceId
    wbAssert ($before.generation -ceq $wbQueueSettings.workerGeneration -and (wbSameScope $before.fencedRequests.context $wbQueueSettings.context)) 'Working owner changed'
    $proof.before=wbView $before
    $projects=@()
    foreach ($id in @('a','b')) {
        $path=Join-Path $wbQueueSettings.workDirectory $id; New-Item -ItemType Directory -Path $path | Out-Null
        $projects+=@{id=$id;workspaceRoot=$path;allowDirectCommands=$true;maxConcurrentMutations=1;recipes=@()}
    }
    $instance='queue-gate-'+[Guid]::NewGuid().ToString('N')
    $config=@{schemaVersion=1;instanceId=$instance;nodeId='isolated-queue-gate';stateDirectory=(Join-Path $wbQueueSettings.workDirectory 'state');worker=@{keepRunningOnUiExit=$true};projects=$projects;workScheduling=@{maxConcurrentWork=1};queuePolicy=@{maxPendingRoots=2;maxPendingRootsPerProject=1}}
    $configPath=Join-Path $wbQueueSettings.workDirectory 'worker.local.json'; $null=wbSave $configPath $config
    $info=wbStart $configPath $instance
    wbAssert (-not (wbSameScope $info.fencedRequests.context $wbQueueSettings.context)) 'Test instance must have a separate journal'
    $null=wbSet $info 'initial-dispatch' '0' 'dispatch_paused'
    $aRequest=wbSubmit 'a' 'a' 'long'; $bRequest=wbSubmit 'b' 'b' 'success'
    $a=(wbFenced $info $aRequest).task; $b=(wbFenced $info $bRequest).task
    Start-Sleep -Milliseconds 150
    $paused=wbInfo $instance; $proof.observations+=,$paused
    wbAssert ((wbView $paused).pendingRoots -eq 2) 'Root quota did not count both tasks'
    foreach ($id in @($a.taskId,$b.taskId)) { $t=wbWait $info $id 'queued'; wbAssert ($null -eq $t.processId) 'Dispatch pause launched a process' }
    $repeat=(wbFenced $info $aRequest).task
    wbAssert ($repeat.taskId -ceq $a.taskId) 'Duplicate at quota created a new task'
    $denied=wbFencedReply $info (wbSubmit 'b' 'quota-denied' 'success')
    wbAssert ($denied.response.kind -eq 'error' -and $denied.response.message.Contains('лимит')) 'Full queue must reject admission'
    $null=wbSet $info 'both-paused' '1' 'paused'
    $null=wbFenced $info @{method='cancel';taskId=$b.taskId}; $null=wbWait $info $b.taskId 'cancelled'
    $deniedRequest=wbSubmit 'b' 'paused-denied' 'success'
    $denied=wbFencedReply $info $deniedRequest
    wbAssert ($denied.response.kind -eq 'error' -and $denied.response.message.Contains('приостановлен')) 'Admission pause must reject before acceptance'
    wbAssert ((wbFenced $info $aRequest).task.taskId -ceq $a.taskId) 'Pause blocked existing-key replay'
    $null=wbSet $info 'admission-resume' '2' 'dispatch_paused'
    $b=(wbFenced $info $deniedRequest).task
    wbAssert ($b.status -eq 'queued') 'Rejected key was incorrectly reserved'
    $null=wbSet $info 'dispatch-resume' '3' 'running'
    $running=wbWait $info $a.taskId 'running'
    $null=wbSet $info 'pause-live-dispatch' '4' 'dispatch_paused'
    $response=wbSet $info 'pause-live-both' '5' 'paused'; $proof.receipt=$response.result.receipt
    $live=wbWait $info $a.taskId 'running'
    wbAssert ($live.processId -eq $running.processId) 'Pause replaced a running owner'
    wbAssert ((wbFenced $info $aRequest).task.processId -eq $running.processId) 'Duplicate changed physical process'
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do {
        $chunks=(wbFenced $info @{method='read_output';taskId=$a.taskId;afterSequence=0}).chunks
        if (@($chunks).Count -gt 0) { break }
        wbAssert ([DateTime]::UtcNow -lt $deadline) 'Running owner output unavailable'
        Start-Sleep -Milliseconds 25
    } while ($true)
    $null=wbFenced $info @{method='cancel';taskId=$a.taskId}; $proof.cancelled=wbWait $info $a.taskId 'cancelled'
    $pending=wbWait $info $b.taskId 'queued'; wbAssert ($null -eq $pending.processId) 'Queued task bypassed pause'
    $oldGeneration=$info.generation; $scope=$info.fencedRequests.context
    $proof.observations+=,(wbInfo $instance)
    wbStop 'before-restart.stderr.log'
    $info=wbStart $configPath $instance
    wbAssert ($info.generation -cne $oldGeneration -and (wbSameScope $info.fencedRequests.context $scope)) 'Restart changed journal scope or not generation'
    wbAssert ((wbView $info).mode -eq 'paused' -and (wbView $info).revision -ceq '6') 'Restart lost durable pause'
    $response=wbRpc $instance @{queueControlVersion=1;context=$scope;command=@{kind='observe';commandId='pause-live-both'}}
    wbAssert ($response.result.kind -eq 'observed' -and ($response.result.receipt | ConvertTo-Json -Compress) -ceq ($proof.receipt | ConvertTo-Json -Compress)) 'Restart lost receipt'
    $proof.replay=(wbSet $info 'pause-live-both' '5' 'paused').result.receipt
    wbAssert (($proof.replay | ConvertTo-Json -Compress) -ceq ($proof.receipt | ConvertTo-Json -Compress)) 'Duplicate mutated receipt'
    $stale=wbRpc $instance @{queueControlVersion=1;context=$scope;command=@{kind='set';commandId='stale';expectedRevision='0';mode='running'}}
    wbAssert ($stale.result.kind -eq 'rejected' -and $stale.result.code -eq 'revision_conflict') 'Stale revision was rebased'
    Start-Sleep -Milliseconds 150
    $pending=wbWait $info $b.taskId 'queued'; wbAssert ($null -eq $pending.processId) 'Restart automatically resumed dispatch'
    $proof.observations+=,(wbInfo $instance)
    $null=wbSet $info 'explicit-resume' '6' 'running'
    $proof.succeeded=wbWait $info $b.taskId 'succeeded'
    wbAssert ($proof.succeeded.exitCode -eq 0) 'Resumed task failed'
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {
        $final=wbInfo $instance
        if ((wbView $final).pendingRoots -eq 0 -and $final.contractCatalog.workScheduling.occupiedSlots -eq 0) { break }
        wbAssert ([DateTime]::UtcNow -lt $deadline) 'Physical cleanup unconfirmed'; Start-Sleep -Milliseconds 25
    } while ($true)
    $proof.observations+=,$final
    wbStop 'after-restart.stderr.log'; $proof.isolatedStopped=$true
    $after=wbInfo $wbQueueSettings.context.instanceId
    wbAssert ($after.generation -ceq $before.generation -and (wbSameScope $after.fencedRequests.context $before.fencedRequests.context)) 'Working worker changed'
    $proof.after=wbView $after
    foreach ($name in @('mode','revision','changedAtMs','lastCommandId')) { wbAssert ($proof.before.$name -ceq $proof.after.$name) "Working queue changed: $name" }
    wbAssert (($proof.before.policy | ConvertTo-Json -Compress) -ceq ($proof.after.policy | ConvertTo-Json -Compress)) 'Working queue policy changed'
    wbAssert ((Get-FileHash -LiteralPath $wbQueueSettings.workerExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $proof.binarySha256) 'Installed executable changed'
    $proof.checks=@('installed-executable-no-rebuild','root-quota-existing-key','rejected-key-not-reserved','separate-pause-controls','running-owner-survives-pause','output-cancel-readable','restart-pause-and-receipt','stale-revision-no-rebase','zero-cleanup-slots','working-queue-unchanged')
    $proof.storage.availableBytesAfter=$drive.AvailableFreeSpace
    $proof.passed=$true
} catch { $proof.error=$_.Exception.Message } finally {
    wbStop 'failure.stderr.log'
    if (Test-Path -LiteralPath $wbQueueSettings.reportDirectory -PathType Container) {
        $ready=wbSave (Join-Path $wbQueueSettings.reportDirectory 'report.json') $proof
        $ready.schemaVersion=1; $ready.passed=$proof.passed
        $null=wbSave (Join-Path $wbQueueSettings.reportDirectory 'report.ready.json') $ready
    }
}
if (-not $proof.passed) { throw $proof.error }
Write-Output 'Installed queue checks passed; isolated worker stopped, working queue unchanged.'
