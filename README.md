# WinBridge

Локальный проект для связи Debian VM и Windows host: выполнять команды в зарегистрированных проектах, доставлять исходники, собирать и запускать приложения, читать логи и показывать пользователю ход работы.

Название выбрано пользователем. Репозиторий отделён от Netwatch. Rust отвечает за runtime, React/Mantine — за UI, Tauri — за native WebView shell и tray. Node.js нужен для сборки UI, а не для runtime.

## Что готово

- Собирающийся Rust workspace: общие DTO, core entry point и отдельные binaries controller/worker.
- Worker CLI: config validation, serve, submit/get/list/output/cancel через local IPC. Controller CLI передаёт запросы по локальному ws:// и читает результаты worker.
- Controller записывает адреса сетевой активности в JSONL с ротацией; блокировки по IP в текущей локальной разработке нет.
- Snapshot runtime: точный Git commit, verified HGFS package, durable import, общий project lease и explicit build из локального поколения. Native UI: manifest/mapping/progress/history/cancel. [Формат](docs/reference/snapshot-format.md); [Windows runtime gate пройден](docs/done/stage0-7/stage3/windows-snapshot-gate-2026-10-06.md); UI исправления приняты пользователем, этап 3 завершён.
- Workflow import/build/start/stdout readiness/log, frozen профиль и step IDs, отдельный registry приложений, byte cursor с rotation/truncation. [Описание](docs/guides/workflows.md); [автоматические Windows проверки](docs/validation/windows-workflow-validation.md), [37 native tests пройдены](docs/done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md).
- SQLite task/output journal с сохранением до ACK, дедупликацией и indeterminate recovery; одна команда на project, независимые stdout/stderr и квота.
- Этап 6 завершён: [delta/local source cache](docs/guides/snapshot-delta.md), [пакетный output](docs/done/stage0-7/stage6/windows-output-gate-2026-10-10.md) и [resume больших payload](docs/done/stage0-7/stage6/windows-resume-gate-2026-10-10.md) проверены автоматически. UI хранит ограниченный Unicode tail и отрисовывает до 28 строк output; полный журнал остаётся у worker.
- Windows process adapter с suspended launch → Job Object → resume, timeout/cancel дерева; Unix development adapter проверен реальным исполнением.
- User SID + instance named pipe с явным DACL; Unix socket для локальной разработки. Tauri shell и UI adapter подключают журнал и отправку задач.
- Тесты контракта команд: Windows пути и отдельные аргументы, отклонение неизвестных режимов и полей.
- Connected UI: форма команды, recent tasks, timeline, output/exit code, cancel и независимая свежесть соединения. Browser: явно помеченная демонстрация и просмотр сохранённого отчёта worker.
- Архитектура, поэтапный roadmap, UX требования, ADR, примеры конфигурации и инструкции следующему агенту.
- Этап 5: читаемый PowerShell EncodedCommand и долговечный журнал внимания с task/workflow/application links, dedup и явным подтверждением. [Что реализовано и что дальше](docs/guides/attention.md); [tray/popup/sound](docs/guides/companion.md) прошли автоматический Windows gate.

**Этап 1 завершён 2026-10-05:** десять Windows runtime checks и пять native UI шагов подтверждены. [Доказательства](docs/done/stage0-7/stage1/windows-gate-2026-10-05.md). **Windows runtime gate этапа 2 пройден:** Debian отправляет реальные Windows команды, получает output, отменяет дерево и восстанавливает ту же task/PID после reconnect. [Отчёт](docs/done/stage0-7/stage2/windows-session-gate-2026-10-05.md); все три новых native UI наблюдения подтверждены пользователем. Этап 2 завершён 2026-10-05. Desktop сохраняет форму рядом с exe. Snapshot runtime прошёл 19 Windows checks 2026-10-06; package select/delivery form restore приняты пользователем без нового ручного gate; workflow/log tail реализованы на fixture; Named profiles, output filters, artifact catalog и отдельный Netwatch IPC probe добавлены; полный Windows Netwatch сценарий прошёл автоматически: 41 native test и desktop check, этап 4 завершён. Этап 5 начат с журнала внимания; tray/popup/sound прошли автоматический native gate; workflow/app attention прошли автоматический native gate; [история/диагностика](docs/guides/history-diagnostics.md) реализованы, [Windows gate](docs/done/stage0-7/stage5/windows-history-gate-2026-10-08.md) пройден; [MCP](docs/guides/mcp.md) прошёл автоматический Windows gate; [настройки/retention](docs/guides/settings-retention.md) прошли [Windows gate](docs/done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md); этап 5 завершён. Browser сам команды не выполняет.

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
  update/           Source/ZIP verifier, update journal и supervisor; runtime apply ещё не реализован
  desktop/          Tauri shell с worker IPC bridge, отдельный Cargo package
crates/
  protocol/         Общие wire DTO и версия протокола
  core/             Логика задач, policy и interfaces модулей
  transport/        Bounded local IPC/WebSocket, handshake, heartbeat и deadlines
  platform/         Process tree/Job Object и local disk adapters
modules/
  workspace-sync/   Снимки, manifest, импорт из shared папки
  runner/           Раскрытие recipe/direct и path policy; процессы у worker
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

`transport` и `platform` входят в workspace. Desktop — отдельный package в `apps/desktop/src-tauri` со своим lockfile, чтобы workspace проверки не требовали GUI SDK. Worker владеет journal/queue/processes; runner, workspace-sync и log-reader реализованы отдельными modules. Core и protocol не зависят от Tauri, конкретной VM или соседнего Netwatch repository.

## Проверка и запуск

Начата [подготовка самообновления U0](docs/guides/worker-update-preparation.md):
`winbridge-update` проверяет исходники и показывает план замен, отдельный build-only
сценарий собирает staging без остановки установленного worker. [19 native tests
и полная Windows сборка](docs/plans/worker-self-update/windows-preparation-gate-2026-10-17.md)
прошли. Автоматическая установка относится к следующим U1–U4.
В U1 добавлены [independent journal/CAS/ownership и проверка source ZIP](docs/plans/worker-self-update/durable-preparation.md).
В trusted library реализована [source transaction](docs/plans/worker-self-update/source-transaction.md):
preimages, intent/receipt, применение без перезаписи локальных правок и readonly
crash inspection. [65 native tests и полный candidate build](docs/plans/worker-self-update/windows-source-transaction-gate-2026-10-17.md)
прошли. CLI меняет только update metadata и читает диагностику; внешние source-write
commands, exe replacement и shutdown ещё не подключены.
Добавлена trusted library [supervisor handoff](docs/plans/worker-self-update/supervisor.md):
отдельный процесс, durable intent/PID/epoch и authenticated IPC; 413 local tests и
[76 native tests / полный candidate build](docs/plans/worker-self-update/windows-supervisor-gate-2026-10-17.md)
прошли. [Worker launcher и supervisor-only lease IPC](docs/plans/worker-self-update/worker-control.md)
добавлены как trusted API; 417 local tests и
[80 native tests / полный candidate build](docs/plans/worker-self-update/windows-worker-control-gate-2026-10-17.md) прошли. Source apply/shutdown/install через IPC и публичное включение
ещё не подключены.
[Managed compiler](docs/plans/worker-self-update/compiler-driver.md) добавляет
trusted library intent/PID/Job/resume и verified cleanup, updater schema 4.
429 local tests, 10 итоговых compiler tests и
[95 native tests / полный candidate build](docs/plans/worker-self-update/windows-compiler-gate-2026-10-18.md) passed.
[Atomic compiler launch](docs/plans/worker-self-update/atomic-executable.md) прошёл
[102 native tests / candidate build](docs/plans/worker-self-update/windows-atomic-launch-gate-2026-10-18.md).
[Source preparation actor](docs/plans/worker-self-update/source-actor.md) добавляет
internal v2 mailbox и ZIP/grant/apply; 438 workspace tests и Linux/Windows Clippy
и [109 native tests / candidate build](docs/plans/worker-self-update/windows-source-actor-gate-2026-10-18.md)
passed. [Compiler actor и coordinated abort](docs/plans/worker-self-update/compiler-actor-abort.md)
добавляют internal v3/v4, config 3 и updater schema 5: intent до ACK, responsive
status и release только после source/Job/output proof. 453 workspace tests и
PowerShell fixture passed; native gate впереди. Добавлен [bounded compiler output](docs/plans/worker-self-update/compiler-output.md),
включая File ID cursor и проверку terminal SHA. Первый Windows compiler actor run
выявил PowerShell verbatim-path error; resolver исправлен, повтор впереди.
Install и public update отключены.

**После распаковки обновления на Windows:** закрыть desktop, остановить прежний worker
через Ctrl+C и из папки исходников выполнить `.\scripts\prepare-windows-snapshot.ps1`.
Сценарий соберёт и обновит постоянный runtime, сохранит настройки/журнал и запустит
worker/desktop. `test-windows-workflow.ps1` предназначен для автоматических тестов;
[назначение сценариев и параметры](docs/validation/windows-workflow-validation.md).

После перезапуска Debian controller нужно снова запустить. Для уже подготовленной
snapshot-конфигурации команда в Debian:

```bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
```

Работающий Windows worker подключится сам. Desktop нужен для просмотра состояния;
для команд агента достаточно оставить worker запущенным. Крестик desktop скрывает
окно в tray; «Завершить интерфейс» полностью закрывает companion и сохраняет формы.

```bash
cargo test --workspace --locked
cargo check --workspace --locked
cargo fmt --all -- --check
python3 scripts/test-export-source.py
cargo run -p winbridge-controller --locked -- --describe
cargo run -p winbridge-worker --locked -- --help
npm ci
npm run check:scaffold
npm run check:contracts
npm run typecheck
npm run build:ui
npm run dev:ui
```

UI dev server слушает только `127.0.0.1`; сборка в `web-dist/`. Windows сборка и проверка этапа 1: [сценарий](docs/validation/windows-validation.md). Этап 2: [локальная сессия Debian ↔ Windows](docs/validation/windows-session-validation.md). Snapshot gate завершён; новые runtime проверки: [автоматический workflow сценарий](docs/validation/windows-workflow-validation.md). Её runtime по умолчанию — постоянный `K:\_proj-26\winbridge\runtime`, задаваемый локальным runtimeDirectory; повторная подготовка сохраняет configs, форму и journal. Worker не устанавливается как service; явный `--config PATH serve` запускает его в пользовательском процессе.

Реальные проверки Unix development adapter (сначала `cargo build --workspace --locked`):

```bash
python3 scripts/validate-local.py
python3 scripts/validate-session.py
npm run test:ui
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked
```

UI integration test использует headless Chromium с sandbox и временный транспортный shim к настоящему Unix worker. Он требует `/usr/bin/chromium`, `WINBRIDGE_BROWSER` или установленный Playwright Chromium. Сам browser UI не получает command endpoint. [Скриншот реального Linux результата](docs/screenshots/local-worker-linux.png).

Пример интерфейса: [task workbench](docs/screenshots/task-workbench.png). Результаты подготовки: [validation](docs/done/stage0-7/stage0/validation.md).

В этой Debian среде при отсутствии инструментов в PATH:

```bash
export PATH=/home/user1/.nvm/versions/node/v24.19.0/bin:/home/user1/.local/bin:$PATH
```

Это установка PATH только для текущей оболочки. Для Windows использовать установленный Rust/MSVC toolchain и подходящий Node.js.

## Навигация

Для пользователя: [краткое руководство v1](docs/user-friendly-guide-v1/README.md) — готовые возможности, интерфейс и состояния, подключение проектов и MCP.

Все разделы собраны в [оглавлении документации](docs/README.md). Для продолжения разработки читать [handoff](docs/handoff.md) и [инструкции агента](AGENTS.md). Промежуточные планы, ревью и отчёты находятся в [архиве этапов 0–7](docs/done/stage0-7/README.md).

Предыдущее предложение из Netwatch перенесено в [исторический контекст](docs/context/README.md); оно не определяет текущие решения.

Этап 4: [изолированный Netwatch cycle](docs/validation/netwatch-validation.md), editable profiles/drafts рядом с desktop, результаты сборки с SHA-256/source commit, фильтрация stdout/stderr/log и build identity worker/desktop. Принятый workflow остаётся frozen в journal; редактирование или восстановление профиля не запускает команду.

Этап 5: [настройки проектов/соединения и output retention](docs/guides/settings-retention.md).
Desktop различает действующий config и сохранённый файл; изменения применяются
после ручного restart worker. Очистка только старого stdout/stderr, с preview и
защитой active/unknown/pending attention. Automatic policy включается явно;
history/dedup/snapshots/reports сохраняются. [Единый native gate](docs/done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md)
пройден: 36 backend tests, два UI запуска, 19 artifacts; этап 5 завершён.

Этап 6: [автоматический baseline](docs/validation/performance-baseline.md) существующей
Git/ZIP/HGFS доставки, Windows import/build и output/journal. Измерения выполняются
через работающий worker; desktop и обновление installed runtime не требуются.
Baseline (59 Windows operations) и [Git batch capture](docs/done/stage0-7/stage6/performance-capture-gate-2026-10-09.md)
завершены: 13.58× ускорение capture в matched comparison, hashes прежние,
21 новая Windows операция прошла.
[Delta](docs/done/stage0-7/stage6/windows-delta-gate-2026-10-09.md), [output batching/virtualization](docs/done/stage0-7/stage6/windows-output-gate-2026-10-10.md) и [pinned chunk resume](docs/done/stage0-7/stage6/windows-resume-gate-2026-10-10.md) прошли automatic gates. Этап 6 завершён; interfaces/profiles этапа 7 также завершены.

Delta этапа 6: [формат, публикация, импорт и автоматическая проверка](docs/guides/snapshot-delta.md). Используется проверенное локальное поколение; прежние snapshots сохраняются. [Оба автоматических Windows gates пройдены](docs/done/stage0-7/stage6/windows-delta-gate-2026-10-09.md); native desktop заново не открывался.

Продолжение этапа 6: [snapshot chunk/resume](docs/guides/snapshot-resume.md) — opt-in
schema 3 с pinned блоками ZIP и явным повтором import после обрыва. [Automatic Windows gate](docs/done/stage0-7/stage6/windows-resume-gate-2026-10-10.md) пройден; существующие full/delta packages сохраняют формат.

Этап 7: [общие профили рецептов и статический runner](docs/guides/project-profiles.md).
Два проекта используют один профиль; journal/cancel/recovery остаются у worker.
Источник task и определения workflow recipes сохраняются до ACK, изменения
конфига применяются после ручного restart. [68 native Windows tests](docs/done/stage0-7/stage7/windows-profiles-gate-2026-10-10.md) прошли автоматически; первый сценарий этапа завершён.

Этап 7 завершён: [реальные Netwatch и WinBridge projects](docs/guides/project-integration.md)
с общим build profile, private IPC, managed process stop и journal recovery прошли
[automatic gate](docs/done/stage0-7/stage7/windows-projects-gate-2026-10-10.md): 59 native Rust tests,
два application cycles и 15 verified artifacts. Рабочие runtime/Netwatch не менялись.
