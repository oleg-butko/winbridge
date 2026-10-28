# Следующий сценарий: controller session после LocalVerified

2026-10-28. Main25 и actual retained19→20 пройдены; worker0.2.0 установлен,
controller.203/.204 доступны. Продолжение U2 без нового installed worker до тех
пор, пока существующий worker служит исправным исполнителем изолированных gates.
Public install/whole update остаётся выключенным.

## Текущая реализация

Добавлены strict DTO в protocol/update/activation_controller.rs и TypeScript
bindings. Pure activation/controller связывает frozen physical6/writer/service
с exact LocalVerified7, сохраняет original deadline и typed history
Issued→Confirmed либо Issued→Indeterminate→Confirmed.10 focused tests прошли.
Это data foundation: network send/runtime/SQL ledger/capability ещё не добавлены,
updater20/actor25/view17/service4 и installed worker не меняются.

Review foundation выявил, что projection сам по себе не содержит Welcome nodeId:
добавлена отдельная проверка current normal-IPC link/attempt/node/session и
lastConnectedAt. Full frame проверяется отдельно по digest/sequence/build/context.
В test matrix есть stale reconnect, coordinated graph/config drift и чужой peer;
никакой новый connector или public permission из этих данных не создаётся.

Проверки foundation:255 updater library tests/4 штатно ignored и75 protocol
all-target tests (включая frozen legacy wire/schema); после дополнительного
positive secondary-endpoint case все10 focused tests passed. Linux/MSVC workspace
all-target/update-validation Clippy прошёл, TypeScript/generated contracts/fmt/
scaffold312 также. Windows исполнения нового runtime ещё нет: runtime отсутствует.
Исходные controller.203 f069697 и worker48c834c продолжают работать независимо от
изменяемого build cache. Evidence `.winbridge/review-2026-10-28/`.

Следующий source шаг — journal21 ledger: три immutable controller tables с FK к
service record, exact graph/readback и CAS history; old schemas возвращают None.
При fresh admission проверять AwaitingController/root revision=source.parent+1
и original local7/nonce/owner (старый activation::owned_current принимает только
VerifyingLocal и здесь неприменим). Duplicate key + другая frozen selection —
KeyConflict; Existing не выдаёт permit. Для history читать actual physical6 и
local7 из immutable history, не восстанавливать их из более поздней строки.
Нормальный open20→21 — consistent backup/epoch fence; synthetic old fixtures
удаляют новые пустые таблицы перед relabel, actual old20 берётся из retained
Main25 archive. Frozen receiver scope20 после migration не получает grant21.

## Выбранные решения

- Подключить обычный network manager к **тому же** resident local-service Worker.
  Config, Store, lock, loaded PID/birth/build, incarnation/generation и committed
  authority остаются прежними; повторный open/migration/begin_authority запрещён.
- Использовать существующий validated ControllerReadinessProjection как ответ
  controller на actual normal Info. Worker уже проверяет ticket, session,
  controller generation, свой build/generation/context и sequence. Новый сетевой
  протокол/отдельный callback listener не нужен. Readiness views/verifications
  в projection не дают permission и не должны автоматически включаться.
- Сохранить exact1–4 configured endpoints и отдельный confirmation endpoint.
  По умолчанию выбирается configured primary; trusted policy может явно выбрать
  другой configured ID. Selection замораживается в command и сверяется с config
  SHA. Payload не добавляет адреса и не меняет primary. Другие связи работают
  независимо; их offline не подменяется global connected и не блокирует
  подтверждение явно выбранной связи. Это не новая граница доверия по IP.
- Intent до effect, ровно один network-start dispatch под original resident
  guard. Все reconnect attempts остаются внутри одного command/original window;
  наблюдение не посылает команду заново. Успех требует свежего connectedAt после
  issuedAt, current ticket и принятого projection именно выбранной связи.
- Receipt содержит immutable service/local hashes, тот же compatibility/authority
  и stamp controller/provider/session/context/sequence плюс SHA full projection.
  Независимый gate сохраняет исходный frame для проверки digest. Timestamp и JSON
  сами по себе не доказывают текущую residency/freshness: нужны live owners.
- Late receipt разрешает уточнить сохранённую неопределённость, не возобновляет
  deadline, не запускает cold network и не даёт commit/release. Roots, dispatch,
  admin и user pause сохраняют прежние границы.

## Последовательность

1. Strict DTO + pure command/receipt/typed FSM. Никакого advertised capability,
   runtime send, DB schema bump или public endpoint на этом шаге.
2. Review foundation и tests:1–4 endpoints, exact config selection, deep scope,
   duplicate/uncertainty/late fact, stale/foreign projection и неизменные старые DTO.
3. Durable ledger и оригинальное guarded admission: предполагаемый updater21,
   empty new tables при actual retained20→21 copy migration/old20 refusal.
   Номера пока проектируемые, не выдавать их за реализованные контракты.
4. Same-child consuming network start/normal IPC observation с bounded original
   deadline, rollback владения при fail и proper join network tasks при close.
   Только original local-service owner; никакого конструктора из archived JSON.
5. Trusted sender/receiver/actor (предполагаемые private5/actor26/view18); frozen
   прежние версии. Parent независимо читает actual normal IPC/selected controller
   projection и проверяет тот же PID/build/config/authority/session.
6. Новый full native run в stable runtime-test-204, controller.203:1144, отдельные
   instance/state/report и старый retained worker17 reference (не installed0.2.0).
   До admission actual network preflight с900s firewall budget; original startup
   window240s не продлевается. Сохранённые Main25 не replay-ить.
7. Review→fix→Job lifetime transfer→whole update commit/scoped maintenance release.
   U2 faults/management, U3 desktop, U4 installed self-update и A4.3 сохраняются.

Controller receipt не вызывает старый root event ControllerConfirmed сразу:
он ведёт AwaitingController→Committed, тогда как Job lifetime ещё не передан.
До отдельного trusted commit сохранять root AwaitingController, physical
LocalVerified7 и закрытые roots; новая controller ledger/view показывает только
свой факт. Не добавлять physical8 или менять frozen старые view DTO ради этого.

## Проверки

Portable модель отделяет valid data от live grant; core подтверждает LocalVerified7
из exact frozen StateOwned6/writer/service. Подмены owner/process/build/config/
authority/session/selected endpoint/projection sequence отклоняются. Неверный hash
full frame, старый connected timestamp и другая связь не становятся подтверждением.
При uncertainty нет automatic replay; новые keys не продлевают исходное окно.
SQL tests проверяют одну транзакцию полного graph и immutable history. Runtime
tests — same Arc/Store/lock/epoch,1–4 sessions, stale tickets и startup cancellation.
Native gate — actual controller Info/projection, lost ACK/cold observe/no replay,
сохранённые tasks/output, закрытые roots и завершение всех owned Jobs.
