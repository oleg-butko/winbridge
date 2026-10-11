# Что тестировать и как оценивать результат

[К отчёту](README.md). WinBridge доставляет и выполняет проверки; **содержание
проверки фичи определяет Netwatch**. Import succeeded, cargo build exit 0,
health ready и «фича работает» — четыре разных утверждения.

## Уровни проверки

| Изменение Netwatch | Быстрая проверка в Debian | Обязательная Windows проверка для соответствующего риска |
| --- | --- | --- |
| Правила конфигурации, расчёт состояния | Unit tests с правильными и ошибочными inputs; regression воспроизводит прежний дефект. | Те же тесты, если поведение зависит от Windows paths/defaults/encoding. |
| Ping/pong, reconnect, будущие интервалы доступности | Управляемое время, локальный WebSocket fixture, loss/deadline/неверный pong. | Native transport integration с local fixture; при изменении межхостового поведения отдельный разрешённый Windows↔Debian fixture. |
| IPC, instance, lifecycle | Integration с отдельным socket, foreign PID, stop чужого процесса, restart. | Named pipe под обычным пользователем: SID/instance, busy pipe, ownership, cancellation/timeout, process tree. |
| Launcher supervision | Library tests без desktop features. | Native start/attach/restart/quit и сохранение externally started instance. |
| Dashboard/UI | TypeScript, component assertions; UI build. | Изолированный launcher + WebView smoke: реальные IPC данные, нужные действия/состояния, сохранение и восстановление. |
| Packaging/paths | Проверка source/export manifests и конфигурации build. | Release portable build, оба exe, version/hash, запуск из отдельного runtime; отсутствие случайных dev dependencies. |

Строки таблицы выбираются по рискам фичи; повторять все старые Windows gates
WinBridge при каждом изменении Netwatch не требуется. Достаточно стабильной
исполнительной базы и нужного набора Netwatch tests. Базовую регрессию WinBridge
повторяет его разработчик при изменении journal/process/delivery contracts.

### Что есть в нынешнем Netwatch

`package.json` задаёт `npm test` как agent tests плюс launcher library tests
с `--no-default-features`. Отдельного frontend test script нет.
`agent/tests/transport.rs` проверяет disconnected → connected/pong/RTT через
loopback fixture. `agent/tests/ipc.rs` целиком помечен `#![cfg(unix)]` и поэтому
**не выполняется на Windows**. Два Windows unit tests в `agent/src/ipc.rs`
используют собственные уникальные pipe names для busy/timeout; они не заменяют
полный managed-agent status/shutdown сценарий. Regression остановки чужого agent
в `launcher/src-tauri/src/supervisor.rs` также ограничена Unix.

Поэтому `npm test` exit 0 на Windows нельзя трактовать как доказательство всей
IPC/lifecycle логики. В отчёте надо перечислять target-specific пропуски.
`--no-default-features --lib` не запускает tray/WebView. Cross-check MSVC target
на Debian подтверждает компиляцию, но не поведение Windows.

Ближайшая предметная работа Netwatch: полноценный Windows IPC integration с
test instance, supervisor ownership/restart tests и UI assertions для изменяемых
панелей. Будущие interval history/charts из README пока нельзя включать в список
реализованных функций или покрытых тестами.

## Повторяемый сценарий проекта

Версионируемый сценарий Netwatch может иметь режимы `agent-tests`, `launcher-tests`,
`portable-build`, `runtime-smoke`, `ui-smoke`. Это **предложение для Netwatch**,
не существующие flags WinBridge. Сценарий принимает явно выбранный набор и Run ID,
показывает фазы/команды и выдаёт маленький машинный report вместе с полными logs.
Так агенту не нужно собирать новый непрозрачный PowerShell blob для каждого запуска.

Каждый прогон закрепляет source SHA/manifest hash, hash самого сценария, реальные
argv/cwd, версии Rust/Cargo/Node/npm/MSVC и использованные build flags.
Записываются версии worker/adapter и generation, OS/архитектура, config fixture hash
без секретов, cache mode и Task/Import/Workflow/Application IDs. Lockfiles должны
входить в commit; `--locked`/`npm ci` фиксируют dependency graph, но не превращают
сборку в hermetic build: toolchain, build scripts и внешние inputs тоже влияют.

Сначала дешёвые тесты, затем Windows unit/integration, потом portable/native UI,
если это нужно. Cold-cache проверять в отдельном новом cache; warm-cache запускать
повторно для скорости. Это разные измерения, не повод регулярно чистить target.
Local fixtures используют уникальные ports/pipe names и deadline; public VPS
и рабочая конфигурация пользователя не нужны для регулярной регрессии.

Startup probe проверяет только готовность. Для runtime фичи нужны assertions,
например: конкретное изменение status после simulated loss, восстановленный
RTT после reconnect, сохранённый интервал после restart. Проверять ожидаемый PID
владельца IPC, не только `service=netwatch` или совпадение app version.
Настоящий процесс и process tree принадлежат worker; test helper не должен
самовольно daemonize приложение и терять ownership.

Для UI предусмотреть заметное тестовое название/баннер, текущую фазу и срок.
Автоматизировать воспроизводимые действия и capture фактического состояния;
скриншот один по себе не подтверждает успешное нажатие. Ручную Windows проверку
оставить для UX, который сценарий ещё не измеряет, явно указав это ограничение.
Native UI режим требует активной пользовательской desktop session, а не только
связи worker с controller.

## Изоляция — prerequisite запуска приложения

| Область | Что требуется |
| --- | --- |
| Source | Exact feature commit; validation patch, если он есть, показан отдельно и не выдаётся за тест исходного commit. |
| Config/data/logs | Уникальный test runtime; explicit desktop role и локальные fixtures; production файлы не читаются для изменения и не перезаписываются. |
| IPC | Instance-specific pipe и проверенный owned PID. `NETWATCH_RUNTIME_DIR` сам по себе Windows pipe не меняет. |
| Launcher | Instance должен учитываться в single-instance механизме, IPC bridge, sidecar supervisor и пользовательском state; отдельный путь exe недостаточен. |
| Processes | IDs только собственных приложений/задач; stop/cancel через worker, ожидание фактического terminal outcome. |
| Reports | Новый run directory, фиксированные bytes/SHA-256, ready marker после завершения записи; нет перезаписи старых доказательств. |

Текущий Netwatch gate патчит только agent, с пустыми peers; он доказывает этот
service smoke, но не отдельный launcher и не сетевую фичу. До штатной поддержки
instance запуск исходного portable Netwatch рядом с production может attach к
чужому agent или активировать существующее окно через single-instance plugin.
Нельзя считать это успешным native тестом своего build.

## Разбор результатов по порядку

1. **Проверить идентичность.** Source/manifest pins, IDs и ожидаемый worker совпали?
   Изменённый commit или неизвестный patch делает evidence неприменимым к фиче.
2. **Проверить execution outcome.** Terminal TaskRecord, exit code и reason.
   Offline означает отсутствие свежих данных; indeterminate не означает failed assertion.
3. **Проверить полноту output/report.** Дочитать все страницы stdout/stderr,
   проверить quotas/removed output/gaps, final report и schema version.
   stderr может содержать нормальную compiler diagnostic при exit 0.
4. **Проверить набор.** Какие test binaries/cases выбраны, сколько passed/failed/
   ignored/skipped, почему? Пустой набор и `--filter`, не совпавший ни с одним
   тестом, могут дать exit 0. «Expected tests not run» означает incomplete verification.
5. **Проверить assertions фичи.** Есть ли regression на исходную проблему,
   негативный случай и наблюдаемый результат, а не просто отсутствие crash?
6. **Проверить cleanup.** Собственное приложение stopped, окна закрыты, нет
   оставшихся descendants. Успешные assertions с неподтверждённой очисткой
   требуют отдельной записи о cleanup, а не безусловного итогового passed.

| Класс проблемы | Пример | Следующий шаг |
| --- | --- | --- |
| Product regression | Assertion failed, wrong status, настоящий runtime crash после ready. | Исправить Netwatch, сохранить failing evidence и добавить regression. |
| Build failure | Compiler/typecheck error в изменённом коде. | Исправить исходники; не запускать старый exe из cache. |
| Environment blocked | Нет MSVC/dependency/WebView, нет разрешённой desktop session. | Исправить подготовку среды через пользователя; не маскировать как defect фичи. |
| Harness failure | Script exception, parser не распознал результаты, ноль ожидаемых тестов. | Исправить сценарий своего проекта; повтор нового прогона только после известного исхода. |
| Delivery failure | Wrong hash, missing base, incomplete package. | Не начинать build; разбирать Import ID и delivery pins. |
| Execution unknown | Потерян ACK/worker crash между эффектом и journal commit. | Сверить прежние key/IDs и последствия; не делать новый автоматический submit. |
| Evidence incomplete | Output truncated/removed, нет final report или hashes. | Сообщить, что доказательств недостаточно; не восстанавливать придуманные результаты из UI tail. |

Сохранять первые неудачные прогоны. Для потенциально flaky проверки повтор имеет
новый Run ID и объяснение; в результате видны все attempts, а не только зелёный.
Regression fixtures должны воспроизводить риск, не повторять реализацию построчно.

## Отчёт для агента и пользователя

Минимальный **предлагаемый** report contract сценария проекта:

```text
schemaVersion, runId
source: commit, snapshotId, manifestSha256, optional validationPatchSha256
environment: OS/arch/toolchain versions, workerBuild, workerGeneration, cacheMode
operations: importId, taskIds, optional workflowId/applicationId
checks[]: name, command, outcome, exitCode, duration, counts, skippedReason
evidence[]: local/shared path, bytes, sha256, producingTaskId
cleanup: outcome, ownedProcessIds, reason
verdict: passed | failed | blocked | incomplete | indeterminate
```

Это не DTO WinBridge: пока report создаёт и проверяет сценарий/клиент Netwatch.
Вердикт passed требует выполненного обязательного набора, успешных assertions,
полных доказательств и подтверждённой cleanup. Пропуски допустимы только когда
они заранее исключены из обязательного набора и отражены в report.

В существующем MCP нет универсального read-artifact/download-file и регистрации
такого TestRunReport. Workflow artifacts дают path/hash после build, но файл
может позже измениться: catalog не является immutable artifact store. Сейчас
сценарий может вывести bounded JSON summary в stdout и, при согласованном доступе,
опубликовать полные reports в namespace WinBridge shared через staging/ready.
Debian сверяет actual bytes/SHA-256 после чтения. Не помещать secret config/output
в report без redaction и не считать один hash показателем корректности теста.
