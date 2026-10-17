# Подготовка обновления worker: первый реализованный пакет U0

2026-10-17. [Подробный план U0–U4](../plans/worker-self-update/README.md).
**Автоматическая установка пока не реализована.** Эти команды проверяют исходники
и собирают отдельный комплект; рабочие exe, config и journal сохраняются.
Worker может работать, desktop не требуется, UI-окна не открываются.

## Что уже делает winbridge-update

`winbridge-update` входит в Rust workspace. После `cargo build --workspace --locked`
появляется в `target/debug`; штатный prepare script пока не устанавливает его в runtime.

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

Результат — JSON. Exit 0: проверка прошла/нет source blockers. Exit 2: source plan
содержит blockers, их описание в stdout. Exit 1: ошибочный config/manifest/path/hash,
структурированное сообщение в stderr. `runtimeUpdateReady: false` означает, что
перезапуск/установка не реализованы. `canApply: true` относится только к исходникам
и не разрешает shutdown или файловые изменения. CLI ничего не записывает.

Config по умолчанию — `winbridge-update.local.json` рядом с exe; можно указать
абсолютный `--config`. Образец: [windows-update.config.json](../../examples/windows-update.config.json).
В нём source/runtime roots и фактический worker config path. Существующие файлы
конфигурации не перезаписывать. Incoming — заранее распакованный локальный каталог;
автоматическая проверка ZIP/import/source apply будет добавлена в U1.

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
запускает native tests и полную сборку кандидата через работающий worker в отдельном
каталоге зарегистрированного fixture-проекта; installed source/runtime не меняются.
Нужен clean source ZIP с built UI. Это не самообновление; production shutdown и
замена файлов появятся после U1–U3 gates и первоначального bootstrap.
