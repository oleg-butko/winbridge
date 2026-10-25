# Actor23/view15: physical StateOwned того же receiver

2026-10-25 America/New_York. Продолжение после [Main22 receiver112/0](windows-activation-process-gate-2026-10-24.md)
и [lock-release adapter7/0](state-lock-handoff.md). **Source реализован;
новый actual state-transfer gate ещё не выполнялся.** Старые результаты не
доказывают этот межпроцессный handoff. Updater18/worker16 прежние;
activation/controller_confirmation/desktop_update=None, public install off.

## Подтверждаемый результат и границы

Тот же private receiver, который уже подтвердил Ready, приобретает прежний
instance lock и подтверждает те же directory/journal/lock FileID, logical
instance/journal incarnation и новый generation. Точная schema, текущий quiesced
lease и immutable accepted shutdown receipt сверяются readonly. Ledger18 сохраняет штатные
HandoffIssued5 → StateOwned6. Форма существующей receipt не меняется.
**Writable Store, новая JournalAuthority, recovery, scheduler, local/controller
service ещё не запускаются.** Readonly Store inspection не активирует epoch и не выполняет миграцию.

```mermaid
sequenceDiagram
    participant P as Original supervisor
    participant J as Updater journal
    participant F as Existing state files
    participant W as Same Ready receiver
    P->>J: CAS/readback HandoffIssued5
    P->>F: Unlock own instance handle; retain pins
    P->>W: Private V2 scope + issued-record digest
    W->>J: Readonly current graph and parent binding
    W->>F: Acquire exact existing lock; retain identities
    W-->>P: Physical StateOwned receipt
    P->>J: CAS/readback StateOwned6
```

## Контракты и владение

- Challenge/Ready contract1 и marker1 сохранены. Отдельные state command/receipt
  contract2 и marker2 проверяются **до release**; старый Ready1 не считается
  поддержкой handoff. В payload нет executable/path/nonce/force/startup permit.
- Command связывает update/attempt/revision5 и domain-separated canonical SHA
  всего current HandoffIssued record. JSON digest сам не даёт permission:
  исходный channel уже аутентифицирован по OS peer/nonce, оба процесса сверяют
  текущий graph/owner/root и actual PID/birth/host/session.
- Parent потребляет тот же Ready guard в единственном blocking slot. Новый
  typed `ReleasedExitCustody` хранит original exited-process pin и именно
  residual state handles для каждого исхода unlock. Ошибка не возвращает Ready
  для retry, не reacquire и не создаёт второго child.
- Before OS unlock библиотека повторяет authority/physical/monotonic deadline
  checks. Child один раз фиксирует оставшееся окно из original admission при
  первом startup snapshot; новый message или wall-clock rollback его не обновляет.
  Перед acquisition проверяет сохранённый monotonic deadline и wall deadline.
- Receiver сверяет loaded runtime/config, current HandoffIssued и свою прежнюю
  Ready receipt, открывает **существующие** state pins, затем пытается приобрести
  тот же lock. Он не создаёт новый state/lock/journal. Readonly inspection обязана
  иметь точную schema, прежний incarnation и те же quiesced lease/accepted shutdown
  receipt. Updater возвращает frozen expected receipt в том же read snapshot,
  что HandoffIssued; факт повторно сверяется до ACK.
- Child удерживает lock/config/runtime/channel в своём owner scope. EOF,
  неизвестная следующая команда, parent death или bounded hold завершают его
  без writer. Следующего writable activation command пока нет.
- StateOwned receipt может быть сохранена как поздний факт после окна; нового
  release/acquisition grant из этого не возникает. Partial write/lost outcome
  даёт uncertainty и не повторяет private command.

## Actor и наблюдение

`HandoffActivationState`/`ObserveActivationState` требуют23, exact Active bound
supervisor, token/revision и отсутствия caller worker command. Состояния
Ready → HandingOff → StateOwned/HandoffFailed. Повтор start во время/после
попытки — observe; нельзя получить owner из durable rows или failure object.

View15 добавляет `activationState`: typed phase, durable revision/phase,
saved physical receipt, nullable original-lock observation, resident process и
его отдельную liveness. Во время blocking work original lock неизвестен; после
release не выдаётся за удержанный. Cold reader сохраняет receipt, но показывает
NotResident и не восстанавливает resources. Saved StateOwned не доказывает
нынешнее владение lock без живого owner/process.

Views1–14 не получают новое поле. Legacy14 после state handoff показывает
Indeterminate с пояснением перейти на23, сохраняя actual process diagnostics;
legacy13 по-прежнему показывает пять retained runtime files, original state
custody — отдельно. Нет превращения physical StateOwned в whole-update success.

## Карта кода

| Файлы | Назначение |
| --- | --- |
| `crates/protocol/src/update/activation_receiver.rs` | Frozen receipt перенесена с Rust re-export; отдельные private V2 messages/marker. |
| `crates/protocol/src/update/activation_state.rs`, `update.rs` | Actions23 и view15, без расширения старых view shapes. |
| `apps/update/src/activation/channel.rs` | Canonical command binding, strict physical receipt validation. |
| `apps/update/src/shutdown/physical.rs` | Consuming original exit/state resource в released/failure custody. |
| `apps/update/src/runtime_replacement/activation_process.rs` | One-shot parent handoff на original IO driver; durable-before-unlock и retained failures. |
| `apps/worker/src/activation.rs` | Exact same-child acquisition/readonly journal/receipt, без Store writer. |
| `apps/update/src/journal/activation.rs`, `supervisor.rs` | Phase-specific readonly child admission и consistent snapshot15. |
| `apps/update/src/supervisor.rs`, `supervisor/activation_files.rs` | Typed23 dispatch, one blocking owner, duplicates/failure/live/cold projections. |

## Проверки source и следующий actual gate

Source checks завершены: focused activation33/0, readonly journal2/0,
mailbox v23 —1/0, workspace741/0 (16 ignored), protocol-jsonschema59/0. Linux validation,
MSVC validation и MSVC production all-target Clippy passed; generated contracts/
check, TypeScript, UI build и fmt passed. Логи/measurements
`.winbridge/update-preparation/activation-state-v23-*`.
Native межпроцессное исполнение пока **не подтверждено**.

После source checks/review экспортировать clean **новую receiver fixture** с
marker2 (1c23d48 имеет только Ready1), затем distinct clean harness producer.
Из installed d052 можно запустить isolated validation без ручного update.
Нужен только installed worker; desktop не нужен, UI-окон не будет. Перед run
обе free-space measures >=6GiB, один cache lease; Windows каталоги не очищать.

Новый Main должен сразу после Ready перейти к handoff, не расходуя original120s
на тяжёлые intermediate audits. Проверить HandoffIssued до unlock, original lock
held до/false после release, actual lock busy под тем же child, одинаковые IDs,
новый generation, неизменные worker DB/WAL и epoch. Root остаётся VerifyingLocal15.
Lost ACK/duplicates не повторяют release/command/create. После owned Job exit —
retained child handle signalled, Jobs empty, тот же lock вновь приобретается;
cold StateOwned/history без ресурсов, обычный serve по-прежнему blocked.
Independent audit обязан связывать оба private prefixes/current scopes и
отвергать PID/FileID/record hash/generation/epoch/history/custody mutations.

После positive gate — отдельная writable activation-транзакция/receipt и schema
policy, local/controller/lifetime/commit, remaining U2 faults/management и U3/U4.
Не включать advertised activation до всех этих drivers.

## Review и подготовка следующего source export

Review усилил journal admission: диапазонная проверка header могла принять
прежнюю schema или один лишь incarnation без matching accepted shutdown.
Теперь inspection требует exact schema и полное равенство сохранённых quiesced
lease/receipt. Два реальных SQLite теста проверяют missing receipt, чужой scope,
изменённые command/process/owner и old/future schema без миграции/смены epoch.
State pins проверяют local path/reparse safety до canonical identity comparison.

Первый selected mailbox test получил EOF, потому что fixture копировала старый
`target/debug/winbridge-update`. После explicit standalone build он прошёл.
Для следующих локальных mailbox checks сначала:

~~~bash
PATH=/home/user1/.local/bin:$PATH CARGO_INCREMENTAL=0 \
  cargo build -p winbridge-update --features update-validation --locked
~~~

При native validation updater также должен собираться/замораживаться до вызовов23.
Это build/test prerequisite, не проблема installed worker.

Итоговый machine summary `activation-state-v23-verified.json`, Debian free
25,678,422,016 bytes после финальных checks (>6GiB), cleanup не требовался.
Source updater18/worker16, supervisor23/view15. Вся цель updater не завершена;
новый actual handoff gate **не был отправлен**. Installed d052 остаётся healthy/
connected/active0. Clean receiver fixture export receipt следующего шага:
`.winbridge/update-preparation/activation-state-v2-fixture-export.json`.
Её ZIP/commit использовать как target нового distinct harness producer; старые
terminal tasks112/0 и7/0 не replay. Нужен новый Rust Main case, PS state mode и
independent Python verifier: они ещё не добавлены в этом source-блоке.

## Готовая receiver2 fixture и точка продолжения

Source commit `1370d6b030e18a22266be1b02d24e4371eee01ac` экспортирован clean:
790 files,5,750,619 bytes, built UI. Shared sources ZIP
`winbridge-20261025T092432Z-1370d6b-257867fc.zip`, SHA256
`579728e29124e3bc914e22f070e3ab5634f68b17d0a0072d74a4387685230656`,
manifest SHA256 `2dc515ce88cb981a633c9b4acd4a1504d4fc2368d088fb73f3373068d3772f16`.
Receipts `.winbridge/update-preparation/activation-state-v2-fixture-{export,pin}.json`.
Это source fixture, не положительный Windows execution proof.

Следующий distinct producer должен добавить actual Main StateOwned scenario,
PowerShell mode и independent verifier. Target — **1370d6b**, не прежняя1c23d48.
Cache20261017-061527-f4639971, schema17 reference
`.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
Schemas18/16 совпадают; actual17→18/refusal проверки сохранить в combined gate.
Новый state task/run ещё не существует; предыдущие112/0 и7/0 terminal.

Ранее measured Ready был91.397s после admission (saved93.571s), окно120s.
Для следующего positive state gate сразу вызывать23 после Ready; не помещать
тяжёлые set/history/duplicate audits перед handoff. Если для boundary proof
понадобятся validation hooks, source fixture их иметь не обязана: они нужны в
frozen producer validation updater. Не увеличивать timeout ради success и не
выдавать исторические data rows за нынешние OS permissions. После source/harness
checks — один isolated run с сохранёнными metadata до ACK и same-run recovery.

Last fresh installed observation: d052/generation8f8d1fb9-e5b1-4984-a17e-d067db46268e,
connected/active0, maintenanceError=null/journalRecoveryRequired=null,
recoveryCoverage.complete=true. Controller PID3922/ws192.168.44.203:1143 работает.
Desktop не нужен, installed worker оставить. Перед новым run снова Info/disk.
Никаких Windows задач для нового state mode пока не отправлялось.
