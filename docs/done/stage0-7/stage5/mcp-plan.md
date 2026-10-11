# MCP stdio — план 2026-10-08

Следующий сценарий согласованного [этапа 5](stage5-plan.md). История/диагностика
прошла автоматический [Windows gate](windows-history-gate-2026-10-08.md).

`winbridge-controller --config PATH mcp` — дочерний stdio процесс клиента.
Он использует существующий controller IPC, без отдельного WebSocket listener,
execution queue или процесса worker. Typed tools соответствуют LocalRequest;
input schemas генерируются из Rust DTO, а не независимого списка параметров.
CLI publish и MCP publish вызывают один существующий handler публикации snapshot.

JSON-RPC lifecycle/UTF-8 newline framing соответствуют фиксированной версии
[MCP 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle).
Поддерживаются initialize, initialized, ping, tools/list и tools/call; capabilities
не объявляют resources/prompts/tasks, которые не реализованы. stdout содержит
только MCP messages, diagnostics идут в stderr. Frames и parsing ограничены.
Tool errors возвращаются отдельно от protocol errors; результат содержит исходный
LocalResponse, IDs и cursor pages. Catalog снабжён описаниями и read-only hints.

Worker остаётся authority: admission/project policy/dedup/lease/journal/отмена
дерева не меняются. Request/idempotency keys задаёт клиент. Адаптер не повторяет
внешнюю команду после потерянного ответа. Потеря stdio клиента не отменяет accepted
task; остановка процесса требует явного cancel/stop tool. Capability guards старого
worker остаются у controller. Diagnostic tool возвращает preview, не записывает
произвольный destination file. Клиентские/системные MCP настройки не меняются.

Проверки: initialize/version/lifecycle, invalid JSON/batch/unknown methods/tools,
frame limits и чистый stdout; schemas/typed requests; actual Unix controller/worker
submit/dedup/output/cancel, lost ACK без automatic replay, работа после закрытия
stdio и capability refusal. Windows: новый MCP adapter в Debian отправляет один
изолированный fixture через здоровый installed worker; output/IDs/dedup сверяются
автоматически. Новые окна/обновление runtime для этого теста не требуются.

Manual retention deletion и редактор projects/connection — дальнейшее расширение;
старые пользовательские snapshots/reports/history не удаляются этим сценарием.
