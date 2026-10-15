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

## Кандидат и проверка

Реализованы Store query, readonly local IPC/WS envelope, negotiation по catalog
и schema hash, controller API/CLI/MCP и desktop page. 316 workspace tests и
отдельная negotiation regression прошли; Linux/Windows-target Clippy,
TypeScript/build, полный browser scenario с actual Unix worker прошли.
Нативный Windows gate и installed new/new read-model route ещё ожидаются.
RPC 1/2 не расширены. Protocol-2 schema SHA-256 остаётся
`dd86d35b8b9ed59aca0bb438491e906b9c25c69970f0121226fdee99b98e9a12`.

Для CLI: `winbridge-controller --config PATH operations REQUEST.json`.
MCP tool: `winbridge_operations`. Вход одинаковый:

```json
{
  "context": { "instanceId": "local-development", "journalIncarnation": "<из info.fencedRequests.context>" },
  "query": { "projectId": null, "kind": null, "search": "", "limit": 25, "cursor": null }
}
```

Следующий запрос сохраняет context/filters, помещает возвращённый nextCursor
в query.cursor. Limit разрешено менять в пределах 1..50. Для новой выборки
нужно явно запросить cursor=null. Статусы между страницами могут меняться;
это не длительная snapshot transaction. В UI фильтры применяются по кнопке,
переход к следующей странице не добавляет новые admissions; открытие подробностей
не запускает команду. При disconnect сохранённый срез помечается устаревшим.

Ревью: cursor не permission, query не создаёт task, schema mismatch не вызывает
fallback. Page decoder проверяет scope всех ссылок, family статуса, decimal
range, порядок/дубли строк, прежние watermarks и последний keyset key.
Ресурсы чужой incarnation не перепривязываются. SQL использует параметры,
поиск `%`/`_` буквальный. Output/argv не включены в сводку.

Watermarks используют rowid при существующей append-only истории. Будущая
компактация/удаление admissions или VACUUM живого журнала потребуют durable
admission sequence либо явной инвалидизации cursors; это ограничение текущего
контракта, а не обещание неизменности rowid при внешнем редактировании БД.
Хранится только наблюдение активных lease проекта. Это не вычисленный scheduler
blocker/fairness; семантика очереди и глобальные ресурсы остаются A4.
