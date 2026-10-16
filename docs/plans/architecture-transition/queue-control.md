# A4.2 — bounded root admission и durable pause

2026-10-15. A4.1 installed gate и последующее ревью завершены. Этот план
проверен до реализации; A4.2 пока не считается готовым.

## Контракт и владение

Один worker/Store остаётся execution authority. Новая опциональная startup policy
`queuePolicy: {maxPendingRoots: N, maxPendingRootsPerProject: M}` ограничивает
принятые queued/running roots; оба лимита 1..10000, M optional и не больше N.
Absent policy сохраняет прежний приём. Текущие пользовательские configs не менять.
Снижение лимита не удаляет и не отменяет принятые операции.

Root — обычный task/import или workflow. Child import/build/start не добавляет
второй admission slot; Ready application после terminal workflow сюда не входит.
Uncertain physical owner продолжает занимать work slot A4.1 независимо от этого
счётчика. Прежний safety cap 128 активных workflows сохраняется отдельно.

Schema 13 добавляет immutable root index, singleton queue state и retained
command receipts. Migration 12→13 выполняет consistent SQLite backup; root index
восстанавливается из прежних workflow links, без смены operation IDs/keys.
Старый worker отказывается от новой schema; рабочий journal не мигрировать до
native candidate gate и обычного ручного обновления.

Внешний root admission внутри BEGIN IMMEDIATE: сначала existing-key/payload,
затем pause/capacity, затем record и root index, commit до ACK. Child admission
использует прежние atomic workflow links и не проходит root quota повторно.
Отказ не сохраняет idempotency key и не объявляет задачу принятой. Повтор уже
принятого запроса работает при pause/full queue; changed payload отклоняется.

Типизированная FSM `QueueMode`: running / admission_paused / dispatch_paused /
paused. Состояние сохраняется после restart. Dispatch pause проверяется внутри
task/import claim до launch intent/attempt/lease. Команды, уже получившие claim,
продолжаются; cancel/status/output/readiness callbacks не приостанавливаются.
Workflow root может создать ожидающий child, но новый физический запуск ждёт.
Advisory scheduler preflight сохраняет отзывчивость IPC; он не заменяет claim.

## Изменение пользователем и наблюдение агентом

Независимый typed local IPC envelope `queueControlVersion: 1` с journal context.
Observe может читать текущее состояние и receipt по command ID. Set содержит
command ID, expected decimal revision и целевой QueueMode. В одной транзакции
проверяются duplicate payload и CAS, сохраняются state + receipt. Старый revision
не перетирает новое решение пользователя; exact duplicate возвращает прежнюю
квитанцию вместе с текущим наблюдением. Receipts/keys не очищаются автоматически.

Этот handler регистрируется только в local named pipe/Unix IPC, с прежними SID/
permissions. Он не становится общим dispatcher команд и не расширяет readonly
extension transport. Нет нового сетевого mutation route/MCP pause tool. Это
граница интерфейса пользователя, не sandbox от уже разрешённых direct commands.

Readonly `winbridge.queue-control` v1 metadata содержит provider/context, revision,
mode, policy, pending totals/by project, typed blockers и freshness. MCP Info
передаёт те же данные. Legacy RPC 1/2 остаётся frozen; отказ admission имеет
понятную причину, а typed blockers читаются отдельно. Contract readiness receipt
не гарантирует свободную очередь и не даёт разрешения снять пользовательскую pause.

Desktop показывает count/limit, обе pause отдельно, причину/время/revision.
Кнопки используют immutable command/context/revision. Lost ACK не считается
успехом: доступно наблюдение receipt/повтор только исходного command ID. При stale
revision требуется refresh и новое явное решение; automatic rebase запрещён.
Disconnected/inspection/unknown metadata не позволяют mutation. Browser demo
не получает endpoint управления. Восстановление окна не повторяет команду.

## Проверки и порядок

1. DTO/schema/metadata interpreters + общие Rust/TS fixtures; frozen RPC hashes.
2. Migration/index и root admission: конкурирующие connections, duplicate/full/
   paused, rollback index, task/import/workflow, child exemption, terminal release.
3. State/receipt CAS, replay после restart, changed key, read-only отказ, lost ACK.
4. Local handler/transport/native UI; WS не диспетчеризует этот envelope. Actual
   task продолжает работать при pause, queued child не запускается, cancel/read
   доступны, resume запускает следующую задачу. CPU/IPC latency проверяется.
5. Полный локальный regression, native isolated Windows gate через здоровый
   установленный worker, затем review; один новый source пакет перед installed
   gate. До обновления schema 13/новый контракт не заявляются установленными.

## Повторное ревью плана

Root index выбран вместо повторного JSON-поиска всех child links на каждое
решение: это связывает admission и root marker атомарно и уменьшает стоимость
большой истории. Index/receipts append-only; counts используют actual journal
status, не независимый счётчик, который может разойтись после сбоя. Existing-key
проверяется до quota; CAS/command receipt не заменяют authority/physical leases.
Pause распространяется на выдачу нового claim, а не на уже запущенный owner.
Отдельный local envelope сохраняет readonly контракт A3.3 и не требует брокера,
Actor library или переделки execution foundation. Для будущего независимого
Store writer нужна та же persisted policy generation, что описана в A4.1/A4.3.


## Кандидат и дополнительное ревью — 2026-10-16

Реализованы DTO/metadata reader, core admission policy, schema 13/root index,
transactional root admission, durable QueueMode/CAS/receipts, local-only handler,
transport/schema guard и desktop controls. Сохранение pending command до send
проверяется readback; reopen не исполняет запрос. Receipt связывается с исходным
command/context/revision; историческая квитанция не подменяет текущую revision.

В initial local regression прошли 336 workspace tests; добавлен отдельный receipt
binding regression. 46 общих Rust/TS cases, typecheck/build, Linux/Windows Clippy,
actual Unix queue UI и полный прежний browser regression прошли. Fixtures ручного
понижения schema обновлены: они удаляют schema-13 objects из synthetic DB.
Найдена и исправлена очередь удалённого project при fairness; записанная задача
получает обычный policy failure вместо вечного ожидания. Native tests сохраняют
queue proof, включают explicit previous-worker schema refusal с тайм-аутом и
новой тестовой БД. Старый exe сохраняется как checksummed local cache для будущих
повторов после обновления runtime; Windows каталоги не удаляются.

До native gate кандидат не считается установленным; рабочие configs/journal
остаются прежними. Для повторного native gate после перехода установленного
worker на schema 13 передавать `--queue-reference-worker` с сохранённым schema-12
exe из успешного report.queueReferenceWorker, не новый рабочий runtime.
