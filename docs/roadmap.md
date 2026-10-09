# Постепенная реализация и расширение

Правило каждого этапа: маленький рабочий сценарий → реальные события → понятный UI → проверка сбоев → обновление handoff. Подготовка архитектуры не считается выполнением runtime функции. Сетевую или файловую часть можно развивать независимо от UI, но этап закрывается только с видимым результатом для пользователя.

## Этап 0 — подготовка репозитория

- [x] Отдельный Git repository, Rust workspace и границы будущих модулей.
- [x] DTO для recipe/direct task и события, CLI help/describe.
- [x] UI prototype с fixtures: timeline, параметры, output, успех/ошибка/потеря свежести.
- [x] Принятые локальные решения, HGFS схема, UX, ADR и handoff.
- [x] Lockfiles и воспроизводимые проверки.

Результат этапа 0: сборка Rust/UI и contract tests проходят; на момент его завершения runtime отсутствовал. Текущее состояние следующего этапа описано ниже.

## Этап 1 — одна команда и реальный журнал на Windows

Статус 2026-10-05: этап завершён по [Windows отчёту и пяти подтверждённым native UI шагам](windows-gate-2026-10-05.md). Worker объявляет implementedCapabilities `task_submit`, `task_cancel`; на момент коммита этапа 1 controller оставался scaffold. Настройки validation и desktop находятся рядом с exe, отчёты — в shared. Дополнительно исправлено сохранение формы команды при повторном открытии окна; этот UI fix проверен Rust storage tests, browser integration и дополнительно подтверждён пользователем на Windows вместе с dev:ui.

- [x] Runtime config loader: один project, разрешённые recipes/direct, относительный cwd, timeout. Версионирование и actionable validation errors.
- [x] Windows worker выполняет команду в локальном fixture project. Direct executable одновременно читает stdout/stderr; recipe использует тот же service.
- [x] Durable task ID, accepted до ACK, idempotency key/payload check, локальное task/output хранилище. State transitions и recovery записываются явно.
- [x] User-scoped named pipe и Tauri shell; desktop только вызывает worker и читает read model. SID + WinBridge instance ID.
- [x] UI показывает источник, executable, массив аргументов, cwd, timestamps, output и exit code; реальные данные через adapter.
- [x] Управляемое дерево процессов, timeout и cancel. Закрытие UI не останавливает worker.

Критерий: пользователь выполняет fixture команду, видит успех и ошибку, отменяет длительный process tree; дубликат запроса не создаёт второй процесс. Сбой после запуска виден как interrupted/indeterminate без молчаливого повторения. Windows проверка обязательна; Linux cross-compile её не заменяет.

## Этап 2 — Debian управляет Windows по локальной сессии

Статус 2026-10-05: [настоящий Windows runtime gate](windows-session-gate-2026-10-05.md) пройден: шесть групп, включая Debian→Windows execution, cancel, dedup и reconnect с тем же PID. Подключение и три новых native UI наблюдения подтверждены пользователем; этап 2 завершён. Адреса VMnet8: VM 192.168.44.203, host 192.168.44.1. [Windows инструкция](windows-session-validation.md).

- [x] Plain `ws://` listener на выбранном локальном IP Debian, Windows outbound connection; никаких TLS/certificates/relay.
- [x] Входящие IP не блокируются; источники активности записываются в JSONL с ротацией и видимыми ошибками записи (решение 2026-10-05).
- [x] Handshake, wire version, реализованные capabilities, instance/generation IDs, request/response correlation, bounded message sizes.
- [x] CLI для submit/get/cancel и подписки на события через controller. Controller не выдаёт собственный статус за подтверждение исполнения worker.
- [x] Worker сохраняет задачу перед подтверждением; reconnect восстанавливает её по прежнему ID и события по sequence.
- [x] UI отдельными состояниями показывает controller online/offline, последнее подтверждение задачи, очередь и возможные действия.
- [x] Постоянный Windows runtime из локального settings JSON; повторная подготовка сохраняет configs/форму/state, диагностика различает TCP/WebSocket/Hello.

Критерий: агент Debian запускает задачу в Windows fixture project, получает output; разрыв соединения не останавливает процесс, reconnect возвращает результат, повтор key не дублирует исполнение. Неверная версия и timeout имеют понятную причину в UI.

## Этап 3 — готовый snapshot через VMware Shared Folders

Статус 2026-10-06: [Windows runtime gate пройден](windows-snapshot-gate-2026-10-06.md), 19 actual import/build/run/cancel/junction checks. Пользователь подтвердил imports/preview/build/failure/history в native UI. По его замечаниям добавлены shared package select, сохранение delivery form и понятный отказ сборки непроверенного snapshot; 48 Rust tests и реальная UI integration проходят. Этап 3 завершён по решению пользователя: UI исправления приняты, подробная ручная проверка отложена. Это принятие не обозначается как дополнительный выполненный Windows UI test.

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

Статус 2026-10-06: этап завершён. Автоматический Windows прогон `20261006-163840-a97da6e3` подтвердил 41 native test, native desktop check и полный import/build/artifact/start/IPC/log/dedup/stop на отдельном Netwatch clone. [Доказательства](windows-workflow-gate-2026-10-06.md), [изоляция](netwatch-validation.md). Editable profiles/drafts, output filters и artifact UI проверены browser integration; подробные ручные Windows UI наблюдения отложены пользователем. Пользователь обновил runtime до 523fc5b; source commit и features подтверждены read-only запросом 2026-10-07.

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

План и порядок реализации согласованы 2026-10-07: [объяснение этапа 5](stage5-plan.md).
Первый сценарий — читаемый PowerShell и долговечный список внимания с acknowledgement.

Первый сценарий реализован: [журнал внимания и границы](attention.md). Task outcomes,
dedup и acknowledgements сохраняются у worker; UI раскрывает EncodedCommand и
предлагает task links/explicit acknowledgement. [Tray/popup/sound/preferences](companion.md)
реализованы и [прошли автоматический Windows gate](windows-companion-gate-2026-10-07.md):
21 core + 9 attention tests, два native UI запуска, 16 verified artifacts.
[Первый прерванный прогон](windows-companion-attempt-2026-10-07.md) остаётся историей;
исправлены обозначение тестового окна, защита close и публикация отчёта.
Workflow/app attention реализованы по [плану](workflow-attention-plan.md) и
[прошли автоматический Windows gate](windows-workflow-attention-gate-2026-10-08.md):
19 backend tests, actual popup → точный workflow → явный UI ACK, 12 verified artifacts. История/диагностика реализованы, [автоматический Windows gate](windows-history-gate-2026-10-08.md) пройден. [MCP adapter](mcp.md) также прошёл actual Windows gate; projects/connection settings и согласованная output retention также прошли [native gate](windows-settings-retention-gate-2026-10-09.md).
[Автоматические Windows проверки](windows-attention-gate-2026-10-07.md): 7 native tests
прошли через установленный worker без повторения завершённого workflow gate.

[Ревью](review-attention-2026-10-07.md) исправило fresh running после failed result commit.
Сценарий [tray/popup/sound/preferences](tray-plan.md) завершён; план был зафиксирован
коммитом `bbba964` до реализации. План workflow/app attention зафиксирован `aeae684`
до реализации; сценарий завершён. [История/диагностика](history-diagnostics.md) добавляет keyset pages, старые IDs, keyboard actions, redacted editable task report и storage usage; [Windows gate](windows-history-gate-2026-10-08.md) пройден: 13 tests, actual UI и 17 artifacts. [MCP](mcp-plan.md) и settings/output retention также завершены.

- [x] Attention event → Tauri popup/tray badge/встроенный звук. Dedup, rate limit, quiet hours, severity.
- [x] Открытие нужной task page из уведомления, acknowledged state; pending attention при закрытом companion.
- [x] Workflow/application attention: typed targets, единый сигнал родительского workflow, атомарная запись и native target navigation gate.
- [x] История task/import/workflow/application, поиск/фильтры/keyset pages, точные старые IDs, быстрые действия и keyboard shortcuts.
- [x] Projects/connection settings, навигация разделов/controls с клавиатуры и защита незаписанных правок.
- [x] Editable task diagnostic с redaction, default omission, create-only save и проверкой readback/SHA-256.
- [x] Output paging/quota, recovery без UI и read-only размеры журнала.
- [x] Согласованная output retention policy и настройки: preview/atomic cleanup, protected targets, durable results и opt-in automatic cleanup. Metadata/history/dedup не удаляются.
- [x] [MCP stdio adapter](mcp.md): 30 typed tools над теми же RPC/handler; contract, actual Unix lost ACK и [Windows gate](windows-mcp-gate-2026-10-08.md) прошли.

Критерий: пользователь понимает, что сделал агент, получает уместный сигнал, открывает причину и может продолжить работу. Рутинные разрешённые действия не требуют повторяющихся confirmations.

## Этап 6 — скорость и объёмы после измерений

Первый сценарий: [воспроизводимый baseline](stage6-plan.md) существующей доставки,
Windows import/build, HGFS tree/archive и объёмов output/journal. План зафиксирован
после ревью этапа 5; performance extensions пока не реализованы.

- [ ] Benchmark HGFS tree/archive/incremental и полный time-to-build; baseline хранить с fixture параметрами.
- [ ] Delta manifest и удаление только ранее управляемых sources, content cache и локальные Windows build caches.
- [ ] Chunk/resume больших объектов, output batching и виртуализация длинных списков UI.
- [ ] При необходимости SocketDelivery как другой adapter. Control session не блокируется большими данными.

Критерий: измеряемое улучшение выбранного сценария, отсутствие регрессии cancel/attention latency, корректное восстановление после прерывания. Не менять транспорт ради обещания абстрактной скорости.

## Этап 7 — каркас для следующих проектов

- [ ] Стабилизировать handler interfaces после runner и workspace-sync, которые уже работают.
- [ ] Project profiles и reusable recipes; общий UI renderer событий и capability metadata.
- [ ] Подключить второе приложение/module без изменений transport и без копирования task service.
- [ ] Лишь при необходимости выделить SDK/package versioning. Динамические plugins и другие ОС — отдельные запросы.

Критерий: следующий проект подключается конфигурацией/модулем, сохраняет journal/cancel/recovery и получает понятный UI.

Этап 5 завершён 2026-10-09: [Windows settings/retention gate](windows-settings-retention-gate-2026-10-09.md)
подтвердил 36 backend tests, два native UI запуска и 19 artifacts.
Пользователь выбрал manual restart для config и включаемую automatic output cleanup;
история/ключи/attention/snapshots/reports остаются. Все сценарии этапа 5 прошли
автоматические Windows gates; новый ручной gate не назначается.

## Следующий конкретный шаг

Этап 6: снять воспроизводимый baseline capture/pack/shared publication/Windows
import/build и объёмов output/journal. Только после измерений выбирать delta,
cache/resume/batching/virtualization. Старые snapshots/reports/history не очищать.
2026-10-09 пользователь обновил installed worker до clean `58f3c14`;
HistoryRead/typed attention/maintenance и здоровый журнал подтверждены read-only Info.
Debian controller снова запущен. [Ревью](review-stage5-2026-10-09.md) исправило
неограниченный readback diagnostic collision; settings/retention завершены.
Новых мелких ручных UI gates не назначать. Если подключённый worker даёт неожиданную
ошибку, остановиться и уточнить по инструкции пользователя. Dirty source mode и
performance extensions остаются будущими задачами.
