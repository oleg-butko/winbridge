# Readonly inspection of the installed worker. The parent task owns this process.
# The caller supplies $wbOperationSettings; no runtime/config file is edited.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
function wbAssert([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function wbReadExact($Stream, [int]$Count) {
    $data = [byte[]]::new($Count); $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.ReadAsync($data, $offset, $Count - $offset)
        wbAssert ($read.Wait(5000)) 'Local pipe read deadline'
        wbAssert ($read.Result -gt 0) 'Local pipe closed early'
        $offset += $read.Result
    }
    return ,$data
}
function wbRpc($Request) {
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $name = 'WinBridge-' + $sid + '-' + $wbOperationSettings.context.instanceId
    $pipe = [IO.Pipes.NamedPipeClientStream]::new('.', $name, [IO.Pipes.PipeDirection]::InOut, [IO.Pipes.PipeOptions]::Asynchronous)
    try {
        $pipe.Connect(3000)
        $body = [Text.Encoding]::UTF8.GetBytes(($Request | ConvertTo-Json -Depth 60 -Compress))
        $pipe.Write([BitConverter]::GetBytes([uint32]$body.Length), 0, 4); $pipe.Write($body, 0, $body.Length); $pipe.Flush()
        $length = [BitConverter]::ToUInt32((wbReadExact $pipe 4), 0)
        wbAssert ($length -gt 0 -and $length -le 1048576) 'Invalid local IPC frame size'
        return ([Text.Encoding]::UTF8.GetString((wbReadExact $pipe $length)) | ConvertFrom-Json -Depth 60)
    } finally { $pipe.Dispose() }
}
function wbSameScope($Context) {
    return ($Context.instanceId -ceq $wbOperationSettings.context.instanceId -and $Context.journalIncarnation -ceq $wbOperationSettings.context.journalIncarnation)
}
function wbInfo {
    $response = wbRpc @{method='info'}
    wbAssert ($response.kind -eq 'info') 'Info unavailable'
    $info = $response.info
    wbAssert ($info.generation -ceq $wbOperationSettings.workerGeneration -and $info.build.buildId -ceq $wbOperationSettings.provider.buildId) 'Installed worker changed'
    wbAssert (wbSameScope $info.fencedRequests.context) 'Journal changed'
    return $info
}
function wbPage($Query) {
    $reply = wbRpc @{extensionVersion=1;contract=@{id='winbridge.operation-read-model';version=1};context=$wbOperationSettings.context;payload=$Query}
    wbAssert ($reply.extensionVersion -eq 1 -and $reply.contract.id -ceq 'winbridge.operation-read-model' -and $reply.contract.version -eq 1) 'Wrong extension reply'
    wbAssert ($reply.result.kind -eq 'data') 'Operation query rejected'
    wbAssert (wbSameScope $reply.context) 'Foreign extension reply'
    $page = $reply.result.payload
    wbAssert ($page.schemaVersion -eq 1 -and (wbSameScope $page.context)) 'Foreign operation page'
    foreach ($record in $page.records) { wbAssert (wbSameScope $record.reference.context) 'Foreign operation reference' }
    return $page
}
function wbWrite([string]$Name, $Value) {
    $path = Join-Path $wbOperationSettings.reportDirectory $Name
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 60))
    $file = [IO.File]::Open($path, [IO.FileMode]::CreateNew)
    try { $file.Write($bytes, 0, $bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
    return @{schemaVersion=1;bytes=$bytes.Length;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
}
$proof = [ordered]@{schemaVersion=1;platform='windows';passed=$false;processId=$PID;taskId=$null;checks=@();error=$null;before=$null;offline=$null;after=$null}
try {
    New-Item -ItemType Directory -Path $wbOperationSettings.reportDirectory -ErrorAction Stop | Out-Null
    $before = wbInfo
    wbAssert ($before.controllerConnection.state -eq 'connected') 'Expected connected controller'
    $tasks = wbRpc @{method='list_tasks'}
    foreach ($summary in $tasks.tasks) {
        $reply = wbRpc @{requestContractVersion=2;context=$wbOperationSettings.context;request=@{method='get_task';taskId=$summary.taskId}}
        wbAssert ($reply.outcome -eq 'responded' -and (wbSameScope $reply.context)) 'Unconfirmed task lookup'
        if ($reply.response.task.processId -eq $PID) { $proof.taskId = $summary.taskId; break }
    }
    wbAssert ($null -ne $proof.taskId) 'Probe parent task not found'
    $query = @{projectId='snapshot-fixture';kind='task';search=$proof.taskId;limit=5;cursor=$null}
    $own = wbPage $query
    wbAssert (@($own.records).Count -eq 1 -and $own.records[0].state.status -eq 'running') 'Probe task must be running'
    $leases = @($own.records[0].resources | Where-Object { $_.owner.operationId -ceq $proof.taskId -and $_.state -eq 'held' })
    wbAssert ($leases.Count -eq 1 -and $leases[0].scope -eq 'project_mutation') 'Actual running resource lease missing'
    $proof.before = @{sessionId=$before.controllerConnection.sessionId;record=$own.records[0]}
    $proof.checks += 'actual-local-pipe-running-state-and-resource'
    foreach ($test in @('unknown-contract','foreign-scope')) {
        $request = @{extensionVersion=1;contract=@{id='winbridge.operation-read-model';version=1};context=$wbOperationSettings.context;payload=$query}
        if ($test -eq 'unknown-contract') { $request.contract.version = 99 } else { $request.context = @{instanceId=$wbOperationSettings.context.instanceId;journalIncarnation='foreign'} }
        $reply = wbRpc $request
        wbAssert ($reply.result.kind -eq 'error' -and $null -eq $reply.context) 'Unsafe readonly rejection'
        wbAssert ($reply.result.code -eq $(if ($test -eq 'unknown-contract') {'unsupported_contract'} else {'scope_mismatch'})) 'Wrong readonly rejection reason'
    }
    $proof.checks += 'unknown-contract-and-foreign-scope-refused'
    $null = wbWrite 'before.ready.json' @{schemaVersion=1;processId=$PID;taskId=$proof.taskId;sessionId=$before.controllerConnection.sessionId}
    $deadline = [DateTime]::UtcNow.AddSeconds(120)
    do {
        $info = wbInfo
        if ($info.controllerConnection.state -ne 'connected') { break }
        wbAssert ([DateTime]::UtcNow -lt $deadline) 'Expected controller disconnect was not observed'
        Start-Sleep -Milliseconds 200
    } while ($true)
    $offline = wbPage $query
    wbAssert ($offline.records[0].state.status -eq 'running' -and $offline.records[0].reference.operationId -ceq $proof.taskId) 'Disconnect changed task identity/state'
    wbAssert (@($offline.records[0].resources | Where-Object { $_.leaseId -ceq $leases[0].leaseId -and $_.state -eq 'held' }).Count -eq 1) 'Disconnect released live resource'
    $proof.offline = @{state=$info.controllerConnection.state;record=$offline.records[0]}
    $proof.checks += 'local-page-remains-readable-during-disconnect'
    $null = wbWrite 'offline.ready.json' @{schemaVersion=1;processId=$PID;taskId=$proof.taskId}
    $deadline = [DateTime]::UtcNow.AddSeconds(90)
    do {
        $info = wbInfo
        if ($info.controllerConnection.state -eq 'connected' -and $info.controllerConnection.sessionId -cne $before.controllerConnection.sessionId) { break }
        wbAssert ([DateTime]::UtcNow -lt $deadline) 'Controller reconnect was not observed'
        Start-Sleep -Milliseconds 200
    } while ($true)
    $after = wbPage $query
    wbAssert ($after.records[0].reference.operationId -ceq $proof.taskId -and $after.records[0].state.status -eq 'running') 'Reconnect replaced running task'
    $proof.after = @{sessionId=$info.controllerConnection.sessionId;record=$after.records[0]}
    $proof.checks += 'new-session-same-task-process-and-journal'
    $proof.passed = $true
    Write-Output 'WinBridge operation page probe Привет ✓'
} catch { $proof.error = $_.Exception.Message } finally {
    $ready = wbWrite 'report.json' $proof; $ready.passed = $proof.passed
    $null = wbWrite 'report.ready.json' $ready
}
if (-not $proof.passed) { throw $proof.error }
