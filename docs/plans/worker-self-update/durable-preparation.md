# U1: независимый журнал и проверенная подготовка

2026-10-17. Это часть [плана U0–U4](README.md), не готовое самообновление.
Рабочие Windows source/exe/config не заменяются; updater не открывает worker journal.
Штатные записи validation tasks сохраняет сам worker. Native gate этого
пакета отмечается отдельно после выполнения, не по результату Linux tests.

## Реализованная граница

`apps/update/src/journal.rs` хранит запросы и переходы в отдельной SQLite schema 1
с application ID WBUP. Worker journal schema 14 и его migration не затрагиваются.
Installation ID создаётся один раз; scope SHA закрепляет canonical source/runtime
и фактический путь worker config. Содержимое config/source и worker build задают
отдельные preconditions запроса, чтобы изменение настроек не меняло installation ID.

- Acceptance: IMMEDIATE transaction → duplicate lookup → payload conflict check →
  scope/current preconditions → single active update → Requested + revision 0 + event.
  Commit выполняется до возврата ACK. Повтор возвращает текущий record прежнего ID,
  даже если runtime уже изменился или действует admission barrier.
- Lifecycle command: собственный key + expected revision + typed event + readable
  reason. State/revision/event/immutable result receipt сохраняются одной transaction.
  Повтор команды возвращает её первоначальный snapshot, даже если update ушёл дальше.
  Проверяются affected rows и actual readback перед commit/ACK: silently ignored
  insert/update или изменённое событие не создают ложного подтверждения.
- Installation-wide sequence отличается от revision одного update. Bounded history
  page и readonly snapshot читаются в одной SQLite read transaction.
- OS exclusive lock и durable owner epoch/UUID используются вместе. Новый владелец
  не появляется по TTL. Callback прежнего epoch отклоняется, включая duplicate.
  После crash новый владелец получает прежний pending record; update не запускается
  заново и не объявляется завершённым из-за освобождения lock.
- Committed/Deferred/FailedBeforeCutover разрешают новый осознанный request с новым
  key. ManualIntervention остаётся installation blocker. Keys/history не очищаются.
- Более новая/чужая/неверсионированная непустая БД отклоняется до WAL/migration writes.
  DB/sidecar/lock links и неверные record/index/history identities дают отказ.

`UpdateRequest` содержит ZIP filename, archive/manifest/source hashes и expected
installation/build/config/baseline; request не назначает local installation paths.
Lifecycle events — внутренний контракт доверенного driver. Agent не может прислать
`dispatchClosed: true` как разрешение на замену. Acceptance/advance пока доступны
только library driver/tests; installation RPC/CLI/MCP submission не объявляются.

## ZIP и staging

`apps/update/src/archive.rs` проверяет готовый source ZIP целиком без распаковки:
512 MiB archive, bounded EOCD/directory до allocation, count из EOCD отдельно от
ZIP parser, только Stored/Deflate, без ZIP64/multipart/prefix/overlap/extra entries,
directories, symlinks или encryption. Manifest ограничивает per-file и total bytes,
Windows paths/case collisions/clean source; actual decompressed bytes и SHA проверяются
streaming. На Windows read handle запрещает изменение/удаление ZIP во время parsing;
после чтения повторно сверяется archive hash.

Library staging создаёт только новый локальный каталог: pinned copy source.zip,
create-only source files, full source readback, затем source-ready.json.
Существующий/частичный stage сохраняется и не переиспользуется автоматически.
Marker доказывает подготовку, но не даёт права менять рабочий src. Реальный source
transaction требует отдельного worker source lease, durable preimages/intents/receipts
и полного reconciliation. Никакого apply через доверие произвольному JSON proof нет.

## Доступные команды

`verify-archive ABSOLUTE_ZIP ARCHIVE_SHA256 MANIFEST_SHA256` — readonly, без config.
`[--config PATH] journal init` создаёт только runtime/update-state/updates.sqlite
и стабильную installation metadata. Повтор init сохраняет ID/epoch/history.
`[--config PATH] journal status [UPDATE_ID]` читает active либо выбранный update,
не захватывая owner lock. Отсутствующая БД не создаётся при status.

Эти команды нужны для диагностики/будущего supervisor; пользователю не требуется
вручную выполнять их для текущих автоматических тестов. Ни одна CLI команда
не принимает install request, не меняет source/exe и не выключает процессы.

## Проверяемые риски

Tests: lost ACK/reopen, competing connections/admission/CAS, atomic rollback через
SQL fault injection, stale owner, busy OS lock, abrupt child process exit, manual
recovery blocker, key retention, unsupported DB unchanged, readonly inspection,
corrupt aggregates, linked lock и CLI preservation. ZIP tests: Stored/Deflate,
hash/commit corruption, duplicate raw names, unknown/traversal/case/directory paths,
oversized/dirty source, symlink, ZIP64/directory allocation limits, create-only stage.

Automatic Windows harness выполняет эти tests, проверяет actual exported ZIP
новым verifier, собирает четыре exe в новый candidate и публикует checksummed proofs.
Переиспользуются только доказанные test/build compiler caches под exclusive leases;
source/test state/staging/reports всегда новые. Рабочий worker остаётся запущенным,
desktop не нужен, UI-окон нет. Это не проверка production handoff/shutdown/apply.

## Следующее расширение

До первого live source write нужны worker source lease и per-file transaction.
Отдельная durable фаза SourceApplying уже введена; сбой внутри неё блокирует новую установку
до reconciliation, хотя рабочие binaries ещё прежние. Затем infrastructure spawn
вне Job Objects, worker maintenance/drain/quiesce/startup guard и IPC receipts.
U1 целиком остаётся открытым; U2–U4 и первоначальный bootstrap впереди.
