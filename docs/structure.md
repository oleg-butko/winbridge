# Структура и правила расширения

Папки отражают границы изменений. Этап 1 добавил transport/platform crates, local worker runtime и отдельный Tauri package. Modules пока описывает следующие границы; первая runner orchestration и SQLite Store находятся в worker, project policy — в core.

| Путь | Владелец логики | Допустимые зависимости |
| --- | --- | --- |
| crates/protocol | DTO/version/errors/identifiers и TypeScript generation | serde; optional ts-rs только для generation, без async runtime/platform/UI |
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

Protocol DTO находятся только в protocol. `npm run generate:contracts` генерирует `ui/src/runtime/contracts.ts` из Rust через optional feature `typescript`; `npm run check:contracts` проверяет отсутствие drift. UI fixtures имеют собственные demo types и не являются wire schema. Tauri передаёт LocalRequest в worker; policy не дублируется в desktop/UI. Browser отчёт проходит проверку формата перед отображением.

App version начинается с 0.1.0; protocolVersion = 1, config schemaVersion = 1. Они развиваются независимо. Cargo workspace, package.json и Tauri config задают app version. После [Windows gate](windows-gate-2026-10-05.md) worker объявляет `task_submit`/`task_cancel`; controller теперь объявляет эти capabilities как dispatch к worker через bounded WebSocket сессию. Handshake передаёт wire version, appVersion, pair/node/generation и capabilities; native Windows gate этапа 2 пока ожидается.

## Данные вне исходников

Desktop/settings JSON находятся рядом с exe; worker/controller runtime config задаётся явным локальным файлом. Journal хранится в настроенном stateDirectory на локальном диске. Shared delivery namespace и Windows workspaces задаются пользователем. Build dependencies/output не коммитить. Original proposal в context — историческая запись; текущие принципы находятся в ADR.
