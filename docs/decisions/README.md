# Принятые решения

| ADR | Решение |
| --- | --- |
| [0001](0001-project-and-stack.md) | Отдельный WinBridge repository, Rust runtime и React/Mantine + Tauri UI. |
| [0002](0002-shared-folder-delivery.md) | Shared folder — доставка снимков, сборка — отдельный Windows workspace. |
| [0003](0003-local-plain-transport.md) | Plain локальный WebSocket без encrypted connections. |
| [0004](0004-project-access-and-ux.md) | Recipes + direct commands в project scope и обязательная прозрачность UI. |

Статус всех четырёх: accepted 2026-10-04. Новые пользовательские требования могут заменить ADR; агент не отменяет их самостоятельно из-за старого proposal.
