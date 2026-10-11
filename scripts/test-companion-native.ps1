<#
.SYNOPSIS
Runs an isolated native desktop/worker. Does not update the installed WinBridge runtime.
.DESCRIPTION
Called by test-windows-workflow.ps1 -CompanionOnly after offline builds. Each desktop
phase uses its actual Tauri windows and React actions. No debugger/listening control port.
Only this script's child PIDs are stopped during cleanup; reports and runtime are retained.
#>
param([Parameter(Mandatory)][string]$SourceDirectory, [Parameter(Mandatory)][string]$ReportDirectory, [switch]$WorkflowAttention, [switch]$HistoryDiagnostics, [switch]$SettingsRetention)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot 'worker-cli.ps1')
$runtime = Join-Path (Split-Path $SourceDirectory -Parent) 'companion-native-runtime'
if (Test-Path -LiteralPath $runtime) { throw 'Refusing to overwrite an existing companion validation runtime' }
New-Item -ItemType Directory -Path $runtime | Out-Null
$project = Join-Path $runtime 'project'
New-Item -ItemType Directory -Path $project | Out-Null
Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-worker.exe') -Destination $runtime
Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-desktop.exe') -Destination $runtime
Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-fixture.exe') -Destination $project
$configPath = Join-Path $runtime 'winbridge-worker.local.json'
$instance = 'tray-gate-' + [guid]::NewGuid().ToString('N')
@{ schemaVersion = 1; instanceId = $instance; nodeId = 'tray-validation'; stateDirectory = (Join-Path $runtime 'state');
    worker = @{ controllerUrl = $null; keepRunningOnUiExit = $true };
    projects = @(@{ id = 'tray-fixture'; workspaceRoot = $project; allowDirectCommands = $true; maxConcurrentMutations = 1; recipes = @() }) } |
    ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $configPath -Encoding utf8
@{ schemaVersion = 1; workerConfigPath = 'winbridge-worker.local.json'; reportDirectory = (Join-Path $runtime 'reports') } |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runtime 'winbridge-desktop.local.json') -Encoding utf8
if ($WorkflowAttention) {
    $delivery = Join-Path $runtime 'delivery'
    New-Item -ItemType Directory -Path $delivery | Out-Null
    $settings = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    $settings | Add-Member -NotePropertyName delivery -NotePropertyValue @{adapter='shared-folder';root=$delivery;namespace='WinBridge'}
    $settings | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $configPath -Encoding utf8
}
$worker = Join-Path $runtime 'winbridge-worker.exe'
$desktop = Join-Path $runtime 'winbridge-desktop.exe'
# All live writers use local NTFS. Publish only after process/output completion.
$localReports = Join-Path $runtime 'reports'
New-Item -ItemType Directory -Path $localReports | Out-Null
$captures = [System.Collections.Generic.List[object]]::new()
$result = [ordered]@{platform='windows'; passed=$false; runtime=$runtime; instanceId=$instance; phases=@(); executions=@(); workerSurvivesQuit=$false; cleanupErrors=@(); error=$null}
function Start-Captured([string]$Executable, [string[]]$Arguments, [string]$Name) {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Executable
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [System.Text.UTF8Encoding]::new($false)
    $start.StandardErrorEncoding = [System.Text.UTF8Encoding]::new($false)
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (-not $process.Start()) { throw "Failed to start $Name" }
    $record = [ordered]@{name=$Name; executable=$Executable; arguments=$Arguments; pid=$process.Id; startedAt=[DateTime]::UtcNow.ToString('o'); exitCode=$null; reportExists=$false}
    $capture = @{Process=$process; Out=$process.StandardOutput.ReadToEndAsync(); Err=$process.StandardError.ReadToEndAsync(); Name=$Name; Record=$record; Completed=$false}
    $captures.Add($capture)
    $result.executions += $record
    return $capture
}
function Complete-Captured($Capture, [bool]$Stop = $false) {
    if ($Capture.Completed) { return }
    $process = $Capture.Process
    if ($Stop -and -not $process.HasExited) { $process.Kill($true) }
    if (-not $process.WaitForExit(5000)) { throw "Owned process $($Capture.Name) did not exit" }
    $Capture.Record.exitCode = $process.ExitCode
    $Capture.Record.finishedAt = [DateTime]::UtcNow.ToString('o')
    foreach ($stream in @(@('Out','stdout'), @('Err','stderr'))) {
        $reader = $Capture[$stream[0]]
        if (-not $reader.Wait(5000)) { throw "Output drain timed out: $($Capture.Name) $($stream[1])" }
        [System.IO.File]::WriteAllText((Join-Path $localReports ($Capture.Name + '.' + $stream[1] + '.log')), $reader.GetAwaiter().GetResult(), [System.Text.UTF8Encoding]::new($false))
    }
    $Capture.Completed = $true
}
try {
    $workerCapture = Start-Captured $worker @('--config', $configPath, 'serve') 'companion-worker'
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        if ($workerCapture.Process.HasExited) { throw 'Isolated worker exited before readiness' }
        try { $info = (Invoke-WorkerJson -Executable $worker -ConfigPath $configPath -Arguments @('info')).info; break }
        catch { if ([DateTime]::UtcNow -ge $deadline) { throw }; Start-Sleep -Milliseconds 100 }
    } while ($true)
    if (-not $info.attentionAvailable -or $info.instanceId -ne $instance) { throw 'Wrong isolated worker or unsupported attention capability' }
    $phases = if ($SettingsRetention) { @('settings','settings-restart') } elseif ($HistoryDiagnostics) { @('history') } elseif ($WorkflowAttention) { @('targets') } else { @('initial','restart') }
    foreach ($phase in $phases) {
        $phaseReport = Join-Path $localReports ('companion-' + $phase + '.json')
        if ($phase -eq 'settings-restart') {
            Complete-Captured $workerCapture $true
            $workerCapture = Start-Captured $worker @('--config',$configPath,'serve') 'worker-settings-restarted'
            $ready=$false
            for ($i=0; $i -lt 100; $i++) { try { $null=Invoke-WorkerJson -Executable $worker -ConfigPath $configPath -Arguments @('info'); $ready=$true; break } catch { Start-Sleep -Milliseconds 100 } }
            if (-not $ready) { throw 'Restarted test worker unavailable' }
        }
        $mode = if ($phase -eq 'settings') { '--validate-settings-retention' } elseif ($phase -eq 'settings-restart') { '--validate-settings-retention-restart' } elseif ($phase -eq 'history') { '--validate-history-diagnostics' } elseif ($phase -eq 'targets') { '--validate-workflow-attention' } elseif ($phase -eq 'initial') { '--validate-companion' } else { '--validate-companion-restart' }
        Write-Output "Opening isolated TEST desktop ($phase). Keep the installed worker running; do not interact with the test window."
        $capture = Start-Captured $desktop @($mode, $phaseReport) ('companion-' + $phase)
        if (-not $capture.Process.WaitForExit(240000)) { throw "Native companion $phase exceeded deadline; last step retained in progress.json" }
        Complete-Captured $capture
        $capture.Record.reportExists = Test-Path -LiteralPath $phaseReport
        if ($capture.Record.exitCode -ne 0 -or -not $capture.Record.reportExists) { throw "Native companion $phase failed: exit $($capture.Record.exitCode), report exists $($capture.Record.reportExists); see progress and logs" }
        $observation = Get-Content -LiteralPath $phaseReport -Raw | ConvertFrom-Json
        $result.phases += $observation
        if (-not $observation.passed -or $observation.platform -ne 'windows') { throw "Native companion $phase reported failure: $($observation.error)" }
        if ($workerCapture.Process.HasExited) { throw 'Quitting the desktop terminated its independent worker' }
        $info = (Invoke-WorkerJson -Executable $worker -ConfigPath $configPath -Arguments @('info')).info
        if ($info.instanceId -ne $instance) { throw 'Worker identity changed after desktop quit' }
    }
    $result.workerSurvivesQuit = $true
    $desktopLog = Join-Path $runtime 'logs\desktop.jsonl'
    if (-not (Test-Path -LiteralPath $desktopLog)) { throw 'Desktop diagnostic log was not created' }
    $events = @(Get-Content -LiteralPath $desktopLog -Encoding utf8 | ForEach-Object { ($_ | ConvertFrom-Json).event })
    foreach ($required in @('native.start','ui.close.request','native.exit.requested')) {
        if ($required -notin $events) { throw "Desktop diagnostics missed required event: $required" }
    }
    if (-not ($WorkflowAttention -or $HistoryDiagnostics -or $SettingsRetention)) {
        foreach ($required in @('window.close_requested','native.hide.done','ui.close.timeout','popup.dismiss.begin','popup.dismiss.end')) {
            if ($required -notin $events) { throw "Desktop recovery diagnostics missed event: $required" }
        }
    }
    $result.desktopDiagnosticsVerified = $true
    $result.passed = $true
} catch {
    $result.error = $_.Exception.Message
    throw
} finally {
    foreach ($capture in $captures) {
        try { Complete-Captured $capture $true }
        catch { $result.cleanupErrors += $_.Exception.Message; $result.passed = $false }
        finally { $capture.Process.Dispose() }
    }
    $result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $localReports 'companion-report.json') -Encoding utf8
    if (Test-Path -LiteralPath (Join-Path $runtime 'logs')) {
        Get-ChildItem -LiteralPath (Join-Path $runtime 'logs') -Filter '*.jsonl' -File | ForEach-Object {
            Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $localReports ('desktop-' + $_.Name))
        }
    }
    # These files are immutable now; outer script hashes actual bytes, not cached HGFS stat.
    Get-ChildItem -LiteralPath $localReports -File | Copy-Item -Destination $ReportDirectory
    Write-Output 'Isolated test desktop/worker cleanup finished. The installed worker should remain running; the working desktop may be closed.'
    if ($result.cleanupErrors.Count -gt 0) { throw ('Companion cleanup failed: ' + ($result.cleanupErrors -join '; ')) }
}
