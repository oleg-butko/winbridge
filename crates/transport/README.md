# Local transport adapter

Один JSON request/response на соединение, little-endian u32 length prefix, максимум 1 MiB и общий deadline 10 секунд. Windows endpoint содержит user SID и instance ID; server использует protected user-only DACL, first-instance protection и reject-remote-clients. Busy pipe client повторяет open в пределах одного deadline. Unix development socket находится в private stateDirectory и имеет mode 0600. Worker ограничивает одновременные client connections до 32.

Domain task status и queue находятся в worker; разрыв клиента не отменяет сохранённую задачу. Plain WebSocket controller↔worker, handshake/heartbeat/event replay — этап 2. Никаких TLS или Netwatch endpoint names.

WebSocket с plain TCP добавлен на этапе 2: JSON handshake/version/pair/capabilities, correlation, 1 MiB bounds, 5 s handshake, 2 s write, 8 s RPC и heartbeat с единым deadline. Worker reconnect не повторяет команды. [Архитектура](../../docs/architecture.md), [Windows session проверка](../../docs/windows-session-validation.md).
