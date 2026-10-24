# Required production drain: schema17 / supervisor19 / view11

**Actual Main/migration gate2026-10-24 пройден:** [33 native tests](windows-compatible-drain-gate-2026-10-24.md),
27 ready files twice/38+19 mutations; production Blocked/duplicate/legacy rejection,
BeginDrain0 и actual16→17/refusal подтверждены. Две предыдущие неуспешные попытки
сохранены; dedicated owner thread8MiB проверен actual Main. Далее activation/U2/U3/U4.
Ниже — описание контракта и история подготовки gate, не текущие задания к запуску.

Текущая проверка2026-10-24: [первый native run](windows-compatible-drain-first-attempt-2026-10-24.md)
дал32 preliminary tests и actual16→17/refusal, но Main не дошёл до нового entry:
stack overflow isolated supervisor во время health admission. Исправляется
heap constructor mailbox future, затем нужен новый full Main run. Нижнее
описание «не выполнен» относится к состоянию перед первой попыткой.

2026-10-23. Продолжение [support actor18](candidate-support-actor.md).
Новый маршрут DrainCompatibleCandidate/ObserveCompatibleDrain связывает допуск
с точным завершённым support attempt. Public install и activation выключены.

Локальные проверки завершены: workspace688/0 (16 ignored), model/SQL7,
actual local IPC1, protocol-jsonschema51, обе all-target Clippy update/worker/
protocol, contracts/TS/UI/Python/docs прошли. Windows gate нового route **не
запускался**. [Полная инструкция следующей сессии](resume-2026-10-23-compatible-drain.md)
содержит exact reference pins, команды и дальнейший план.

Заключительный review observer дополнительно требует exact passive decision
shape, отдельный UUID операции, согласованные timestamps/reason и duplicate/cold
view11 без вымышленных authority fields. Main audit расширен с32 до38 negative
mutations; migration audit остаётся19. Старый16 режим сохраняет28 mutations.

## Данные и транзакции

compatible_drain_records хранит одну immutable decision на update:
Blocked с исходным outcome/assessment либо Admitted с exact drain attempt.
Сохраняются отдельный Operation ID, support attempt/revision/domain-separated
SHA256, owner/epoch/parent revision и timestamp. FK связывают support record
и, только при Admitted, существующий drain outbox. Pending query не становится
terminal blocker. Unknown/unsupported/checked-with-blockers не разрешают drain.

Blocked записывается без root transition и без drain intent/commands.
Admitted сохраняется в том же advance_with_driver transaction callback, что
создаёт frozen DrainIntent1, три command outbox entries и root BeginDrain edge.
Ignored write/readback error откатывают всё. Ни JSON, ни cold read не создают
fresh permit. DrainPermit удерживает sealed production binding только в памяти.

После выбора production route для update legacy v13 не может обойти Blocked
или переобозначить чужой outbox как production. Старые payload/action shapes
сохраняются; старые внутренние сценарии без выбранного нового route работают
по прежнему контракту. Delivery callbacks сверяют точный bound record.

Перед положительным решением blocking bridge проверяет current executor/
schema/owner/root, live lease, source/health/physical cleanup, compiler output,
artifacts/config/worker. Exact pins удерживаются через admission; consuming
drain driver повторно приобретает/проверяет их перед RPC. Объявление future
drivers в JSON не включает отсутствующий actual driver.

## Actor и наблюдение

Новый19/view11 использует тот же единственный blocking slot. Admission вне
mailbox, durable decision до ACK, dispatch только по fresh permit. Lost ACK
не отзывает принятое решение; duplicate возвращает тот же record. Pending
admission показывает Admitting; Blocked содержит assessment и source reason.
Abort использует drain-active coordination и ждёт завершения callback.
Views1–10 не получают compatibleDrain.

## Карта кода

- apps/update/src/compatible_drain.rs: typed decision/model/digest/preflight.
- apps/update/src/journal/compatible_drain.rs: schema17, scoped read/decision,
  immutable FK link; journal/drain.rs: atomic callback и bound delivery.
- apps/update/src/preparation.rs и supervisor.rs: bridge/actor/client.
- crates/protocol/src/update/compatible_drain.rs: strict passive view.
- Synthetic downgrade fixtures удаляют dependent schema17 table первыми.
- Tests: model3/SQL4 (включая16→17 backup/fencing), protocol19 shape и actual
  IPC rejection without support. Положительная future metadata в model test
  не утверждает наличие OS effects или реализованных activation drivers.

## Автоматический Windows gate — подготовлен, не выполнен

Explicit --compatible-drain / -CompatibleDrainGate выбирает schema17 на обеих
сторонах, новый proof kind и productionDrainEnforced=true. Старый schema16
support gate сохраняет свой формат и успешно перепроверен readonly.

Combined gate использует33 native tests в10 groups: прежние22 prerequisites,
новые model/SQL7 +contract1 +mailbox1 +actual migration1 и Main1.
Native Main должен вызвать новый entry, сохранить Blocked, повторить запрос,
попытаться legacy fallback и подтвердить zero BeginDrain, SourceHeld revision1,
normal IPC и unchanged old images. Затем explicit fixture abort/cold history.

Migration16→17 идёт только на новой SQLite Backup API копии successful retained
support16 history. Original DB/nonempty WAL/image/manifest сохраняются; backup,
epoch, старые rows/record и refusal actual old16 init/status проверяются.
Новый decision/permit из исторических данных не появляется.

На момент этого документа Windows execution нового19 route и actual migration
ещё впереди. Local/cross tests не заменяют этот gate. Нельзя объявлять whole
updater готовым: activation/local health/controller confirmation, remaining U2
faults, desktop U3 и public/bootstrap U4 остаются незавершёнными.

## Последовательность после локальных проверок

1. Review + full workspace/Clippy/contracts, clean commit и source export17
   как ограниченная candidate fixture (не production-ready target).
2. Новый clean producer commit/export17 с distinct source identity; одна
   isolated Windows задача с target fixture17 и reference pin successful16
   run20261023-233438-99674bc1. Worker only, no UI.
3. Terminal task + ready files twice + independent main/migration mutation audit;
   при observation timeout читать тот же Task ID, не submit заново.
4. После gate review продолжить authorized activation из resident replacement
   guard. Current query1/backup15/preparation16/replacement17 contracts frozen;
   execution capability рекламировать лишь после реального driver/gate.
