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
    [switch]$Help
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($Help -or -not $ReportDirectory -or -not $SourceArchiveSha256) {
    Write-Host @'
test-windows-workflow.ps1 — автоматические Windows тесты WinBridge.
Этот сценарий агент запускает из Debian; он не обновляет рабочие exe и не открывает desktop.

Чтобы обновить и запустить WinBridge после распаковки исходников:
  1. Закройте desktop и остановите прежний worker через Ctrl+C.
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
        @{ name = 'worker-runtime-snapshots-workflows'; arguments = @('test', '-p', 'winbridge-worker', '--features', 'bundled-sqlite', '--test', 'runtime', '--test', 'snapshots', '--test', 'workflows', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
        @{ name = 'desktop-native-check'; arguments = @('check', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--locked', '--offline') }
    )
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
    $report.passed = $true
} catch {
    $report.error = $_.Exception.Message
    Write-Output $report.error
} finally {
    Set-Location -LiteralPath $oldLocation.Path
    $env:CARGO_TARGET_DIR = $oldTarget
    $env:TEMP = $oldTemp
    $env:TMP = $oldTmp
    $report.finishedAt = [DateTime]::UtcNow.ToString('o')
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $reportPath -Encoding utf8
    $artifacts = @(Get-ChildItem -LiteralPath $ReportDirectory -File | ForEach-Object {
        [ordered]@{ path = $_.Name; bytes = $_.Length; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    [ordered]@{ schemaVersion = 1; platform = 'windows'; passed = $report.passed; files = $artifacts } |
        ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.ready.json') -Encoding utf8
}
if (-not $report.passed) { exit 1 }
