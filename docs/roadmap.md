# Постепенная реализация и расширение

Правило каждого этапа: маленький рабочий сценарий → реальные события → понятный UI → проверка сбоев → обновление handoff. Подготовка архитектуры не считается выполнением runtime функции. Сетевую или файловую часть можно развивать независимо от UI, но этап закрывается только с видимым результатом для пользователя.

## Архитектурный переход A0–A9 — новая задача 2026-10-13

Этапы 0–7 ниже завершены и сохраняются как история. Пользователь поручил
объединить Actor Model + FSM, consistency/extensibility и consistency techniques,
повторно проверить план, зафиксировать его и начать выполнение.
[План и зависимости](plans/architecture-transition/README.md),
[повторное ревью](plans/architecture-transition/review.md).

- [x] A0: [frozen legacy contracts/journal tests и ADR инвариантов](plans/architecture-transition/baseline.md); 14 новых/171 workspace tests passed, Windows cross-check tests passed. Runtime не меняется.
- [x] A1: [Task FSM через прежний wire API](plans/architecture-transition/task-fsm.md); 182 workspace tests и [66 native Windows tests](plans/architecture-transition/windows-task-fsm-gate-2026-10-13.md) passed, runtime/wire/schema совместимы.
- [x] A2: [durable transitions](plans/architecture-transition/durable-transitions.md); task revisions/CAS A2.1 завершён (197 workspace/[107 native tests](plans/architecture-transition/windows-task-cas-gate-2026-10-13.md)); [A2.2 domain CAS/atomic links](plans/architecture-transition/domain-transitions.md) завершён (213 workspace/[125 native tests](plans/architecture-transition/windows-domain-cas-gate-2026-10-13.md)); [A2.3.1 authority/attempt tokens](plans/architecture-transition/ownership-context.md) завершены (229 workspace/[141 native tests](plans/architecture-transition/windows-ownership-gate-2026-10-13.md)); [A2.3.2 durable leases](plans/architecture-transition/project-resource-leases.md) завершены (245 workspace/[157 native tests](plans/architecture-transition/windows-resource-leases-gate-2026-10-13.md)); [A2.4.1 physical process reconciliation](plans/architecture-transition/physical-reconciliation.md) завершён ([164 native tests](plans/architecture-transition/windows-reconciliation-gate-2026-10-13.md)); [A2.4.2a journal inspection](plans/architecture-transition/journal-inspection.md) завершён ([176 native tests](plans/architecture-transition/windows-inspection-diagnostic-2026-10-13.md)); controlled archive/new-pair admission завершён ([197 native tests](plans/architecture-transition/windows-recovery-pair-gate-2026-10-14.md)).
- [x] A3: [discovery/read model](plans/architecture-transition/discovery.md); A3.1 завершён (286 workspace, 24 Rust/TS cases, [203 native tests](plans/architecture-transition/windows-discovery-gate-2026-10-14.md)); [A3.2a foundation](plans/architecture-transition/readiness-foundation.md) завершён ([39 native tests/desktop check](plans/architecture-transition/windows-readiness-gate-2026-10-14.md)); [A3.2b runtime producer](plans/architecture-transition/paired-runtime.md) actual paired/restart gate passed; [A3.2c UI projection](plans/architecture-transition/readiness-projection.md) [actual installed pair/projection gate](plans/architecture-transition/windows-projection-pair-2026-10-15.md) завершён; A3.3 read model — [48 native tests/desktop gate](plans/architecture-transition/windows-operation-pages-gate-2026-10-15.md) пройден; [actual installed read-model gate](plans/architecture-transition/windows-operation-pages-installed-2026-10-15.md) завершён.
- [x] A3.3: [согласованные страницы операций](plans/architecture-transition/operation-pages.md), A3.2 [actual pair/projection gate](plans/architecture-transition/windows-projection-pair-2026-10-15.md) завершён; A3.3 native candidate прошёл 48 tests/desktop compile, actual installed новый контракт проверен; A3 завершён.
- [ ] A4: global resources, очередь и fairness для нескольких проектов; первый пакет [A4.1 work slots](plans/architecture-transition/work-scheduling.md) — кандидат, native gate ожидается.
- [ ] A5: local actor pilot и постепенная миграция owners.
- [ ] A6: Run/Step и contract-tested one-shot executor/report.
- [ ] A7: outbox + inbox для нужного durable receiver (условный этап).
- [ ] A8: два независимых потребителя и automatic paired native gates.
- [ ] A9: расширение FSM/контрактов; дальнейшие изменения по доказанной потребности.

A0, A1, A2.1, A2.2, A2.3.1, A2.3.2 и A2.4.1 завершены;
A2.4.2a завершён: local/browser и
[176 native Windows tests](plans/architecture-transition/windows-inspection-diagnostic-2026-10-13.md) passed;
следующий пакет A2.4.2b — controlled restore/new incarnation/admission. До native gates process/runtime
изменения не считаются готовыми. Пользователь выбрал
[новую пару/журнал с архивом](plans/architecture-transition/restore-admission-options.md).
[A2.4.2b.1 request fence](plans/architecture-transition/fenced-requests.md) завершён:
271 local tests, browser gate и [186 native tests/desktop compile](plans/architecture-transition/windows-fenced-request-gate-2026-10-13.md) прошли.
Archive preparation и physical cutover [b.2/b.3](plans/architecture-transition/recovery-pair.md)
завершены: 280 local tests, cross-checks/browser и
[197 native tests/desktop compile](plans/architecture-transition/windows-recovery-pair-gate-2026-10-14.md) passed.
Полный A2 закрыт после финального ревью; далее A3 discovery/read model.

## Этап 0 — подготовка репозитория

- [x] Отдельный Git repository, Rust workspace и границы будущих модулей.
- [x] DTO для recipe/direct task и события, CLI help/describe.
- [x] UI prototype с fixtures: timeline, параметры, output, успех/ошибка/потеря свежести.
- [x] Принятые локальные решения, HGFS схема, UX, ADR и handoff.
- [x] Lockfiles и воспроизводимые проверки.

Результат этапа 0: сборка Rust/UI и contract tests проходят; на момент его завершения runtime отсутствовал. Текущее состояние следующего этапа описано ниже.

## Этап 1 — одна команда и реальный журнал на Windows

Статус 2026-10-05: этап завершён по [Windows отчёту и пяти подтверждённым native UI шагам](done/stage0-7/stage1/windows-gate-2026-10-05.md). Worker объявляет implementedCapabilities `task_submit`, `task_cancel`; на момент коммита этапа 1 controller оставался scaffold. Настройки validation и desktop находятся рядом с exe, отчёты — в shared. Дополнительно исправлено сохранение формы команды при повторном открытии окна; этот UI fix проверен Rust storage tests, browser integration и дополнительно подтверждён пользователем на Windows вместе с dev:ui.

- [x] Runtime config loader: один project, разрешённые recipes/direct, относительный cwd, timeout. Версионирование и actionable validation errors.
- [x] Windows worker выполняет команду в локальном fixture project. Direct executable одновременно читает stdout/stderr; recipe использует тот же service.
- [x] Durable task ID, accepted до ACK, idempotency key/payload check, локальное task/output хранилище. State transitions и recovery записываются явно.
- [x] User-scoped named pipe и Tauri shell; desktop только вызывает worker и читает read model. SID + WinBridge instance ID.
- [x] UI показывает источник, executable, массив аргументов, cwd, timestamps, output и exit code; реальные данные через adapter.
- [x] Управляемое дерево процессов, timeout и cancel. Закрытие UI не останавливает worker.

Критерий: пользователь выполняет fixture команду, видит успех и ошибку, отменяет длительный process tree; дубликат запроса не создаёт второй процесс. Сбой после запуска виден как interrupted/indeterminate без молчаливого повторения. Windows проверка обязательна; Linux cross-compile её не заменяет.

## Этап 2 — Debian управляет Windows по локальной сессии

Статус 2026-10-05: [настоящий Windows runtime gate](done/stage0-7/stage2/windows-session-gate-2026-10-05.md) пройден: шесть групп, включая Debian→Windows execution, cancel, dedup и reconnect с тем же PID. Подключение и три новых native UI наблюдения подтверждены пользователем; этап 2 завершён. Адреса VMnet8: VM 192.168.44.203, host 192.168.44.1. [Windows инструкция](validation/windows-session-validation.md).

- [x] Plain `ws://` listener на выбранном локальном IP Debian, Windows outbound connection; никаких TLS/certificates/relay.
- [x] Входящие IP не блокируются; источники активности записываются в JSONL с ротацией и видимыми ошибками записи (решение 2026-10-05).
- [x] Handshake, wire version, реализованные capabilities, instance/generation IDs, request/response correlation, bounded message sizes.
- [x] CLI для submit/get/cancel и подписки на события через controller. Controller не выдаёт собственный статус за подтверждение исполнения worker.
- [x] Worker сохраняет задачу перед подтверждением; reconnect восстанавливает её по прежнему ID и события по sequence.
- [x] UI отдельными состояниями показывает controller online/offline, последнее подтверждение задачи, очередь и возможные действия.
- [x] Постоянный Windows runtime из локального settings JSON; повторная подготовка сохраняет configs/форму/state, диагностика различает TCP/WebSocket/Hello.

Критерий: агент Debian запускает задачу в Windows fixture project, получает output; разрыв соединения не останавливает процесс, reconnect возвращает результат, повтор key не дублирует исполнение. Неверная версия и timeout имеют понятную причину в UI.

## Этап 3 — готовый snapshot через VMware Shared Folders

Статус 2026-10-06: [Windows runtime gate пройден](done/stage0-7/stage3/windows-snapshot-gate-2026-10-06.md), 19 actual import/build/run/cancel/junction checks. Пользователь подтвердил imports/preview/build/failure/history в native UI. По его замечаниям добавлены shared package select, сохранение delivery form и понятный отказ сборки непроверенного snapshot; 48 Rust tests и реальная UI integration проходят. Этап 3 завершён по решению пользователя: UI исправления приняты, подробная ручная проверка отложена. Это принятие не обозначается как дополнительный выполненный Windows UI test.

- [x] Общие Rust/TypeScript DTO manifest/ready и bounded metadata preflight: Windows пути, case collisions, file/directory conflicts, hashes/size syntax и byte counts.
- [x] Выбор mapping `/mnt/hgfs/shared` ↔ `K:\VMs\shared` и отдельного Windows workspace root.
- [x] Capture точного commit SHA из Git objects; dirty working tree исключён.
- [ ] Immutable dirty snapshot с явной маркировкой — отдельное последующее расширение.
- [x] Namespace, `.part`, manifest, ready marker, RPC import и локальный verified staging.
- [x] Policy/actual-byte/link guards и Windows compile для path/size/hash/traversal/collision/reserved names/incomplete transfers.
- [x] Настоящий Windows runtime gate: import/build/run/cancel/NTFS junction, 19 checks.
- [x] Пользователь принял новые UI исправления; подробные ручные наблюдения отложены по его решению.
- [x] Workspace lease: импорт и сборка одного поколения не пересекаются. Runtime configs и пользовательские данные сохраняются отдельно.
- [x] UI preview состава snapshot, bytes/files, progress и причины отказа, адреса обоих представлений shared folder.

Критерий: build запускается из verified Windows snapshot; потеря mount или обрыв записи не ведёт к сборке неполных исходников. Повтор import не создаёт другую задачу. Не изменяются файлы вне namespace/зарегистрированного workspace.

## Этап 4 — первый полный цикл разработки

Статус 2026-10-06: этап завершён. Автоматический Windows прогон `20261006-163840-a97da6e3` подтвердил 41 native test, native desktop check и полный import/build/artifact/start/IPC/log/dedup/stop на отдельном Netwatch clone. [Доказательства](done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md), [изоляция](validation/netwatch-validation.md). Editable profiles/drafts, output filters и artifact UI проверены browser integration; подробные ручные Windows UI наблюдения отложены пользователем. Пользователь обновил runtime до 523fc5b; source commit и features подтверждены read-only запросом 2026-10-07.

- [x] Workflow fixture: выбрать SHA → publish/import → build → start → stdout readiness → log tail.
- [x] Принятый профиль и step IDs в journal; ошибка сборки блокирует запуск следующего шага, dedup не создаёт повторный процесс.
- [x] Process registry для приложений, отдельно от одноразовых build tasks; явный stop и indeterminate recovery.
- [x] Log read/tail: byte offset, UTF-8, truncation/rotation/prefix rewrite, page/UI квота. Task output сохраняется в journal; файл лога читается у приложения.
- [x] UI workflow steps/timeline, ссылки на build/start tasks, app status и связанный лог; принятие request не считается завершением.
- [x] Автоматические Windows regression через старый fixed worker, isolated source/build/state и checksum-verified shared report.
- [x] Netwatch profile с отдельными config/logs/IPC и фактическим health/PID probe; production экземпляр не использовался.
- [x] Отдельные editable именованные profiles/drafts, фильтрация output и artifact catalog; Rust storage и browser integration прошли.

Критерий: на примере Netwatch агент получает стабильный snapshot, собирает на Windows, запускает его, читает лог и видит все шаги в UI. Падение приложения и провал readiness показываются отдельно от успешной сборки.

## Этап 5 — внимание пользователя и нормальная ежедневная работа

Дополнение 2026-10-10: исправлены ложная dirty configuration после undo проекта,
неограниченное ожидание закрытия и no-op popup hide. [Диагностический desktop log](guides/desktop-diagnostics.md)
и [42 Windows tests + два native UI запуска](done/stage0-7/stage5/windows-desktop-diagnostics-gate-2026-10-10.md)
проверены автоматически; это исправления завершённого этапа, отдельный manual gate не требуется.

План и порядок реализации согласованы 2026-10-07: [объяснение этапа 5](done/stage0-7/stage5/stage5-plan.md).
Первый сценарий — читаемый PowerShell и долговечный список внимания с acknowledgement.

Первый сценарий реализован: [журнал внимания и границы](guides/attention.md). Task outcomes,
dedup и acknowledgements сохраняются у worker; UI раскрывает EncodedCommand и
предлагает task links/explicit acknowledgement. [Tray/popup/sound/preferences](guides/companion.md)
реализованы и [прошли автоматический Windows gate](done/stage0-7/stage5/windows-companion-gate-2026-10-07.md):
21 core + 9 attention tests, два native UI запуска, 16 verified artifacts.
[Первый прерванный прогон](done/stage0-7/stage5/windows-companion-attempt-2026-10-07.md) остаётся историей;
исправлены обозначение тестового окна, защита close и публикация отчёта.
Workflow/app attention реализованы по [плану](done/stage0-7/stage5/workflow-attention-plan.md) и
[прошли автоматический Windows gate](done/stage0-7/stage5/windows-workflow-attention-gate-2026-10-08.md):
19 backend tests, actual popup → точный workflow → явный UI ACK, 12 verified artifacts. История/диагностика реализованы, [автоматический Windows gate](done/stage0-7/stage5/windows-history-gate-2026-10-08.md) пройден. [MCP adapter](guides/mcp.md) также прошёл actual Windows gate; projects/connection settings и согласованная output retention также прошли [native gate](done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md).
[Автоматические Windows проверки](done/stage0-7/stage5/windows-attention-gate-2026-10-07.md): 7 native tests
прошли через установленный worker без повторения завершённого workflow gate.

[Ревью](done/stage0-7/stage5/review-attention-2026-10-07.md) исправило fresh running после failed result commit.
Сценарий [tray/popup/sound/preferences](done/stage0-7/stage5/tray-plan.md) завершён; план был зафиксирован
коммитом `bbba964` до реализации. План workflow/app attention зафиксирован `aeae684`
до реализации; сценарий завершён. [История/диагностика](guides/history-diagnostics.md) добавляет keyset pages, старые IDs, keyboard actions, redacted editable task report и storage usage; [Windows gate](done/stage0-7/stage5/windows-history-gate-2026-10-08.md) пройден: 13 tests, actual UI и 17 artifacts. [MCP](done/stage0-7/stage5/mcp-plan.md) и settings/output retention также завершены.

- [x] Attention event → Tauri popup/tray badge/встроенный звук. Dedup, rate limit, quiet hours, severity.
- [x] Открытие нужной task page из уведомления, acknowledged state; pending attention при закрытом companion.
- [x] Workflow/application attention: typed targets, единый сигнал родительского workflow, атомарная запись и native target navigation gate.
- [x] История task/import/workflow/application, поиск/фильтры/keyset pages, точные старые IDs, быстрые действия и keyboard shortcuts.
- [x] Projects/connection settings, навигация разделов/controls с клавиатуры и защита незаписанных правок.
- [x] Editable task diagnostic с redaction, default omission, create-only save и проверкой readback/SHA-256.
- [x] Output paging/quota, recovery без UI и read-only размеры журнала.
- [x] Согласованная output retention policy и настройки: preview/atomic cleanup, protected targets, durable results и opt-in automatic cleanup. Metadata/history/dedup не удаляются.
- [x] [MCP stdio adapter](guides/mcp.md): 30 typed tools над теми же RPC/handler; contract, actual Unix lost ACK и [Windows gate](done/stage0-7/stage5/windows-mcp-gate-2026-10-08.md) прошли.

Критерий: пользователь понимает, что сделал агент, получает уместный сигнал, открывает причину и может продолжить работу. Рутинные разрешённые действия не требуют повторяющихся confirmations.

## Этап 6 — скорость и объёмы после измерений

Статус 2026-10-10: этап 6 завершён [automatic chunk/resume gate](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md). Первый сценарий: [воспроизводимый baseline](done/stage0-7/stage6/stage6-plan.md) существующей доставки,
Windows import/build, HGFS tree/archive и объёмов output/journal. План зафиксирован
после ревью этапа 5; выбранные расширения теперь измерены и проверены.

- [x] [Baseline HGFS tree/archive и time-to-build](done/stage0-7/stage6/performance-baseline-2026-10-09.md) с fixture параметрами: 3 повтора, 59 actual Windows операций, output/control/cancel/storage evidence.
- [x] [Первая измеренная оптимизация capture](done/stage0-7/stage6/performance-capture-gate-2026-10-09.md): bounded Git batch, 13.58× в 9 matched pairs, identical hashes и 21 Windows operation без изменения wire/ready.
- [x] [Incremental full/delta](done/stage0-7/stage6/windows-delta-gate-2026-10-09.md): три fresh Debian → Windows one-file commit; ZIP 2 367 994→193 bytes, publication/import timings и actual build/output/control/cancel/attention.
- [x] [Delta от verified snapshot](guides/snapshot-delta.md): schema 2/pinned base/local source cache, full target verification, CLI/MCP/UI; 40 native tests и cross-host gate прошли. Старые поколения сохраняются; удаление отражается новым составом.
- [x] [Пакетное чтение output и виртуализация строк](done/stage0-7/stage6/windows-output-gate-2026-10-10.md): bounded pages/cursors/tails, не более 28 DOM rows; 11 Windows tests, три native IPC пары, hashes/dedup/restart/control и browser integration.
- [x] [Chunk/resume больших объектов](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md): manifest-pinned 1 MiB blocks, explicit import после cancel/crash, exact Debian pins и offline build; 39 native tests, shared ZIP bytes на 75% меньше.
- [x] Оценена необходимость SocketDelivery/дальнейшей list virtualization: текущие gates не выявили необходимости. Shared resume и bounded output/history достаточно для измеренных сценариев; другой adapter и дополнительная виртуализация выбираются при новых данных.

Критерий: измеряемое улучшение выбранного сценария, отсутствие регрессии cancel/attention latency, корректное восстановление после прерывания. Не менять транспорт ради обещания абстрактной скорости.

## Этап 7 — каркас для следующих проектов

Первый вертикальный сценарий реализован: [общие recipe profiles](guides/project-profiles.md),
статический CommandHandler/runner, metadata и общий JournalTimeline. Task origin и
workflow recipe definitions сохраняются до ACK; queued workflow с изменённым
рецептом не начинает новые import/build/start. [Automatic Windows gate](done/stage0-7/stage7/windows-profiles-gate-2026-10-10.md)
passed: 68 native tests, five verified artifacts; 152 workspace tests и full
browser integration проходят. [Второй прикладной сценарий](done/stage0-7/stage7/windows-projects-gate-2026-10-10.md)
прошёл: isolated Netwatch и WinBridge используют общий профиль, 59 native Rust
tests и оба application cycles, 15 verified artifacts. 153 workspace tests;
**этап 7 завершён 2026-10-10**.

- [x] Статические handler interfaces runner/workspace-sync закреплены [контрактами](guides/project-integration.md) и проверены на двух реальных проектах; ACK/journal/processes остаются у worker.
- [x] Project profiles и reusable recipes; общий UI renderer событий и capability metadata.
- [x] Второе приложение: WinBridge worker рядом с isolated Netwatch, без изменений transport/RPC и копирования task service; actual IPC/log/artifacts/managed stop/restart подтверждены.
- [x] Необходимость SDK/package versioning оценена: оба consumer используют статический workspace contract, отдельный SDK не нужен. Динамические plugins и другие ОС — отдельные запросы.

Критерий: следующий проект подключается конфигурацией/модулем, сохраняет journal/cancel/recovery и получает понятный UI.

Этап 5 завершён 2026-10-09: [Windows settings/retention gate](done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md)
подтвердил 36 backend tests, два native UI запуска и 19 artifacts.
Пользователь выбрал manual restart для config и включаемую automatic output cleanup;
история/ключи/attention/snapshots/reports остаются. Все сценарии этапа 5 прошли
автоматические Windows gates; новый ручной gate не назначается.

## После основного roadmap

Этапы 0–7 завершены. Новый цикл A0–A9 описан в начале этого документа.
Следующий проект подключается по [готовой инструкции](guides/project-integration.md)
через registry/recipe profile и прежний workflow/journal/cancel/recovery. Native
evidence и актуальный source archive находятся в handoff. Новые возможности
выбираются по следующей пользовательской задаче: dirty snapshots, source-cache
cleanup или другой delivery adapter остаются отдельными расширениями с собственными
критериями. SDK/plugins без потребителя не вводить. Старые snapshots/reports/history
не очищать; после reboot заново проверять связь и build identity.
