# Installed executable only; all mutable state belongs to this parent task's test run.
# The caller supplies $wbWorkSettings. No working config or runtime is edited.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
function wbAssert([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function wbSameBuild($Actual, $Expected) {
    foreach ($name in @('version','sourceCommit','sourceDirty','sourceManifestSha256','buildId')) {
        wbAssert ($Actual.$name -ceq $Expected.$name) "Executable identity changed: $name"
    }
}
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
    wbSameBuild $response.info.build $wbWorkSettings.provider
    return $response.info
}
function wbSameScope($Actual, $Expected) {
    return ($Actual.instanceId -ceq $Expected.instanceId -and $Actual.journalIncarnation -ceq $Expected.journalIncarnation)
}
function wbFenced($Info, $Request) {
    $reply = wbRpc $Info.instanceId @{requestContractVersion=2;context=$Info.fencedRequests.context;request=$Request}
    wbAssert ($reply.outcome -eq 'responded' -and (wbSameScope $reply.context $Info.fencedRequests.context)) 'Unconfirmed fenced reply; retain original key'
    if ($reply.response.kind -eq 'error') { throw ('Worker rejected request: ' + $reply.response.message) }
    return $reply.response
}
function wbView($Info, $Limit) {
    $view = $Info.contractCatalog.workScheduling
    wbAssert ($view.schemaVersion -eq 1 -and -not $view.readOnly) 'Unsupported/readonly scheduling observation'
    wbSameBuild $view.provider $wbWorkSettings.provider
    wbAssert (wbSameScope $view.context $Info.fencedRequests.context) 'Foreign scheduling observation'
    wbAssert ($view.maxConcurrentWork -eq $Limit -and $view.uncertainSlots -eq 0) 'Wrong policy/uncertain owner'
    return $view
}
function wbWaitTask($Info, [string]$TaskId, [string]$Status) {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $task = (wbFenced $Info @{method='get_task';taskId=$TaskId}).task
        if ($task.status -eq $Status -and ($Status -ne 'running' -or $null -ne $task.processId)) { return $task }
        wbAssert ([DateTime]::UtcNow -lt $deadline) "Task $TaskId did not reach $Status"
        Start-Sleep -Milliseconds 25
    } while ($true)
}
function wbSave([string]$Path, $Value) {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 70))
    $file = [IO.File]::Open($Path, [IO.FileMode]::CreateNew)
    try { $file.Write($bytes, 0, $bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
    return @{bytes=$bytes.Length;sha256=(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}
}
$proof = [ordered]@{schemaVersion=1;platform='windows';profile='installed-work-scheduling-v1';passed=$false;provider=$wbWorkSettings.provider;binarySha256=$null;scenarios=@();before=$null;after=$null;error=$null}
$scenarios = [Collections.Generic.List[object]]::new()
try {
    wbAssert ($env:OS -eq 'Windows_NT') 'Windows required'
    New-Item -ItemType Directory -Path $wbWorkSettings.workDirectory -ErrorAction Stop | Out-Null
    New-Item -ItemType Directory -Path $wbWorkSettings.reportDirectory -ErrorAction Stop | Out-Null
    $proof.binarySha256 = (Get-FileHash -LiteralPath $wbWorkSettings.workerExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    $before = wbInfo $wbWorkSettings.context.instanceId
    wbAssert ($before.generation -ceq $wbWorkSettings.workerGeneration -and (wbSameScope $before.fencedRequests.context $wbWorkSettings.context)) 'Installed owner changed'
    $proof.before = $before.contractCatalog.workScheduling
    foreach ($limit in @(1,2,$null)) {
        $label = if ($null -eq $limit) { 'legacy' } else { [string]$limit }
        $directory = Join-Path $wbWorkSettings.workDirectory $label
        New-Item -ItemType Directory -Path $directory -ErrorAction Stop | Out-Null
        $projects = @()
        foreach ($id in @('a','b','c')) {
            $path = Join-Path $directory $id; New-Item -ItemType Directory -Path $path | Out-Null
            $projects += @{id=$id;workspaceRoot=$path;allowDirectCommands=$true;maxConcurrentMutations=1;recipes=@()}
        }
        $instance = 'work-gate-' + [Guid]::NewGuid().ToString('N')
        $config = @{schemaVersion=1;instanceId=$instance;nodeId='isolated-work-gate';stateDirectory=(Join-Path $directory 'state');worker=@{keepRunningOnUiExit=$true};projects=$projects}
        if ($null -ne $limit) { $config.workScheduling = @{maxConcurrentWork=$limit} }
        $configPath = Join-Path $directory 'worker.local.json'; $null = wbSave $configPath $config
        $start = [Diagnostics.ProcessStartInfo]::new($wbWorkSettings.workerExecutable)
        $start.UseShellExecute=$false; $start.CreateNoWindow=$true; $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
        $start.StandardOutputEncoding=[Text.UTF8Encoding]::new($false); $start.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
        foreach ($arg in @('--config',$configPath,'serve')) { $start.ArgumentList.Add($arg) }
        $worker=[Diagnostics.Process]::new(); $worker.StartInfo=$start
        $info=$null; $ids=@{}; $requests=@{}; $out=$null; $err=$null; $started=$false
        try {
            $null=$worker.Start(); $started=$true; $out=$worker.StandardOutput.ReadToEndAsync(); $err=$worker.StandardError.ReadToEndAsync()
            $deadline=[DateTime]::UtcNow.AddSeconds(15)
            do {
                try { $info=wbInfo $instance; break } catch {
                    if ($worker.HasExited -or [DateTime]::UtcNow -ge $deadline) { throw }; Start-Sleep -Milliseconds 50
                }
            } while ($true)
            $view=wbView $info $limit
            wbAssert ($view.occupiedSlots -eq 0 -and $view.queuedOperations -eq 0) 'New test state is not empty'
            foreach ($project in @('a','b','c')) {
                for ($n=0; $n -lt 15; $n++) {
                    $key=$project + '-' + $n
                    $request=@{method='submit';request=@{protocolVersion=1;requestId=$key;idempotencyKey=$key;projectId=$project;timeoutMs=120000;command=@{kind='direct';executable=$wbWorkSettings.fixtureExecutable;arguments=@('long');workingDirectory='.'}}}
                    $response=wbFenced $info $request
                    wbAssert ($response.kind -eq 'task') 'Task admission unconfirmed'
                    $ids[$key]=$response.task.taskId; $requests[$key]=$request
                }
            }
            $expected=if ($null -eq $limit) {3} else {[int]$limit}
            foreach ($project in @('a','b','c')[0..($expected-1)]) { $null=wbWaitTask $info $ids[$project+'-0'] 'running' }
            $timer=[Diagnostics.Stopwatch]::StartNew(); $initial=wbInfo $instance; $timer.Stop()
            $view=wbView $initial $limit
            wbAssert ($timer.ElapsedMilliseconds -lt 5000) 'Loaded IPC lacks margin below its deadline'
            wbAssert ($view.occupiedSlots -eq $expected -and $view.queuedOperations -eq 45-$expected) 'Incorrect global capacity/queue observation'
            $first=wbWaitTask $info $ids['a-0'] 'running'
            $repeat=wbFenced $info $requests['a-0']
            wbAssert ($repeat.task.taskId -ceq $first.taskId -and $repeat.task.processId -eq $first.processId) 'Duplicate changed task/process'
            $counter=[IO.File]::Open((Join-Path $directory 'a/runs.log'),[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
            $reader=[IO.StreamReader]::new($counter,[Text.UTF8Encoding]::new($false))
            try { $lines=@($reader.ReadToEnd().Trim().Split("`n")); wbAssert ($lines.Count -eq 1 -and $lines[0].StartsWith(([string]$first.processId + ' '))) 'Duplicate launched another physical process' } finally { $reader.Dispose() }
            $case=[ordered]@{maxConcurrentWork=$limit;admissions=45;initialSlots=$view.occupiedSlots;ipcInfoMs=$timer.ElapsedMilliseconds;initialInfo=$initial;afterCancelInfo=$null;blockedPage=$null;cancelMs=@();cleanupSlots=$null;isolatedStopped=$false}
            if ($null -ne $limit) {
                $blocked=if ($limit -eq 1) {'b'} else {'c'}
                $request=@{extensionVersion=1;contract=@{id='winbridge.operation-read-model';version=1};context=$info.fencedRequests.context;payload=@{projectId=$blocked;kind='task';search=$ids[$blocked+'-0'];limit=5;cursor=$null}}
                $reply=wbRpc $instance $request
                wbAssert ($reply.result.kind -eq 'data' -and (wbSameScope $reply.context $info.fencedRequests.context)) 'Operation page unconfirmed'
                $case.blockedPage=$reply.result.payload; $records=@($case.blockedPage.records)
                wbAssert ($records.Count -eq 1 -and $records[0].state.status -eq 'queued' -and $records[0].reason.Contains('Общий лимит')) 'Old backlog wait reason not visible'
                $transitions=@(@{old='a';next='c'})
                if ($limit -eq 1) { $transitions=@(@{old='a';next='b'},@{old='b';next='c'}) }
                foreach ($transition in $transitions) {
                    $old=$ids[$transition.old+'-0']; $next=$ids[$transition.next+'-0']
                    $cancel=[Diagnostics.Stopwatch]::StartNew(); $null=wbFenced $info @{method='cancel';taskId=$old}
                    $null=wbWaitTask $info $old 'cancelled'; $cancel.Stop(); $case.cancelMs += $cancel.ElapsedMilliseconds
                    $null=wbWaitTask $info $next 'running'; $case.afterCancelInfo=wbInfo $instance
                    wbAssert ((wbView $case.afterCancelInfo $limit).occupiedSlots -le $limit) 'Global limit exceeded after cancel'
                }
            }
            foreach ($id in $ids.Values) { $null=wbFenced $info @{method='cancel';taskId=$id} }
            $deadline=[DateTime]::UtcNow.AddSeconds(15)
            do {
                $final=wbInfo $instance; $view=wbView $final $limit
                if ($final.activeTaskCount -eq 0 -and $view.occupiedSlots -eq 0) { break }
                wbAssert ([DateTime]::UtcNow -lt $deadline) 'Physical cleanup unconfirmed'; Start-Sleep -Milliseconds 25
            } while ($true)
            $case.cleanupSlots=$view.occupiedSlots
        } finally {
            # Kill only this owned test process tree. A failed probe still cannot leak fixtures.
            if ($started) {
                if (-not $worker.HasExited) { $worker.Kill($true) }
                wbAssert ($worker.WaitForExit(10000)) 'Isolated worker cleanup unconfirmed'
            }
            if ($out) { $null=$out.GetAwaiter().GetResult() }
            if ($err) { [IO.File]::WriteAllText((Join-Path $directory 'worker.stderr.log'),$err.GetAwaiter().GetResult(),[Text.UTF8Encoding]::new($false)) }
            $worker.Dispose()
        }
        $case.isolatedStopped=$true; $scenarios.Add($case)
        Write-Output "Installed pool $label passed; isolated worker stopped."
    }
    $after=wbInfo $wbWorkSettings.context.instanceId
    wbAssert ($after.generation -ceq $before.generation -and (wbSameScope $after.fencedRequests.context $before.fencedRequests.context)) 'Working worker changed'
    $proof.after=$after.contractCatalog.workScheduling
    wbAssert ((Get-FileHash -LiteralPath $wbWorkSettings.workerExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $proof.binarySha256) 'Installed executable changed during probe'
    $proof.passed=$true
} catch { $proof.error=$_.Exception.Message } finally {
    $proof.scenarios=$scenarios.ToArray()
    if (Test-Path -LiteralPath $wbWorkSettings.reportDirectory -PathType Container) {
        $ready=wbSave (Join-Path $wbWorkSettings.reportDirectory 'report.json') $proof
        $ready.schemaVersion=1; $ready.passed=$proof.passed
        $null=wbSave (Join-Path $wbWorkSettings.reportDirectory 'report.ready.json') $ready
    }
}
if (-not $proof.passed) { throw $proof.error }
