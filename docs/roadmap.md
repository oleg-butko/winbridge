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

Статус 2026-10-06: [Windows runtime gate пройден](windows-snapshot-gate-2026-10-06.md), 19 actual import/build/run/cancel/junction checks. Пользователь подтвердил imports/preview/build/failure/history в native UI. По его замечаниям добавлены shared package select, сохранение delivery form и понятный отказ сборки непроверенного snapshot; 48 Rust tests и реальная UI integration проходят. Этап на завершающей Windows UX проверке этих исправлений, ещё не закрыт.

- [x] Общие Rust/TypeScript DTO manifest/ready и bounded metadata preflight: Windows пути, case collisions, file/directory conflicts, hashes/size syntax и byte counts.
- [x] Выбор mapping `/mnt/hgfs/shared` ↔ `K:\VMs\shared` и отдельного Windows workspace root.
- [x] Capture точного commit SHA из Git objects; dirty working tree исключён.
- [ ] Immutable dirty snapshot с явной маркировкой — отдельное последующее расширение.
- [x] Namespace, `.part`, manifest, ready marker, RPC import и локальный verified staging.
- [x] Policy/actual-byte/link guards и Windows compile для path/size/hash/traversal/collision/reserved names/incomplete transfers.
- [x] Настоящий Windows runtime gate: import/build/run/cancel/NTFS junction, 19 checks.
- [ ] Подтверждение новых native UI исправлений: список пакетов и сохранение delivery form; показ Unicode результата отдельной задачи запуска.
- [x] Workspace lease: импорт и сборка одного поколения не пересекаются. Runtime configs и пользовательские данные сохраняются отдельно.
- [x] UI preview состава snapshot, bytes/files, progress и причины отказа, адреса обоих представлений shared folder.

Критерий: build запускается из verified Windows snapshot; потеря mount или обрыв записи не ведёт к сборке неполных исходников. Повтор import не создаёт другую задачу. Не изменяются файлы вне namespace/зарегистрированного workspace.

## Этап 4 — первый полный цикл разработки

- [ ] Workflow: выбрать SHA → deliver/import → build → start → проверить готовность → log tail.
- [ ] Сохранённый pipeline и step IDs; ошибка сборки блокирует запуск следующего шага.
- [ ] Process registry для приложений, отдельно от одноразовых build tasks.
- [ ] Log read/tail: offset, encoding, truncation/rotation, квота и история.
- [ ] UI task detail с timeline, фильтрацией output, artifacts, process status и связанным логом. Никаких fake success при принятии request.

Критерий: на примере Netwatch агент получает стабильный snapshot, собирает на Windows, запускает его, читает лог и видит все шаги в UI. Падение приложения и провал readiness показываются отдельно от успешной сборки.

## Этап 5 — внимание пользователя и нормальная ежедневная работа

- [ ] Attention event → Tauri popup/tray badge/встроенный звук. Dedup, rate limit, quiet hours, severity.
- [ ] Открытие нужной task page из уведомления, acknowledged state; pending attention при закрытом companion.
- [ ] Projects, history/audit, connection settings, command palette и доступность с клавиатуры.
- [ ] Retention, quotas, output paging, recovery без UI; экспорт диагностического task report с редактированием секретов.
- [ ] MCP stdio adapter использует те же операции, что CLI; domain logic не дублируется.

Критерий: пользователь понимает, что сделал агент, получает уместный сигнал, открывает причину и может продолжить работу. Рутинные разрешённые действия не требуют повторяющихся confirmations.

## Этап 6 — скорость и объёмы после измерений

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

## Следующий конкретный шаг

Обновить fixed Windows runtime архивом с UX исправлениями и подтвердить три наблюдения из [инструкции](windows-snapshot-validation.md), включая delivery form restore. 19 Windows runtime checks уже прошли; не повторять их без новой причины. После UI подтверждения закрыть этап 3 и перейти к сохранённому workflow этапа 4. Dirty source mode и performance extensions не выдавать за выполненные.
