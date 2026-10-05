# Постепенная реализация и расширение

Правило каждого этапа: маленький рабочий сценарий → реальные события → понятный UI → проверка сбоев → обновление handoff. Подготовка архитектуры не считается выполнением runtime функции. Сетевую или файловую часть можно развивать независимо от UI, но этап закрывается только с видимым результатом для пользователя.

## Этап 0 — подготовка репозитория

- [x] Отдельный Git repository, Rust workspace и границы будущих модулей.
- [x] DTO для recipe/direct task и события, CLI help/describe.
- [x] UI prototype с fixtures: timeline, параметры, output, успех/ошибка/потеря свежести.
- [x] Принятые локальные решения, HGFS схема, UX, ADR и handoff.
- [x] Lockfiles и воспроизводимые проверки.

Готовность: сборка Rust/UI и contract tests проходят. Реальных listeners, processes, sync и journal ещё нет.

## Этап 1 — одна команда и реальный журнал на Windows

- [ ] Runtime config loader: один project, разрешённые recipes/direct, относительный cwd, timeout. Добавить версионирование и actionable validation errors.
- [ ] Windows worker выполняет одну команду в локальном fixture project. Начать с direct executable, одновременно читать stdout/stderr; затем recipe использует тот же service.
- [ ] Durable task ID, accepted до ACK, idempotency key/payload check, локальное task/output хранилище. State transitions и recovery записываются явно.
- [ ] User-scoped named pipe и Tauri shell; desktop только вызывает worker и читает read model. Не повторять Netwatch SID-only имя: нужен WinBridge instance ID.
- [ ] UI показывает источник, executable, массив аргументов, cwd, timestamps, output и exit code; реальные данные заменяют fixtures через adapter.
- [ ] Управляемое дерево процессов, timeout и cancel. Закрытие UI не останавливает worker.

Критерий: пользователь выполняет fixture команду, видит успех и ошибку, отменяет длительный process tree; дубликат запроса не создаёт второй процесс. Сбой после запуска виден как interrupted/indeterminate без молчаливого повторения. Windows проверка обязательна; Linux cross-compile её не заменяет.

## Этап 2 — Debian управляет Windows по локальной сессии

- [ ] Plain `ws://` listener на выбранном локальном IP Debian, Windows outbound connection; никаких TLS/certificates/relay.
- [ ] Handshake, wire version, реализованные capabilities, instance/generation IDs, request/response correlation, bounded message sizes.
- [ ] CLI для submit/get/cancel и подписки на события через controller. Controller не выдаёт собственный статус за подтверждение исполнения worker.
- [ ] Worker сохраняет задачу перед подтверждением; reconnect восстанавливает её по прежнему ID и события по sequence.
- [ ] UI отдельными состояниями показывает controller online/offline, последнее подтверждение задачи, очередь и возможные действия.

Критерий: агент Debian запускает задачу в Windows fixture project, получает output; разрыв соединения не останавливает процесс, reconnect возвращает результат, повтор key не дублирует исполнение. Неверная версия и timeout имеют понятную причину в UI.

## Этап 3 — готовый snapshot через VMware Shared Folders

- [ ] Выбор mapping `/mnt/hgfs/shared` ↔ `K:\VMs\shared` и отдельного Windows workspace root.
- [ ] Capture точного commit SHA; затем поддержка immutable dirty snapshot с явной маркировкой.
- [ ] Namespace, `.part`, manifest, ready marker, RPC import и локальный verified staging.
- [ ] Windows path/size/hash validation; защитить от traversal, case collision, reserved names, junctions и неполной передачи.
- [ ] Workspace lease: импорт и сборка одного поколения не пересекаются. Runtime configs и пользовательские данные сохраняются отдельно.
- [ ] UI preview состава snapshot, bytes/files, progress и причины отказа, адреса обоих представлений shared folder.

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

Начать этап 1: config + local worker task service на fixture project и durable result. Предпочтение — один реально исполняемый Windows сценарий и его UI, затем усложнение. Не переписывать планы заново и не начинать с TLS, mounts, service installation или преждевременного extraction Netwatch.
