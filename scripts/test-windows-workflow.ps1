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
.PARAMETER ProjectsOnly
Проверяет реальные Netwatch/WinBridge snapshots с общим build profile, private IPC,
artifact/log, managed stop и journal restart. Desktop и сетевые соединения не нужны.
.PARAMETER IdentityOnly
Проверяет build identity на настоящей Windows/Git: parent checkout, Unicode paths
и скрытые index flags. Запускает только protocol tests, без desktop/test worker.
.PARAMETER UpdatePreparationOnly
Проверяет updater journal/ZIP, source apply/crash/migration, drain/quiesce и startup guard
на отдельных worker/journal. Собирает четыре candidate exe без установки и UI-окон.
.PARAMETER UpdateReferenceWorker
Путь к известному schema-14 worker.exe. Создаётся проверенная копия для запуска
с НОВЫМ тестовым journal schema 15; рабочий worker и его журнал не открываются.
.PARAMETER UpdateReferenceUpdater
Сохранённый предыдущий updater.exe для отказа открыть НОВУЮ тестовую schema 3.
.PARAMETER UpdateReferenceUpdaterSha256
Проверенный SHA-256 предыдущего updater: source и create-only копия сверяются до запуска.
.PARAMETER TaskFsmOnly
Проверяет A0 baseline и A1 Task FSM: сохранение launch intent до spawn, ошибку записи PID,
cancel/timeout/process tree, recovery и совместимость существующих workflows.
Рабочие exe/state не меняются; desktop и тестовые UI-окна не открываются.
.PARAMETER ReadinessOnly
Проверяет typed readiness/policy, frozen wire, native Info/request context,
согласованные страницы операций (SQLite snapshot, scoped contract, реальный IPC) и desktop compile.
Без schema-6 reference, тестовых UI-окон или изменения рабочего runtime.
.PARAMETER QueueReferenceWorker
Сохранённый schema-12 exe для отказа открыть тестовую schema 13; путь/SHA находятся
в предыдущем report.queueReferenceWorker. Рабочая БД не открывается.
.PARAMETER ResourceReferenceWorker
Schema-13 exe для отказа открыть тестовую schema 14. Копия с SHA сохраняется
в local test run и report.resourceReferenceWorker для будущих повторов.
.PARAMETER TaskCasOnly
Проверяет revisions/CAS tasks/imports/workflows/applications/attention,
миграции schema 6/7/8/9/10/11 → 12, owner/attempt tokens, atomic child links, output/retention rollback,
read-only recovery/latch, legacy contracts и process/workflow regressions без desktop/UI-окон.
Требует TaskCasReferenceWorker; старый exe запускается с НОВЫМ тестовым state,
чтобы подтвердить отказ открыть новую схему. Установленный журнал не открывается.
.PARAMETER TaskCasReferenceWorker
Read-only путь schema-6 worker.exe для TaskCasOnly; рабочий процесс не заменяется.
.PARAMETER TaskCasRuntimeDiagnostic
Только с TaskCasOnly: узкий process/recovery прогон с отметками запуска, resume,
PID binding и cleanup. Без UI, reference worker и полного CAS gate. Успех этого
режима не означает завершение этапа. Диагностика не записывает команды или output.
.PARAMETER BuildCacheRunId
Full CAS/readiness: переиспользовать только Cargo target прежнего подтверждённого CAS прогона.
Исходники, test-temp, состояния и отчёт остаются новыми; lease исключает одновременную сборку.
.PARAMETER BuildCacheReportSha256
SHA-256 прежнего report.json, который Debian проверил вместе с ready artifacts.
Провалившиеся тесты допустимы как compiler cache, но их результат не наследуется.
.PARAMETER OutputReferenceWorker
Путь рабочего worker.exe только для запуска отдельного reference runtime с новым
state directory. Рабочий процесс, конфиг и журнал не изменяются.
.PARAMETER CompanionOnly
Собирает отдельный desktop/worker и проверяет tray, popup, звук, переход к задаче и restart.
Требует source archive с --include-built-ui; рабочие exe не заменяет.
.EXAMPLE
.\scripts\test-windows-workflow.ps1 -Help
.NOTES
Полная инструкция: docs/validation/windows-workflow-validation.md.
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
    [switch]$ProjectsOnly,
    [switch]$IdentityOnly,
    [switch]$UpdatePreparationOnly,
    [string]$UpdatePreparationArchivePath = '',
    [string]$UpdatePreparationCacheRunId = '',
    [string]$UpdatePreparationCacheSha256 = '',
    [string]$UpdateReferenceWorker = '',
    [string]$UpdateReferenceUpdater = '',
    [string]$UpdateReferenceUpdaterSha256 = '',
    [switch]$TaskFsmOnly,
    [switch]$TaskCasOnly,
    [switch]$ReadinessOnly,
    [switch]$TaskCasRuntimeDiagnostic,
    [string]$TaskCasReferenceWorker = '',
    [string]$QueueReferenceWorker = '',
    [string]$ResourceReferenceWorker = '',
    [string]$BuildCacheRunId = '',
    [string]$BuildCacheReportSha256 = '',
    [string]$ProjectsPreparation = '',
    [string]$ProjectsPreparationSha256 = '',
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
Объяснение и примеры: docs/validation/windows-workflow-validation.md
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
    sourceCommit = $manifest.sourceCommit; sourceDirty = $manifest.dirty; diagnosticOnly = [bool]$TaskCasRuntimeDiagnostic; readinessOnly = [bool]$ReadinessOnly; startedAt = [DateTime]::UtcNow.ToString('o'); commands = @(); error = $null }
New-Item -ItemType Directory -Path $ReportDirectory -ErrorAction Stop | Out-Null
$reportPath = Join-Path $ReportDirectory 'report.json'
$oldTarget = $env:CARGO_TARGET_DIR
$oldIncremental = $env:CARGO_INCREMENTAL
$oldTemp = $env:TEMP
$oldTmp = $env:TMP
$oldDeltaShared = $env:WINBRIDGE_DELTA_GATE_SHARED
$oldOutputReport = $env:WINBRIDGE_OUTPUT_GATE_REPORT
$oldOutputReference = $env:WINBRIDGE_OUTPUT_REFERENCE_WORKER
$oldTaskCasReference = $env:WINBRIDGE_TASK_CAS_REFERENCE_WORKER
$oldTaskCasReport = $env:WINBRIDGE_TASK_CAS_REPORT
$oldWorkSchedulingReport = $env:WINBRIDGE_WORK_SCHEDULING_REPORT
$oldQueueControlReport = $env:WINBRIDGE_QUEUE_CONTROL_REPORT
$oldQueueReferenceWorker = $env:WINBRIDGE_QUEUE_LEGACY_WORKER
$oldResourceReport = $env:WINBRIDGE_RESOURCE_SCHEDULING_REPORT
$oldResourceReferenceWorker = $env:WINBRIDGE_RESOURCE_LEGACY_WORKER
$oldUpdateCoordinationReport = $env:WINBRIDGE_UPDATE_COORDINATION_REPORT
$oldUpdateReferenceWorker = $env:WINBRIDGE_UPDATE_REFERENCE_WORKER
$oldUpdateReferenceUpdater = $env:WINBRIDGE_UPDATE_REFERENCE_UPDATER
$oldSourceTransactionReport = $env:WINBRIDGE_SOURCE_TRANSACTION_REPORT
$oldSupervisorReport = $env:WINBRIDGE_SUPERVISOR_REPORT
$oldUpdateControlReport = $env:WINBRIDGE_UPDATE_CONTROL_REPORT
$oldUpdateBuildReport = $env:WINBRIDGE_UPDATE_BUILD_REPORT
$oldAtomicReport = $env:WINBRIDGE_ATOMIC_GATE_REPORT
$oldSourcePreparationReport = $env:WINBRIDGE_SOURCE_PREPARATION_REPORT
$oldCompilerActorReport = $env:WINBRIDGE_COMPILER_ACTOR_REPORT
$oldUpdateTestPwsh = $env:WINBRIDGE_UPDATE_TEST_PWSH
$oldSupervisorSourceReport = $env:WINBRIDGE_SUPERVISOR_SOURCE_REPORT
$oldSupervisorActualSource = $env:WINBRIDGE_SUPERVISOR_ACTUAL_SOURCE
$oldProcessTrace = $env:WINBRIDGE_PROCESS_TRACE
$oldProfileReport = $env:WINBRIDGE_PROFILE_GATE_REPORT
$oldResumeShared = $env:WINBRIDGE_RESUME_GATE_SHARED
$oldResumeDelivery = $env:WINBRIDGE_RESUME_GATE_DELIVERY
$oldResumePublication = $env:WINBRIDGE_RESUME_GATE_PUBLICATION
$oldLocation = Get-Location
$cacheLease = $null
try {
    . (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
    foreach ($file in $manifest.files) {
        $filePath = Join-Path $source $file.path
        $item = Get-Item -LiteralPath $filePath
        if ($item.Length -ne $file.bytes -or (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) {
            throw "Source manifest mismatch: $($file.path)"
        }
    }
    $env:CARGO_TARGET_DIR = Join-Path (Split-Path $source -Parent) 'test-build-cache'
    $previousCache = ''
    if ($UpdatePreparationCacheRunId -or $UpdatePreparationCacheSha256) {
        if (-not $UpdatePreparationOnly -or $BuildCacheRunId -or $BuildCacheReportSha256 -or $UpdatePreparationCacheRunId -notmatch '^\d{8}-\d{6}-[0-9a-f]{8}$' -or $UpdatePreparationCacheSha256 -notmatch '^[0-9a-f]{64}$') { throw 'Invalid update preparation cache receipt' }
        $updatePriorRun = Join-Path (Split-Path (Split-Path $source -Parent) -Parent) $UpdatePreparationCacheRunId
        $priorReportPath = Join-Path (Join-Path (Split-Path $ReportDirectory -Parent) $UpdatePreparationCacheRunId) 'report.json'
        if ((Get-FileHash -LiteralPath $priorReportPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $UpdatePreparationCacheSha256) { throw 'Update preparation cache receipt changed' }
        $prior = Get-Content -LiteralPath $priorReportPath -Raw | ConvertFrom-Json
        $expected = @('update-source-policy-and-fsm','update-native-preflight','update-legacy-contracts','update-verifier-build')
        if ($prior.commands.Count -in @(7,9,10,11,12,13,14)) { $expected += @('update-worker-durable-coordination','update-worker-native-coordination','update-previous-worker-refusal') }
        if ($prior.commands.Count -in @(9,10,11,12,13,14)) { $expected += @('update-platform-no-clobber','update-previous-updater-refusal') }
        if ($prior.commands.Count -in @(10,11,12,13,14)) { $expected += @('update-infrastructure-domain') }
        if ($prior.commands.Count -in @(11,12,13,14)) { $expected += @('update-supervisor-staged-source') }
        if ($prior.commands.Count -in @(12,13,14)) { $expected += @('update-supervisor-ipc') }
        if ($prior.commands.Count -in @(13,14)) { $expected += @('update-worker-control') }
        if ($prior.commands.Count -eq 14) { $expected += @('update-platform-atomic-executable') }
        if ($prior.sourceDirty -or $prior.platform -ne 'windows' -or $prior.commands.Count -ne $expected.Count -or @($prior.commands | Where-Object { $_.exitCode -ne 0 }).Count -or (($prior.commands.name -join ',') -cne ($expected -join ','))) { throw 'Previous update preparation did not complete native groups' }
        $previousCache = Join-Path $updatePriorRun 'test-build-cache'
        if (-not [IO.Path]::GetFullPath($previousCache).Equals([IO.Path]::GetFullPath($prior.buildCacheDirectory),[StringComparison]::OrdinalIgnoreCase)) { throw 'Choose original update compiler cache owner run' }
        $report.buildCacheRequested = [ordered]@{runId=$UpdatePreparationCacheRunId;sourceCommit=$prior.sourceCommit;reportSha256=$UpdatePreparationCacheSha256}
        $report.updatePreparationReuse = @{runId=$UpdatePreparationCacheRunId;reportSha256=$UpdatePreparationCacheSha256}
    }
    if ($BuildCacheRunId -or $BuildCacheReportSha256) {
        if ((-not $TaskCasOnly -and -not $ReadinessOnly) -or $TaskCasRuntimeDiagnostic -or $BuildCacheRunId -notmatch '^\d{8}-\d{6}-[0-9a-f]{8}$' -or $BuildCacheReportSha256 -notmatch '^[0-9a-f]{64}$') {
            throw 'Build-cache reuse requires full CAS/readiness and a pinned prior full-CAS receipt'
        }
        $priorRun = Join-Path (Split-Path (Split-Path $source -Parent) -Parent) $BuildCacheRunId
        $priorReport = Join-Path (Join-Path (Split-Path $ReportDirectory -Parent) $BuildCacheRunId) 'report.json'
        if ((Get-FileHash -LiteralPath $priorReport -Algorithm SHA256).Hash.ToLowerInvariant() -ne $BuildCacheReportSha256) { throw 'Build-cache receipt changed' }
        $previous = Get-Content -LiteralPath $priorReport -Raw | ConvertFrom-Json
        if ($previous.sourceDirty -or $previous.platform -ne 'windows' -or $previous.diagnosticOnly -or $previous.sourceCommit -notmatch '^[0-9a-f]{40}$') { throw 'Prior build-cache receipt identity mismatch' }
        $previousCache = Join-Path $priorRun 'test-build-cache'
        $report.buildCacheRequested = [ordered]@{ runId = $BuildCacheRunId; sourceCommit = $previous.sourceCommit; reportSha256 = $BuildCacheReportSha256 }
    }
    $selection = Get-WorkflowBuildCacheSelection -DefaultPath $env:CARGO_TARGET_DIR -PreviousPath $previousCache
    $env:CARGO_TARGET_DIR = $selection.directory
    if ($selection.reused) { $report.buildCacheReusedFrom = $report.buildCacheRequested }
    if ($selection.skipped) {
        $report.buildCacheReuseSkipped = $selection.skipped
        Write-Output 'Prior compiler cache is absent; using fresh cache in this new run. Existing runs are retained.'
    }
    # All validations take the same lease, including first-time builds. Reusing
    # compiler artifacts never reuses source, temporary test state, or proof.
    $cachePath = [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    Assert-WorkflowCachePath $cachePath
    $cacheDrive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($cachePath))
    if ($cacheDrive.DriveType -ne [IO.DriveType]::Fixed) { throw 'Build cache must use a local fixed disk' }
    New-Item -ItemType Directory -Path $cachePath -Force | Out-Null
    $leasePath = Join-Path $cachePath '.winbridge-build-cache.lock'
    if ((Test-Path -LiteralPath $leasePath) -and ((Get-Item -LiteralPath $leasePath).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Refusing linked build-cache lease' }
    $cacheLease = [IO.File]::Open($leasePath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    $report.buildCacheDirectory = $cachePath
    $env:CARGO_INCREMENTAL = '0'
    $env:TEMP = Join-Path (Split-Path $source -Parent) 'test-temp'
    $env:TMP = $env:TEMP
    New-Item -ItemType Directory -Path $env:TEMP -Force | Out-Null
    Set-Location -LiteralPath $source
    $cargo = (Get-Command cargo.exe -ErrorAction Stop).Source
    if ($TaskCasRuntimeDiagnostic -and -not $TaskCasOnly) { throw 'TaskCasRuntimeDiagnostic requires TaskCasOnly' }
    if (($UpdatePreparationCacheRunId -or $UpdatePreparationCacheSha256) -and -not $UpdatePreparationOnly) { throw 'Update cache reuse requires UpdatePreparationOnly' }
    if ($UpdateReferenceWorker -and -not $UpdatePreparationOnly) { throw 'UpdateReferenceWorker requires UpdatePreparationOnly' }
    $groups = @(
        @{ name = 'core-and-log-reader'; arguments = @('test', '-p', 'winbridge-core', '-p', 'winbridge-log-reader', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
        @{ name = 'readiness-ipc'; arguments = @('test', '-p', 'winbridge-transport', '--lib', 'readiness', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
        @{ name = 'worker-runtime-snapshots-workflows'; arguments = @('test', '-p', 'winbridge-worker', '--features', 'bundled-sqlite', '--test', 'runtime', '--test', 'snapshots', '--test', 'workflows', '--test', 'attention', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
        @{ name = 'desktop-native-check'; arguments = @('check', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--locked', '--offline') }
    )
    if (@($AttentionOnly,$CompanionOnly,$WorkflowAttentionOnly,$HistoryDiagnosticsOnly,$SettingsRetentionOnly,$DeltaOnly,$OutputOnly,$ResumeOnly,$ProfilesOnly,$ProjectsOnly,$IdentityOnly,$TaskFsmOnly,$TaskCasOnly,$ReadinessOnly,$UpdatePreparationOnly).Where({ $_ }).Count -gt 1) { throw 'Choose exactly one isolated scenario' }
    if ($TaskCasOnly -or $ReadinessOnly -or $UpdatePreparationOnly) {
        $drive = [System.IO.DriveInfo]::new([System.IO.Path]::GetPathRoot($source))
        $report.storage = [ordered]@{ volume = $drive.Name; availableBytesBefore = $drive.AvailableFreeSpace }
        if ($drive.AvailableFreeSpace -lt 8GB) { throw 'Less than 8 GiB free for an isolated build; no existing run is deleted' }
    }
    if ($TaskCasOnly) {
        if ($NetwatchProfile) { throw 'TaskCasOnly requires an isolated scenario' }
        if ($TaskCasRuntimeDiagnostic) {
            $env:WINBRIDGE_PROCESS_TRACE = '1'
            $groups = @(
                @{ name='process-resume-guard'; arguments=@('test','-p','winbridge-platform','--lib','windows::tests::','--locked','--offline','--','--test-threads=1','--nocapture') },
                @{ name='process-runtime-recovery-diagnostic'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','runtime','--test','reconciliation','--test','recovery_mode','--locked','--offline','--','--test-threads=1','--nocapture') }
            )
        } else {
            if (-not $TaskCasReferenceWorker) { throw 'TaskCasOnly requires an explicit schema-6 reference worker' }
            if (-not (Test-Path -LiteralPath $TaskCasReferenceWorker -PathType Leaf)) { throw 'TaskCasReferenceWorker exe not found' }
            $env:WINBRIDGE_TASK_CAS_REFERENCE_WORKER = (Resolve-Path -LiteralPath $TaskCasReferenceWorker).Path
            $env:WINBRIDGE_TASK_CAS_REPORT = $ReportDirectory
            $groups = @(
            @{ name='task-cas-pure-fsm'; arguments=@('test','-p','winbridge-core','--test','task_fsm','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='task-cas-process-resume-guard'; arguments=@('test','-p','winbridge-platform','--lib','windows::tests::','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='task-cas-fenced-contract'; arguments=@('test','-p','winbridge-protocol','--test','request_context','--test','discovery','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='task-cas-frozen-contracts'; arguments=@('test','-p','winbridge-protocol','-p','winbridge-worker','--features','winbridge-worker/bundled-sqlite','--test','legacy_baseline','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='task-cas-native-writers'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','task_cas','--test','domain_cas','--test','ownership','--test','leases','--test','reconciliation','--test','recovery_mode','--test','request_context','--test','recovery_pair','--test','discovery','--test','task_fsm','--test','runtime','--test','attention','--test','maintenance','--test','output','--test','history','--test','snapshots','--test','workflows','--test','resume','--test','profiles','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='task-cas-older-worker-refusal'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','task_cas','--test','discovery','--locked','--offline','--','--ignored','--test-threads=1','--nocapture') }
            @{ name='task-cas-desktop-contract-check'; arguments=@('check','--manifest-path','apps/desktop/src-tauri/Cargo.toml','--locked','--offline') }
            )
        }
    }
    if ($ReadinessOnly) {
        if ($NetwatchProfile) { throw 'ReadinessOnly requires an isolated scenario' }
        $env:WINBRIDGE_TASK_CAS_REPORT = $ReportDirectory
        $env:WINBRIDGE_WORK_SCHEDULING_REPORT = $ReportDirectory
        $env:WINBRIDGE_QUEUE_CONTROL_REPORT = $ReportDirectory
        if (-not $QueueReferenceWorker -or -not (Test-Path -LiteralPath $QueueReferenceWorker -PathType Leaf)) { throw 'Queue gate requires an explicit previous installed worker executable' }
        $referenceCopy = Join-Path (Split-Path -Parent $source) 'previous-schema12-worker.exe'
        $referenceSha = (Get-FileHash -LiteralPath $QueueReferenceWorker -Algorithm SHA256).Hash.ToLowerInvariant()
        [IO.File]::Copy((Resolve-Path -LiteralPath $QueueReferenceWorker).Path, $referenceCopy, $false)
        if ((Get-FileHash -LiteralPath $referenceCopy -Algorithm SHA256).Hash.ToLowerInvariant() -cne $referenceSha -or (Get-FileHash -LiteralPath $QueueReferenceWorker -Algorithm SHA256).Hash.ToLowerInvariant() -cne $referenceSha) { throw 'Previous worker changed during cache capture' }
        $report.queueReferenceWorker = @{path=$referenceCopy;sha256=$referenceSha}
        $env:WINBRIDGE_QUEUE_LEGACY_WORKER = $referenceCopy
        $env:WINBRIDGE_RESOURCE_SCHEDULING_REPORT = $ReportDirectory
        if (-not $ResourceReferenceWorker -or -not (Test-Path -LiteralPath $ResourceReferenceWorker -PathType Leaf)) { throw 'Resource gate requires an explicit previous schema-13 installed executable' }
        $resourceCopy = Join-Path (Split-Path -Parent $source) 'previous-schema13-worker.exe'
        $resourceSha = (Get-FileHash -LiteralPath $ResourceReferenceWorker -Algorithm SHA256).Hash.ToLowerInvariant()
        [IO.File]::Copy((Resolve-Path -LiteralPath $ResourceReferenceWorker).Path, $resourceCopy, $false)
        if ((Get-FileHash -LiteralPath $resourceCopy -Algorithm SHA256).Hash.ToLowerInvariant() -cne $resourceSha -or (Get-FileHash -LiteralPath $ResourceReferenceWorker -Algorithm SHA256).Hash.ToLowerInvariant() -cne $resourceSha) { throw 'Resource reference changed during cache capture' }
        $report.resourceReferenceWorker = @{path=$resourceCopy;sha256=$resourceSha}
        $env:WINBRIDGE_RESOURCE_LEGACY_WORKER = $resourceCopy
        $groups = @(
            @{ name='readiness-pure-policy'; arguments=@('test','-p','winbridge-core','--test','readiness','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='readiness-wire-compatibility'; arguments=@('test','-p','winbridge-protocol','--features','jsonschema','--test','discovery','--test','projection','--test','legacy_baseline','--test','request_context','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='readiness-cache-freshness'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','projection::tests','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='readiness-native-info-and-scope'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','discovery','--test','request_context','--test','maintenance','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='operations-query-contract'; arguments=@('test','-p','winbridge-core','--test','operation_query','-p','winbridge-protocol','--test','operations','--features','jsonschema','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='operations-transaction-snapshot'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','store::operation_pages::tests','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='operations-native-ipc'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','operations','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='work-scheduling-policy-contract'; arguments=@('test','-p','winbridge-core','--test','work_scheduling','-p','winbridge-protocol','--test','work_scheduling','--features','jsonschema','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='work-scheduling-durable-capacity'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','store::work_scheduling::tests','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='work-scheduling-native-pool'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','work_scheduling','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='work-scheduling-workflow-handoff'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','workflows','work_scheduling_','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='queue-control-contract'; arguments=@('test','-p','winbridge-core','--test','queue_control','-p','winbridge-protocol','--test','queue_control','--features','jsonschema','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='queue-control-durable-admission'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','store::queue_control::tests','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='queue-control-native-runtime'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','queue_control','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='queue-control-previous-worker-refusal'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','queue_control','previous_worker_refuses_schema_thirteen','--locked','--offline','--','--ignored','--test-threads=1','--nocapture') },
            @{ name='resource-scheduling-contract'; arguments=@('test','-p','winbridge-core','--test','resource_scheduling','-p','winbridge-protocol','--test','resource_scheduling','--features','jsonschema','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='resource-scheduling-atomic-sets'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','store::resource_scheduling::tests','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='resource-scheduling-native-runtime'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','resource_scheduling','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='resource-scheduling-previous-worker-refusal'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','resource_scheduling','previous_worker_refuses_schema_fourteen','--locked','--offline','--','--ignored','--test-threads=1','--nocapture') },
            @{ name='readiness-desktop-contract-check'; arguments=@('check','--manifest-path','apps/desktop/src-tauri/Cargo.toml','--locked','--offline') }
        )
    }
    if ($TaskFsmOnly) {
        if ($NetwatchProfile) { throw 'TaskFsmOnly requires an isolated scenario' }
        $groups = @(
            @{ name='task-fsm-pure-transitions'; arguments=@('test','-p','winbridge-core','--test','task_fsm','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='task-fsm-frozen-wire-and-journal'; arguments=@('test','-p','winbridge-protocol','-p','winbridge-worker','--features','winbridge-worker/bundled-sqlite','--test','legacy_baseline','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='task-fsm-native-faults-and-lifecycle'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','task_fsm','--test','runtime','--test','attention','--test','workflows','--locked','--offline','--','--test-threads=1','--nocapture') }
        )
    }
    if ($IdentityOnly) {
        if ($NetwatchProfile) { throw 'IdentityOnly requires an isolated scenario' }
        $groups = @(
            @{ name='source-build-identity-and-protocol'; arguments=@('test','-p','winbridge-protocol','--locked','--offline','--','--test-threads=1','--nocapture') }
        )
    }
    if ($ProjectsOnly) {
        if ($NetwatchProfile -or -not $ProjectsPreparation -or -not $ProjectsPreparationSha256) { throw 'ProjectsOnly needs a pinned preparation file and isolated scenario' }
        if ((Get-FileHash -LiteralPath $ProjectsPreparation -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ProjectsPreparationSha256) { throw 'Project preparation SHA-256 mismatch' }
        $groups = @(
            @{ name='projects-config-runner-identity-policy'; arguments=@('test','-p','winbridge-core','-p','winbridge-runner','-p','winbridge-protocol','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='projects-profiles-workflow-regression'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','profiles','--test','workflows','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='projects-coordinator-build'; arguments=@('build','-p','winbridge-worker','--features','bundled-sqlite','--bins','--locked','--offline') }
        )
    }
    if ($ProfilesOnly) {
        if ($NetwatchProfile) { throw 'ProfilesOnly requires an isolated scenario' }
        $env:WINBRIDGE_PROFILE_GATE_REPORT = $ReportDirectory
        $groups = @(
            @{ name='profile-config-policy'; arguments=@('test','-p','winbridge-core','-p','winbridge-runner','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='profile-native-ipc-projects-dedup-cancel-recovery'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','profiles','--locked','--offline','--','--test-threads=1','--nocapture') },
            @{ name='profile-runner-snapshot-workflow-regression'; arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','runtime','--test','snapshots','--test','workflows','--test','resume','--test','profiles','--locked','--offline','--','--test-threads=1','--nocapture') }
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
            @{ name = 'companion-policy-and-persistence'; arguments = @('test', '-p', 'winbridge-core', '-p', 'winbridge-platform', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
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
    if ($UpdatePreparationOnly) {
        if ($manifest.dirty -or -not $manifest.includesBuiltUi) { throw 'Update preparation gate requires a clean source archive with built UI' }
        . (Join-Path $source 'scripts/session-runtime.ps1')
        if (-not $UpdateReferenceWorker) { throw 'Update coordination gate requires an explicit schema-14 reference exe' }
        Assert-SessionDirectory ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($UpdateReferenceWorker)))
        Assert-SessionFile $UpdateReferenceWorker
        $referenceCopy = Join-Path (Split-Path $source -Parent) 'update-reference-worker.exe'
        $referenceSha = (Get-FileHash -LiteralPath $UpdateReferenceWorker -Algorithm SHA256).Hash.ToLowerInvariant()
        [IO.File]::Copy((Resolve-Path -LiteralPath $UpdateReferenceWorker).Path,$referenceCopy,$false)
        if ((Get-FileHash -LiteralPath $referenceCopy -Algorithm SHA256).Hash.ToLowerInvariant() -cne $referenceSha -or (Get-FileHash -LiteralPath $UpdateReferenceWorker -Algorithm SHA256).Hash.ToLowerInvariant() -cne $referenceSha) { throw 'Update reference changed during capture' }
        $report.updateReferenceWorker = @{path=$referenceCopy;sha256=$referenceSha}
        $env:WINBRIDGE_UPDATE_REFERENCE_WORKER = $referenceCopy
        if (-not $UpdateReferenceUpdater -or $UpdateReferenceUpdaterSha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Explicit pinned previous updater reference required' }
        Assert-SessionFile $UpdateReferenceUpdater
        if ((Get-FileHash -LiteralPath $UpdateReferenceUpdater -Algorithm SHA256).Hash.ToLowerInvariant() -cne $UpdateReferenceUpdaterSha256) { throw 'Previous updater identity mismatch' }
        $updaterCopy = Join-Path (Split-Path $source -Parent) 'update-reference-updater.exe'
        [IO.File]::Copy((Resolve-Path -LiteralPath $UpdateReferenceUpdater).Path,$updaterCopy,$false)
        if ((Get-FileHash -LiteralPath $updaterCopy -Algorithm SHA256).Hash.ToLowerInvariant() -cne $UpdateReferenceUpdaterSha256 -or (Get-FileHash -LiteralPath $UpdateReferenceUpdater -Algorithm SHA256).Hash.ToLowerInvariant() -cne $UpdateReferenceUpdaterSha256) { throw 'Previous updater changed during capture' }
        $report.updateReferenceUpdater = @{path=$updaterCopy;sha256=$UpdateReferenceUpdaterSha256}
        $env:WINBRIDGE_UPDATE_REFERENCE_UPDATER = $updaterCopy
        $env:WINBRIDGE_SOURCE_TRANSACTION_REPORT = Join-Path $ReportDirectory 'source-transaction-proof.json'
        $env:WINBRIDGE_SUPERVISOR_REPORT = Join-Path $ReportDirectory 'supervisor-proof.json'
        $env:WINBRIDGE_UPDATE_CONTROL_REPORT = Join-Path $ReportDirectory 'update-control-proof.json'
        $env:WINBRIDGE_UPDATE_BUILD_REPORT = Join-Path $ReportDirectory 'update-build-proof.json'
        $env:WINBRIDGE_ATOMIC_GATE_REPORT = Join-Path $ReportDirectory 'atomic-launch-proof.json'
        $env:WINBRIDGE_SOURCE_PREPARATION_REPORT = Join-Path $ReportDirectory 'source-preparation-proof.json'
        $env:WINBRIDGE_COMPILER_ACTOR_REPORT = Join-Path $ReportDirectory 'compiler-actor-proof.json'
        $env:WINBRIDGE_UPDATE_TEST_PWSH = (Get-Process -Id $PID).Path
        $env:WINBRIDGE_SUPERVISOR_ACTUAL_SOURCE = $source
        $env:WINBRIDGE_SUPERVISOR_SOURCE_REPORT = Join-Path $ReportDirectory 'supervisor-source-proof.json'
        $env:WINBRIDGE_UPDATE_COORDINATION_REPORT = Join-Path $ReportDirectory 'update-coordination-proof.json'
        $groups = @(
            @{name='update-source-policy-and-fsm';arguments=@('test','-p','winbridge-core','--test','update','--locked','--offline','--','--test-threads=1','--nocapture')},
            @{name='update-native-preflight';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--locked','--offline','--','--test-threads=1','--nocapture')},
            @{name='update-legacy-contracts';arguments=@('test','-p','winbridge-protocol','--test','legacy_baseline','--locked','--offline','--','--test-threads=1','--nocapture')},
            @{name='update-verifier-build';arguments=@('build','-p','winbridge-update','--features','bundled-sqlite','--locked','--offline')},
            @{name='update-worker-durable-coordination';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','store::update::tests','--locked','--offline','--','--test-threads=1','--nocapture')},
            @{name='update-worker-native-coordination';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','update_coordination','--locked','--offline','--','--test-threads=1','--nocapture')},
            @{name='update-previous-worker-refusal';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','update_coordination','previous_worker_refuses_schema_fifteen','--locked','--offline','--','--ignored','--test-threads=1','--nocapture')},
            @{name='update-platform-no-clobber';arguments=@('test','-p','winbridge-platform','--lib','source_files','--locked','--offline','--','--test-threads=1','--nocapture')},
            @{name='update-previous-updater-refusal';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--test','source_transaction','previous_updater_refuses_current_schema_without_writes','--locked','--offline','--','--ignored','--test-threads=1','--nocapture')},
            @{name='update-infrastructure-domain';arguments=@('test','-p','winbridge-platform','--lib','infrastructure','--locked','--offline','--','--test-threads=1','--nocapture')},
            @{name='update-supervisor-staged-source';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','supervisor::tests::actual_staged_source_reaches_ready_without_mutation','--locked','--offline','--','--exact','--ignored','--test-threads=1','--nocapture')},
            @{name='update-supervisor-ipc';arguments=@('test','-p','winbridge-transport','--lib','supervisor::tests::busy_pipe_waits_without_sending','--locked','--offline','--','--exact','--test-threads=1','--nocapture')},
            @{name='update-worker-control';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','update_control::tests','--locked','--offline','--','--test-threads=1','--nocapture')},
            @{name='update-platform-atomic-executable';arguments=@('test','-p','winbridge-platform','--lib','windows_atomic::tests','--locked','--offline','--','--test-threads=1','--nocapture')},
            @{name='update-compiler-actor';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','update_control::tests::compiler_actor_commits_before_dispatch_stays_responsive_and_aborts_cleanly','--locked','--offline','--','--exact','--ignored','--test-threads=1','--nocapture')}
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
    if ($UpdatePreparationOnly) {
        $atomic = Get-Content -LiteralPath (Join-Path $ReportDirectory 'atomic-launch-proof.json') -Raw | ConvertFrom-Json
        if ($atomic.kind -ne 'atomic_executable_proof' -or $atomic.platform -ne 'windows' -or $atomic.runtimeApplied -or $atomic.windowsOpened -or $atomic.build.sourceDirty -or $atomic.build.sourceCommit -cne $manifest.sourceCommit) { throw 'Atomic executable proof identity incomplete' }
        foreach ($case in @('argvAndSuspended','noResume','domainAndPrivateJob','rejectedBeforeCreate','suspendCount','ownerCrashBeforePidReceipt','stdioOnly')) {
            if ($atomic.$case.passed -ne $true) { throw "Atomic executable case missing: $case" }
        }
        if ($atomic.ownerCrashBeforePidReceipt.jobEmpty -ne $true -or $atomic.ownerCrashBeforePidReceipt.rootExited -ne $true -or $atomic.ownerCrashBeforePidReceipt.userCodeNotRun -ne $true -or $atomic.stdioOnly.unlistedHandleInherited -ne $false -or $atomic.stdioOnly.stdinEof -ne $true) { throw 'Atomic cleanup/handle proof incomplete' }
        $preparation = Get-Content -LiteralPath (Join-Path $ReportDirectory 'source-preparation-proof.json') -Raw | ConvertFrom-Json
        if ($preparation.kind -ne 'supervisor_source_preparation_proof' -or $preparation.passed -ne $true -or $preparation.platform -ne 'windows' -or $preparation.build.sourceDirty -or $preparation.build.sourceCommit -cne $manifest.sourceCommit -or $preparation.runtimeApplied -or $preparation.windowsOpened) { throw 'Source preparation proof identity incomplete' }
        foreach ($check in @('mailboxResponsive','stopWhileBusyRejected','duplicateObserved','fixtureSourceApplied','workerLeaseRetained','coordinatedAbortCompleted')) {
            if ($preparation.$check -ne $true) { throw "Source preparation check missing: $check" }
        }
        if ($preparation.accepted.update.phase -ne 'preparing' -or $preparation.accepted.driverActive -ne $true -or $preparation.busy.driverActive -ne $true -or $preparation.final.driverActive -ne $false -or $null -ne $preparation.final.driverError -or $preparation.final.update.phase -ne 'building' -or $preparation.final.source.phase -ne 'verified' -or $preparation.final.sourceAppliedFiles -ne 2 -or $null -ne $preparation.final.build) { throw 'Source preparation lifecycle proof incomplete' }
        $actor = Get-Content -LiteralPath (Join-Path $ReportDirectory 'compiler-actor-proof.json') -Raw | ConvertFrom-Json
        if ($actor.kind -ne 'supervisor_compiler_actor_proof' -or $actor.passed -ne $true -or $actor.platform -ne 'windows' -or $actor.build.sourceDirty -or $actor.build.sourceCommit -cne $manifest.sourceCommit -or $actor.runtimeApplied -or $actor.candidateVerified -or $actor.windowsOpened) { throw 'Compiler actor proof identity incomplete' }
        foreach ($check in @('singleCompiler','mailboxResponsive','rawReleaseRejected','busyAbortRejected','coordinatedAbortCompleted')) { if ($actor.$check -ne $true) { throw "Compiler actor check missing: $check" } }
        if ($actor.accepted.build.phase -ne 'intent' -or $actor.busy.build.phase -ne 'running' -or $actor.final.build.phase -ne 'completed' -or $actor.final.build.outcome -ne 'succeeded' -or $actor.final.build.cleanupVerified -ne $true -or $actor.final.driverActive -ne $false -or $null -ne $actor.final.driverError -or $actor.abort.phase -ne 'completed' -or $actor.abort.workerReleased.phase -ne 'released') { throw 'Compiler actor lifecycle/abort proof incomplete' }
        $compiler = Get-Content -LiteralPath (Join-Path $ReportDirectory 'update-build-proof.json') -Raw | ConvertFrom-Json
        if (-not $compiler.passed -or $compiler.platform -ne 'windows' -or -not $compiler.compilationFixture -or -not $compiler.duplicateObserved -or -not $compiler.workerLeaseRetained -or $compiler.runtimeApplied -or $compiler.candidateVerified -or $compiler.windowsOpened -or $compiler.record.phase -ne 'completed' -or $compiler.record.outcome -ne 'succeeded' -or -not $compiler.record.cleanupVerified -or $compiler.record.exitCode -ne 0 -or $compiler.build.sourceDirty -or $compiler.build.sourceCommit -cne $manifest.sourceCommit) { throw 'Managed compiler fixture proof incomplete' }
        $updateControl = Get-Content -LiteralPath (Join-Path $ReportDirectory 'update-control-proof.json') -Raw | ConvertFrom-Json
        if (-not $updateControl.passed -or $updateControl.platform -ne 'windows' -or -not $updateControl.forgedPeerRejected -or -not $updateControl.duplicateObserved -or -not $updateControl.prematureDrainRejected -or -not $updateControl.sourceVerified -or $updateControl.runtimeApplied -or $updateControl.sourceEffectsEnabled -or $updateControl.windowsOpened -or $updateControl.stages.Count -ne 3 -or $updateControl.stages[-1].phase -ne 'released' -or $updateControl.build.sourceDirty -or $updateControl.build.sourceCommit -cne $manifest.sourceCommit) { throw 'Worker-owned launch/control proof incomplete' }
        $supervisor = Get-Content -LiteralPath (Join-Path $ReportDirectory 'supervisor-proof.json') -Raw | ConvertFrom-Json
        if (-not $supervisor.passed -or $supervisor.platform -ne 'windows' -or -not $supervisor.parentExited -or -not $supervisor.survivedParent -or -not $supervisor.exclusiveOwner -or $supervisor.runtimeApplied -or $supervisor.sourceEffectsEnabled -or $supervisor.windowsOpened -or $supervisor.active.phase -ne 'active' -or $supervisor.stopped.phase -ne 'stopped' -or -not $supervisor.active.spec.excludedDomain -or $supervisor.build.sourceDirty -or $supervisor.build.sourceCommit -cne $manifest.sourceCommit) { throw 'Standalone supervisor proof incomplete' }
        $sourceSmoke = Get-Content -LiteralPath (Join-Path $ReportDirectory 'supervisor-source-proof.json') -Raw | ConvertFrom-Json
        if (-not $sourceSmoke.passed -or $sourceSmoke.platform -ne 'windows' -or $sourceSmoke.runtimeApplied -or $sourceSmoke.sourceEffectsEnabled -or $sourceSmoke.sourceBefore.manifestSha256 -cne $sourceSmoke.sourceAfter.manifestSha256 -or $sourceSmoke.ready.phase -ne 'ready' -or $sourceSmoke.active.phase -ne 'active' -or $sourceSmoke.stopped.phase -ne 'stopped' -or $sourceSmoke.build.sourceCommit -cne $manifest.sourceCommit -or $sourceSmoke.build.sourceDirty) { throw 'Actual staged source supervisor smoke incomplete' }
        $sourceProof = Get-Content -LiteralPath (Join-Path $ReportDirectory 'source-transaction-proof.json') -Raw | ConvertFrom-Json
        if (-not $sourceProof.passed -or $sourceProof.platform -ne 'windows' -or -not $sourceProof.sourceVerified -or -not $sourceProof.workerLeaseRetained -or $sourceProof.runtimeApplied -or $sourceProof.windowsOpened -or $sourceProof.status.appliedFiles -ne 4 -or $sourceProof.status.transaction.phase -ne 'verified' -or -not $sourceProof.inspection.sourceMatchesTarget -or $sourceProof.inspection.canReplay -or $sourceProof.build.sourceDirty -or $sourceProof.build.sourceCommit -cne $manifest.sourceCommit) { throw 'Source transaction proof incomplete' }
        $coordination = Get-Content -LiteralPath (Join-Path $ReportDirectory 'update-coordination-proof.json') -Raw | ConvertFrom-Json
        if (-not $coordination.passed -or $coordination.platform -ne 'windows' -or $coordination.runtimeApplied -or $coordination.windowsOpened -or $coordination.observations.Count -ne 6) { throw 'Native coordination proof incomplete' }
        & (Join-Path $source 'scripts/test-update-stage.ps1') -ScratchRoot (Join-Path (Split-Path $source -Parent) 'update-stage-test')
        if (-not $?) { throw 'Update staging test failed' }
        $candidateDirectory = Join-Path (Split-Path $source -Parent) 'update-candidate'
        $sourceManifestSha = (Get-FileHash -LiteralPath (Join-Path $source 'winbridge-source-manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant()
        if (-not $UpdatePreparationArchivePath) { throw 'Update preparation requires the pinned source ZIP path for actual archive verification' }
        $archiveProof = & (Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-update.exe') verify-archive $UpdatePreparationArchivePath $SourceArchiveSha256 $sourceManifestSha
        if ($LASTEXITCODE -ne 0) { throw 'Actual source ZIP verification failed' }
        $archiveProof = ($archiveProof -join "`n") | ConvertFrom-Json
        if ($archiveProof.kind -ne 'source_archive_verified' -or -not $archiveProof.readOnly -or $archiveProof.archiveSha256 -cne $SourceArchiveSha256 -or $archiveProof.source.manifestSha256 -cne $sourceManifestSha -or $archiveProof.source.sourceCommit -cne $manifest.sourceCommit) { throw 'Actual ZIP/source identity proof incomplete' }
        $archiveProof | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'update-archive-proof.json') -Encoding utf8
        $cacheArguments = @{}
        if ($UpdatePreparationCacheRunId -or $UpdatePreparationCacheSha256) {
            $cacheArguments.ValidationCacheDirectory = $updatePriorRun
        }
        & (Join-Path $source 'scripts/build-windows-update.ps1') -SourceDirectory $source -ManifestSha256 $sourceManifestSha -StagingDirectory $candidateDirectory -VerifierExecutable (Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-update.exe') @cacheArguments
        if (-not $?) { throw 'Update candidate build script failed' }
        $candidate = Get-Content -LiteralPath (Join-Path $candidateDirectory 'candidate-report.json') -Raw | ConvertFrom-Json
        if ($candidate.status -ne 'built' -or $candidate.runtimeApplied -or $candidate.candidateVerified -or $candidate.artifacts.Count -ne 4) { throw 'Candidate preparation proof incomplete' }
        [IO.File]::Copy((Join-Path $candidateDirectory 'candidate-report.json'),(Join-Path $ReportDirectory 'update-candidate-proof.json'),$false)
        $report.updatePreparation = @{passed=$true;candidateDirectory=$candidateDirectory;manifestSha256=$sourceManifestSha;runtimeApplied=$false;windowsOpened=$false}
    }
    if ($ProjectsOnly) {
        & (Join-Path $source 'scripts/test-projects-native.ps1') -ReportDirectory $ReportDirectory -PreparationPath $ProjectsPreparation
        $proof = Get-Content -LiteralPath (Join-Path $ReportDirectory 'projects-proof.json') -Raw | ConvertFrom-Json
        if (-not $proof.passed) { throw 'Two real project workflows failed' }
        $report.projects = @{ passed = $proof.passed; projects = $proof.projects.projectId; proof = 'projects-proof.json'; preparationSha256 = $ProjectsPreparationSha256 }
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
    if ($cacheLease) { $cacheLease.Dispose() }
    Set-Location -LiteralPath $oldLocation.Path
    $env:CARGO_TARGET_DIR = $oldTarget
    $env:CARGO_INCREMENTAL = $oldIncremental
    $env:TEMP = $oldTemp
    $env:TMP = $oldTmp
    $env:WINBRIDGE_DELTA_GATE_SHARED = $oldDeltaShared
    $env:WINBRIDGE_OUTPUT_GATE_REPORT = $oldOutputReport
    $env:WINBRIDGE_OUTPUT_REFERENCE_WORKER = $oldOutputReference
    $env:WINBRIDGE_TASK_CAS_REFERENCE_WORKER = $oldTaskCasReference
    $env:WINBRIDGE_TASK_CAS_REPORT = $oldTaskCasReport
    $env:WINBRIDGE_WORK_SCHEDULING_REPORT = $oldWorkSchedulingReport
    $env:WINBRIDGE_QUEUE_CONTROL_REPORT = $oldQueueControlReport
    $env:WINBRIDGE_QUEUE_LEGACY_WORKER = $oldQueueReferenceWorker
    $env:WINBRIDGE_RESOURCE_SCHEDULING_REPORT = $oldResourceReport
    $env:WINBRIDGE_RESOURCE_LEGACY_WORKER = $oldResourceReferenceWorker
    $env:WINBRIDGE_UPDATE_COORDINATION_REPORT = $oldUpdateCoordinationReport
    $env:WINBRIDGE_UPDATE_REFERENCE_WORKER = $oldUpdateReferenceWorker
    $env:WINBRIDGE_UPDATE_REFERENCE_UPDATER = $oldUpdateReferenceUpdater
    $env:WINBRIDGE_SOURCE_TRANSACTION_REPORT = $oldSourceTransactionReport
    $env:WINBRIDGE_SUPERVISOR_REPORT = $oldSupervisorReport
    $env:WINBRIDGE_UPDATE_CONTROL_REPORT = $oldUpdateControlReport
    $env:WINBRIDGE_UPDATE_BUILD_REPORT = $oldUpdateBuildReport
    $env:WINBRIDGE_ATOMIC_GATE_REPORT = $oldAtomicReport
    $env:WINBRIDGE_SOURCE_PREPARATION_REPORT = $oldSourcePreparationReport
    $env:WINBRIDGE_COMPILER_ACTOR_REPORT = $oldCompilerActorReport
    $env:WINBRIDGE_UPDATE_TEST_PWSH = $oldUpdateTestPwsh
    $env:WINBRIDGE_SUPERVISOR_SOURCE_REPORT = $oldSupervisorSourceReport
    $env:WINBRIDGE_SUPERVISOR_ACTUAL_SOURCE = $oldSupervisorActualSource
    $env:WINBRIDGE_PROCESS_TRACE = $oldProcessTrace
    $env:WINBRIDGE_PROFILE_GATE_REPORT = $oldProfileReport
    $env:WINBRIDGE_RESUME_GATE_SHARED = $oldResumeShared
    $env:WINBRIDGE_RESUME_GATE_DELIVERY = $oldResumeDelivery
    $env:WINBRIDGE_RESUME_GATE_PUBLICATION = $oldResumePublication
    if (($TaskCasOnly -or $ReadinessOnly -or $UpdatePreparationOnly) -and $report.storage) { $report.storage.availableBytesAfter = $drive.AvailableFreeSpace }
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
