# A3 — discovery и согласованное наблюдение

Дата: 2026-10-14. База: A2 завершён, `85cf724`.
Общий [план](README.md). A3.1 реализован, [first native gate](windows-discovery-gate-2026-10-14.md)
прошёл (202 tests), финальный scope-guard review fix проверяется повторно;
полный A3 ещё не завершён.

## Последовательность

1. **A3.1, optional catalog:** WorkerInfo несёт поддерживаемые contract IDs и
   точный build provider. Это сведения о реализации, не permission и не evidence
   проверки пары. Hello, closed capabilities, v1 mutations/fingerprints не меняются.
   Неизвестные read-only metadata сохраняются через новые hops; старый hop может
   их отбросить. Missing/malformed/unknown catalog оставляет безопасный legacy режим.
2. **A3.2, readiness:** разделить advertised support, проверку build, проверку
   конкретных consumer/provider/route/contract и разрешение проекта. Evidence
   хранит identity/hash; reconnect/другой build/route не наследует enable.
   Нельзя объявлять paired verification по одиночному compile или WorkerInfo.
3. **A3.3, общий read model:** bounded pages из одной SQLite read transaction,
   scoped OperationRef/revision/status/reason/resources/event cursor. Freshness
   отдельно от сохранённого результата; archive — отдельная область без actions.
   Новые состояния scheduler входят только с A4, отдельную UI FSM не создавать.

## Первый вертикальный пакет

`contractCatalog` — optional JSON metadata. Transport сохраняет неизвестную
read-only форму, typed interpreter принимает только известный schema version,
ограниченное число уникальных contract IDs и build identity, совпадающий с Info.
Для известных entries используются типизированные Rust/TS DTO; unknown metadata
не проходит как известный контракт. Поддержка legacy RPC 1 и fenced RPC 2
advertised отдельно от текущего execution gate/inspection.

CLI/MCP получают catalog через существующий Info; UI показывает его как сведения
о совместимости, не включает новые действия. Verified/pair-tested пока не выдаются:
их receipt store и разрешение появятся отдельным пакетом A3.2. Текущая dispatch
policy A2 остаётся authority, catalog не заменяет pinned request context.

## Ревью плана и проверки

Typed catalog прямо в WorkerInfo мог бы сломать весь Info при неизвестном enum
или новой форме metadata. Поэтому wire metadata открыта, interpreter известной
формы строгий; команды остаются закрытыми. Worker не объявляет собственную
проверку пары. Frozen legacy reader хранится отдельно от нового codec, чтобы
добавление catalog не переписывало историческую compatibility baseline.

Проверки: missing/valid/malformed/future metadata, duplicate/oversized entries,
чужой build, old-reader drop, new-reader forward; реальный worker → WebSocket →
controller IPC → MCP и TS interpreter. Native compiled worker Info и старый
reference Info проверяются на изолированных journals. Full paired enable потребует
actual updated Windows worker на постоянном exe path; старый runtime остаётся
test launcher до этого шага. Gate A3.1 не завершает полный A3.

Cost/rollback: additive metadata без migration БД и новых executable actions.
Удаление catalog возвращает legacy discovery. Будущая negotiation/read model
имеют отдельные версии; metadata не расширяет старый strict command envelope.

## Реализация первого пакета и ревью

285 workspace tests passed; дополнительно known-reader/TS parity проверена на
22 общих fixtures. Real Unix route сохраняет catalog через worker/WebSocket/
controller IPC/MCP. Fake future metadata и explicit null прошли actual WebSocket.
Frozen v1 Info reader скопирован из `a42cd7e` отдельно от current codec: будущие
поля он отбрасывает. Native regular/ignored checks добавлены к full CAS gate;
reference запускается с новым config/state и завершается подтверждённой cleanup.

Ревью исправило explicit-null loss у Option<Value>, разницу JSON integer 1/1.0
между Rust/JS и trailing-newline ID. Normalization относится только к metadata;
command DTOs не менялись. UI показывает compatibility summary внутри details,
offline сведения помечает как последние сохранённые. Typecheck/build и schema/TS
checks прошли; paired enable и consistent read model по-прежнему впереди.

Последующее ревью: omitted nullable build fields нормализуются в UI как Rust None;
24 shared cases. Подтверждённый error reply также обязан иметь исходный journal
scope; чужой/null scope даёт unconfirmed, не разрешает автоматический rebind/retry.
286 workspace tests прошли; final native review gate ожидается.
