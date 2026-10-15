# Readonly probe of the installed worker's real named pipe. No test UI or new connection.
# $wbProjectionSettings is supplied by the saved, caller-pinned parent task.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
function wbAssert([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function wbReadExact($Stream, [int]$Count) {
    $data = [byte[]]::new($Count); $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.ReadAsync($data, $offset, $Count - $offset)
        wbAssert ($read.Wait(5000)) 'Local pipe read deadline'
        wbAssert ($read.Result -gt 0) 'Local pipe closed before a complete response'
        $offset += $read.Result
    }
    return ,$data
}
function wbInfo {
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $name = 'WinBridge-' + $sid + '-' + $wbProjectionSettings.context.instanceId
    $pipe = [IO.Pipes.NamedPipeClientStream]::new('.', $name, [IO.Pipes.PipeDirection]::InOut, [IO.Pipes.PipeOptions]::Asynchronous)
    try {
        $pipe.Connect(3000)
        $body = [Text.Encoding]::UTF8.GetBytes('{"method":"info"}')
        $pipe.Write([BitConverter]::GetBytes([uint32]$body.Length), 0, 4); $pipe.Write($body, 0, $body.Length); $pipe.Flush()
        $length = [BitConverter]::ToUInt32((wbReadExact $pipe 4), 0)
        wbAssert ($length -gt 0 -and $length -le 1048576) 'Invalid local Info frame size'
        $response = [Text.Encoding]::UTF8.GetString((wbReadExact $pipe $length)) | ConvertFrom-Json -Depth 60
        wbAssert ($response.kind -eq 'info') 'Local Info failed'
        $info = $response.info
        wbAssert ($info.generation -ceq $wbProjectionSettings.workerGeneration) 'Working worker generation changed'
        wbAssert ($info.build.buildId -ceq $wbProjectionSettings.provider.buildId) 'Working provider build changed'
        wbAssert ($info.fencedRequests.context.instanceId -ceq $wbProjectionSettings.context.instanceId -and $info.fencedRequests.context.journalIncarnation -ceq $wbProjectionSettings.context.journalIncarnation) 'Working journal scope changed'
        return $info
    } finally { $pipe.Dispose() }
}
function wbReady($Info) {
    if ($Info.controllerConnection.state -ne 'connected') { return $false }
    $property = $Info.contractCatalog.PSObject.Properties['controllerProjection']
    if (-not $property -or $null -eq $property.Value) { return $false }
    $projection = $property.Value
    wbAssert ($projection.sessionId -ceq $Info.controllerConnection.sessionId -and $projection.controllerGeneration -ceq $Info.controllerConnection.controllerGeneration) 'Foreign cached projection epoch'
    $views = @($projection.views | Where-Object { $_.receiptId -ceq $wbProjectionSettings.receiptId -and $_.projectId -eq 'fixture' })
    return ($views.Count -eq 1 -and $views[0].readiness.supported -and $views[0].readiness.buildVerified -and $views[0].readiness.pairTested -and $views[0].readiness.enabled -and @($views[0].readiness.reasons).Count -eq 0)
}
function wbWrite([string]$Name, $Value) {
    $path = Join-Path $wbProjectionSettings.reportDirectory $Name
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 60))
    $file = [IO.File]::Open($path, [IO.FileMode]::CreateNew)
    try { $file.Write($bytes, 0, $bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
    return @{schemaVersion=1;bytes=$bytes.Length;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
}
$proof = [ordered]@{schemaVersion=1;platform='windows';passed=$false;processId=$PID;checks=@();error=$null;before=$null;offline=$null;after=$null}
try {
    New-Item -ItemType Directory -Path $wbProjectionSettings.reportDirectory -ErrorAction Stop | Out-Null
    $before = wbInfo; wbAssert (wbReady $before) 'Expected verified pair not present in local pipe projection'
    $proof.before = $before.contractCatalog.controllerProjection
    $proof.checks += 'actual-local-pipe-ready'
    $null = wbWrite 'before.ready.json' @{schemaVersion=1;processId=$PID;sessionId=$before.controllerConnection.sessionId;sequence=$proof.before.sequence}
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    do {
        $info = wbInfo
        $projection = $info.contractCatalog.PSObject.Properties['controllerProjection']
        if ($info.controllerConnection.state -ne 'connected' -and (-not $projection -or $null -eq $projection.Value)) { break }
        wbAssert ([DateTime]::UtcNow -lt $deadline) 'Controller disconnect/cache invalidation was not observed'
        Start-Sleep -Milliseconds 200
    } while ($true)
    $proof.offline = @{state=$info.controllerConnection.state;projectionAbsent=$true;workerGeneration=$info.generation}
    $proof.checks += 'disconnect-clears-local-cache'
    $null = wbWrite 'offline.ready.json' @{schemaVersion=1;processId=$PID;projectionAbsent=$true}
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    do {
        $info = wbInfo
        if (wbReady $info) { break }
        wbAssert ([DateTime]::UtcNow -lt $deadline) 'Verified projection not restored after reconnect'
        Start-Sleep -Milliseconds 200
    } while ($true)
    wbAssert ($info.controllerConnection.sessionId -cne $before.controllerConnection.sessionId) 'Reconnect reused the old session'
    $proof.after = $info.contractCatalog.controllerProjection
    $proof.checks += 'new-session-same-receipt-ready'
    $proof.checks += 'same-process-worker-and-journal'
    $proof.passed = $true
} catch { $proof.error = $_.Exception.Message } finally {
    $ready = wbWrite 'report.json' $proof; $ready.passed = $proof.passed
    $null = wbWrite 'report.ready.json' $ready
}
if (-not $proof.passed) { throw $proof.error }
