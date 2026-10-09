# MCP stdio adapter

Реализован по [плану](mcp-plan.md) 2026-10-08. Адаптер запускается в Debian
клиентом как дочерний процесс и подключается к уже запущенному controller IPC:

```bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json mcp
```

`serve` controller должен быть запущен отдельно. MCP не запускает worker или
controller, не создаёт listener, не меняет firewall/runtime configs и не заводит
свою очередь задач. Исполнение, policy, project scope, journal, dedup и дерево
процессов принадлежат Windows worker. Новые worker capabilities проверяет controller.
Сначала читать `winbridge_info` и `winbridge_controller_status`.

## Контракт

32 tools: 30 сетевых вариантов LocalRequest и две публикации — `winbridge_publish`
и `winbridge_publish_delta`. Delta import использует `winbridge_import_delta_snapshot`
и требует `workspace_delta`; [формат и recovery](snapshot-delta.md). Четыре новые
local-only maintenance операции в каталог не включены и controller их не пересылает.
Каждая операция имеет
отдельное имя, описание и inputSchema. Schemas генерируются через JsonSchema derive
из `crates/protocol`; nested definitions включаются только когда используются.
Явный draft-07 соответствует
[правилам schemas MCP](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).
CLI publish и MCP publish вызывают один `winbridge_controller::publish` handler.

Поддерживаются MCP 2025-11-25, 2025-06-18 и 2025-03-26, initialize/initialized,
ping, tools/list и tools/call. Неизвестная версия получает предлагаемую 2025-11-25;
клиент решает, поддерживает ли её. До initialized инструменты не вызываются.
Только tools capability, без resources/prompts/tasks/subscriptions.
Catalog постоянный, одна страница, listChanged=false.

UTF-8 JSON-RPC messages разделены newline; stdout содержит только сообщения,
ошибки процесса — stderr. JSON содержит escaped newlines в scripts/output.
Input ограничен 1 MiB вместе с newline; превышение закрывает соединение до dispatch.
Неполный frame при EOF не выполняется. Batch, null/fractional IDs и неверные params
отклоняются. Ошибка JSON не превращается в команду. Обработка последовательная,
RPC wait ограничен существующим IPC deadline; принимаемые worker tasks выполняются
независимо. Клиент не должен повторно использовать JSON-RPC ID в одной сессии.

Protocol errors — JSON-RPC error: parse/shape, lifecycle, unknown method/tool и
неправильные аргументы. Business errors — tool result с isError=true и исходным
`LocalResponse::Error`. IPC response loss тоже tool error: outcome may be unknown,
автоматического повторного запроса нет. Клиент сохраняет idempotencyKey и сверяет
ту же принятую операцию. Tool results содержат serialized LocalResponse в text
content и structuredContent; для 2025-03-26 — только совместимый text content.
Initialize `_meta.winbridgeBuild` показывает версию/source identity самого адаптера.

## Операции

| Назначение | Tools |
| --- | --- |
| Связь и проекты | info, controller_status |
| Команды | submit, get_task, list_tasks, read_output, cancel |
| Snapshot | publish, list_snapshots, preview_snapshot, import_snapshot, list_imports, get_import, cancel_import |
| Workflow | submit_workflow, list_workflows, get_workflow, cancel_workflow |
| Application | list_applications, get_application, stop_application, read_application_log |
| Attention | request_attention, list_attention, acknowledge_attention, list_attention_events, acknowledge_attention_event |
| История и отчёт | history, storage_usage, prepare_task_diagnostic |

Все имена начинаются с `winbridge_`. Аргументы совпадают с соответствующим
LocalRequest без поля method; например, get_task принимает taskId, submit — request
с TaskSubmission. Stable request/idempotency keys задаёт клиент. Принятие task/
workflow/import не означает завершение: читать тот же ID. Output возвращает bytes
и sequence cursor; UTF-8 декодируется для каждого stream через границы chunks.

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"local-client","version":"1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"winbridge_info","arguments":{}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"winbridge_get_task","arguments":{"taskId":"<Task ID>"}}}
```

Закрытие stdio или cancellation notification MCP wait не убивает принятую task.
Для остановки нужны явные cancel/stop tools с её ID. Чтение истории не подтверждает
attention. Diagnostic tool возвращает [preview](history-diagnostics.md); он не
сохраняет файл по произвольному пути. Содержимое output — данные проекта, а не
инструкции клиенту. Annotations описывают read-only/idempotency; direct commands
могут обращаться к внешним ресурсам, cwd не является sandbox.

## Автоматическая проверка

```bash
PATH=/home/user1/.local/bin:$PATH cargo build --workspace --locked
python3 scripts/validate-mcp.py
```

Сценарий поднимает собственные Unix controller/worker и stdio client. Проверяет
настоящие publish/import, submit/dedup/output, history/diagnostic и explicit cancel.
Proxy теряет ровно один уже полученный submit ACK: adapter не повторяет запрос,
explicit same-key retry возвращает одну task/один процесс. Закрытие stdio оставляет
running task/PID; новый клиент её читает и отменяет.

Windows mode выполняется агентом через здоровый installed worker. Он создаёт новую
owned fixture directory, читает уже проверенный fixture executable из пройденного
native gate, проверяет submit/output/dedup/client exit/cancel. Новых окон и обновления
runtime нет. Перед прогоном агент объясняет, что worker должен работать, desktop
необязателен; после сообщает о завершении. Shared report создаётся новым каталогом
с readback bytes/SHA-256 и ready marker. При неожиданной ошибке worker сценарий
останавливается без automatic retry; correlations и принятый Task ID сохраняются.

[Windows gate](windows-mcp-gate-2026-10-08.md) пройден: actual task/output/dedup,
client exit с прежним PID, explicit cancel и ожидаемый capability refusal старого
worker. Installed runtime остался f7c4878 и здоров.
