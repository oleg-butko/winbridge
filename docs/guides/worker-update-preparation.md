# Подготовка обновления worker: U0 и первые части U1

2026-10-17. [Подробный план U0–U4](../plans/worker-self-update/README.md).
**Автоматическая установка пока не реализована.** Эти команды проверяют исходники
и собирают отдельный комплект; рабочие exe, config и journal сохраняются.
Worker может работать, desktop не требуется, UI-окна не открываются.
[U0 Windows gate](../plans/worker-self-update/windows-preparation-gate-2026-10-17.md)
пройден: 19 native tests, output-link guard и сборка всех четырёх binaries.

## Что уже делает winbridge-update

`winbridge-update` входит в Rust workspace. После `cargo build --workspace --locked`
появляется в `target/debug`; штатный prepare script пока не устанавливает его в runtime.
В U1 добавлены [независимый журнал и проверка ZIP](../plans/worker-self-update/durable-preparation.md).
В worker добавлена [координация исходников и остановки запусков](../plans/worker-self-update/worker-coordination.md)
с понятным readonly блоком состояния. Она пока используется только trusted driver/tests;
внешней команды установки нет. [Source transaction](../plans/worker-self-update/source-transaction.md)
реализована как trusted library driver; native gate ожидается. Shutdown и установка ещё не реализованы.

```powershell
# SHA берётся из проверенного пакета; путь должен быть абсолютным.
.\target\debug\winbridge-update.exe verify-source K:\path\to\incoming <MANIFEST_SHA256>
.\target\debug\winbridge-update.exe --config K:\path\to\winbridge-update.local.json plan K:\path\to\incoming <MANIFEST_SHA256>
```

Первый вызов проверяет per-file hashes/размеры и точный состав исходников.
Второй сравнивает новую поставку с текущим `src` и предыдущим
`winbridge-source-manifest.json`: показывает Add/Replace/Remove/Keep и blockers.
Локальные правки, локальное удаление, неизвестные source-файлы, case/layout conflicts
не исправляются автоматически. Links/junctions/hardlinks отклоняются.
Dirty packages не подходят для автоматического обновления.
Tauri `apps/desktop/src-tauri/gen`, Python caches и root `release` считаются
генерируемыми данными: они сохраняются и не включаются в source ownership.

Результат — JSON. Exit 0: проверка прошла/нет source blockers. Exit 2: source plan
содержит blockers, их описание в stdout. Exit 1: ошибочный config/manifest/path/hash,
структурированное сообщение в stderr. `runtimeUpdateReady: false` означает, что
перезапуск/установка не реализованы. `canApply: true` относится только к исходникам
и не разрешает shutdown или файловые изменения. Эти две команды ничего не записывают.

Config по умолчанию — `winbridge-update.local.json` рядом с exe; можно указать
абсолютный `--config`. Образец: [windows-update.config.json](../../examples/windows-update.config.json).
В нём source/runtime roots и фактический worker config path. Существующие файлы
конфигурации не перезаписывать. Incoming — заранее распакованный локальный каталог;
проверка ZIP доступна отдельно. Source-write команды для пользователя/агента пока нет.

```powershell
.\target\debug\winbridge-update.exe verify-archive K:\path\to\source.zip <ZIP_SHA256> <MANIFEST_SHA256>
.\target\debug\winbridge-update.exe --config K:\path\to\winbridge-update.local.json journal init
.\target\debug\winbridge-update.exe --config K:\path\to\winbridge-update.local.json journal status
```

`verify-archive` проверяет SHA архива/manifest, точный состав и actual decompressed
bytes/SHA; распаковки и записи нет. Поддерживаются обычные Stored/Deflate ZIP.
`journal init` создаёт только отдельную БД `runtime\update-state\updates.sqlite`
и installation metadata; worker/config/source/exe не меняются. Повтор сохраняет ID.
Текущая updater schema 2: при наличии schema 1 init мигрирует её только при свободном
owner lock, предварительно создавая проверенную согласованную SQLite backup.
Schema 1 остаётся доступной через readonly status без миграции.
`journal status [UPDATE_ID]` читает согласованный record, owner epoch и последнюю
sequence, не захватывая владение и не создавая отсутствующую БД. Это диагностика
будущего supervisor; сейчас пользователю не требуется выполнять команды вручную.
Запрос установки и внутренние FSM transitions не открыты через CLI/RPC/MCP.

Диагностика подготовленной библиотечным driver операции:

```powershell
.\target\debug\winbridge-update.exe --config K:\path\to\winbridge-update.local.json journal source-status <UPDATE_ID> 0 100
.\target\debug\winbridge-update.exe --config K:\path\to\winbridge-update.local.json journal source-inspect <UPDATE_ID> 0 100
```

Оба вызова readonly. Первый показывает план/intent/receipt; второй — совпадение
файлов с baseline/target и пути preimages/held. После сбоя повторное применение
запрещено. Даже совпавший target без квитанции не означает успешное обновление.
Не удалять staging и не снимать worker startup guard автоматически.

## Сборка кандидата без остановки worker

```powershell
.\scripts\build-windows-update.ps1 `
  -SourceDirectory K:\_proj-26\winbridge\src `
  -ManifestSha256 <MANIFEST_SHA256> `
  -StagingDirectory K:\_proj-26\winbridge\runtime\update\<NEW_UPDATE_ID> `
  -VerifierExecutable K:\path\to\verified\winbridge-update.exe
```

Нужны установленный проверенный verifier и clean source manifest. Путь staging
должен быть новым, вне `src`. Скрипт измеряет свободное место, проверяет source,
собирает Rust workspace с bundled SQLite и desktop с `custom-protocol`. При
наличии verified built UI он переиспользуется; иначе выполняются npm ci/build:ui.
Основной build cache остаётся в `src/target`, desktop cache — в его обычном target.
После сборки source проверяется ещё раз, actual worker identity сверяется с manifest.

В staging создаются четыре exe в `payload`, логи и `candidate-report.json`:
`status: built` подтверждает сборку и hashes; `candidateVerified: false` означает,
что health/contract gate будущего installed runtime ещё не выполнялся.
`runtimeApplied: false` подтверждает отсутствие установки. Ошибка останавливает
сценарий, имеющиеся файлы staging сохраняются для диагностики.

Это отдельный build-only сценарий. `prepare-windows-snapshot.ps1 -PrepareOnly`
по-прежнему может копировать binaries в runtime и не является его заменой.

## Автоматическая проверка разработчиком

`scripts/test-update-stage.ps1` проверяет create-only staging и неизменность
тестового runtime на PowerShell. Rust tests проверяют CLI, ownership/path guards
и Update FSM. `scripts/validate-workflow-native.py --update-preparation-only`
запускает native tests журнала/ZIP/source transaction/FSM, проверку actual source ZIP и полную сборку кандидата через работающий worker в отдельном
каталоге зарегистрированного fixture-проекта; installed source/runtime не меняются.
Нужен clean source ZIP с built UI и явные ссылки на прежние exe:
`--update-reference-worker <schema-14 worker>` и
`--update-reference-updater <schema-1 updater> --update-reference-updater-sha256 <SHA>`.
Они копируются в тестовый каталог и используются только с новой тестовой БД
для проверки отказа старой версии. Путь/hash updater берутся из проверенного
предыдущего candidate report. Это не самообновление; production shutdown и
замена файлов появятся после U1–U3 gates и первоначального bootstrap.

Для повтора developer gate есть `--update-preparation-reuse-run <run ID>`:
проверяются checksums прежнего отчёта и terminal task с успешными native groups,
затем переиспользуются только native-test/Rust/desktop compiler caches под exclusive leases.
Source, test state, staging и итоговый отчёт всегда новые. Default build script
по-прежнему использует caches в постоянном `src`; test-only
`ValidationCacheDirectory` не меняет расположение installed runtime.
