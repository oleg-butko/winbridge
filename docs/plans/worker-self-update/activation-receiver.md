# Suspended launcher и private receiver до writer

2026-10-24, продолжение после checkpoint547a487. **Library и private worker entry
добавлены; [actor22/view14](activation-process-actor.md) подключён,
[actual Main22 gate112/0 пройден](windows-activation-process-gate-2026-10-24.md).** Прежний
[native97/0](windows-activation-files-gate-2026-10-24.md) относится к producer941a2e6
и доказывает read binding, а не этот запуск. Public install остаётся выключенным,
activation/controller_confirmation/desktop_update=None. Schemas18/16 не менялись; новый actor22/view14 описан отдельно, views1–13 frozen.

## Зачем отдельный путь

Обычный `serve` проверяет maintenance/startup barrier и открывает Store после
instance lock. Во время обновления это слишком рано: старое состояние ещё
принадлежит original supervisor. Новый private entry проверяет загруженные файлы
и родителя, отвечает Ready и остаётся **без instance lock/Store/scheduler**.
Обычный startup guard сохраняется. Нет ignore-guard switch, cold launch permit
или восстановления authority из JSON.

## Последовательность library driver

`ActivationFilesGuard::launch_receiver` потребляет единственный live files guard.
Он создаёт отдельного non-Serde `ActivationProcessGuard`, удерживающего original
file/state custody, один process/Job, channel и bounded stdout/stderr captures.

1. Strict Prepared/revision0 admission, exact worker image и capability marker
   private receiver v1. Старый binary не запускается с неизвестным CLI.
2. Bind parent-owned same-user/no-remote named pipe до создания процесса.
   Endpoint derived от attempt UUID, secret в имени отсутствует.
3. Guarded append Creating в SQLite и commit/readback **до OS effect**.
   После slow graph/file/custody I/O повторно проверить original live deadline.
4. Existing platform atomic adapter создаёт suspended executable в fresh owned
   Job через job/handle attribute lists. Никакого автоматического resume.
5. Сохранить actual PID/birth/host/session/Job в Created до fallible allocation
   stdout/stderr reader threads; child пока suspended. Подтверждённый
   `SpawnFailure.no_process_created` допускает CreationFailed; неизвестный исход
   сохраняется как uncertainty и никогда не разрешает второй процесс.
6. Сохранить ResumeGranted, снова проверить custody/deadline и только тогда
   resume. Stdout/stderr читаются независимо сразу после получения child owner.
7. Принимать pipe connection только от exact созданного child PID под retained
   process pin. Другому same-user клиенту nonce не передавать.
8. Challenge с transient nonce; child проверяет actual pipe server PID и pinned
   original supervisor stamp, digest сохранённого nonce и authoritative graph.
9. Ready от того же child и loaded set/config; сохранить ReadyForState. Original
   instance/state lock остаётся у updater. Channel/resources остаются resident.

Journal append helper выводит признак нового grant из следующего enum state,
а не из caller bool. Creating/ResumeGranted/HandoffIssued проверяют original
deadline. Actual Created/Ready facts разрешено сохранить позднее без нового
grant. Отдельное ожидание факта не продлевает старое окно для OS effects.
Текущий public Prepared checker не принимает progressed states; execution owner
имеет отдельный current-record checker. Raw writer не опубликован.

## Worker entry и private contract

Internal CLI: `--activate-update ABSOLUTE_CONFIG UPDATE_ID ATTEMPT_ID`.
В аргументах нет nonce; updater journal/endpoint выводятся из текущего loaded
runtime и saved attempt, а не из произвольного переданного пути. Runtime files
и config удерживаются read handles с запретом write/delete.

Приём выполняется на отдельном owner thread8MiB с current-thread Tokio runtime,
чтобы graph-heavy startup future не увеличивал ordinary worker main frame.
Это сохраняет урок прежнего native stack overflow original supervisor.

Wire DTO и marker находятся в `crates/protocol/src/update/activation_receiver.rs`:
Challenge/Ready contract1, deny_unknown_fields, максимум32768 bytes до allocation
body. Challenge не реализует Debug; decode/auth errors не печатают private input.
`ActivationReady` перенесён из updater model в protocol без изменения JSON shape;
старый Rust path переэкспортируется, schema18 не изменяется.
Новые private сообщения не зарегистрированы в обычном RPC/MCP/catalog/UI.

Readonly receiver snapshot сверяет весь retained activation graph, текущий
VerifyingLocal root/revision, active installation/epoch, exact Active supervisor,
ResumeGranted и actual child stamp. Snapshot сам не даёт writer permission:
ещё требуются live parent/pipe authentication и nonce.

После Ready child ожидает, удерживая pins, максимум startup timeout. EOF,
неизвестная команда, parent death или timeout завершают receiver без writer.
Partial read не отменяется и не возобновляется при периодическом parent poll.
**State-transfer command ещё не реализована.** Hold timeout после Ready —
ограничение ожидания, а не обновлённый activation grant.

## Ошибки и ресурсы

`ActivationLaunchFailure` сохраняет тот же process/files guard. По возможности
durable uncertainty записывается на текущий actual prefix; ошибка записи не
превращается в no-process receipt. Нельзя десериализовать/клонировать guard,
взять failure как новый Prepared или автоматически replay create/resume.

Drop native owner запрашивает kill-on-close через existing bounded
`close_unconfirmed`; это **не подтверждение empty tree**. Job сохраняет нынешнюю
семантику task/probe cleanup. Lifetime handoff, detach и снятие kill-on-close
здесь не реализованы. Output captures держат максимум64KiB каждого потока,
продолжают drain после лимита и отмечают truncation/read errors.

## Карта изменений

| Место | Назначение |
| --- | --- |
| `crates/protocol/src/update/activation_receiver.rs` | Private v1 DTO/marker, неизменённый ActivationReady receipt shape. |
| `apps/update/src/activation/channel.rs` | Scope/loaded identity validators, bounded framing без secret diagnostics. |
| `apps/update/src/journal/activation.rs` | Readonly child graph admission; private guarded append/current checker. |
| `apps/update/src/runtime_replacement/activation_process.rs` | Consuming suspended OS driver и resident process/failure owner. |
| `apps/worker/src/activation.rs`, `main.rs` | Private pre-writer receiver/owner thread; normal serve прежний. |

## Готовая source fixture после local/cross review

Receiver/driver commit `1c23d48520316af0bf493cbf1be5bf441dffcea3` экспортирован
чистым с built UI:774 files,5,682,595 bytes. Archive в shared sources:
`winbridge-20261024T215731Z-1c23d48-0490a451.zip`.
ZIP SHA256 `08003571809f0733dfa9dedd75d9f86f13de68c96c9affec0dec615a7a811575`,
manifest SHA256 `249205a0d2d9d54ad6254dd260987d639996649a92acd2b7adac3d4e844df2d6`.
Pins в `.winbridge/update-preparation/activation-receiver-fixture18-export.json`
и `activation-receiver-fixture18-pin.json`. Проверены ZIP hash, manifest clean/
commit/UI, source receiver marker/CLI и schema18. **Это source fixture, не
проверенный Windows executable или запуск.** Следующий actor producer должен
иметь другой clean commit; старый target7f6a029 не использовать как receiver.

## Следующий реальный gate

1. Local/cross checks и review завершены, receiver/driver commit и fixture
   сохранены выше. Compilation/model не доказывают настоящий запуск Windows.
2. Использовать clean receiver fixture1c23d48 с built UI выше. Старый target7f6a029
   не содержит receiver и обязан быть отклонён до CreateProcess. Подготовить distinct producer
   с actor dispatch/harness. [Export и окружение](resume-2026-10-24-activation-files.md).
3. [Actor22/view14](activation-process-actor.md) добавлен, frozen21/13 не расширен.
   Move единственного Bound guard в blocking owner; duplicates только observe.
   После success/failure вернуть native owner actor. Older files views должны
   отражать удерживаемые ресурсы, а не исчезать из-за перемещения между owners.
4. Actual Main должен доказать durable Creating до OS, actual PID/Job до resume,
   authenticated Ready, отсутствие Store/dispatch, тот же state lock, unchanged
   worker DB/WAL, один PID/attempt при duplicates, cold no-relaunch. После owned
   Job cleanup доказать exit/tree-empty отдельно; одного Drop недостаточно.
5. Faults на create/receipt/resume/ACK/channel boundaries; wrong nonce/PID/build/
   config/epoch/deadline. Independent verifier и отрицательные mutations; ready
   files проверить дважды. Installed runtime не менять, UI-окон не требуется.

Далее — durable HandoffIssued и acquisition того же lock FileID новым owner,
StateOwned/local verification, actual controller confirmation и lifetime handoff.
Нужность worker schema17 зависит от будущего ownership receipt и ещё не решена;
нынешняя compatibility policy same-schema-only. U2 faults/management, U3 desktop
и U4 actual automatic update остаются открытыми. [Общий план](README.md).

## Проверки этого изменения

Логи и machine receipts: `.winbridge/update-preparation/activation-receiver-*`,
`activation-receiver-checks.json` и `activation-receiver-final-checks.json`.
Workspace721/0 (16 ignored), channel4/0, Linux all-target validation Clippy,
MSVC validation/production all-target Clippy, protocol-jsonschema55/0,
generated contracts/check/TypeScript/UI build/fmt прошли. Final MSVC validation/production Clippy после перестановки Created до reader
allocation тоже прошли. Scaffold266/local links и diff check пройдены.

Review сохранил strict Prepared admission и выявил порядок PID receipt/reader
allocation: actual PID/Job теперь записывается первым, пока child suspended.
Fact после deadline не даёт нового grant; cancel/retry framed read не применяется.
Новый native execution ещё не запускался; compile/lint не подтверждает OS effects.
Installed worker остаётся прежним d052; его update для изолированной проверки
не требуется. Desktop не нужен.
