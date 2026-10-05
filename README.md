# WinBridge

Локальный каркас для связи Debian VM и Windows host: выполнять команды в зарегистрированных проектах, доставлять исходники, собирать и запускать приложения, читать логи и показывать пользователю ход работы.

Название выбрано пользователем. Репозиторий отделён от Netwatch. Rust отвечает за runtime, React/Mantine — за UI, Tauri запланирован как Windows tray/WebView shell. Node.js нужен для сборки UI, а не для будущего runtime.

## Что готово

- Собирающийся Rust workspace: общие DTO, core entry point и отдельные binaries controller/worker.
- CLI `--help` и `--describe` с честным описанием готовности: implemented capabilities пока пусты.
- Тесты контракта команд: Windows пути и отдельные аргументы, отклонение неизвестных режимов и полей.
- Интерактивная демонстрация UI: timeline, параметры команды, output, успех, ошибка и отсутствие свежего подтверждения.
- Архитектура, поэтапный roadmap, UX требования, ADR, примеры конфигурации и инструкции следующему агенту.

Исполнение команд, сетевые listener, доставка снимков и сохранение журнала пока не реализованы. UI демонстрация использует fixtures и не выполняет операции на Windows.

## Принятые решения

- Работа только внутри локальной пары Windows ↔ Debian VM. Команды/события через plain WebSocket `ws://`, без TLS и сертификатов.
- `/mnt/hgfs/shared` в Debian соответствует `K:\VMs\shared` на Windows. Готовые снимки доставляются через общий каталог; сборка идёт в отдельной папке Windows.
- Разрешены рецепты и произвольные команды в зарегистрированной папке проекта.
- Windows worker хранит фактические результаты и продолжает принятые задачи после разрыва соединения или закрытия UI.
- Каждая функция включает понятный UI, полный журнал действий и обработку сбоев.

## Структура

```text
apps/
  controller/       Debian coordinator; сейчас CLI scaffold
  worker/           Windows execution authority; сейчас CLI scaffold
  desktop/          Запланированный Tauri shell
crates/
  protocol/         Общие wire DTO и версия протокола
  core/             Логика задач, policy и interfaces модулей
  transport/        Запланированные WebSocket и local IPC adapters
  platform/         Запланированные операции Windows/Linux
modules/
  workspace-sync/   Снимки, manifest, импорт из shared папки
  runner/           Рецепты, direct commands, lifecycle процессов
  log-reader/       Чтение и tail зарегистрированных логов
  attention/        События внимания, popup, звук
ui/src/
  app/              Компоновка приложения
  features/tasks/   Task workbench
  fixtures/         Явно демонстрационные данные
examples/           Образцы будущей конфигурации
scripts/            Проверки подготовки проекта
tests/fixtures/     Локальные fixtures для следующих этапов
docs/               Архитектура, UX, roadmap, ADR и handoff
```

`transport`, `platform`, `desktop` и `modules` пока имеют описание границ; они не объявлены Cargo packages. Новые crates создавать при появлении реального кода. Core и protocol не зависят от Tauri, конкретной VM или соседнего Netwatch repository.

## Проверка и запуск scaffold

```bash
cargo test --workspace --locked
cargo check --workspace --locked
cargo run -p winbridge-controller --locked -- --describe
cargo run -p winbridge-worker --locked -- --help
npm ci
npm run check:scaffold
npm run typecheck
npm run build:ui
npm run dev:ui
```

UI dev server слушает только `127.0.0.1`; открыть адрес, показанный Vite. Сборка находится в `web-dist/`. CLI не принимает runtime config и не запускает сервисы на этом этапе.

Пример интерфейса: [task workbench](docs/screenshots/task-workbench.png). Результаты подготовки: [validation](docs/validation.md).

В этой Debian среде при отсутствии инструментов в PATH:

```bash
export PATH=/home/user1/.nvm/versions/node/v24.19.0/bin:/home/user1/.local/bin:$PATH
```

Это установка PATH только для текущей оболочки. Для Windows использовать установленный Rust/MSVC toolchain и подходящий Node.js.

## Навигация

Начать с [handoff](docs/handoff.md) и [инструкций агента](AGENTS.md). Далее: [архитектура](docs/architecture.md), [доставка через HGFS](docs/shared-folders.md), [поэтапный план](docs/roadmap.md), [UI/UX](docs/ux.md), [границы модулей](docs/structure.md), [решения пользователя](docs/decisions/README.md).

Предыдущее предложение из Netwatch перенесено в [исторический контекст](docs/context/README.md); оно не определяет текущие решения.
