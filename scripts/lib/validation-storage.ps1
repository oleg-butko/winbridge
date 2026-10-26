# Human-readable storage roles, not a cleanup authorization or task completion marker.
# Requires Assert-WorkflowCachePath from build-cache.ps1.
function Get-WorkflowTemporaryDirectory {
    param([Parameter(Mandatory)][string]$RunDirectory)
    Assert-WorkflowCachePath $RunDirectory
    $legacy=Join-Path $RunDirectory 'test-temp'
    $temporary=Join-Path $RunDirectory 'tmp-test-temp'
    # Old diagnostic runs retain their existing paths and proof identities.
    if (Test-Path -LiteralPath $legacy) {$temporary=$legacy}
    Assert-WorkflowCachePath $temporary
    if ((Test-Path -LiteralPath $temporary) -and -not (Test-Path -LiteralPath $temporary -PathType Container)) {throw 'Temporary path is not a directory'}
    return [IO.Path]::GetFullPath($temporary)
}

function Write-WorkflowStorageGuide {
    param([Parameter(Mandatory)][string]$RunDirectory)
    Assert-WorkflowCachePath $RunDirectory
    $guide=Join-Path $RunDirectory 'README.storage.txt'
    Assert-WorkflowCachePath $guide
    if (Test-Path -LiteralPath $guide) {return} # Preserve existing/manual annotations.
    $text=@'
WinBridge: что хранится в этом каталоге прогона

ЭТО СМЕШАННЫЙ КАТАЛОГ. Его имя/run ID не означает, что всё можно удалить.
Этот README описывает роли, но не подтверждает завершение задачи.

tmp-test-temp/     Временные файлы тестов нового прогона.
test-temp/         То же имя в старых прогонах; пути старых прогонов сохранены.
test-build-cache/  Повторно используемый Cargo cache. Пересоздаваемый, но удаление
                   замедлит следующие сборки. Проверить cache lease и владельца.
winbridge/         Снимок исходников; внутри могут быть большие SDK Cargo caches:
                   target/ и apps/desktop/src-tauri/target/. Это не весь tmp.
shutdown-main-cases/ и другие runtime/state/candidate/backup каталоги:
                   МОГУТ БЫТЬ ЭТАЛОНАМИ следующих updater проверок. Сохранять,
                   пока references из проверенных отчётов не сняты явно.
*.log, *.json      Отчёты/receipts/доказательства; статус failed не означает,
                   что все каталоги прогона теперь бесполезны.

Принцип новых имён: tmp-* = временные рабочие данные, cache = повторно
используемая сборка, retained reference = сохраняемый эталон. Не переименовывать
старые пути: они привязаны к hash/identity в проверенных отчётах и конфигурации.

Удаление tmp-* возможно ПОСЛЕ terminal результата задачи, завершения её test
processes и проверки, что данные не входят в retained references. Во время
работы/при неизвестном исходе не удалять. Сам README не является такой проверкой.
Не удалять весь родительский run только потому, что внутри есть tmp-* или cache.

Инвентаризация без удаления:
  scripts/inspect-windows-validation-storage.ps1 -Root <workflow-validation>
Подробно: docs/validation/validation-storage.md
'@
    $handle=[IO.File]::Open($guide,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    try {$bytes=[Text.UTF8Encoding]::new($false).GetBytes($text);$handle.Write($bytes,0,$bytes.Length)} finally {$handle.Dispose()}
}
