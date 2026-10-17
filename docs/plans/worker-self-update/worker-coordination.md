# U1: владение исходниками и закрытие исполнения

2026-10-17. Продолжение [durable preparation](durable-preparation.md).
Это foundation для будущего независимого updater. Source apply, shutdown,
supervisor spawn, замена runtime и установка пока не реализованы. Методы доступны
доверенному Rust driver внутри процесса; mutation IPC/MCP capability не объявлена.

## Два независимых журнала

Updater сохраняет операцию в своей SQLite. Worker schema **15** хранит отдельную
координацию: `worker_update_leases` и immutable `worker_update_commands`.
Schema 14 мигрирует с consistent `VACUUM INTO` backup; старый exe должен отказаться
от schema 15. Обновление установки не считается подтверждённым по одному из журналов.

Spec связывает update ID, instance/incarnation/generation worker, owner UUID/epoch
updater, canonical local source root, SHA baseline/target manifests и проекты,
чьи зарегистрированные workspace пересекаются с source root. Worker сам вычисляет
пересечение. Пока это доверенный adapter; внешняя проверка владельца updater и
локальный IPC с SID/DACL остаются следующими зависимостями.

Acceptance и каждое изменение проходят IMMEDIATE transaction, CAS/revision,
проверку worker authority и readback до ACK. Одна активная lease. Повтор command ID
с прежним payload возвращает **исходный receipt**, даже если FSM уже ушла дальше;
изменённый payload/устаревшая revision отклоняются. Receipt не выдаёт новое право
выполнять файловые действия. Оба журнала нужно сохранять после неизвестного исхода.

## Последовательность

| Состояние worker | Что допускается |
| --- | --- |
| SourcePending | Новые roots связанных проектов запрещены. Принятая работа, включая children workflow, завершается; остальные проекты работают. |
| SourceHeld | Полный baseline проверен, связанных принятых работ и владельцев нет. Запуск связанных проектов запрещён, остальные работают. Будущий updater сможет применять только свой source transaction. |
| Draining | Новые roots запрещены для всей установки. Принятые задачи и children workflow продолжаются; приложения не отменяются автоматически. |
| Quiescing | В transaction подтверждены нулевые accepted/logical/in-flight counts и закрыты dispatch/обычные journal mutations. Затем выполняется Windows physical scan. |
| Quiesced | Physical scan подтверждает отсутствие владельцев и неопределённости. Это готовность к handoff, **не успешная установка**. |
| Released | Отменена только эта lease. Пользовательская пауза остаётся прежней. |

AcquireSource и CloseDispatch сверяют DB records/resources/attempts и in-memory
исполнителей. Ready/starting/stopping/indeterminate приложения учитываются через
связанную task; отсутствие активных потоков само по себе недостаточно. Физический
scan разрешён только после committed Quiescing. Occupied/Unknown оставляют запуск
закрытым. Проверки повторяются при claim и до process prepare/resume или import I/O.
Периодическая reconciliation тоже проверяет заморозку под Store lock и не превращает
ожидаемую остановку записей в journal fault.

До SourceHeld можно отказаться без source readback: права записи не выдавались.
После SourceHeld Release требует SHA одного из двух pinned manifests и независимой
полной проверки actual source tree самим worker. Частично заменённые/изменённые
исходники не освобождаются. Verifier общий для updater и worker, расположен в
workspace-sync; неизвестные файлы, links, hashes и preserved data проверяются
тем же policy, что в U0. Это защита объявленных workspace, не sandbox от внешних
редакторов или произвольных команд, пишущих за пределами своего cwd.

## Startup, настройки и восстановление

Create-only `worker-update-guard.json` сохраняется до принятия lease. Обычный startup
проверяет guard и active DB lease **до Store::open/миграции** и повторяет проверку
под instance lock. Потеря файла не снимает DB barrier. Неполный/чужой marker или
ошибка записи сохраняют блокировку; TTL и автоматического удаления нет.
После Released удаляется только marker с точным spec. Старый release receipt не
может убрать guard следующего update. Read-only inspection ничего не освобождает.

Новая journal authority и recovery/archive/new-pair не обходят активное обновление:
updater не принадлежит managed process domain, поэтому пустой domain не доказывает
его завершение. Recovery preparation отклоняется до создания нового каталога.
Поддерживаемый reconciliation/handoff updater появится позднее; вручную удалять
marker или запускать старый exe при неизвестном исходе нельзя.

Новые config saves и output cleanup ждут Release; прежние receipts остаются
читаемыми. Cleanup counter и admission согласованы с тем же lock. ConfigRestartPending
блокирует новое обновление и новые grants; отказ от обновления разрешён без применения
сохранённых настроек. User queue pause хранится отдельно и не отменяется updater.

## Наблюдаемость и совместимость

`Info.updateCoordination` — optional opaque readonly metadata. Desktop объясняет
ожидание исходников, drain, physical scan и handoff; при offline показывает stale
срез, при неизвестном формате/чужой области — отсутствие подтверждения. Кнопок
установки/снятия блокировки здесь нет. При отсутствии поля UI не заявляет, что update
завершён: старый codec может не передать optional metadata.

RPC 1/2 requests и legacy enums не расширены. Optional Info изменяет actual schema
SHA RPC 2 на `ca892a30ddabf3a3b24942f8d9d04c1c7edef45fa60f28df5566b32f465a25fa`;
это осознанно пересмотренный fixture, а не сохранение прежнего digest. Frozen v1 codec
читает ответ и отбрасывает новое поле; нынешний codec сохраняет неизвестные версии.
Прежняя pair evidence с другим schema/build не подтверждает новую пару; после
bootstrap потребуется новая actual pair verification. Remote update capability
будет отдельным контрактом.

## Проверка и следующая граница

Local workspace: **393 tests passed**. Включены concurrent acceptance/CAS, ignored
writes, сохранённые receipts, workflow children при drain, неизвестные physical
owners, config/output gates, pre-migration guard и его потеря, real process cancel,
сохранение PID другого проекта и user pause, full source readback и periodic freeze.
Production React renderer проверяется по actual IPC observations.

Windows gate расширен до семи групп: прежние четыре и worker durable coordination,
native coordination, previous-worker refusal. Он запускает отдельные экземпляры,
сохраняет `update-coordination-proof.json`, проверяет actual ZIP и собирает четыре
candidate exe. Reference schema-14 exe копируется create-only с SHA до/после; его
запуск использует новый тестовый config/state. Installed runtime не заменяется.
[Native gate clean b0d5730](windows-worker-coordination-gate-2026-10-17.md) прошёл:
52 tests, 7 groups, 11 checked artifacts и полный build/staging четырёх exe.
Renderer по checksummed Windows IPC observations прошёл; installed worker прежний.

Следом: per-file source intents/preimages/receipts + full-tree reconciliation,
независимый supervisor вне task Jobs, authenticated local handshake и exact
shutdown/handoff. U1 целиком остаётся открытым, затем U2–U4 и единственный bootstrap.
