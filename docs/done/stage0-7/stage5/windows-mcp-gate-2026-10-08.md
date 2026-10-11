# MCP → настоящий Windows worker — 2026-10-08

Сценарий завершён автоматически, без обновления Windows runtime и новых окон.
Дочерний stdio MCP adapter в Debian использовал existing controller IPC/WebSocket.

- Adapter source: `cc181567e8b423aa975edf13b507f7201bba2386`, sourceDirty=false.
- Shared report: `K:\VMs\shared\WinBridge\mcp-validation\20261008-072201-391f141c`.
- Report SHA-256: `e401fa6a6b063090c53eb61bde1c78f47f80c87f662346a2ee51ff795e0af2c1`.
- Ready marker и фактические report bytes/hash проверены Debian.
- Setup task: `10ebc77b-925f-4bc7-936d-a86e42cdc0cc`, succeeded.
- Fixture task: `e7f6e079-bfb1-41ca-a857-9bd07d96cbb4`, succeeded/exit 0.
- Client-exit task: `e1e75274-498b-41d5-9128-41aed25c6b6c`, running с прежним PID
  после закрытия stdio; затем explicit cancel, terminal cancelled.

Новая fixture directory находится внутри зарегистрированного snapshot-fixture:
`mcp-validation\20261008-072201-391f141c`. Исполняемый fixture копируется из
пройденного [native history gate](windows-history-gate-2026-10-08.md); старый
каталог/отчёты не перезаписываются. Setup command и fixture tasks остаются в
обычном worker journal и доступны desktop. Повтор того же idempotencyKey вернул
ту же task/PID; UTF-8 output содержит «Привет ✓».

Worker f7c4878 не объявляет HistoryRead. MCP history вызов получил ожидаемый
capability refusal `not transmitted`; неизвестный RPC не отправлялся старому
worker. Это заранее предусмотренная отрицательная проверка, не сбой worker.

Debian controller обновлён до cc18156 перед прогоном, без изменения local config.
Текущий exec session 27844, generation `6928c40a-2c66-4bde-8226-a6b59d2fb789`,
session `90a2ff78-83e3-435b-975b-f0d78b0e0947`, `ws://192.168.44.203:1143`.
Старый exec session 6831 завершён штатно. Worker generation остался
`8c5d8c94-697c-4e9f-9cdb-765930ee36f3`; build/config/state не заменены,
activeTaskCount=0, journalRecoveryRequired=null, controller connected/lastError=null.
Пользователю до прогона объяснены worker/desktop requirements и planned reconnect;
после сообщено: desktop можно завершить, worker оставить работающим.

Дополнительно прошли 107 workspace Rust tests, Clippy, Windows all-targets compile,
contracts/typecheck, standalone desktop check и actual Unix MCP integration.
Unix proxy потерял один уже полученный ACK: automatic replay отсутствовал,
explicit same-key retry дал одну task/один реальный процесс. Проверены actual
publish/import, output, history/diagnostic и client exit/cancel.

MCP adapter сценарий завершён. Новые history/diagnostic/typed attention функции
доступны в проверенной тестовой Windows сборке; installed worker остаётся старым,
его обновление объединяется с desktop, без новых мелких ручных gates. Этап 5
остаётся открытым для projects/connection settings и полной retention policy.
