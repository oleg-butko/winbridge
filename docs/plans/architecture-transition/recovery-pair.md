# A2.4.2b.2/b.3 — архив и новая execution pair

Дата: 2026-10-14. После [request fence b.1](fenced-requests.md).
Решение пользователя: прежняя история как архив, новые instance/incarnation,
явное переподключение, contract 2. Local/browser gates и
[197 native tests/desktop compile](windows-recovery-pair-gate-2026-10-14.md) прошли.
Canonical drive prefix bug исправлен. b.2/b.3 и полный A2 завершены.

## Архив, подготовка и activation

Source переводится в latched `serve-recovery`. После закрытия inspection
`prepare-recovery NEW_DIRECTORY` под exclusive worker lock делает consistent
SQLite backup API snapshot и создаёт отдельный пустой schema-12 journal. История,
raw payload, keys и output остаются в архиве; active queue/leases/attempts не
переносятся. Archive bytes могут отличаться от source layout; raw rows проверены.

Target получает UUID instance ID, новую incarnation, необратимый require_fenced,
epoch 0 и отдельный config. Controller URL отключён до явной настройки новой пары.
Exe paths постоянны; config пользователя не перезаписывается. Destination —
новый local directory вне source и зарегистрированных project workspaces.
Traversal, links/reparse, hardlinks и занятый source отвергаются до записи.

Gate сохраняется **до** создания target journal. Preparation DTO содержит typed
Prepared/Blocked observation, archive SHA-256, contexts и witness fingerprints.
Partial files остаются blocked. Activation — отдельное действие с новой physical
check/source lock и create-only receipt; повтор того же результата idempotent.
Старый Blocked может стать активируемым после остановки процессов; Prepared само
не является permission. Normal startup проверяет receipt, context/admission,
hashes, retirement и physical domain **до** Store::open/owner activation.

Минимальный schema-12 admission-header reader отделён от current-schema runtime
reader: последующие миграции смогут сохранять старые recovery receipts без
миграции архивной БД. Проекция старых archive schemas — задача read model A3.

## Whole execution domain

Restore может потерять целые записи: inventory не является полнотой процессов.
Снаружи SQLite хранится create-only `execution-domain.json` с bootstrap basis,
context/path и первым контейнером. Новые owner members добавляются в
`execution-domains/`; sync/readback выполняются **до** IPC/dispatch и запуска.
Каждый durable owner UUID/logon session имеет отдельный global outer Job.
Он назначается CREATE_SUSPENDED child **перед** per-task Job; worker сам в него
не входит. Все managed CreateProcess descendants остаются в общей области.

Один Job навсегда не используется: Job связан с session, а nesting зависит от
родительской hierarchy. Новый owner создаёт новый контейнер, recovery через
global namespace проверяет **все** контейнеры. PID, disconnect, новая БД и
отсутствующий row не дают proof. Query-only результат: Empty/Occupied/Unknown.
Другой host, partial/foreign member или отсутствие completeness не разрешают
activation. Ordinary reconciliation и project leases сохраняют прежний смысл.

Windows API основания: [nested jobs](https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs)
задают root→child order и aggregate accounting;
[global namespace](https://learn.microsoft.com/en-us/windows/win32/termserv/kernel-object-namespaces)
доступен для query из другой session;
[CreateJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-createjobobjectw)
описывает session binding/default creator ACL;
[Job lifecycle](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
связывает уничтожение с закрытием handles и завершением связанных процессов.
Firewall/services/privileges не меняются.

Complete bootstrap допустим для fresh journal либо обычного **не восстановленного**
startup current journal без незавершённых прежних владельцев, после reconciliation.
Legacy unresolved даёт Uncovered; inspection не создаёт missing witness по пустой
БД. Unix adapter не подтверждает Windows cleanup. Это managed process-tree
contract, не sandbox или учёт внешних service/WMI effects. Ручной rollback всех
state files, потеря registry, перенос/clone host не обнаруживаются автоматически.
Restore выполняется после latch, сохраняя внешний witness. Лимит — 4096 owners;
безопасная compaction станет отдельной функцией, удалять registry произвольно нельзя.

## Повторное восстановление и UI

`journal-recovery.required` — preparation gate, сохраняется после activation.
`journal-source-retired.required` — постоянная source retirement, всегда важнее
прежнего receipt. Поэтому A→B→C не позволяет старому B снова запуститься.
Удаление gate у подготовленной пары также не разрешает normal start.

Optional Info.recoveryCoverage и inspection UI объясняют область, архив,
отдельную пару и явное переподключение. CLI recovery — local operation, новый
remote maintenance RPC не добавлен. Old requests не перепривязываются;
отсутствие key в архиве не означает, что эффекта не было. Archive viewer — A3.

## Ревью и evidence

Исправлены повторный receipt у retired source, missing gate, foreign witness до
epoch, member scope bindings, session/hierarchy reuse, partial files и Windows
archive sync через write-capable handle. Synthetic v1 fixture убирает witness,
которого v1 не имел; реальный malformed witness по-прежнему блокирует startup.

**280 workspace tests**, Linux/Windows-target Clippy, UI typecheck/build/contracts,
bound-request tests и real Unix browser gate passed. 197 native tests подтвердили
реальную activation, stale/v1 refusal, dedup после restart, живой unlisted
process, canonical local path и цепочку A→B→C. Installed schema-6 worker пока не меняется.

Архив сохраняет SQLite history и записанный stdout/stderr. Внешние application log,
artifact files и workspaces целиком не копируются: сохранённый путь не является
исторической копией файла. При чтении внешних файлов важны их identity/hash;
archive read model A3 будет показывать эти ссылки отдельно от сохранённого output.
