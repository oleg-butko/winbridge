# Attention

Persisted attention event, severity, dedup, acknowledged/missed state. Presentation выполняет Tauri companion: popup, tray badge, встроенный soundId. Quiet hours и rate limits задаёт пользователь. Этап 5.

Первый рабочий сценарий реализован в protocol/core/worker и UI: durable task attention,
request dedup, explicit acknowledgement, paging и task links. См. [контракт](../../docs/attention.md).
Native popup/tray/sound и настройки ещё не реализованы; эта папка не отдельный Cargo package.
