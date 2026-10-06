# Автоматические Windows проверки workflow

## Что запустить после распаковки исходников

Чтобы обновить **рабочий WinBridge и увидеть новый UI**, закройте desktop и остановите
прежний worker через Ctrl+C в его окне. Из корня распакованных исходников, где лежат
`Cargo.toml`, `scripts` и `package.json`, выполните в PowerShell:

```powershell
.\scripts\prepare-windows-snapshot.ps1
```

Например, если исходники находятся в `K:\_proj-26\winbridge\src`, выполнять команду
нужно из этой папки. ZIP содержит верхнюю папку `winbridge`: при извлечении в новую
папку корень исходников может оказаться на один уровень глубже. Ориентир — `Cargo.toml`
рядом с папкой `scripts`.

Сценарий собирает Rust worker/fixture, устанавливает зависимости UI через `npm ci`,
собирает desktop и обновляет exe в постоянном `K:\_proj-26\winbridge\runtime`.
Затем открывает desktop и запускает worker с прежним snapshot config. Существующие
настройки, формы и journal сохраняются; при upgrade SQLite создаётся consistent backup.
Новое окно PowerShell с worker оставить открытым. Debian controller уже запускает агент;
после reconnect UI покажет «Controller Debian подключён» и «Цикл разработки».

`test-windows-workflow.ps1` выполняет **автоматические тесты**, а не обновление рабочего
приложения. Результаты автоматических прогонов записаны в [Windows gate](windows-workflow-gate-2026-10-06.md).
Повторять их для установки не требуется. Если запущенный сценарий ждёт `SourceDirectory:`,
прервать запрос через Ctrl+C и выполнить команду обновления выше.

## Что означает SourceDirectory и остальные параметры теста

| Параметр | Значение и назначение |
| --- | --- |
| `SourceDirectory` | Корень распакованных исходников: папка с `Cargo.toml`, `scripts` и `winbridge-source-manifest.json`. Пример: `K:\_proj-26\winbridge\src`. Из этих файлов собираются тесты; здесь проверяется соответствие source manifest. |
| `ReportDirectory` | Новая папка одного прогона в shared, например `K:\VMs\shared\WinBridge\workflow-validation\manual-20261006-01`. Туда записываются отчёт и test logs; агент может их прочитать из Debian. |
| `SourceArchiveSha256` | SHA-256 исходного ZIP. Связывает результат с конкретной проверенной версией, а не только с произвольной папкой исходников. Не SHA папки или config. |

В обновлённой версии `SourceDirectory` определяется от папки самого сценария, если его
не передать; без report directory/hash сценарий выводит объяснение и завершает работу
без тестов и записи файлов. Справка:

```powershell
.\scripts\test-windows-workflow.ps1 -Help
Get-Help .\scripts\test-windows-workflow.ps1 -Full
```

В уже скопированном архиве `652635b` параметры обязательны; полный ручной вызов выглядит
так (приведён для отдельного повторного теста, для обновления используйте команду выше):

```powershell
$report = 'K:\VMs\shared\WinBridge\workflow-validation\manual-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
.\scripts\test-windows-workflow.ps1 `
  -SourceDirectory (Get-Location).Path `
  -ReportDirectory $report `
  -SourceArchiveSha256 '5a9e4e7e06d483d054d3184f45466917de50c44c161d92e7bbaefefcb10b5eb2'
```

Хэш в примере относится только к `winbridge-20261006T172619Z-652635b-c8d60318.zip`.
Для другого ZIP взять его SHA-256 из соответствующего `.zip.sha256` или `Get-FileHash`.
Ручной тест требует неизменённых source files и `winbridge-source-manifest.json` из ZIP.
Проверка собирает отдельные тестовые exe; рабочий desktop автоматически не запускается.

## Как выполняется автоматическая проверка

Сценарий использует уже подключённый разрешённый worker, собирает новую версию в отдельном локальном подкаталоге зарегистрированного `snapshot-fixture` и запускает настоящие Windows тесты. Агент читает checksum-verified report из shared. Пользовательский fixed runtime/конфигурация/SQLite остаются рабочими; новый тестовый worker использует уникальный SID+instance pipe без WebSocket, поэтому менять сетевые правила для его пути не нужно.

## Запуск из Debian

```bash
python3 scripts/export-source.py --output-directory /mnt/hgfs/shared/WinBridge/sources
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/<archive>.zip
```

Требуется connected Windows worker, allowDirectCommands зарегистрированного проекта, pwsh.exe, Git и MSVC Cargo в PATH worker. Зависимости должны быть доступны локально: проверки выполняются `--locked --offline`, без установки глобальных инструментов. Скрипт не повторяет ручные gates этапов 1–3 и не запускает production Netwatch.

Source export содержит точный manifest всех source files с hash/размером, sourceCommit и честный dirty flag. Тестовый экспорт working tree с dirty=true не выдаётся за чистый Git commit и не является runtime snapshot dirty mode. ZIP проверяется до извлечения; после извлечения каждый файл сверяется с manifest. Тестовые source/build-cache/TEMP/journal находятся на локальном Windows диске, вне HGFS. Bootstrap сохраняет новый frozen request и TaskReport в Debian `.winbridge/workflow-validation/<run>`; lost ACK повторяет прежний request.

Сценарий PowerShell [test-windows-workflow.ps1](../scripts/test-windows-workflow.ps1) запускает две группы:

1. Core/config/desktop persistence и log reader: byte paging/Unicode, append/truncate/rewrite/rotate, guards.
2. Worker integration runtime/snapshots/workflows: actual execution/output/cancel, NTFS guards, imports/migrations, import→build→start→ready→log, dedup/leases, build failure со старым exe, readiness и общий startup timeout, stop до готовности, позднее падение приложения, авария отдельного worker с Job close и indeterminate recovery без повторения.

Тестовая авария касается только созданного harness worker. Отмена внешней задачи завершается её управляемым деревом. Новый desktop автоматически не открывается: native визуальные наблюдения пользователь отложил. Реальная browser UI integration проверяется отдельно `npm run test:ui` с Unix worker/WebSocket и transport shim; она не подменяет Windows Win32 checks.

## Отчёт

`K:\VMs\shared\WinBridge\workflow-validation\<run>` содержит `report.json`, два полных test logs и `report.ready.json` с hashes/размерами. Report включает actual platform=windows, source archive hash/commit/dirty, argv и exit code каждой группы. Ready публикуется после logs/report. Debian harness сверяет каждый artifact, exit code task, обе группы и source SHA-256. Ошибка bootstrap сохраняет task/output в локальном отчёте; старый run не удаляется и не переиспользуется.

При изменении runtime code повторяется автоматический сценарий; изменение только документации не требует нового Windows gate. Пользователю нужен один общий update fixed runtime после готового выпуска, а не цикл ручных проверок после каждого fix. Обновление через уже существующий `prepare-windows-snapshot.ps1`: закрыть desktop, штатно остановить worker, выполнить сценарий из новой source версии. Он сохраняет configs/state и отказывается заменять активные exe. Подробная инструкция этапа 3 остаётся [здесь](windows-snapshot-validation.md); новый worker объявляет workflowAvailable/log_read, UI показывает «Цикл разработки».

Расширенный прогон этапа 4 включает readiness IPC tests и [изолированный Netwatch](netwatch-validation.md). Опция `--netwatch-profile` передаёт подготовленный frozen профиль дополнительному test worker. Не запускайте эти test scripts вручную для обычного обновления приложения.
