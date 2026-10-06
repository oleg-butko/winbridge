# WinBridge

Локальный проект для связи Debian VM и Windows host: выполнять команды в зарегистрированных проектах, доставлять исходники, собирать и запускать приложения, читать логи и показывать пользователю ход работы.

Название выбрано пользователем. Репозиторий отделён от Netwatch. Rust отвечает за runtime, React/Mantine — за UI, Tauri — за native WebView shell; tray запланирован. Node.js нужен для сборки UI, а не для runtime.

## Что готово

- Собирающийся Rust workspace: общие DTO, core entry point и отдельные binaries controller/worker.
- Worker CLI: config validation, serve, submit/get/list/output/cancel через local IPC. Controller CLI передаёт запросы по локальному ws:// и читает результаты worker.
- Controller записывает адреса сетевой активности в JSONL с ротацией; блокировки по IP в текущей локальной разработке нет.
- Snapshot runtime: точный Git commit, verified HGFS package, durable import, общий project lease и explicit build из локального поколения. Native UI: manifest/mapping/progress/history/cancel. [Формат](docs/snapshot-format.md); [Windows runtime gate пройден](docs/windows-snapshot-gate-2026-10-06.md); завершается native UX проверка формы доставки.
- SQLite task/output journal с сохранением до ACK, дедупликацией и indeterminate recovery; одна команда на project, независимые stdout/stderr и квота.
- Windows process adapter с suspended launch → Job Object → resume, timeout/cancel дерева; Unix development adapter проверен реальным исполнением.
- User SID + instance named pipe с явным DACL; Unix socket для локальной разработки. Tauri shell и UI adapter подключают журнал и отправку задач.
- Тесты контракта команд: Windows пути и отдельные аргументы, отклонение неизвестных режимов и полей.
- Connected UI: форма команды, recent tasks, timeline, output/exit code, cancel и независимая свежесть соединения. Browser: явно помеченная демонстрация и просмотр сохранённого отчёта worker.
- Архитектура, поэтапный roadmap, UX требования, ADR, примеры конфигурации и инструкции следующему агенту.

**Этап 1 завершён 2026-10-05:** десять Windows runtime checks и пять native UI шагов подтверждены. [Доказательства](docs/windows-gate-2026-10-05.md). **Windows runtime gate этапа 2 пройден:** Debian отправляет реальные Windows команды, получает output, отменяет дерево и восстанавливает ту же task/PID после reconnect. [Отчёт](docs/windows-session-gate-2026-10-05.md); все три новых native UI наблюдения подтверждены пользователем. Этап 2 завершён 2026-10-05. Desktop сохраняет форму рядом с exe. Snapshot runtime прошёл 19 Windows checks 2026-10-06; package select/delivery form restore ждут Windows подтверждения; log tail и attention — следующие этапы. Browser сам команды не выполняет.

## Принятые решения

- Работа только внутри локальной пары Windows ↔ Debian VM. Команды/события через plain WebSocket `ws://`, без TLS и сертификатов.
- `/mnt/hgfs/shared` в Debian соответствует `K:\VMs\shared` на Windows. Готовые снимки доставляются через общий каталог; сборка идёт в отдельной папке Windows.
- Разрешены рецепты и произвольные команды в зарегистрированной папке проекта.
- Windows worker хранит фактические результаты и продолжает принятые задачи после разрыва соединения или закрытия UI.
- Каждая функция включает понятный UI, полный журнал действий и обработку сбоев.

## Структура

```text
apps/
  controller/       Debian coordinator; ws:// listener и CLI submit/get/cancel/watch
  worker/           Local execution authority, CLI, SQLite, fixture executable
  desktop/          Tauri shell с worker IPC bridge, отдельный Cargo package
crates/
  protocol/         Общие wire DTO и версия протокола
  core/             Логика задач, policy и interfaces модулей
  transport/        Bounded local IPC/WebSocket, handshake, heartbeat и deadlines
  platform/         Process tree/Job Object и local disk adapters
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

`transport` и `platform` входят в workspace. Desktop — отдельный package в `apps/desktop/src-tauri` со своим lockfile, чтобы workspace проверки не требовали GUI SDK. Runner service сейчас находится в worker; `modules` описывает будущие границы. Core и protocol не зависят от Tauri, конкретной VM или соседнего Netwatch repository.

## Проверка и запуск

```bash
cargo test --workspace --locked
cargo check --workspace --locked
cargo fmt --all -- --check
cargo run -p winbridge-controller --locked -- --describe
cargo run -p winbridge-worker --locked -- --help
npm ci
npm run check:scaffold
npm run check:contracts
npm run typecheck
npm run build:ui
npm run dev:ui
```

UI dev server слушает только `127.0.0.1`; сборка в `web-dist/`. Windows сборка и проверка этапа 1: [сценарий](docs/windows-validation.md). Этап 2: [локальная сессия Debian ↔ Windows](docs/windows-session-validation.md). Текущий gate: [snapshot/import/build](docs/windows-snapshot-validation.md). Её runtime по умолчанию — постоянный `K:\_proj-26\winbridge\runtime`, задаваемый локальным runtimeDirectory; повторная подготовка сохраняет configs, форму и journal. Worker не устанавливается как service; явный `--config PATH serve` запускает его в пользовательском процессе.

Реальные проверки Unix development adapter (сначала `cargo build --workspace --locked`):

```bash
python3 scripts/validate-local.py
python3 scripts/validate-session.py
npm run test:ui
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked
```

UI integration test использует headless Chromium с sandbox и временный транспортный shim к настоящему Unix worker. Он требует `/usr/bin/chromium`, `WINBRIDGE_BROWSER` или установленный Playwright Chromium. Сам browser UI не получает command endpoint. [Скриншот реального Linux результата](docs/screenshots/local-worker-linux.png).

Пример интерфейса: [task workbench](docs/screenshots/task-workbench.png). Результаты подготовки: [validation](docs/validation.md).

В этой Debian среде при отсутствии инструментов в PATH:

```bash
export PATH=/home/user1/.nvm/versions/node/v24.19.0/bin:/home/user1/.local/bin:$PATH
```

Это установка PATH только для текущей оболочки. Для Windows использовать установленный Rust/MSVC toolchain и подходящий Node.js.

## Навигация

Начать с [handoff](docs/handoff.md) и [инструкций агента](AGENTS.md). Далее: [архитектура](docs/architecture.md), [доставка через HGFS](docs/shared-folders.md), [поэтапный план](docs/roadmap.md), [UI/UX](docs/ux.md), [границы модулей](docs/structure.md), [решения пользователя](docs/decisions/README.md).

Предыдущее предложение из Netwatch перенесено в [исторический контекст](docs/context/README.md); оно не определяет текущие решения.
