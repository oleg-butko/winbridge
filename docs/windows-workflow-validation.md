# Автоматические Windows проверки workflow

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
