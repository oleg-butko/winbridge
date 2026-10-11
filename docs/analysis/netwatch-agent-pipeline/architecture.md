# Взгляд разработчика WinBridge

[К отчёту](README.md). Цель — агент Netwatch может проверить фичу по устойчивому
контракту, не изучая внутренние gate scripts и не меняя WinBridge под каждый тест.

## Фундамент, который нужно сохранять

| Контракт | Почему его трудно менять | Что сохранять при расширении |
| --- | --- | --- |
| Worker — единственный execution owner. | Через него связаны acceptance, queue, process tree, output и UI. Второй runner порождает разные версии результата и потерянные процессы. | MCP/CLI/UI вызывают один service; adapters не запускают собственную execution queue. |
| Durable acceptance и idempotency. | В SQLite уже есть ключи, payload fingerprints, child IDs и recovery ранее принятых операций. | ACK только после commit; одинаковый key/payload возвращает прежнюю запись; indeterminate не исполняется повторно. |
| Исторический intent и frozen commands. | Изменение смысла старых recipe/task/workflow records может исполнить другую команду после restart. | Recipe provenance/pins и параметры фиксируются до ACK; старые записи не получают выдуманных definitions. |
| Точный источник + verified delivery. | Pins связаны с source, delta base, staging, ready, import и artifact provenance. | Неполные/shared/повреждённые исходники не становятся buildable; старые generations сохраняются. |
| Windows ownership и scope. | Job Object, suspended spawn, SID pipe/DACL, junction/path checks и deadlines отражают реальные свойства ОС. | Один process owner, явный tree cancel/stop, bounded drains и раздельные stdout/stderr. `cwd` не выдаётся за sandbox. |
| Read model и независимая свежесть. | UI и агенты уже используют IDs/status/offsets для восстановления. | Offline не переписывает task outcome; startup success не означает продолжающееся здоровье application. |
| Protocol/config/journal compatibility. | Старые peers и сохранённая история живут дольше frontend. | Capability guard до новых payload, independent versions, consistent SQLite migration backup и проверка upgrade/recovery. |

Это правильный фундамент для Netwatch. Замена WebSocket, HGFS, Rust или Tauri
не устраняет обнаруженные ограничения тестирования и сейчас не нужна. Локальная
модель доверия остаётся прежней: одна пара Windows↔Debian, `ws://`, без новой
TLS/relay инфраструктуры. Регистрация project scope не изолирует произвольный
trusted executable от файлов/сети пользователя.

## Что легко менять без нового основания

- Команды и сценарии **Netwatch**: Cargo flags, выбор тестов, fixture config,
  assertions, packaging, report parser. Они версионируются с самой фичей.
- Worker project configuration, fixed recipes и общий recipe profile.
  Изменения требуют ручного restart, но не нового transport/SQLite service.
- Клиентскую последовательность MCP вызовов, локальный Run ID journal,
  summary для пользователя и bounded polling.
- UI presentation новых результатов через общие badges/timeline/output,
  когда underlying read model определён.

`CommandHandler` уже отделяет раскрытие команды от запуска. Однако registry
runner сейчас содержит два статических handlers, а `CommandSource` — закрытый
enum recipe/direct. Это внутренний контракт одного выпуска, не dynamic plugin SDK.
Новый project script не требует нового handler; новый command kind потребует DTO,
policy, worker integration и compatibility checks.

## Ограничения, важные для следующих фич

### 1. Workflow описывает запуск service, а не произвольную проверку

`WorkflowSubmission` требует build/start/readiness/log. Worker создаёт массив
`[import, build, start, readiness, log]`; продвижение и child links используют
позиции и специальные условия для этих шагов. Тесты внутри build исполнимы,
но не имеют собственного durable результата. Build-only/test-only, несколько
suites, assertions после ready, cleanup и выборочный повтор не выражены этим DTO.
Просто добавить ещё один индекс в `workflows.rs` будет дёшево сегодня и дорого
при следующем типе шага: recovery, attention и UI придётся дописывать снова.

Durable orchestration и Store фактически находятся в `apps/worker`, а не за
универсальными Store/Runner interfaces в core. Core содержит pure policy и
конкретный CommandHandler. Наличие папки `modules` ещё не делает workflow engine
расширяемым. Разделить planner/policy и execution wiring полезно при добавлении
второго pipeline, сохранив транзакции и process ownership worker.

### 2. Immutable source и mutable build workspace пока совпадают

`verify_tree` принимает только manifest paths и два `.winbridge` metadata files.
Это сильная гарантия source identity, но стандартные Node/Tauri pipelines пишут
новые files рядом с source. Netwatch создаёт `node_modules`, `web-dist`, `release`;
даже Cargo по умолчанию пишет `target`. Вынесенный Cargo target решает Rust agent
build, но не весь portable/UI cycle.

Нужна отдельная сущность **execution workspace, полученная из verified snapshot**:
immutable source/cache остаётся прежним, а build/test имеют явный локальный
рабочий каталог и разрешённые generated paths. Копирование нельзя делать writable
hard links к базовому snapshot; build менял бы исходники cache. Отдельная копия
и проверка source paths/bytes дают более простой первый вариант.

Execution workspace фиксирует run/source pin, source file set, generated directories,
cache bindings и result path. Проверки до/после запуска отражают изменения tracked
source, а не молча считают их исходным commit. В первой версии provenance доказывает
начальное/конечное состояние, не hermetic execution против враждебного build script.
Простой fixed mutable checkout тоже возможен через direct tasks/Git bundle, но
это иной source mode со своим сериализованным update/build контрактом; не следует
незаметно приравнивать его к verified snapshot.

Commit snapshot не содержит `.git`, untracked dependencies и metadata рабочего
checkout. Git symlinks/submodules отклоняются; dirty snapshot mode не реализован.
Если build нуждается в Git metadata или внешнем дереве, проект должен объявить
это как input, а будущая source capability — сохранить его provenance. Не разрешать
тихо обращаться к случайному соседнему checkout ради успешного теста.

### 3. Agent-facing результат пока слишком близок к raw task output

В `LocalResponse::Error` есть message, без стабильного error code/outcome class.
MCP раздельно показывает JSON-RPC input errors и `isError`, но клиенту трудно
отличить недостающую capability, известный admission refusal и неизвестный
исход отправленной мутации без анализа текста.

Output API возвращает arrays of bytes; страницу до 192 KiB трудно напрямую
анализировать в контексте модели. Нет TestRun DTO, агрегата обязательных suites,
универсального чтения report/artifact, machine-readable skipped reasons и
короткого итогового verdict. Artifact catalog фиксирует historical metadata
до start, не immutable copy содержимого. Для обычной отдельной test task такого
catalog нет. `prepare_task_diagnostic` — editable preview диагностики, не API
проверенных результатов тестов.

MCP обрабатывает запросы последовательно, поддерживает tools, но не subscriptions/
resources/tasks. Списки recent operations ограничены десятью; старые IDs доступны
через get/history. Клиентское polling достаточно для первых Netwatch циклов,
но нужен удобный resume по Run ID и краткий progress/result без постоянного
возврата всех events/output. CLI `watch` уже существует; MCP wait tool пока нет.

### 4. Configuration profiles имеют узкую параметризацию

Общий профиль один на project, без inheritance/includes; bindings только
`${workspaceRoot}` и `${projectId}`. Recipe argv фиксированы, request arguments
к recipe отклоняются. В `CommandSource` нет per-task environment map.
Run ID, test selection, profile/config paths приходится передавать direct argv
или кодировать в сценарии. Это допустимый текущий обход, а не reason добавлять
непроверенный shell templating или evaluation конфигурации.

Две registries задаются отдельно: worker roots/recipes и controller source roots.
MCP Info не выдаёт source registry/toolchain preflight. Settings maintenance
остаётся local-only; добавление проекта обычным агентом не предусмотрено.

### 5. Scope и concurrency пока грубые

Config разрешает ровно одну mutation на project; одна live application на project.
Проекты с пересекающимися roots запрещены. Два агента в одном Netwatch project
делят очередь/cache/application. Ready освобождает startup lease, а direct tasks
не имеют декларации «только probe»/«меняет binary»: клиент обязан не заменять
exe работающего приложения. UI actions пользователя также могут создавать tasks.

Первый generic pipeline должен держать reservation до окончания своих tests и
cleanup, не запирать весь project навсегда после service ready. Несколько test
instances, resource-specific leases и parallel steps — дальнейшие расширения,
когда один последовательный цикл стабилен. Simple concurrency flag >1 без
cache/process/resource ownership не решит проблему.

Timeout обычной task допускает до 24 часов; startup workflow ограничен 30 минутами,
readiness — 3 минутами. Очередь входит в общий startup timeout. Cold build может
упереться в этот предел, хотя самостоятельная test task допустима. Source/cache
cleanup пока нет, история/keys сохраняются: регулярные runs требуют контроля
места и будущей явной retention policy, не удаления каталогов по glob из агента.

### 6. Предметная готовность привязана к Netwatch

`ReadinessProbe` содержит stdout token либо `NetwatchProbe`; реализация последнего
лежит в `crates/transport/src/readiness.rs`, знает service name и pipe format.
OS PID check — полезная общая гарантия, а правила health JSON/имени pipe — предметный
adapter. Следующее приложение с HTTP/status/probe иначе добавит новый named field
в protocol и ветку transport. Постепенно выделить общий probe contract и static
adapters; предметные сценарии Netwatch остаются в его репозитории.

Текущий Netwatch probe также требует `server PID == ApplicationRecord.processId`.
Это подходит для прямого запуска agent. Если start запускает launcher, IPC server
будет его sidecar с другим PID: просто заменить start executable на launcher
недостаточно. Общий contract должен уметь связать probe с конкретным owned child
на основании process ownership/Job Object; совпадение self-reported PID или
предположение по имени/родительскому PID не заменяет такой проверки.

Netwatch IPC instance/launcher isolation нужно исправлять **в Netwatch**.
WinBridge не может безопасно компенсировать SID-global endpoint, меняя только cwd.
Проверенный validation patch — временное доказательство механизма, не основа
постоянного product pipeline. Полная поддержка generic probes не отменяет этой задачи.

### 7. Уведомление не является барьером перед выполнением

Attention durable и typed; popup/sound требуют desktop и учитывают preferences.
Explicit `request_attention` ссылается на уже принятую task. Если нужен гарантированный
pre-start сигнал, это новый admission/presentation contract: нельзя обещать его
существующим вызовом после submit. Для обычных разрешённых тестов достаточно
видимого run-start event, объяснения агента и истории; не вводить обязательное
ручное подтверждение каждой проверки.

## Где расширять

| Изменение | Основные места | Что проверить вместе с ним |
| --- | --- | --- |
| Новая фича, regression/assertion, Windows сценарий Netwatch | В Netwatch: `agent/tests`, unit modules, `launcher/src-tauri`, `ui`, `scripts`, `package.json`. | Correct instance, выбранный набор, exit propagation, UI/lifecycle assertions. |
| Команды проекта, общие recipes | [config](../../../crates/core/src/config.rs), [profiles](../../../crates/core/src/profiles.rs), [runner](../../../modules/runner/src/lib.rs); конкретные команды — local config/Netwatch scripts. | Expanded argv/cwd, scope, changed-recipe refusal, provenance. |
| Workspace preparation из pinned source | [workspace-sync](../../../modules/workspace-sync/src/lib.rs), core source/path policy, [imports](../../../apps/worker/src/imports.rs). | Actual byte verification, writable-copy isolation, cancellation/restart, lease и read model. |
| Generic pipeline/step/test-result DTO | [protocol workflow](../../../crates/protocol/src/workflow.rs), новая pure planner policy в core. | Rust/TypeScript/schema parity, compatibility, limits, frozen fingerprints. |
| Durable step orchestration/results | [worker workflows](../../../apps/worker/src/workflows.rs), [Store](../../../apps/worker/src/store.rs), worker service/artifacts/history/attention. | Transaction-before-ACK, child dedup, crash boundaries, migration, cleanup outcome. |
| Agent-facing summaries/errors/read-report | [controller MCP](../../../apps/controller/src/mcp.rs), protocol responses, controller dispatch. | Bounded schema/content, old client/worker refusal, read-only semantics, lost ACK без replay. |
| Probe interface и adapters | Pure core policy; предметный static module; [transport readiness](../../../crates/transport/src/readiness.rs) и platform IPC/PID primitives. | Probe чужого PID, deadline, bounded response, cancellation и прозрачный command. |
| UI проверки pipeline/report | [WorkflowWorkbench](../../../ui/src/features/workflows/WorkflowWorkbench.tsx), task/history/attention views, desktop native adapter. | Честные passed/failed/skipped/incomplete, actual phases, output links, close/reconnect. |
| Регрессия WinBridge | Protocol/runner/worker tests, delivery tests; Windows native scripts. | Source/worktree, dedup/recovery/cancel/lease и проверенные report bytes. |

Следовать цепочке DTO → policy → worker/journal → adapter → read model/MCP/UI
→ fault tests. Предметный script может появиться раньше нового DTO, если честно
сохраняет evidence и не создаёт вторую очередь/process owner.

## Предлагаемый порядок изменений

Это рекомендации следующего развития, **не новые закрытые этапы roadmap**.

| Приоритет | Изменение и владелец | Зачем и критерий готовности |
| --- | --- | --- |
| P0 | Штатный test instance Netwatch, Windows IPC integration и версионируемый feature test script — Netwatch. | Original feature commit тестируется без скрытого patch; production и test agent/launcher одновременно работают, чужой PID/config/log/state не затрагиваются; failures дают ненулевой итог. |
| P1 | Public agent playbook + read-only preflight/короткий result — WinBridge. | Агент знает project/source/capabilities/toolchain и получает IDs, status, bounded readable output, полноту evidence и stable error/outcome class без изучения внутренних scripts. |
| P1 | Execution workspace + линейный pipeline v1 — WinBridge. | Import → prepare → command/test(s) → optional start/probe/assert → cleanup; каждый шаг durable, имеет ID/outcome и ссылку на child task. Netwatch UI build не портит immutable base. |
| P1 | Проверяемый TestRunReport и чтение сохранённых evidence — WinBridge + Netwatch. | Единый schemaVersion/counts/skips/verdict; bounded read по catalog ID с hash pin, final report переживает reconnect; обязательный отсутствующий тест делает run incomplete. |
| P2 | Общий probe contract и ограниченные typed recipe parameters — WinBridge. | Новый project probe не меняет transport; test selection/Run ID передаются без shell substitution, command/origin сохраняются до ACK. |
| P2 | Согласованная cleanup cache/workspaces policy и измерения latency — WinBridge. | Preview, reference protection для active runs/artifacts, явное применение и recovery; сокращение времени измерено на Netwatch, исходная история/keys сохраняются. |

Начать pipeline с последовательных typed steps `command`, `application_start`,
`probe`, `application_stop` и explicit result/evidence, а не произвольного DAG.
Import/prepare фиксируют source до steps. Cleanup должен иметь собственную
семантику: провал assertions вызывает попытку owned cleanup, но при journal fault
нельзя выдавать неподтверждённый stop за успех или повторять неизвестную мутацию.
Повтор одного неудачного шага не должен незаметно запускать его в другой source/
runtime generation; первый вариант может требовать нового осознанного run.

Новый `PipelineSubmission` лучше вводить отдельной capability, сохранив старый
`WorkflowSubmission` и его payload fingerprint. Legacy startup можно адаптировать
к planner, но сохранённые IDs/recipe pins/recovery semantics должны остаться прежними.
Strict old peers нельзя слать новые поля «на всякий случай»; absence нового поля
в старом payload сохраняет прежнее dedup-представление. Journal upgrade не означает
разрешённый rollback после новых записей.

Дополнительный критерий приёмки — самостоятельный consumer test: клиент из
Netwatch запускает проверку только через опубликованные MCP schemas и свой
версионируемый script, без `~/winbridge/scripts`, прямого worker CLI и чтения SQLite.
Пройденный stage 7 service gate подтверждает внутренние adapters, но не заменяет
такую проверку внешнего agent contract. Удаление private знаний из этого клиента
должно оставить доступными submit, resume, analysis и owned cleanup.

Для agent client достаточно маленького workflow helper/рекомендованного сценария
над MCP с local Run ID journal; полноценный independently released SDK имеет
смысл только при нескольких реальных потребителях. Dynamic plugin loading,
distributed workers, облачный scheduler и переписывание transport сейчас добавят
стоимость, не решая Netwatch source isolation и test evidence.

## Как внешний агент запрашивает улучшение

Запрос сопровождающему содержит:

1. Feature commit и сценарий без секретов: какой зарегистрированный project,
   конкретные MCP arguments, worker build/capabilities, Task/Import IDs.
2. Наблюдаемое ограничение: expected/actual, bounded diagnostic и проверенные logs.
3. Что нельзя решить в Netwatch script/config без потери гарантий.
4. Минимальную новую операцию/поле и критерии успеха, ошибок и recovery.
5. Требования UI, внимания пользователя и совместимости старого worker.

Например: «После `npm ci` snapshot generation не проходит следующий verified
launch из-за node_modules. Нужен execution workspace от manifest pin: подготовка
не меняет base, все команды/изменения source видны, cancel/crash оставляют
проверяемый результат. При повторе preparation key не создаёт вторую копию;
полный portable Netwatch build и test проходят без production запуска».

Для каждого нового основания нужны automatic native fault tests: lost ACK,
changed payload/recipe, build/test failure без stale start, crash между intent/
spawn/commit, timeout/cancel descendants, disconnect при running, чужой PID,
source mutation, partial report и cleanup failure. Старый startup workflow и
старый client остаются в regression. Дальше агент Netwatch проверяет свои
assertions через этот контракт, а не запускает весь WinBridge gate при каждой фиче.
