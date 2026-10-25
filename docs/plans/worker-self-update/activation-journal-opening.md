# Existing-state connection для StateOwned receiver

Актуальное продолжение: [ledger19/private writer3](journal-writer-runtime.md)
уже вызывает эту library из worker private handler. Actor24 и actual native
writer gate ещё впереди. Остальной текст фиксирует component checkpoint f5dcec1.

2026-10-25 America/New_York. Продолжение worker schema17
[atomic journal activation](resume-2026-10-25-journal-activation.md).
**Trusted library реализована; private receiver/actor пока её не вызывают.**
Updater18/worker17/supervisor23/view15/private physical2 не меняются. Новый
private writer opcode/marker и public activation support не объявляются.

## Что обеспечивает новый объект

`ActivationJournal` в `apps/worker/src/store/activation_open.rs` потребляет
**уже полученный** `LockedShutdownState`, а не path/JSON proof. Внутри одного
non-Clone/non-Serde owner находятся SQLite Store, прежние state pins/instance
lock, immutable command/accepted shutdown и признак единственной попытки.

Обычного Store extraction/Deref/serve/dispatch API нет. SQLite закрывается до
освобождения lock. `ActivationJournalOpenFailure` при ошибке сохраняет тот же
lock и pins; предоставляет причину/observation/revalidate, но не reusable
startup permit или неограниченный writer. После ошибки самой активации Store
и lock остаются в original object для наблюдения/reconciliation.

Порядок открытия:

1. Exact command/shutdown/schema/physical state scope, исходное wall-clock
   окно и held FileID; безопасные WAL/SHM/rollback-journal paths.
2. Live checkpoint callback; readonly existing connection. В одном SQLite
   snapshot проверяются exact schema17, существующий WAL mode, old authority
   stamp, полный Quiesced lease/accepted shutdown и отсутствие прежней
   activation этого update.
3. Повторные state/live/window checks. RW open **без SQLITE_OPEN_CREATE**;
   никакого generic `Store::open`, миграции, mode assignment или
   `begin_authority`. Сначала connection остаётся query_only.
4. Повтор всего DB binding на новом connection и сверка с readonly header;
   ещё один live/state/window checkpoint. Только connection-local настройки
   synchronous=FULL/foreign_keys=ON/query_only=OFF.

Само открытие не меняет authority/epoch и не снимает maintenance. Constructor
доступен доверенному Rust-коду, но не доказывает authenticated child или current
updater graph: callback будущего private driver обязан проверять exact child,
parent, held runtime/config, текущую authority/revision и **original monotonic
deadline**. JSON digest и один instance lock этого не заменяют.

`activate` фиксирует попытку **до** обращения к SQLite и повторяет state/live
checks вокруг существующего atomic header-CAS/receipt method. Повтор после
успеха только читает Observed receipt, без callback/новой epoch. После ошибки
без receipt второй transaction attempt запрещён, включая известный rollback:
дальше observe/reconcile. Cold reopening по старой receipt не создаёт нового
owner. Пока нет API передачи Store в обычный Worker/service; это следующий
проверяемый runtime шаг после trusted activation admission.

## Review: скрытая запись при закрытии последнего connection

Первые семь tests прошли, но удерживали fixture connection старого Store.
Review выявил непокрытый случай **реально закрытого старого SQLite connection**.
SQLite по умолчанию может выполнить checkpoint и удалить WAL при закрытии
последнего connection. Это документированное поведение
[SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE](https://www.sqlite.org/c3ref/c_dbconfig_defensive.html#sqlitedbconfignockptonclose).

Добавленный regression test **сначала воспроизвёл ошибку**:
`Unactivated connection Drop changed DB/WAL`. То есть даже без authority CAS
подготовленный и закрытый новый connection менял retained source DB/WAL.
Лог `.winbridge/update-preparation/activation-open-last-connection-before-fix.log`
содержит terminal101/1failed; installed worker не затрагивался.

Исправление: обе existing connections, RO и RW, устанавливают и проверяют
`SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE=true` **до первого schema/database query**.
Ошибка неподдержанной настройки не продолжается как успешное opening.
Регрессия проверяет byte-for-byte DB/WAL после constructor, после Drop без
активации и после final-check failure, когда других SQLite connections нет.
Это connection policy данного guarded startup, не глобальное изменение SQLite
или политики обычного worker. Не отключать её до отдельного проверенного
handoff к normal runtime.

## Проверки и их границы

Pipeline завершён: **focused8/0, workspace768/0 (16 ignored)**, Linux/MSVC
all-target validation Clippy с warnings=errors и fmt passed. Scaffold/links и
diff check также passed. Логи `.winbridge/update-preparation/activation-open-*-checked.log`,
summary `activation-open-checked.json`; source inspection в
[общем checkpoint](resume-2026-10-25-journal-activation.md).

Focused tests используют реальные SQLite transactions и OS file locks, synthetic
child/graph metadata. Проверяют exact lock/bytes/header/maintenance, stale/foreign
scope, schema16/18 и DELETE-mode refusal без migration/mode change, все три
preparation checkpoint failures, rollback без повторной попытки, cold receipt
refusal и последний connection/WAL. Unix дополнительно проверяет missing/rebound
main DB без CREATE. Это не доказательство настоящего Windows child activation;
Windows-specific sharing ещё проверяется combined native writer gate.

Fresh disk query task `04d9b81a-35be-4ff0-80b2-b4c0c83ac1bf` terminal succeeded/0:
Windows13791174656 / Debian25228640256 bytes перед checks. Receipt
`.winbridge/update-preparation/activation-open-disk-20261025T144014Z-abe77b85.json`.
Рабочий d052/runtime не заменялся; desktop/новых UI окон нет. После terminal
проверок desktop можно закрыть, installed worker оставить работать.
Fresh Info2026-10-25T14:51:03Z: прежний generation8f8d1fb9-e5b1-4984-a17e-d067db46268e,
controller connected, active0, maintenance/recovery errors=null, coverage.complete=true.
Выборочная запись `activation-open-post-info.json` в том же local evidence directory.
Debian после checks25223417856 bytes, cleanup не понадобился. Cargo terminal/exit0,
новых native runtime tasks нет; disk task завершён.

## Следующий шаг

Теперь нужен **graph-bound updater writer intent/immutable receipt ledger** и
следующий private command/handler, который потребляет тот же resident Owned
guard и создаёт `ActivationJournal` с настоящими live checks. Expected old stamp
берётся из verified final backup в том же updater snapshot. Ledger/schema,
contract/marker и actor/view versioning вводятся явно, старые формы frozen.

После source checks/review — новая clean receiver17 fixture, distinct producer
и один combined native writer gate, включая actual worker16→17/refusal.
Старый target1370d6b/worker16 и physical132/0 не доказывают этот компонент.
Original120s deadline не продлевать; expensive immutable audits делать после
receipt. Затем local/controller/lifetime/commit и remaining U2/U3/U4.
