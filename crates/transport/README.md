# Local transport adapter

Один JSON request/response на соединение, little-endian u32 length prefix, максимум 1 MiB и общий deadline 10 секунд. Windows endpoint содержит user SID и instance ID; server использует protected user-only DACL, first-instance protection и reject-remote-clients. Busy pipe client повторяет open в пределах одного deadline. Unix development socket находится в private stateDirectory и имеет mode 0600. Worker ограничивает одновременные client connections до 32.

Domain task status и queue находятся в worker; разрыв клиента не отменяет сохранённую задачу. Plain WebSocket controller↔worker, handshake/heartbeat/event replay — этап 2. Никаких TLS или Netwatch endpoint names.
