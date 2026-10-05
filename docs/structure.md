# Структура и правила расширения

Папки отражают границы изменений. Scaffold содержит только два crates и два binaries; transport/platform/desktop/modules представлены описаниями до начала их реальной реализации.

| Путь | Владелец логики | Допустимые зависимости |
| --- | --- | --- |
| crates/protocol | DTO/version/errors/identifiers | serde; без async runtime/platform/UI |
| crates/core | Task/project policy, orchestration, Store/Runner/Delivery interfaces | protocol; инфраструктура через interfaces |
| crates/transport | Sessions, IPC, WebSocket | protocol, async runtime, platform IPC primitives |
| crates/platform | Processes/files/path handling | OS APIs; без Tauri и UI DTO |
| modules/runner | Recipe/direct command service | core, protocol, platform |
| modules/workspace-sync | Manifest/import/delivery | core, protocol, platform, adapter implementations |
| modules/log-reader | Read/tail зарегистрированных логов | core/platform; bounded reads |
| modules/attention | Attention request state | core/protocol; native presentation в desktop |
| apps/controller | Wiring Debian adapters и agent entry points | core, protocol, transport, capture/delivery |
| apps/worker | Wiring Windows runner/store/projects | core, protocol, transport/platform/modules |
| apps/desktop | Tray/WebView/notification presentation | protocol/IPC client; domain logic в worker |
| ui/src/app | Layout/navigation/query/event integration | feature components и shared UI |
| ui/src/features | Project/task/log/attention экраны | generated contracts, UI read model |
| tests/fixtures | Локальные проекты, manifest/output/recovery fixtures | Никаких live VPS или user secrets |

Не делить каждую маленькую функцию на crate. Вынести adapter при появлении второго implementation или настоящей platform границы. Нельзя зависеть от `/home/user1/netwatch` или копировать его runtime files.

## Как добавить проект

Project configuration + зарегистрированные recipes, source allowlist и paths. Task service и UI одинаковые для разных проектов. Если нужен новый тип действия, добавить capability handler с явными inputs/events/result/cancel/recovery и экраном/renderer.

## Контракты и версии

Protocol DTO находятся только в protocol. UI fixtures имеют собственные demo types и не являются wire schema. При подключении реального IPC генерировать TypeScript из Rust DTO либо schema и проверять sample messages; не дублировать rules в Tauri и UI.

App version начинается с 0.1.0; protocolVersion = 1, config schemaVersion = 1. Они развиваются независимо. На этом этапе Cargo workspace и package.json задают app version; Tauri config добавится при создании desktop. Capabilities в handshake содержат только реализованные функции.

## Данные вне исходников

Runtime config и journal хранить в user data directory. Shared delivery namespace и Windows workspaces задаются пользователем. Build dependencies/output не коммитить. Original proposal в context — историческая запись; текущие принципы находятся в ADR.
