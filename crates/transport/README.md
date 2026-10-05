# Будущий transport adapter

Plain локальный WebSocket controller↔worker, user-scoped Unix socket/named pipe, framing, timeout, heartbeat и reconnect/event replay. Domain task status находится в core/worker. В scaffold Cargo package ещё нет; создавать при этапе 2 или появлении real local IPC на этапе 1. Никаких TLS или fixed Netwatch endpoint names.
