# A0 — legacy baseline

Дата: 2026-10-13. Planning commit: `d524e68`. Runtime source: `a42cd7e`.
Статус: **A0 завершён**. Добавлены ADR, frozen fixtures и 14 baseline tests;
новый runtime ещё не реализован. Следующий пакет — A1.

[ADR 0005](../../decisions/0005-gradual-state-and-contract-migration.md) закрепляет
последовательность миграции. [Frozen fixtures](../../../tests/fixtures/legacy-v1/README.md)
и тесты используют production DTO/Store, а не модель будущей архитектуры.
User configs, журналы, runtime binaries и dependency graph не меняются.

## Что покрыто

- Hello/Submit round-trip, семь wire states, строгий отказ неизвестным commands/
  Hello fields/capabilities; новые internal states не проникают в v1.
- Точные bytes fingerprint, Windows path/literal argv/Unicode; request correlation
  не меняет intent. Исторический payload используется прямо при retry.
- Старый TaskRecord с отсутствующими optional fields; running без PID допустим
  как launch intent, а не доказательство фактического процесса.
- SQLite schema 1 → 6, raw payload/record сохранены, consistent backup читается;
  unversioned existing DB и неподдерживаемая будущая схема отвергаются без смены rows/schema.
- Независимый reader видит acceptance commit; lost ACK/reopen возвращает прежний
  ID, frozen resolved command и один record. Другой payload даёт conflict;
  одинаковые keys разных projects независимы.
- Recovery running с PID/без PID → indeterminate; queued/terminal сохранены;
  повтор recovery не добавляет события/attention и не создаёт новые задачи.
- Running cancel остаётся intent, повтор не добавляет event; queued cancel завершается
  до launch; late finish/cancel не переписывают terminal через Store finish/cancel.
- Output sequence общий для stdout/stderr одной task, независим между tasks и
  от task-event sequence; byte cursor продолжается после reopen.
- Optional WorkerInfo metadata терпимо читается, но старый codec его теряет:
  будущий catalog требует full-route проверки и безопасного legacy fallback.

## Проверки

| Команда | Результат |
| --- | --- |
| `cargo test --locked -p winbridge-protocol -p winbridge-worker --test legacy_baseline` | 6 protocol + 8 Store tests passed. |
| `cargo test --workspace --locked` | 171 tests passed, включая 14 новых; 0 failed/ignored. Исполнение process/IPC regressions — Unix development adapter. |
| `cargo check --locked -p winbridge-protocol -p winbridge-worker --test legacy_baseline --target x86_64-pc-windows-msvc` | Passed; компиляция тестов под Windows, без запуска на Windows. |
| `cargo fmt --all -- --check` | Passed. |
| `npm run check:scaffold` | Passed: 114 Markdown документов, ссылки/configs/examples. |
| `git diff --check` | Passed. |

Здесь PATH включал `/home/user1/.local/bin` для Rust и
`/home/user1/.nvm/versions/node/v24.19.0/bin` для Node; startup files не менялись.
Тесты автоматически входят в `cargo test --workspace`. При будущем native FSM
gate включить оба `legacy_baseline` targets в выбранный approved scenario;
нынешние узкие Windows modes с явным списком `--test` не начинают запускать
их автоматически. Для native baseline нужен `winbridge-worker/bundled-sqlite`,
как в остальных Windows Store tests. Новый ручной Windows прогон не назначался.

## Границы доказательства

Синтетический SQL — не пользовательская БД. Здесь нет OS spawn, реальных Windows
handles, network handshake или native UI. Это baseline для будущих изменений,
не новый Windows gate. Текущие native receipts остаются историческим evidence.
Новые revision/epochs/outbox, общая FSM и scheduler ещё не реализованы.
Import/workflow/application migration fixtures и complete route/TS matrix
добавляются перед изменением этих domains. Тесты terminal проверяют public
finish/cancel, а не обещают, что нынешний низкоуровневый Store::save уже имеет CAS.

Следующий пакет A1 выделяет pure typed Task FSM с прежней внешней проекцией;
process lifecycle интеграция потребует automatic Windows gate.

## Резервные копии прежних незакоммиченных документов

До изменения оглавлений сделаны и проверены `cmp` два снимка. Остальные изменённые
старые файлы были clean tracked; новые файлы созданы этой задачей. Backup каталоги
не изменялись. Восстановление возвращает именно состояние **до этой задачи**,
а не текущую документацию плана; команды выполняются только при необходимости.

`docs/analysis/README.md`:

```sh
cp -a -- /home/user1/backup/codex/snapshots/20261013T123430.553215152Z-30739/home/user1/winbridge/docs/analysis/README.md /home/user1/winbridge/docs/analysis/README.md
```

`docs/README.md`:

```sh
cp -a -- /home/user1/backup/codex/snapshots/20261013T123430.747972556Z-30754/home/user1/winbridge/docs/README.md /home/user1/winbridge/docs/README.md
```
