# Следующий сценарий: normal local service после writer

2026-10-26. **Data foundation и journal20 format/readback реализованы; runtime service ещё отсутствует.**
Реализация начата после полного положительного Main24 и независимого review.
[Текущий run и результаты](review-2026-10-25-latency.md).

После positive Main24 добавлена **только data foundation**:
`crates/protocol/src/update/activation_service.rs` и
`apps/update/src/activation/service{.rs,/record.rs,/tests.rs}`. Strict command/
receipt DTO, binding к confirmed writer/физическому6 и pure FSM Issued→Confirmed
либо Issued→Indeterminate→Confirmed реализованы;7 portable regressions passed.
Дополнительный epoch, чужие PID/build/config/generation, refresh deadline и reissue
отклоняются. Late receipt — сохранённый факт, не новый grant.
**Journal20 format/readback добавлен**: [подробный checkpoint](service-ledger-2026-10-26.md).
Original-guard admission/native driver/receiver4/actor25/view17/actual service пока нет.
Ни Store, ни IPC этот код не открывает; private capability marker не добавлен.
Следующий source шаг — consuming durable admission под original Written guard;
примитивы записи journal20 пока доступны только тестам.
Проверки первоначальной data foundation2026-10-25: workspace791/0 (16 ignored), focused service7/0,
Linux/MSVC workspace all-target validation Clippy, TypeScript feature и generated
UI contracts/fmt/scaffold281 passed. После journal20: workspace804/0 (17 ignored),
Linux/MSVC all-target Clippy/fmt/scaffold290 passed. Native Main25 не запускался:
runtime не добавлен.

## Граница сценария

Тот же private child после fresh worker17 CAS/receipt начинает reconciliation и
обычный local IPC. Его incarnation, epoch и state-owned generation остаются
прежними; project roots/dispatch закрыты существующей maintenance lease.
Новая controller session и передача Job lifetime остаются отдельными шагами.
LocalVerified не означает Committed или готовое самообновление.

Для этого выбран отдельный service intent/history в **updater20**, private
service4, actor25/view17. Schema20 формат реализован; остальные номера пока
проектируемые, не текущие CLI actions или
advertised capabilities. Worker journal остаётся17: локальный service не требует
новой authority или миграции active worker state. Существующие views1–16 и
private1/2/3 сохраняют формы и семантику.

## Владение и admission

1. Factory связывает confirmed writer receipt и immutable physical StateOwned6,
   final backup, old accepted shutdown и текущий root/owner/supervisor в одной
   SQLite read transaction. Холодная строка даёт только read model.
2. Только resident Written guard с тем же child/Job/channel получает consuming
   service dispatch. Intent сохраняется до одной private send. Новый command ID
   не превращает уже начатую попытку в новую.
3. Private child сверяет command/graph, config/runtime pins, PID/birth, generation,
   schema/protocol, parent liveness и remaining original startup deadline.
   Новая фаза не обновляет admittedAt и не начинает новые240s.
4. Worker constructor принимает **сам ActivationJournal вместе с Fresh permit**.
   Он не вызывает Store::open, begin_authority, миграцию или повторный try_lock.
   Нужен crate-private consuming transfer с exact receipt/header/state проверками;
   нельзя открывать public способ извлечь Store/lock из serialized receipts.
5. Worker удерживает полный LockedShutdownState, включая pinned anchors, весь
   срок службы. Одного скопированного File handle недостаточно для прежней custody.
   Ошибка подготовки не возвращает reusable permit и сохраняет ownership до exit.

## Сервис и фактическая готовность

- Переиспользовать recovery/reconciliation существующего Worker; выделить общий
  constructor из open_inner, отделив получение state/authority от initialization.
  Не копировать scheduler, task service или IPC handler во второй implementation.
- Config/path/domain/witness проверки и прежние user pause/settings сохраняются.
  Recover/import/application observations выполняются до первой IPC ready receipt.
  Maintenance Quiesced продолжает блокировать roots/dispatch; восстановление
  queued tasks не запускает их. Не отключать её через изменение user queue pause.
- Local-only startup не запускает network::connect. Наличие controllerUrl в
  frozen config не является разрешением открыть controller session. Info должен
  объяснять ожидание следующей фазы, а не показывать вымышленную свежую связь.
- Вынести фактический IPC bind/readiness из serve_inner: Ready посылается после
  успешного bind, reconciliation и доступного обычного readonly handler.
- Родитель проверяет **настоящий normal IPC**, PID сервера pipe, loaded
  build/manifest/config/context/generation/schema и прежние tasks/output. Частный
  Ready frame или тестовый controllerUrl=None не заменяют эту проверку.
- Только подтверждённая immutable service receipt разрешает trusted LocalVerified
  в activation7/root LocalVerified. Lost ACK/timeout/partial write сохраняют
  неопределённый исход; late факт допустим, повтор send или новый child запрещены.

## Обязательные проверки

Portable DTO/FSM/ledger tests: duplicate/conflicting keys; owner/root/schema drift;
unknown outcomes; historical prefix/late receipt; legacy view JSON; cold observe
не получает grant. Constructor tests: тот же Store/header epoch+1, retained lock,
отсутствие normal open/migration; user pause и Quiesced lease сохраняются.

Новый isolated Main25: Main shutdown → backup → replacement → same child/lock →
writer → normal IPC/reconciliation → old task/output reads и blocked new roots →
duplicate/lost ACK/cold observe → actual child/Job exit. Тяжёлые аудиты после
startup receipt; timings обязательны. Actual updater19→20/refusal проверять на
consistent copies retained writer journal, сохраняя original history/evidence.

Public install и advertised activation/controller/desktop остаются выключенными.
После positive service gate: новая реальная controller session → durable scoped
activation Job lifetime transfer → commit/scoped maintenance release → remaining
U2 faults/management → U3 desktop → U4 installed update → возврат к A4.3.
