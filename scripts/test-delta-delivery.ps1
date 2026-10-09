<#
.SYNOPSIS
Imports Debian-produced full/delta packages through an isolated, already tested worker.
.DESCRIPTION
Called by validate-delta-delivery.py after --delta-only passed. No build of WinBridge,
desktop windows, network connection, installed runtime change or snapshot cleanup.
The worker executable/CLI helper come from the verified native source run.
#>
param([Parameter(Mandatory)][string]$WorkerExecutable,
      [Parameter(Mandatory)][string]$WorkerCliScript,
      [Parameter(Mandatory)][string]$ExpectedSourceCommit,
      [Parameter(Mandatory)][string]$ProfilePath,
      [Parameter(Mandatory)][string]$ProfileSha256,
      [Parameter(Mandatory)][string]$LocalDirectory,
      [Parameter(Mandatory)][string]$ReportDirectory)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($env:OS -ne 'Windows_NT') { throw 'Actual Windows is required' }
if ((Get-FileHash -LiteralPath $ProfilePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ProfileSha256) { throw 'Delta profile SHA-256 mismatch' }
. $WorkerCliScript
$profile = Get-Content -LiteralPath $ProfilePath -Raw | ConvertFrom-Json
if (Test-Path -LiteralPath $LocalDirectory) { throw 'Refusing to overwrite a test workspace' }
New-Item -ItemType Directory -Path $LocalDirectory | Out-Null
$workspace = Join-Path $LocalDirectory 'project'
New-Item -ItemType Directory -Path $workspace | Out-Null
$configPath = Join-Path $LocalDirectory 'worker.local.json'
$config = [ordered]@{ schemaVersion=1; instanceId=('delta-gate-' + [guid]::NewGuid().ToString('N')); nodeId='delta-test'; stateDirectory=(Join-Path $LocalDirectory 'state');
    worker=@{controllerUrl=$null;keepRunningOnUiExit=$true}; delivery=@{adapter='shared-folder';root=$profile.deliveryRoot;namespace='WinBridge'};
    projects=@(@{id='fixture';workspaceRoot=$workspace;allowDirectCommands=$true;maxConcurrentMutations=1;recipes=@()}) }
$config | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $configPath -Encoding utf8
$result = [ordered]@{schemaVersion=1;platform='windows';passed=$false;profileSha256=$ProfileSha256;worker=$null;cases=@();controlLatencyMs=@();cancel=$null;attention=$null;error=$null}
$process = $null
function Call-Worker([string[]]$Arguments) { Invoke-WorkerJson -Executable $WorkerExecutable -ConfigPath $configPath -Arguments $Arguments }
function Save-Request([object]$Request, [string]$Name) {
    $path = Join-Path $LocalDirectory ($Name + '.json')
    if (Test-Path -LiteralPath $path) { throw 'Request already exists; no automatic replay' }
    $Request | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $path -Encoding utf8
    return $path
}
function Check-Info {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $info = (Call-Worker -Arguments @('info')).info
    $result.controlLatencyMs += $clock.Elapsed.TotalMilliseconds
    if ($info.generation -ne $result.worker.generation -or $info.journalRecoveryRequired -or $info.maintenanceError) { throw 'Isolated worker health/generation changed unexpectedly' }
}
function Import-Package([object]$Ready, [string]$Name) {
    $request = @{protocolVersion=1;requestId=[guid]::NewGuid().ToString();projectId='fixture';snapshotId=$Ready.snapshotId;manifestSha256=$Ready.manifestSha256;idempotencyKey=$Name;timeoutMs=90000}
    $path = Save-Request $request $Name
    $method = if ($Ready.schemaVersion -eq 2) {'import-delta'} else {'import'}
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $accepted = (Call-Worker -Arguments @($method,$path)).import
    $id = $accepted.importId
    do {
        $record = (Call-Worker -Arguments @('get-import',$id)).import
        if ($record.status -notin @('queued','running')) { break }
        Check-Info
        if ($clock.Elapsed.TotalSeconds -gt 100) { throw "Import $id deadline; reconcile the retained request" }
        Start-Sleep -Milliseconds 80
    } while ($true)
    if ($record.status -ne 'succeeded') { throw "Import $id failed: $($record.reason)" }
    return @{record=$record;observedMs=$clock.Elapsed.TotalMilliseconds}
}
function Run-Task([string]$SnapshotId, [string]$Executable, [string[]]$Arguments, [string]$Name) {
    $request = @{protocolVersion=1;requestId=[guid]::NewGuid().ToString();projectId='fixture';snapshotId=$(if ($SnapshotId) {$SnapshotId} else {$null});idempotencyKey=$Name;timeoutMs=90000;command=@{kind='direct';executable=$Executable;arguments=$Arguments;workingDirectory='.'}}
    $path = Save-Request $request $Name
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $accepted = (Call-Worker -Arguments @('submit',$path)).task
    do {
        $task = (Call-Worker -Arguments @('get',$accepted.taskId)).task
        if ($task.status -notin @('queued','running')) { break }
        Check-Info
        if ($clock.Elapsed.TotalSeconds -gt 100) { throw "Task $($task.taskId) deadline; reconcile the retained request" }
        Start-Sleep -Milliseconds 80
    } while ($true)
    if ($task.status -ne 'succeeded' -or $task.exitCode -ne 0) { throw "Task $($task.taskId) failed: $($task.reason)" }
    return $task
}
try {
    $process = Start-Process -FilePath $WorkerExecutable -ArgumentList @('--config',('"'+$configPath+'"'),'serve') -NoNewWindow -PassThru -RedirectStandardOutput (Join-Path $ReportDirectory 'worker.stdout.log') -RedirectStandardError (Join-Path $ReportDirectory 'worker.stderr.log')
    $clock = [Diagnostics.Stopwatch]::StartNew()
    do {
        if ($process.HasExited) { throw 'Isolated worker exited before IPC became ready' }
        try { $info = (Call-Worker -Arguments @('info')).info; break } catch { if ($clock.Elapsed.TotalSeconds -gt 15) {throw}; Start-Sleep -Milliseconds 100 }
    } while ($true)
    if (-not $info.snapshotDeltaAvailable -or $info.build.sourceCommit -ne $ExpectedSourceCommit -or $info.build.sourceDirty) { throw 'Test worker is not the clean, delta-capable native source build' }
    $result.workerExecutableSha256=(Get-FileHash -LiteralPath $WorkerExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    $result.worker = $info
    $result.baseImport = Import-Package $profile.base 'base'
    $cache = Join-Path $workspace 'build-cache'
    foreach ($case in $profile.cases) {
        if ($case.index % 2 -eq 0) {
            $delta = Import-Package $case.delta.ready ('delta-'+$case.index)
            $full = Import-Package $case.full.ready ('full-'+$case.index)
        } else {
            $full = Import-Package $case.full.ready ('full-'+$case.index)
            $delta = Import-Package $case.delta.ready ('delta-'+$case.index)
        }
        $repeatPath = Join-Path $LocalDirectory ('delta-'+$case.index+'.json')
        $repeat = (Call-Worker -Arguments @('import-delta',$repeatPath)).import
        if ($repeat.importId -ne $delta.record.importId) { throw 'Duplicate delta request created another import' }
        $fullManifest = Get-Content -LiteralPath (Join-Path $full.record.workspacePath '.winbridge/manifest.json') -Raw | ConvertFrom-Json
        $deltaManifest = Get-Content -LiteralPath (Join-Path $delta.record.workspacePath '.winbridge/manifest.json') -Raw | ConvertFrom-Json
        if (($fullManifest.files | ConvertTo-Json -Depth 10 -Compress) -ne ($deltaManifest.files | ConvertTo-Json -Depth 10 -Compress)) { throw 'Full and delta manifests differ' }
        $build = Run-Task -SnapshotId $case.delta.ready.snapshotId -Executable 'cargo.exe' -Arguments @('build','--locked','--offline','--target-dir',$cache) -Name ('build-'+$case.index)
        if ($build.snapshot.manifestSha256 -ne $case.delta.ready.manifestSha256 -or $build.snapshot.source.commit -ne $case.commit) { throw 'Build source pin differs from Debian publication' }
        $run = Run-Task -SnapshotId '' -Executable (Join-Path $cache 'debug/snapshot-fixture.exe') -Arguments @() -Name ('run-'+$case.index)
        $bytes = [Collections.Generic.List[byte]]::new()
        $sequence = 0
        do {
            $page = (Call-Worker -Arguments @('output',$run.taskId,[string]$sequence)).chunks
            foreach ($chunk in $page) { if ($chunk.stream -eq 'stdout') { $bytes.AddRange([byte[]]$chunk.bytes) }; $sequence=$chunk.sequence }
        } while ($page.Count -gt 0)
        $stdout = [Text.Encoding]::UTF8.GetString($bytes.ToArray())
        if (-not $stdout.Contains($case.expectedOutput)) { throw 'Actual built executable output differs from the Debian commit' }
        $result.cases += @{index=$case.index;fullImport=$full;deltaImport=$delta;build=$build;run=$run;stdout=$stdout;filesEqual=$true}
    }
    # Actual managed tree cancellation and explicit attention after delta I/O.
    $fixture = Join-Path (Split-Path $WorkerExecutable -Parent) 'winbridge-fixture.exe'
    $path = Save-Request @{protocolVersion=1;requestId=[guid]::NewGuid().ToString();projectId='fixture';idempotencyKey='cancel-probe';timeoutMs=60000;command=@{kind='direct';executable=$fixture;arguments=@('tree');workingDirectory='.'}} 'cancel-probe'
    $task = (Call-Worker -Arguments @('submit',$path)).task
    $clock = [Diagnostics.Stopwatch]::StartNew()
    do {
        $current = (Call-Worker -Arguments @('get',$task.taskId)).task
        if ($current.status -eq 'running' -and $current.processId) {
            $chunks=(Call-Worker -Arguments @('output',$task.taskId,'0')).chunks
            $runningBytes=[Collections.Generic.List[byte]]::new()
            foreach ($chunk in $chunks) {if ($chunk.stream -eq 'stdout') {$runningBytes.AddRange([byte[]]$chunk.bytes)}}
            $runningOutput=[Text.Encoding]::UTF8.GetString($runningBytes.ToArray())
            if ($runningOutput -match 'child pid=(\d+)') {$childId=[int]$Matches[1];break}
        }
        if ($clock.Elapsed.TotalSeconds -gt 15 -or $current.status -notin @('queued','running')) { throw 'Tree fixture did not enter running' }
        Start-Sleep -Milliseconds 80
    } while ($true)
    $clock.Restart(); $null = Call-Worker -Arguments @('cancel',$task.taskId)
    do {
        $current = (Call-Worker -Arguments @('get',$task.taskId)).task
        if ($current.status -notin @('queued','running')) {break}
        if ($clock.Elapsed.TotalSeconds -gt 15) { throw 'Cancellation deadline' }
        Start-Sleep -Milliseconds 80
    } while ($true)
    if ($current.status -ne 'cancelled' -or (Get-Process -Id $current.processId -ErrorAction SilentlyContinue) -or (Get-Process -Id $childId -ErrorAction SilentlyContinue)) { throw 'Owned tree did not terminate' }
    $result.cancel = @{task=$current;childId=$childId;runningOutput=$runningOutput;observedMs=$clock.Elapsed.TotalMilliseconds}
    $path = Save-Request @{requestId=[guid]::NewGuid().ToString();taskId=$task.taskId;idempotencyKey='attention-probe';severity='info';message='Проверка delta завершена · automatic isolated gate'} 'attention-probe'
    $clock.Restart();$attention=Call-Worker -Arguments @('attention',$path)
    $result.attention=@{response=$attention;observedMs=$clock.Elapsed.TotalMilliseconds}
    Check-Info
    $finalInfo = (Call-Worker -Arguments @('info')).info
    if ($finalInfo.activeTaskCount -ne 0) { throw 'Isolated worker still has active tasks' }
    $result.finalWorker=$finalInfo
    $result.passed=$true
} catch {
    $result.error=$_.Exception.Message
    throw
} finally {
    # This PID belongs only to this scenario. No installed worker is stopped.
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
    $result | ConvertTo-Json -Depth 60 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'windows-report.json') -Encoding utf8
}
