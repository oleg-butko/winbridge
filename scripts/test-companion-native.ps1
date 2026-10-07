<#
.SYNOPSIS
Runs an isolated native desktop/worker. Does not update the installed WinBridge runtime.
.DESCRIPTION
Called by test-windows-workflow.ps1 -CompanionOnly after offline builds. Each desktop
phase uses its actual Tauri windows and React actions. No debugger/listening control port.
Only this script's child PIDs are stopped during cleanup; reports and runtime are retained.
#>
param([Parameter(Mandatory)][string]$SourceDirectory, [Parameter(Mandatory)][string]$ReportDirectory)
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
@{ schemaVersion = 1; workerConfigPath = 'winbridge-worker.local.json' } |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runtime 'winbridge-desktop.local.json') -Encoding utf8
$worker = Join-Path $runtime 'winbridge-worker.exe'
$desktop = Join-Path $runtime 'winbridge-desktop.exe'
$workerProcess = $null
$desktopProcess = $null
$result = [ordered]@{platform='windows'; passed=$false; runtime=$runtime; instanceId=$instance; phases=@(); workerSurvivesQuit=$false; error=$null}
try {
    $workerProcess = Start-Process -FilePath $worker -ArgumentList @('--config', ('"' + $configPath + '"'), 'serve') -NoNewWindow -PassThru -RedirectStandardOutput (Join-Path $ReportDirectory 'companion-worker.stdout.log') -RedirectStandardError (Join-Path $ReportDirectory 'companion-worker.stderr.log')
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        if ($workerProcess.HasExited) { throw 'Isolated worker exited before readiness' }
        try { $info = (Invoke-WorkerJson -Executable $worker -ConfigPath $configPath -Arguments @('info')).info; break }
        catch { if ([DateTime]::UtcNow -ge $deadline) { throw }; Start-Sleep -Milliseconds 100 }
    } while ($true)
    if (-not $info.attentionAvailable -or $info.instanceId -ne $instance) { throw 'Wrong isolated worker or unsupported attention capability' }
    foreach ($phase in @('initial', 'restart')) {
        $phaseReport = Join-Path $ReportDirectory ('companion-' + $phase + '.json')
        $mode = if ($phase -eq 'initial') { '--validate-companion' } else { '--validate-companion-restart' }
        $desktopProcess = Start-Process -FilePath $desktop -ArgumentList @($mode, ('"' + $phaseReport + '"')) -PassThru -RedirectStandardOutput (Join-Path $ReportDirectory ('companion-' + $phase + '.stdout.log')) -RedirectStandardError (Join-Path $ReportDirectory ('companion-' + $phase + '.stderr.log'))
        if (-not $desktopProcess.WaitForExit(240000)) { throw "Native companion $phase exceeded deadline" }
        $desktopProcess.Refresh()
        if ($desktopProcess.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $phaseReport)) { throw "Native companion $phase failed; see retained logs/report" }
        $observation = Get-Content -LiteralPath $phaseReport -Raw | ConvertFrom-Json
        if (-not $observation.passed -or $observation.platform -ne 'windows') { throw "Native companion $phase reported failure" }
        $result.phases += $observation
        if ($workerProcess.HasExited) { throw 'Quitting the desktop terminated its independent worker' }
        $info = (Invoke-WorkerJson -Executable $worker -ConfigPath $configPath -Arguments @('info')).info
        if ($info.instanceId -ne $instance) { throw 'Worker identity changed after desktop quit' }
    }
    $result.workerSurvivesQuit = $true
    $result.passed = $true
} catch {
    $result.error = $_.Exception.Message
    throw
} finally {
    if ($desktopProcess -and -not $desktopProcess.HasExited) { Stop-Process -Id $desktopProcess.Id -ErrorAction SilentlyContinue }
    if ($workerProcess -and -not $workerProcess.HasExited) { Stop-Process -Id $workerProcess.Id -ErrorAction SilentlyContinue }
    $result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'companion-report.json') -Encoding utf8
}
