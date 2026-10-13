# A1 — типизированная Task FSM

Дата: 2026-10-13. База: A0, `f8c22d5`.
Статус: runtime implementation добавлена; local checks пройдены,
automatic native Windows gate ещё не пройден. A1 пока не закрыт.

## Что изменилось

[Pure reducer](../../../crates/core/src/task.rs) принимает прежний TaskRecord,
typed event и время от worker, возвращает новый record и proposed effect.
Он не открывает SQLite, не запускает процессы и не генерирует IDs/time.
Acceptance factory сохраняет прежние поля и первый journal event.

| Внутреннее состояние | Проекция v1 | Смысл |
| --- | --- | --- |
| Queued | `queued` | Задача принята, исполнение ещё не назначено. |
| Starting | `running`, PID отсутствует | Durable launch intent; наличие процесса ещё не подтверждено. |
| Running | `running`, PID сохранён | Worker зарегистрировал созданный управляемый процесс. |
| Starting/Running с cancellation requested | `running`, cancel flag | Намерение отмены; terminal только после подтверждения. |
| Terminal(outcome) | Пять прежних terminal statuses | Подтверждённое завершение либо неизвестный исход. |

Typed events: LaunchIntent, ProcessStarted, CancellationRequested, Completed,
Recovered. Completion принимает только TaskOutcome, где нет queued/running.
Повтор completion/cancel/recovery не меняет terminal. Повтор наблюдения того же
PID не добавляет событие; другой PID, launch вне Queued и PID observation после
terminal отвергаются. Это phase guards; durable attempt/epoch guards ещё впереди в A2.

[Store::transition](../../../apps/worker/src/store.rs) читает current record,
проверяет identity, вызывает reducer и фиксирует state/event одной транзакцией.
Для terminal outcome та же транзакция пишет attention. Только успешно сохранённый
launch decision возвращается scheduler и разрешает создание execution thread.
Cancel effect сохраняет flag; прежний process owner читает его и останавливает дерево.
OS spawn, drain, timeout и завершение дерева остаются у прежнего platform/worker owner.

Все production task lifecycle writes — acceptance, launch intent, PID, cancel,
finish и recovery — теперь проходят этот pure contract. Workflow/application/import
FSM не переносились. Low-level Store::save пока остаётся для legacy fixtures;
revision/CAS/resource epoch и защита остальных metadata writers — пакет A2.

## Совместимость и наблюдение

Wire version 1, DTO, JSON/TS schemas, DB schema 6, fingerprints, IDs и зависимости
не меняются. UI/CLI/MCP читают те же состояния, причины, command/output и события.
Starting не попадает новым enum value в старый wire. Существующее правило race
сохраняется: confirmed exit в execution loop может быть прочитан раньше cancel/
timeout; одна cancellation request сама не превращает exit 0 в cancelled.

Прежний Store допускает terminal completion до PID, например preflight/spawn
failure и synthetic fixture completion. A1 это сохраняет; reducer не делает вид,
что legacy записи доказывают физическое исполнение. Время перехода приходит одним
значением для timestamp и event; вместо отдельных вызовов clock внутри mutation.

## Ревью и исправления

- Нет dispatch до commit; ошибка launch write оставляет queued и ноль процессов.
- Ошибка PID commit после spawn приводит через прежний execution recovery к
  indeterminate; ManagedChild drop прекращает дерево, повтор key не запускает его заново.
- Найдено: `UPDATE` может вернуть успех с нулём изменённых rows (например SQLite
  RAISE(IGNORE)). Теперь changed rows обязаны равняться одному; иначе proposed effect
  не разрешён. Отдельный regression подтверждает отсутствие запуска.
- Сверяется ID row/read-model, чтобы corrupted record не выдал несуществующий
  journal identity как разрешение исполнения. Это не заменяет CAS/epoch A2.
- Recovery по-прежнему выбирает только running records; не добавлены extra get/
  clone всех terminal записей поверх прежнего общего чтения.
- В качестве эффекта actor restart/recovery нигде не выдаётся relaunch.

## Проверки и автоматический Windows gate

[Core tests](../../../crates/core/tests/task_fsm.rs): 7 groups покрывают phase
guards, legacy projection, intent cancellation, cancel/PID и exit/cancel races,
terminal immutability и recovery без relaunch.
[Worker tests](../../../apps/worker/tests/task_fsm.rs): launch commit abort/ignore,
PID commit failure с настоящим spawn и отсутствие replay, сохранённая cancel/PID race.
A0 frozen wire/journal tests входят в тот же native gate.

В Debian пройдены 182 workspace tests, Windows-target check workspace и
protocol/core/worker integration tests, targeted Clippy, fmt и scaffold/link checks.
Cross-check подтверждает компиляцию, но не выполнение Windows процесса.

Добавлен режим `--task-fsm-only` в
[Debian driver](../../../scripts/validate-workflow-native.py) и `-TaskFsmOnly` в
[PowerShell scenario](../../../scripts/test-windows-workflow.ps1). Он запускает
core FSM, обе baseline suites и native task/runtime/attention/workflow regressions.
Desktop и UI test windows не открываются. Working worker нужен только как канал
запуска; он и его state/configs не заменяются, test instances используют отдельные
local state/workspaces. Отчёты/ready и logs находятся в новом shared run namespace.

До native gate A1 остаётся implemented / native gate pending; следующий A2 не
включать как замену обязательной проверки process lifecycle.
