# Supervisor22/view14: один pre-writer receiver

2026-10-24. Actor dispatch добавлен к [launcher/receiver library](activation-receiver.md).
**Actual native execution gate ещё не выполнен.**
[Closing checkpoint и инструкция новой сессии](resume-2026-10-24-activation-process.md):
Main22 harness/verifier уже добавлены, следующий шаг — clean producer и запуск. Updater18/worker16 прежние;
activation/controller_confirmation/desktop_update=None, public install off.
Новые поля есть только у view14; frozen views1–13 не меняются.

## Поведение и владение

- `LaunchActivationReceiver` и `ObserveActivationProcess` требуют22, exact
  Active bound supervisor/token/revision, без caller command/paths/receipt/nonce.
- Начать запуск можно только из того же resident Bound files guard. Actor
  перемещает его в единственный blocking slot; копии handles/новой подготовки нет.
- ACK Launching означает volatile dispatch. Creating/Created/ResumeGranted/Ready
  пишет trusted library вокруг настоящих OS effects, а не caller или actor ACK.
- Повтор start во время работы/после успеха/ошибки — observe той же попытки.
  `LaunchFailed` хранит тот же failure owner; повторного CreateProcess нет.
- После completion native guard возвращается actor. Удерживаются private channel,
  exact read files, original state lock и child/Job. Передачи state writer ещё нет.
- Legacy view13 продолжает показывать5 held files при перемещении владельца в
  launcher/Ready/failure. Cold reader не восстанавливает этот статус из истории.

Типизированные states actor: Idle → Binding → Bound → Launching → Ready/LaunchFailed;
отдельный Failed сохраняет resources при ошибке file binding. Возврата в Bound
после попытки запуска нет. Scope проверяется на каждой итерации mailbox.
Не прерывать blocking owner из-за потери client ACK/тайм-аута наблюдения.

## Данные и живое наблюдение

`ActivationProcessView` разделяет durable phase/revision, saved process/Ready,
resident process, custody и liveness. NotResident после crash означает отсутствие
original owner; даже сохранённый Ready не превращается в право заново запустить
или открыть журнал. Во время blocking work custody неизвестна (`null`), на cold
reader false. Live Ready показывается только при положительном OS liveness и
удержании original state; потеря свежего наблюдения даёт Indeterminate.

Есть bounded stdout/stderr tail (512 chars), признак неполного output и окончания
capture. Это диагностика наблюдаемого процесса, не квитанция cleanup. Полные
bounded captures остаются у native guard. Никаких secret nonce/permits в view.
Измеренная максимальная view13 из retained97 proof —11413 bytes; новые сведения
и tails помещаются в существующий32KiB канал (actual native размер проверить).

## Исправление lifetime I/O по результатам review

Поллинг graph-heavy launcher идёт на owner thread8MiB. IPC регистрируется на
**исходном долгоживущем IO driver supervisor** через Tokio Handle. Создание
временного Runtime на launcher thread было бы ошибкой: после возврата Guard
runtime завершился бы вместе с thread, а retained pipe понадобился бы следующему
шагу state handoff.

`activation/owner_stack.rs::on_original_io` сохраняет input до успешного старта
thread. Ошибка отсутствующего runtime/thread creation возвращает тот же input,
без вызова factory. Future создаётся/поллится на выделенном стеке; исходный
supervisor event loop продолжает обслуживать I/O и mailbox. Panic не выдаётся
за возвращённые resources: прежний durable barrier остаётся, новый owner не
конструируется из данных.

Регрессионная проверка использует настоящий Tokio IPC listener, созданный через
этот же adapter: после завершения owner thread listener принимает соединение и
обменивается bytes через original runtime (Unix socket на Debian, named pipe на
Windows). Нового TCP endpoint/firewall rule этот test не требует. Отдельно проверяется возврат того же
input без factory при отсутствии runtime. Это проверка lifetime Tokio I/O, не
замена будущего named-pipe/Windows исполнения.

## Карта кода

| Место | Изменение |
| --- | --- |
| `crates/protocol/src/update/activation_process.rs` | Versioned readonly projection, typed durable/runtime phases, live/cold diagnostics. |
| `apps/update/src/activation/process_view.rs` | Cold projection сохраняет receipts, не handles/permission. |
| `apps/update/src/activation/owner_stack.rs` | Большой стек, original IO driver, resource return on startup error. |
| `apps/update/src/supervisor/activation_files.rs` | Единственный owner FSM, move Bound guard и completion без повторного create. |
| `apps/update/src/supervisor.rs` | Typed22 client/handler и existing blocking slot. |
| `apps/update/src/journal/supervisor.rs` | Consistent snapshot14; older views omit the new field. |
| `apps/worker/src/update_control.rs` | Real mailbox tests: observe22, invalid version/scope/token/command и no-custody launch refusal. |

## Что проверить на настоящей Windows дальше

Готовая clean target source fixture1c23d48 и exact pins находятся в
[receiver document](activation-receiver.md). Она имеет receiver v1, но не новый
actor22. Для gate нужен distinct clean producer с новым actor/harness.
Installed d052 используется только для запуска изолированного сценария; рабочий
runtime/state и desktop не заменять. Перед heavy build обновить обе disk measures,
один cache lease; Windows directories не очищать.

1. [Explicit Main/PowerShell/Python process mode](windows-activation-process-gate-2026-10-24.md)
   и verifier добавлены. Следом clean producer export и actual run; старые
   preparation/files `--verify-run` сохранены. Не replay terminal runs.
2. В original supervisor: Prepare → Bind → Launch без тяжёлых history/duplicate
   audits между этими grants. Они выполняются после Ready. Original120000ms
   deadline не продлевать; late fact не превращается в новый grant.
3. Validation-only barriers непосредственно после durable Creating до OS и после
   Created до resume. На них проверить журнал, actual PID/Job и отсутствие
   writer; release marker не является запуском/authority. Исходный parent/thread
   продолжает работать, а main worker уже штатно завершён прежним shutdown proof.
4. Actual Ready: один exact PID/birth/session/host, live Job/process, loaded set/
   config/attempt совпадают, original instance lock занят, DB/WAL unchanged,
   normal serve blocked, update root остаётся VerifyingLocal. Проверить retained
   channel после retirement launcher thread, а не только факт Ready до возврата.
5. Duplicate22 не создаёт process/attempt; old view13 без new field, но5 files
   всё ещё held. После owned test Job termination требовать actual exit/empty,
   cold saved Ready/history без handles/повторного launch, обычный startup refused.
6. Independent verifier связывает реальные snapshots/receipts; mutation cases для
   PID/Job/source/config/epoch/nonce/deadline/history/ACK/unknown effects. Ready
   files хэшировать дважды. Этот gate не означает StateOwned/controller/commit.

После positive gate/review — HandoffIssued и acquisition того же instance lock
FileID, ownership receipt/schema policy, local/controller/lifetime, remaining
U2 faults/management, U3/U4. Не включать advertised activation до этих drivers.

## Проверки source

Логи/receipts `.winbridge/update-preparation/activation-process-*`.
Focused activation29/0, финальный owner-IPC2/0, workspace727/0 (16 ignored),
protocol-jsonschema57/0 прошли. Linux validation/MSVC validation/MSVC production
all-target Clippy, generated contracts/check/TS/UI/fmt прошли. В export-types
добавлены три новые декларации после выявленной TypeScript ошибки. Final MSVC
проверка включает IPC-тест после замены TCP. Scaffold267/local links и diff check пройдены.
Machine summary: `activation-process-verified.json` в ignored log directory.
Actual Windows execution в этом блоке ещё не был. Native mailbox/model tests
при cross-check только компилируются; Linux mailbox выполнен workspace-проверкой.
