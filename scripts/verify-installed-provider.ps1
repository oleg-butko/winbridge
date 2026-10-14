# Compiled into the controller. $wbVerification is a caller-pinned, JSON-decoded
# settings object supplied by the producer; only a new isolated state is written.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($env:OS -ne 'Windows_NT') { throw 'Actual Windows provider verification required' }
function wbAssert($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function wbSameBuild($Actual, $Expected) {
    foreach ($name in @('version','sourceCommit','sourceDirty','sourceManifestSha256','buildId')) {
        wbAssert ($Actual.$name -ceq $Expected.$name) "Provider build changed: $name"
    }
}
function wbNativeJson([string[]]$Arguments) {
    $start = [Diagnostics.ProcessStartInfo]::new($wbVerification.workerExecutable)
    $start.UseShellExecute = $false; $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
    $start.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $client = [Diagnostics.Process]::new(); $client.StartInfo = $start
    try {
        $null = $client.Start()
        $out = $client.StandardOutput.ReadToEndAsync(); $err = $client.StandardError.ReadToEndAsync()
        if (-not $client.WaitForExit(15000)) { $client.Kill(); $client.WaitForExit(); throw 'Provider client timeout; no automatic request retry' }
        $text = $out.GetAwaiter().GetResult(); $diagnostic = $err.GetAwaiter().GetResult()
        if (-not $text.Trim()) { throw "Missing provider JSON: $diagnostic" }
        return ($text | ConvertFrom-Json -Depth 50)
    } finally { $client.Dispose() }
}
function wbInvoke($Request) {
    $path = Join-Path $wbVerification.workDirectory ('request-' + [Guid]::NewGuid().ToString('N') + '.json')
    [IO.File]::WriteAllText($path, ($Request | ConvertTo-Json -Depth 40 -Compress), [Text.UTF8Encoding]::new($false))
    return wbNativeJson @('--config',$wbConfig,'invoke-v2',$path)
}
function wbRequest($Inner) { return @{ requestContractVersion=2; context=$wbInfo.fencedRequests.context; request=$Inner } }
function wbTask([string]$TaskId) {
    $response = wbInvoke (wbRequest @{method='get_task';taskId=$TaskId})
    wbAssert ($response.outcome -eq 'responded' -and $response.response.kind -eq 'task') 'Task observation unconfirmed'
    return $response.response.task
}
function wbWait([string]$TaskId) {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $task = wbTask $TaskId
        if ($task.status -notin @('queued','running')) { return $task }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Provider task completion timeout; accepted task is not repeated'
}
function wbSubmit([string]$Key,[string]$Mode) {
    $body = @{ method='submit'; request=@{protocolVersion=1;requestId=$Key;projectId='probe';idempotencyKey=$Key;timeoutMs=60000;
        command=@{kind='direct';executable=$wbVerification.fixtureExecutable;arguments=@($Mode);workingDirectory='.'}} }
    $response = wbInvoke (wbRequest $body)
    wbAssert ($response.outcome -eq 'responded' -and $response.response.kind -eq 'task') 'Provider submission unconfirmed'
    return @{ task=$response.response.task; request=$body }
}
$wbWorker = $null; $wbOut = $null; $wbErr = $null; $wbFailure = $null; $wbInfo = $null
$wbChecks = [Collections.Generic.List[string]]::new()
$wbProof = [ordered]@{schemaVersion=1;profile='installed-provider-rpc2-v1';platform='windows';passed=$false;build=$null;binarySha256=$null;checks=@();error=$null}
try {
    wbAssert ([IO.Path]::IsPathFullyQualified($wbVerification.workDirectory)) 'Verification workDirectory must be absolute'
    New-Item -ItemType Directory -Path $wbVerification.workDirectory -ErrorAction Stop | Out-Null
    New-Item -ItemType Directory -Path $wbVerification.reportDirectory -ErrorAction Stop | Out-Null
    $wbProof.binarySha256 = (Get-FileHash -LiteralPath $wbVerification.workerExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    $description = wbNativeJson @('--describe'); wbSameBuild $description.build $wbVerification.expectedBuild
    $wbProof.build = $description.build; $wbChecks.Add('binary-description')
    $wbConfig = Join-Path $wbVerification.workDirectory 'worker.local.json'
    $wbProject = Join-Path $wbVerification.workDirectory 'project'; New-Item -ItemType Directory -Path $wbProject | Out-Null
    $config = @{schemaVersion=1;instanceId=('pv-' + [Guid]::NewGuid().ToString('N'));nodeId='provider-verification';stateDirectory=(Join-Path $wbVerification.workDirectory 'state');maxOutputBytesPerTask=16384;
        worker=@{keepRunningOnUiExit=$true};projects=@(@{id='probe';workspaceRoot=$wbProject;allowDirectCommands=$true;maxConcurrentMutations=1;recipes=@()})}
    [IO.File]::WriteAllText($wbConfig,($config | ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))
    $start = [Diagnostics.ProcessStartInfo]::new($wbVerification.workerExecutable); $start.UseShellExecute=$false; $start.CreateNoWindow=$true
    $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
    $start.StandardOutputEncoding=[Text.UTF8Encoding]::new($false); $start.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
    foreach ($arg in @('--config',$wbConfig,'serve')) { $start.ArgumentList.Add($arg) }
    $wbWorker=[Diagnostics.Process]::new();$wbWorker.StartInfo=$start;$null=$wbWorker.Start()
    $wbOut=$wbWorker.StandardOutput.ReadToEndAsync();$wbErr=$wbWorker.StandardError.ReadToEndAsync()
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do {
        try { $wbInfo=(wbNativeJson @('--config',$wbConfig,'info')).info; break } catch {
            if ($wbWorker.HasExited -or [DateTime]::UtcNow -ge $deadline) { throw }; Start-Sleep -Milliseconds 100
        }
    } while ($true)
    wbSameBuild $wbInfo.build $wbVerification.expectedBuild
    wbAssert ($wbInfo.contractSchemas.schemas[0].sha256 -ceq $wbVerification.expectedSchemaSha256) 'Actual provider schema mismatch'
    wbAssert ($wbInfo.fencedRequests.requestContractVersion -eq 2) 'Provider does not implement request contract 2'
    wbAssert ($wbInfo.projectPolicies.provider.buildId -ceq $wbInfo.build.buildId) 'Policy provider mismatch'
    $wbChecks.Add('ipc-info-schema-policy')
    $success=wbSubmit 'provider-success' 'success';$done=wbWait $success.task.taskId
    wbAssert ($done.status -eq 'succeeded' -and $done.exitCode -eq 0) 'Provider success task failed'
    $repeat=wbInvoke (wbRequest $success.request);wbAssert ($repeat.response.task.taskId -ceq $done.taskId) 'Duplicate key created a different task'
    wbAssert (@([IO.File]::ReadAllLines((Join-Path $wbProject 'runs.log'))).Count -eq 1) 'Duplicate key created another physical process'
    $wbChecks.Add('task-success-dedup')
    $output=wbInvoke (wbRequest @{method='read_output';taskId=$done.taskId;afterSequence=0})
    $bytes=[Collections.Generic.List[byte]]::new()
    foreach ($chunk in $output.response.chunks) { if ($chunk.stream -eq 'stdout') { $bytes.AddRange([byte[]]$chunk.bytes) } }
    wbAssert ([Text.Encoding]::UTF8.GetString($bytes.ToArray()).Contains('Привет ✓')) 'UTF-8 output lost'
    $wbChecks.Add('output-exit')
    $wrong=wbRequest $success.request; $wrong.context=@{instanceId=$wbInfo.instanceId;journalIncarnation='wrong-incarnation'}
    $rejected=wbInvoke $wrong;wbAssert ($rejected.outcome -eq 'rejected_before_dispatch' -and $rejected.response.kind -eq 'error') 'Wrong scope not rejected before dispatch'
    $wbChecks.Add('stale-scope-rejected')
    $tree=wbSubmit 'provider-tree' 'tree'
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    $wbTreeChild = $null
    do {
        $running=wbTask $tree.task.taskId
        $page=wbInvoke (wbRequest @{method='read_output';taskId=$tree.task.taskId;afterSequence=0})
        $treeBytes=[Collections.Generic.List[byte]]::new()
        foreach($chunk in $page.response.chunks) { if($chunk.stream -eq 'stdout') { $treeBytes.AddRange([byte[]]$chunk.bytes) } }
        $treeText=[Text.Encoding]::UTF8.GetString($treeBytes.ToArray())
        if ($treeText -match 'child pid=(\d+)') { $wbTreeChild=[int]$Matches[1]; break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    wbAssert ($running.status -eq 'running' -and $running.processId) 'Tree did not start'
    wbAssert ($wbTreeChild -and (Get-Process -Id $wbTreeChild -ErrorAction SilentlyContinue)) 'Managed descendant did not start'
    $cancel=wbInvoke (wbRequest @{method='cancel';taskId=$tree.task.taskId});wbAssert ($cancel.outcome -eq 'responded') 'Cancellation unconfirmed'
    $cancelled=wbWait $tree.task.taskId;wbAssert ($cancelled.status -eq 'cancelled') 'Provider did not finish cancellation'
    wbAssert (-not (Get-Process -Id $running.processId -ErrorAction SilentlyContinue)) 'Cancelled root remains alive'
    wbAssert (-not (Get-Process -Id $wbTreeChild -ErrorAction SilentlyContinue)) 'Cancelled descendant remains alive'
    $wbChecks.Add('managed-tree-cancel')
    wbAssert ((Get-FileHash -LiteralPath $wbVerification.workerExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $wbProof.binarySha256) 'Provider executable changed during verification'
} catch { $wbFailure=$_.Exception.Message } finally {
    if ($wbWorker) {
        try {
            if (-not $wbWorker.HasExited) { $wbWorker.Kill($true) }
            wbAssert ($wbWorker.WaitForExit(10000)) 'Isolated provider cleanup unconfirmed'
            $null=$wbOut.GetAwaiter().GetResult();$null=$wbErr.GetAwaiter().GetResult()
            $wbChecks.Add('isolated-provider-stopped')
        } catch { $wbFailure=$_.Exception.Message } finally { $wbWorker.Dispose() }
    }
    $wbProof.checks=@($wbChecks);$wbProof.error=$wbFailure;$wbProof.passed=($null -eq $wbFailure -and $wbChecks.Count -eq 7)
    if (Test-Path -LiteralPath $wbVerification.reportDirectory -PathType Container) {
        $path=Join-Path $wbVerification.reportDirectory 'provider-report.json';$data=[Text.UTF8Encoding]::new($false).GetBytes(($wbProof|ConvertTo-Json -Depth 30));$file=[IO.File]::Open($path,[IO.FileMode]::CreateNew)
        try { $file.Write($data);$file.Flush($true) } finally { $file.Dispose() }
        $digest=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes($path))).ToLowerInvariant()
        [IO.File]::WriteAllText((Join-Path $wbVerification.reportDirectory 'provider-report.ready.json'),(@{schemaVersion=1;bytes=$data.Length;sha256=$digest;passed=$wbProof.passed}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
    }
}
if (-not $wbProof.passed) { throw "Installed provider verification failed: $wbFailure" }
Write-Output 'Installed provider RPC-2 verification passed; isolated provider stopped.'
