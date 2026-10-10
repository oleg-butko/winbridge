<#
.SYNOPSIS
Проверяет исходники WinBridge на настоящей Windows и сохраняет отчёт в shared.
.DESCRIPTION
Это автоматический тестовый сценарий, вызываемый из Debian через уже работающий worker.
Для обновления рабочего worker/desktop используйте .\scripts\prepare-windows-snapshot.ps1.
Сценарий проверяет source manifest, собирает тесты Cargo offline, запускает изолированные
worker/process/import/workflow проверки и записывает report.json, logs и report.ready.json.
.PARAMETER SourceDirectory
Корень распакованных исходников с Cargo.toml, scripts и winbridge-source-manifest.json.
По умолчанию определяется как родитель папки scripts этого сценария, независимо от cwd.
Пример: K:\_proj-26\winbridge\src. Это папка исходников, из которых собираются тесты.
.PARAMETER ReportDirectory
Новая папка одного отчёта, обычно K:\VMs\shared\WinBridge\workflow-validation\<run>.
.PARAMETER SourceArchiveSha256
SHA-256 исходного ZIP, из которого распакованы исходники. Нужен для связи отчёта с
проверенной версией. Агент передаёт его автоматически; вручную можно взять из .zip.sha256.
.PARAMETER Help
Показывает назначение сценария и команду обновления, без сборки или записи файлов.
.PARAMETER AttentionOnly
Запускает только новые stage-5 attention tests; завершённый Windows workflow gate не повторяется.
.PARAMETER WorkflowAttentionOnly
Проверяет новые workflow/application events, миграцию и native popup → workflow navigation.
.PARAMETER HistoryDiagnosticsOnly
Проверяет историю, восстановление journal, redaction и native UI → диагностический файл.
.PARAMETER SettingsRetentionOnly
Проверяет локальные настройки, сохранение без влияния на активный процесс, перезапуск
отдельного worker, preview/atomic очистку output и включаемую automatic policy.
.PARAMETER DeltaOnly
Проверяет full/delta на настоящем HGFS, отдельный worker journal, dedup/recovery
и offline build с проверенным source pin. Desktop и тестовые окна не открываются.
.PARAMETER OutputOnly
Проверяет bounded output pages, настоящий IPC, hashes обоих потоков, dedup,
cursor restart и cancel/attention. Desktop и тестовые окна не открываются.
.PARAMETER ResumeOnly
Проверяет manifest-pinned blocks на настоящем HGFS, cancel/corruption и actual
crash/restart отдельного worker, dedup, explicit resume и offline pinned build.
Рабочие exe/state не меняются; desktop и тестовые окна не открываются.
.PARAMETER ProfilesOnly
Проверяет общий профиль для двух проектов: реальные argv/cwd, запрет direct, dedup,
cancel tree, crash/restart и frozen command/provenance. Desktop не открывается.
.PARAMETER OutputReferenceWorker
Путь рабочего worker.exe только для запуска отдельного reference runtime с новым
state directory. Рабочий процесс, конфиг и журнал не изменяются.
.PARAMETER CompanionOnly
Собирает отдельный desktop/worker и проверяет tray, popup, звук, переход к задаче и restart.
Требует source archive с --include-built-ui; рабочие exe не заменяет.
.EXAMPLE
.\scripts\test-windows-workflow.ps1 -Help
.NOTES
Полная инструкция: docs/windows-workflow-validation.md.
#>
param(
    [string]$SourceDirectory = '',
    [string]$ReportDirectory = '',
    [string]$SourceArchiveSha256 = '',
    [string]$NetwatchProfile = '',
    [switch]$AttentionOnly,
    [switch]$CompanionOnly,
    [switch]$WorkflowAttentionOnly,
    [switch]$HistoryDiagnosticsOnly,
    [switch]$SettingsRetentionOnly,
    [switch]$DeltaOnly,
    [switch]$OutputOnly,
    [switch]$ResumeOnly,
    [switch]$ProfilesOnly,
    [string]$ResumeDeliveryRoot = '',
    [string]$ResumePublicationFile = '',
    [string]$OutputReferenceWorker = '',
    [switch]$Help
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($Help -or -not $ReportDirectory -or -not $SourceArchiveSha256) {
    Write-Host @'
test-windows-workflow.ps1 — автоматические Windows тесты WinBridge.
Этот сценарий агент запускает из Debian; он не обновляет рабочие exe.
-CompanionOnly открывает отдельный тестовый desktop для автоматической проверки tray.

Чтобы обновить и запустить WinBridge после распаковки исходников:
  1. Завершите desktop через «Завершить интерфейс» в окне/tray и остановите worker через Ctrl+C.
  2. Из папки исходников выполните: .\scripts\prepare-windows-snapshot.ps1

Для отдельного тестового прогона нужны:
  -SourceDirectory      Папка с Cargo.toml, scripts и winbridge-source-manifest.json.
                        По умолчанию: корень исходников этого сценария.
  -ReportDirectory      Новая папка отчёта в K:\VMs\shared\WinBridge\workflow-validation.
  -SourceArchiveSha256  SHA-256 исходного ZIP для связи отчёта с проверенной версией.

Тесты агент запускает автоматически; повтор вручную для обновления не нужен.
Объяснение и примеры: docs/windows-workflow-validation.md
'@
    if ($Help) { exit 0 }
    Write-Host 'Тесты не запущены: укажите ReportDirectory и SourceArchiveSha256 или используйте команду обновления выше.'
    exit 2
}
if (-not $SourceDirectory) { $SourceDirectory = Join-Path $PSScriptRoot '..' }
if ($env:OS -ne 'Windows_NT') { throw 'This scenario requires actual Windows' }
$source = (Resolve-Path -LiteralPath $SourceDirectory).Path
$manifest = Get-Content -LiteralPath (Join-Path $source 'winbridge-source-manifest.json') -Raw | ConvertFrom-Json
$report = [ordered]@{ schemaVersion = 1; platform = 'windows'; passed = $false; sourceArchiveSha256 = $SourceArchiveSha256;
    sourceCommit = $manifest.sourceCommit; sourceDirty = $manifest.dirty; startedAt = [DateTime]::UtcNow.ToString('o'); commands = @(); error = $null }
New-Item -ItemType Directory -Path $ReportDirectory -ErrorAction Stop | Out-Null
$reportPath = Join-Path $ReportDirectory 'report.json'
$oldTarget = $env:CARGO_TARGET_DIR
$oldTemp = $env:TEMP
$oldTmp = $env:TMP
$oldDeltaShared = $env:WINBRIDGE_DELTA_GATE_SHARED
$oldOutputReport = $env:WINBRIDGE_OUTPUT_GATE_REPORT
$oldOutputReference = $env:WINBRIDGE_OUTPUT_REFERENCE_WORKER
$oldProfileReport = $env:WINBRIDGE_PROFILE_GATE_REPORT
$oldResumeShared = $env:WINBRIDGE_RESUME_GATE_SHARED
$oldResumeDelivery = $env:WINBRIDGE_RESUME_GATE_DELIVERY
$oldResumePublication = $env:WINBRIDGE_RESUME_GATE_PUBLICATION
$oldLocation = Get-Location
try {
    foreach ($file in $manifest.files) {
        $filePath = Join-Path $source $file.path
        $item = Get-Item -LiteralPath $filePath
        if ($item.Length -ne $file.bytes -or (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) {
            throw "Source manifest mismatch: $($file.path)"
        }
    }
    $env:CARGO_TARGET_DIR = Join-Path (Split-Path $source -Parent) 'test-build-cache'
    $env:TEMP = Join-Path (Split-Path $source -Parent) 'test-temp'
    $env:TMP = $env:TEMP
    New-Item -ItemType Directory -Path $env:TEMP -Force | Out-Null
    Set-Location -LiteralPath $source
    $cargo = (Get-Command cargo.exe -ErrorAction Stop).Source
    $groups = @(
        @{ name = 'core-and-log-reader'; arguments = @('test', '-p', 'winbridge-core', '-p', 'winbridge-log-reader', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
        @{ name = 'readiness-ipc'; arguments = @('test', '-p', 'winbridge-transport', '--lib', 'readiness', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
        @{ name = 'worker-runtime-snapshots-workflows'; arguments = @('test', '-p', 'winbridge-worker', '--features', 'bundled-sqlite', '--test', 'runtime', '--test', 'snapshots', '--test', 'workflows', '--test', 'attention', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
        @{ name = 'desktop-native-check'; arguments = @('check', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--locked', '--offline') }
    )
    if (@($AttentionOnly,$CompanionOnly,$WorkflowAttentionOnly,$HistoryDiagnosticsOnly,$SettingsRetentionOnly,$DeltaOnly,$OutputOnly,$ResumeOnly,$ProfilesOnly).Where({ $_ }).Count -gt 1) { throw 'Choose exactly one isolated scenario' }
    if ($ProfilesOnly) {
        if ($NetwatchProfile) { throw 'ProfilesOnly requires an isolated scenario' }
        $env:WINBRIDGE_PROFILE_GATE_REPORT = $ReportDirectory
        $groups = @(
            @{ name='profile-config-policy'; arguments=@('test','-p','winbridge-core','-p','winbridge-runner','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='profile-native-ipc-projects-dedup-cancel-recovery'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','profiles','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='profile-runner-snapshot-workflow-regression'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','runtime','--test','snapshots','--test','workflows','--locked','--offline','--','--test-threads=1','--nocapture') }
        )
    }
    if ($ResumeOnly) {
        if ($NetwatchProfile) { throw 'ResumeOnly requires an isolated scenario' }
        $env:WINBRIDGE_RESUME_GATE_SHARED = $ReportDirectory
        if ([bool]$ResumeDeliveryRoot -ne [bool]$ResumePublicationFile) { throw 'Cross-host resume needs both delivery root and pinned publication file' }
        if ($ResumeDeliveryRoot) {
            $env:WINBRIDGE_RESUME_GATE_DELIVERY = (Resolve-Path -LiteralPath $ResumeDeliveryRoot).Path
            $env:WINBRIDGE_RESUME_GATE_PUBLICATION = (Resolve-Path -LiteralPath $ResumePublicationFile).Path
        } else {
            $env:WINBRIDGE_RESUME_GATE_DELIVERY = $null
            $env:WINBRIDGE_RESUME_GATE_PUBLICATION = $null
        }
        $groups = @(
            @{ name='resume-metadata-policy'; arguments=@('test','-p','winbridge-core','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='resume-hgfs-blocks-corruption-cancel-delta'; arguments=@('test','-p','winbridge-workspace-sync','--test','resume','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='resume-native-worker-crash-restart-dedup-pinned-build'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','resume','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='resume-tree-timeout-regression'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','runtime','--locked','--offline','--','--test-threads=1','--nocapture') }
        )
    }
    if ($OutputOnly) {
        if ($NetwatchProfile -or -not $OutputReferenceWorker) { throw 'OutputOnly needs an explicit read-only reference worker path and isolated scenario' }
        $env:WINBRIDGE_OUTPUT_GATE_REPORT = $ReportDirectory
        $env:WINBRIDGE_OUTPUT_REFERENCE_WORKER = (Resolve-Path -LiteralPath $OutputReferenceWorker).Path
        $groups = @(
            @{ name='output-bounds-native-ipc-cursor-control'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','output','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='output-quota-tree-timeout-regression'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','runtime','--locked','--offline','--','--test-threads=1','--nocapture') }
        )
    }
    if ($DeltaOnly) {
        if ($NetwatchProfile) { throw 'DeltaOnly requires an isolated scenario' }
        # Only newly owned run directories are used; actual HGFS bytes are verified.
        $env:WINBRIDGE_DELTA_GATE_SHARED = $ReportDirectory
        $groups = @(
            @{ name='delta-metadata-and-path-policy'; arguments=@('test','-p','winbridge-core','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='delta-hgfs-full-equivalence-cancel-and-base-verification'; arguments=@('test','-p','winbridge-workspace-sync','--lib','--test','delta','--test','delivery','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='delta-worker-dedup-recovery-build-source-pin'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','snapshots','delta_worker_','--locked','--offline','--','--test-threads=1','--nocapture') }
        )
    }
    if ($SettingsRetentionOnly) {
        if ($NetwatchProfile -or -not $manifest.includesBuiltUi) { throw 'Settings retention validation needs built UI and an isolated scenario' }
        $groups = @(
            @{ name='settings-retention-transactions-and-recovery'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','maintenance','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='config-policy-and-preservation'; arguments=@('test','-p','winbridge-core','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='diagnostic-bounded-readback-review'; arguments=@('test','-p','winbridge-platform','reports::','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='companion-worker-build'; arguments=@('build','-p','winbridge-worker','--features','bundled-sqlite','--bins','--locked','--offline') },
            @{ name='companion-desktop-build'; arguments=@('build','--manifest-path','apps/desktop/src-tauri/Cargo.toml','--features','custom-protocol','--locked','--offline') }
        )
    }
    if ($HistoryDiagnosticsOnly) {
        if ($NetwatchProfile) { throw 'HistoryDiagnosticsOnly cannot be combined with a Netwatch profile' }
        if (-not $manifest.includesBuiltUi) { throw 'History UI validation requires --include-built-ui' }
        $groups = @(
            @{ name='history-pagination-and-diagnostics'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','history','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='history-pipeline-and-journal-recovery'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','workflows','--test','snapshots','history_','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='child-journal-fault-review'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','workflows','attention_uncommitted_child_result','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='diagnostic-file-publication'; arguments=@('test','-p','winbridge-platform','reports::','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='diagnostic-redaction'; arguments=@('test','-p','winbridge-core','redaction_','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='desktop-report-directory'; arguments=@('test','-p','winbridge-core','report_directory_','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='companion-worker-build'; arguments=@('build','-p','winbridge-worker','--features','bundled-sqlite','--bins','--locked','--offline') },
            @{ name='companion-desktop-build'; arguments=@('build','--manifest-path','apps/desktop/src-tauri/Cargo.toml','--features','custom-protocol','--locked','--offline') }
        )
    }
    if ($AttentionOnly) {
        if ($NetwatchProfile) { throw 'AttentionOnly cannot be combined with a Netwatch profile' }
        $groups = @(@{ name = 'worker-attention'; arguments = @('test', '-p', 'winbridge-worker', '--features', 'bundled-sqlite', '--test', 'attention', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') })
    }
    if ($CompanionOnly) {
        if ($AttentionOnly -or $NetwatchProfile) { throw 'CompanionOnly cannot be combined with another scenario' }
        if (-not $manifest.includesBuiltUi) { throw 'Companion validation requires a source archive with --include-built-ui' }
        $groups = @(
            @{ name = 'companion-policy-and-persistence'; arguments = @('test', '-p', 'winbridge-core', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
            @{ name = 'worker-attention'; arguments = @('test', '-p', 'winbridge-worker', '--features', 'bundled-sqlite', '--test', 'attention', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
            @{ name = 'companion-worker-build'; arguments = @('build', '-p', 'winbridge-worker', '--features', 'bundled-sqlite', '--bins', '--locked', '--offline') },
            @{ name = 'companion-desktop-build'; arguments = @('build', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--features', 'custom-protocol', '--locked', '--offline') }
        )
    }
    if ($WorkflowAttentionOnly) {
        if ($AttentionOnly -or $CompanionOnly -or $NetwatchProfile) { throw 'WorkflowAttentionOnly cannot be combined with another scenario' }
        if (-not $manifest.includesBuiltUi) { throw 'Workflow attention UI validation requires --include-built-ui' }
        $groups = @(
            @{ name='attention-compatibility-and-migration'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','attention','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='workflow-application-attention'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','workflows','attention_','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='companion-worker-build'; arguments=@('build','-p','winbridge-worker','--features','bundled-sqlite','--bins','--locked','--offline') },
            @{ name='companion-desktop-build'; arguments=@('build','--manifest-path','apps/desktop/src-tauri/Cargo.toml','--features','custom-protocol','--locked','--offline') }
        )
    }
    foreach ($group in $groups) {
        $log = Join-Path $ReportDirectory ($group.name + '.log')
        $commandArgs = $group.arguments
        & $cargo @commandArgs 2>&1 | Tee-Object -FilePath $log | ForEach-Object { Write-Output $_ }
        $code = $LASTEXITCODE
        $report.commands += [ordered]@{ name = $group.name; executable = $cargo; arguments = $commandArgs; exitCode = $code; log = (Split-Path $log -Leaf) }
        if ($code -ne 0) { throw "Native test group $($group.name) failed with exit $code; see $log" }
    }
    if ($NetwatchProfile) {
        & (Join-Path $source 'scripts/test-netwatch-native.ps1') -SourceDirectory $source -ReportDirectory $ReportDirectory -ProfilePath $NetwatchProfile
        $report.netwatch = Get-Content -LiteralPath (Join-Path $ReportDirectory 'netwatch-report.json') -Raw | ConvertFrom-Json
        if (-not $report.netwatch.passed) { throw 'Isolated Netwatch workflow check failed' }
    }
    if ($CompanionOnly -or $WorkflowAttentionOnly -or $HistoryDiagnosticsOnly -or $SettingsRetentionOnly) {
        & (Join-Path $source 'scripts/test-companion-native.ps1') -SourceDirectory $source -ReportDirectory $ReportDirectory -WorkflowAttention:$WorkflowAttentionOnly -HistoryDiagnostics:$HistoryDiagnosticsOnly -SettingsRetention:$SettingsRetentionOnly
        $report.companion = Get-Content -LiteralPath (Join-Path $ReportDirectory 'companion-report.json') -Raw | ConvertFrom-Json
        if (-not $report.companion.passed) { throw 'Isolated companion check failed' }
    }
    $report.passed = $true
} catch {
    $report.error = $_.Exception.Message
    Write-Output $report.error
} finally {
    Set-Location -LiteralPath $oldLocation.Path
    $env:CARGO_TARGET_DIR = $oldTarget
    $env:TEMP = $oldTemp
    $env:TMP = $oldTmp
    $env:WINBRIDGE_DELTA_GATE_SHARED = $oldDeltaShared
    $env:WINBRIDGE_OUTPUT_GATE_REPORT = $oldOutputReport
    $env:WINBRIDGE_OUTPUT_REFERENCE_WORKER = $oldOutputReference
    $env:WINBRIDGE_PROFILE_GATE_REPORT = $oldProfileReport
    $env:WINBRIDGE_RESUME_GATE_SHARED = $oldResumeShared
    $env:WINBRIDGE_RESUME_GATE_DELIVERY = $oldResumeDelivery
    $env:WINBRIDGE_RESUME_GATE_PUBLICATION = $oldResumePublication
    $report.finishedAt = [DateTime]::UtcNow.ToString('o')
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $reportPath -Encoding utf8
    $artifacts = @(Get-ChildItem -LiteralPath $ReportDirectory -File | ForEach-Object {
        # HGFS metadata may lag: size and digest must describe the same captured bytes.
        $content = [System.IO.File]::ReadAllBytes($_.FullName)
        $digest = [System.Security.Cryptography.SHA256]::HashData($content)
        [ordered]@{ path = $_.Name; bytes = $content.LongLength; sha256 = [Convert]::ToHexString($digest).ToLowerInvariant() }
    })
    [ordered]@{ schemaVersion = 1; platform = 'windows'; passed = $report.passed; files = $artifacts } |
        ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.ready.json') -Encoding utf8
}
if (-not $report.passed) { exit 1 }
