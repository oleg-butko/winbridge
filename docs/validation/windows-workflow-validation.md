# Автоматические Windows проверки workflow

Архитектурный A2.1: `--task-cas-only` / `-TaskCasOnly` проверяет
[task revisions/CAS](../plans/architecture-transition/durable-transitions.md),
schema migration, stale writes, atomic output/cleanup и process/workflow regressions.
Debian driver требует `--task-cas-reference-worker` с read-only Windows путём
schema-6 worker.exe: этот exe запускается только с НОВЫМ тестовым config/state,
чтобы подтвердить отказ открыть актуальную schema 12. Установленный journal не открывается,
working runtime не заменяется. Нужен только запущенный worker; desktop необязателен,
тестовых UI-окон нет. Это automatic gate, не команда установки/обновления. A3.1 добавляет catalog
reader checks, actual candidate IPC и isolated старый worker Info; отдельный
`discovery-reference-proof.json` подтверждает missing catalog и cleanup reference.

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/<source-archive>.zip \
  --task-cas-only \
  --task-cas-reference-worker 'K:\_proj-26\winbridge\runtime\winbridge-worker.exe'
```

Для расследования process deadlines есть узкий режим
`--task-cas-only --task-cas-runtime-diagnostic` / `-TaskCasOnly -TaskCasRuntimeDiagnostic`.
Он запускает resume guard, runtime, reconciliation и read-only recovery; reference
worker и UI не запускаются. В report `diagnosticOnly: true`: успех не заменяет
полный CAS gate. `WINBRIDGE_PROCESS_TRACE=1` действует только в дочернем тестовом
сценарии и восстанавливается в finally; в лог попадают timestamps, task/PID/thread
и этапы prepare/bind/resume/exit/cleanup, без arguments или содержимого output.
Свободное место local volume измеряется до/после; при менее 8 GiB тестовая сборка
не начинается. Существующие каталоги и рабочий runtime не очищаются.

Полный CAS gate может использовать `--task-cas-reuse-diagnostic <run-id>` вместе
с тем же `--archive` и `--task-cas-reference-worker`. Driver принимает только
успешный checksum-verified diagnostic этого точного archive; Windows повторно
проверяет manifest/entrypoint/source files и запрещает linked source/cache.
Повторно используется source/build cache; тесты исполняются заново с новыми states
и новым shared report. Trace включён и для full gate. Прошлый report сохраняется.
Это экономит место без удаления старых runs и не разрешает reuse другой версии.

Для новой версии исходников используйте `--task-cas-reuse-build-cache <run-id>`.
Это отдельный режим: распаковываются новые исходники и создаётся новый отчёт,
переиспользуется только `test-build-cache` прежнего full CAS run. Driver проверяет
SHA-256 всех ready artifacts и terminal task в том же проекте. Прежние тесты могут
быть failed, если компиляция завершилась и тесты исполнялись. Их результат не
переносится: все группы выполняются заново. Если выбранный run сам заимствовал
cache, driver проверяет цепочку parent receipts (не более 32, без циклов) и
передаёт исходного owner, а не папку последнего отчёта. Windows проверяет pinned receipt,
source identity, local path/reparse и держит exclusive cache lease; Cargo сам
пересобирает изменённые компоненты. Старые состояния и reports сохраняются.
Старые source files для cache reuse не нужны. Если compiler cache уже удалён,
выбирается новый cache текущего run после проверки free-space threshold; report
хранит `buildCacheRequested` и `buildCacheReuseSkipped: cache_missing`. Driver
проверяет этот новый путь. Links, non-directory и занятая lease не дают fallback.

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/<source-archive>.zip \
  --task-cas-only --task-cas-runtime-diagnostic
```

Архитектурный A1: `--task-fsm-only` / `-TaskFsmOnly` проверяет
[типизированную FSM задач](../plans/architecture-transition/task-fsm.md) и frozen
A0 baseline. В одной партии выполняются launch/PID commit fault tests, cancel/
timeout/process tree, recovery/attention и существующие workflow regressions.
Нужен working Windows worker; desktop необязателен, тестовых UI-окон не будет.
Рабочие binaries/config/state не заменяются. Source archive собирается отдельно,
tests/state/workspaces локальные, verified report/ready — в shared. Это режим
агента для gate, а не команда обновления рабочего runtime.

Этап 7: `--projects-only` / `-ProjectsOnly` выполняет [два реальных проекта](../guides/project-integration.md)
Netwatch/WinBridge через общий build profile и прежний task service. Driver сам
готовит отдельные Git clones, validation commits/diffs и pinned Debian packages,
передаёт preparation file с SHA-256. Проверяются import/build/artifacts/start/
readiness/log, private worker IPC, nested tree stop и journal recovery. Нужен
только рабочий Windows worker; desktop закрыт, окон и network connections тестовых
экземпляров нет. Полные исходники, caches и journals теста находятся на local disk;
reports и подготовленные immutable packages — в новом namespace WinBridge/shared.

Сценарий `--output-only` / `-OutputOnly` проверяет [пакетный output](../done/stage0-7/stage6/output-batching-plan.md)
без desktop. Агент передаёт `--output-reference-worker` с путём installed worker.exe:
этот exe запускается только с новым тестовым config/state для сравнения с новой
сборкой. Три пары flood output проверяют actual bytes/SHA-256, число страниц,
Info latency, cancel/attention и restart чтения прежнего cursor без повторного процесса.
Проверки byte/JSON bounds, tiny rows и quota/drain включены в тот же прогон.
Рабочий worker должен оставаться запущенным; desktop закрыт, новых окон не будет.
Настоящий native UI визуально этим сценарием не проверяется: incremental buffer,
scroll/filter/pause и ограниченный DOM отдельно проверяет browser integration с
реальным Unix worker. Reports/ready сохраняют source identity и raw timings;
сравнение Windows IPC не подменяет end-to-end WebSocket measurement.

Сценарий `--settings-retention-only` (агент Debian) / `-SettingsRetentionOnly`
(PowerShell) проверяет [настройки и очистку output](../guides/settings-retention.md) одной
партией backend tests и двумя последовательными TEST окнами desktop. Первое
сохраняет конфиг при прежнем PID, второе проверяет активную политику после
перезапуска только тестового worker. Рабочий worker должен оставаться запущенным;
рабочий desktop необязателен. Тестовые окна не трогать до сообщения о завершении.

## Что запустить после распаковки исходников

Чтобы обновить **рабочий WinBridge и увидеть новый UI**, завершите desktop через
«Завершить интерфейс» в окне/tray (крестик нового desktop скрывает окно) и остановите
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
приложения. Результаты автоматических прогонов записаны в [Windows gate](../done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md).
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

Для tray-сценария агент использует `--companion-only` и архив с `--include-built-ui`.
`-CompanionOnly` собирает native desktop со встроенным frontend и автоматически
проверяет отдельные Tauri окна; npm на Windows для этого прогона не нужен.
Сценарий открывает только тестовый desktop и не заменяет рабочий runtime.
Подробное назначение действий и границы доказательства: [companion](../guides/companion.md).

Для новых workflow/application уведомлений используется `--workflow-attention-only`
и архив с `--include-built-ui`. PowerShell `-WorkflowAttentionOnly` запускает attention
и отфильтрованные workflow tests, собирает worker/desktop offline. Единственное
тестовое окно проверяет реальную ошибку import, popup без focus, переход к настоящему
workflow и явное UI acknowledgement. Оно показывает TEST banner и текущий шаг,
закрывается само после save/quit. Не трогать это окно до сообщения о завершении;
рабочий desktop необязателен, установленный worker оставить работающим. Старый
tray gate не повторяется. См. [attention](../guides/attention.md).

Для истории и task diagnostic используется `--history-diagnostics-only` и архив
с `--include-built-ui`. PowerShell `-HistoryDiagnosticsOnly` проверяет pagination,
mixed pipeline/journal recovery, regression child result, redaction, локальную
настройку report directory и create-only publication/junction guard. Затем строит
отдельные worker/desktop offline и открывает одно обозначенное TEST окно.
Оно читает старую страницу истории, открывает точную task, подготавливает безопасный
и расширенный preview, заменяет тестовый секрет, редактирует JSON и сохраняет
его в собственный report directory с readback/SHA-256 и same-ID retry.
Закрывается само, после остановки тестовых процессов готовые отчёты публикуются
в shared. Установленный worker нужен, рабочий desktop необязателен; не трогать
тестовое окно до сообщения о завершении. [Контракт](../guides/history-diagnostics.md).

## Запуск из Debian

```bash
python3 scripts/export-source.py --output-directory /mnt/hgfs/shared/WinBridge/sources
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/<archive>.zip
```

Требуется connected Windows worker, allowDirectCommands зарегистрированного проекта, pwsh.exe, Git и MSVC Cargo в PATH worker. Зависимости должны быть доступны локально: проверки выполняются `--locked --offline`, без установки глобальных инструментов. Скрипт не повторяет ручные gates этапов 1–3 и не запускает production Netwatch.

Source export содержит точный manifest всех source files с hash/размером, sourceCommit и честный dirty flag. Тестовый экспорт working tree с dirty=true не выдаётся за чистый Git commit и не является runtime snapshot dirty mode. ZIP проверяется до извлечения; после извлечения каждый файл сверяется с manifest. Тестовые source/build-cache/TEMP/journal находятся на локальном Windows диске, вне HGFS. Bootstrap сохраняет новый frozen request и TaskReport в Debian `.winbridge/workflow-validation/<run>`; lost ACK повторяет прежний request.

Сценарий PowerShell [test-windows-workflow.ps1](../../scripts/test-windows-workflow.ps1) запускает две группы:

1. Core/config/desktop persistence и log reader: byte paging/Unicode, append/truncate/rewrite/rotate, guards.
2. Worker integration runtime/snapshots/workflows: actual execution/output/cancel, NTFS guards, imports/migrations, import→build→start→ready→log, dedup/leases, build failure со старым exe, readiness и общий startup timeout, stop до готовности, позднее падение приложения, авария отдельного worker с Job close и indeterminate recovery без повторения.

Тестовая авария касается только созданного harness worker. Отмена внешней задачи завершается её управляемым деревом. В обычном backend сценарии desktop не открывается. Режимы CompanionOnly и WorkflowAttentionOnly открывают обозначенные тестовые окна и сами выполняют native UI проверки. Реальная browser UI integration проверяется отдельно `npm run test:ui` с Unix worker/WebSocket и transport shim; она не подменяет Windows Win32 checks.

## Отчёт

`K:\VMs\shared\WinBridge\workflow-validation\<run>` содержит `report.json`, два полных test logs и `report.ready.json` с hashes/размерами. Report включает actual platform=windows, source archive hash/commit/dirty, argv и exit code каждой группы. Ready публикуется после logs/report. Debian harness сверяет каждый artifact, exit code task, обе группы и source SHA-256. Ошибка bootstrap сохраняет task/output в локальном отчёте; старый run не удаляется и не переиспользуется.

При изменении runtime code повторяется автоматический сценарий; изменение только документации не требует нового Windows gate. Пользователю нужен один общий update fixed runtime после готового выпуска, а не цикл ручных проверок после каждого fix. Обновление через уже существующий `prepare-windows-snapshot.ps1`: закрыть desktop, штатно остановить worker, выполнить сценарий из новой source версии. Он сохраняет configs/state и отказывается заменять активные exe. Подробная инструкция этапа 3 остаётся [здесь](windows-snapshot-validation.md); новый worker объявляет workflowAvailable/log_read, UI показывает «Цикл разработки».

Расширенный прогон этапа 4 включает readiness IPC tests и [изолированный Netwatch](netwatch-validation.md). Опция `--netwatch-profile` передаёт подготовленный frozen профиль дополнительному test worker. Не запускайте эти test scripts вручную для обычного обновления приложения.

Прогон завершения этапа 4: 41 native test, Windows desktop check и изолированный Netwatch cycle passed; [IDs, hashes и ограничения](../done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md). Test report reader использует явный UTF-8 для CLI stdout/stderr, сохраняя кириллицу независимо от консольной code page.

## Delta этапа 6 без новых окон

`validate-workflow-native.py --delta-only --controller-config ... --archive ...`
запускает новые core/sync/worker tests через installed worker. Desktop не нужен.
Cargo собирает изолированные tests offline; пакеты проверяются на actual HGFS
в новом shared run, workspace/SQLite/build cache находятся на local disk.
`delta-delivery-proof.json` сохраняет full/delta hashes/bytes/timings,
`delta-worker-proof.json` — import/dedup/recovery/build/Unicode/tamper evidence.
`report.ready.json` подтверждает размеры и hashes artifacts. Installed exe/config
не заменяются. После завершения desktop можно оставить закрытым, worker оставить.

## Resume gate без ручных действий

`scripts/validate-workflow-native.py --controller-config CONFIG --archive ZIP --resume-only`
запускает `test-windows-workflow.ps1 -ResumeOnly` через рабочий Windows worker.
Desktop закрыт; новых окон нет. Тесты собирают новую версию в собственном runtime
и проверяют actual HGFS blocks/cancel/corruption/delta, NTFS junction, owned worker
crash/restart, same-key reconciliation, explicit resume, pinned offline build и
process tree/timeout regression. Fixture публикуется в Debian, Windows проверяет
тот же ready/manifest/blob pin и использует его после planned crash. Installed exe/config/journal не заменяются.
`resume-proof.json` и logs включаются в checksummed report с ready последним.
После проверки тестовые worker остановлены; рабочий worker оставьте запущенным.

## Общие профили рецептов (этап 7)

`validate-workflow-native.py --profiles-only` передаёт `-ProfilesOnly` в
`test-windows-workflow.ps1`. Рабочий worker должен быть запущен; desktop не нужен,
тестовых окон нет. Сценарий собирает отдельные exe в новом local runtime и проверяет
два проекта с общим профилем, literal argv/Unicode, dedup/conflict, запрет direct,
cancel tree, crash/restart, frozen origin и queued command guard. Регрессии
snapshot/workflow включают закрепление recipe definitions до workflow ACK:
изменённый профиль и legacy queued recipe workflow не запускают дочерние операции.

`profiles-proof.json`, `report.json` и logs проверяются по `report.ready.json` из
Debian; source commit/dirty/archive identity сверяются с native worker. Installed
worker exe/config/state и прежние shared reports не меняются. После завершения
нужно оставить рабочий worker; desktop можно держать закрытым. Новый ручной gate
не нужен. [Формат и настройка profiles](../guides/project-profiles.md).

## Узкая проверка версии сборки

Для изменений только в определении build identity используйте из Debian:

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/ИМЯ-АРХИВА.zip \
  --identity-only
```

Должен работать установленный Windows worker; desktop не нужен. Сценарий проверяет
ZIP и manifest, распаковывает новую копию на локальном диске Windows и передаёт
`-IdentityOnly` в `test-windows-workflow.ps1`. Выполняются только protocol tests:
настоящий Git checkout, Unicode paths, parent checkout и скрытые изменения через
`assume-unchanged`/`skip-worktree`. Тестовые worker/desktop и окна не запускаются.
Отчёт и logs проверяются по `report.ready.json`; версии и состояние установленного
worker сверяются до/после. После прогона desktop можно открыть или закрыть,
рабочий worker оставить запущенным. Повтор прикладных сценариев не требуется.

Debian-проверка экспорта: `python3 scripts/test-export-source.py`. Метка clean
требует совпадения реально экспортированных путей/байтов с закреплённым Git commit,
а не только пустого `git status`. Для Git-сборки со скрытыми index entries runtime
консервативно сообщает `sourceDirty=true`: отсутствие изменений не подтверждено.
