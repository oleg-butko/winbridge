# A3.3 — согласованные страницы операций

2026-10-15. Зависимость: A3.2 actual installed pair прошёл.

Первый вертикальный сценарий объединяет task/import/workflow/application в одной
странице из SQLite read transaction. OperationRef содержит journal scope, вид и ID;
запись показывает typed state, revision, причину, ресурсы, связи и event cursor.
Последний сохранённый результат отделён от свежести соединения. Это observation,
новые mutating states/scheduler не вводятся до A4.

Keyset cursor закрепляет scope, fingerprint immutable filters и четыре admission
rowid watermarks. Новые admissions не попадают между уже открытыми страницами,
даже при одинаковом/откатившемся времени. Статусы/ресурсы перечитываются в одной
транзакции каждой страницы; это не обещание замороженного состояния всех страниц.
После смены scope/query cursor отвергается. Large journal counters передаются
decimal strings без потери точности JavaScript. Страница ограничена 50 записями;
причины/labels/resources имеют явные bounds/truncation. Output читается отдельно.

Новый readonly contract `winbridge.operation-read-model` v1 идёт через отдельный
scoped extension envelope/session opcode. Frozen worker RPC 1/2, fingerprints и
closed capabilities не меняются. Transport envelope версионирован отдельно от
business contract; payload интерпретируется только known typed handler. Unknown
ref/version/schema отвергается до dispatch, legacy fallback отсутствует. Сейчас
регистрируется только readonly handler; generic arbitrary-command gateway не
добавляется. Будущие actor contracts получают свои DTO/test profiles/admission,
не требуют добавлять каждый business message в transport enum.

Поставка: protocol/core validation → Store transaction/query → negotiated local
IPC/WS/controller CLI/MCP → UI history page → native candidate gate. При смене
scope старые ссылки остаются историческими, actions не перепривязываются.
Старая history API сохраняется. Новый contract требует обновления Windows worker
только после готового tested candidate, без промежуточного ручного checklist.

Ревью плана: cursor не используется как permission; rowid watermark — admission
fence, не глобальная revision; sequence относится к конкретной операции и допускает
gaps. Read transaction не держится между запросами. Нет новой миграции journal,
брокера, outbox или копирования пользовательской БД; concurrency проверяется на
отдельных synthetic stores. Tests: tie/reopen/filter/scope, concurrent writer
между чтением операций и ресурсов, Decimal range, malformed/future contracts,
bounded pages, actual route/readonly UI и Windows native query.
