# U1: worker launcher и локальное управление lease

2026-10-17. Продолжение [supervisor handoff](supervisor.md). Реализованы внутренний
worker launcher и отдельный supervisor-only IPC. Source apply через этот IPC,
shutdown, runtime replacement и публичный CLI/MCP update ещё отключены.
417 workspace tests и 9 итоговых update/control tests прошли. Clippy, Windows/tests
cross-check, fmt/contracts, typecheck/build:ui/scaffold и script syntax passed.
[Native gate clean c705299](windows-worker-control-gate-2026-10-17.md): 80 tests /
13 groups и полный candidate build passed. Блок проверен; U1 целиком открыт.

## Кто выбирает параметры

Доверенный installation driver вызывает `Worker::enable_update_control` до `serve`:
он задаёт фиксированный updater config и SHA установленного проверенного updater.
Этот SHA должен происходить из trusted release metadata, а не из запроса агента.
Production policy/release admission и их включение относятся к U2–U4; обычный worker
CLI пока не вызывает эту функцию. Наличие config само по себе ничего не включает.

Worker проверяет, что updater config находится в runtime и указывает на config
именно текущего worker. Он фиксирует build, instance/incarnation/generation,
PID/birth/host/session, config SHA и собственный локальный endpoint. Эти параметры
добавлены в immutable `SupervisorSpec.worker`; старые unbound records читаются без
перезаписи. Supervisor никогда не выбирает новый worker после reconnect. Замена worker в U2
потребует отдельного verified handoff с новым binding receipt, без переписывания
начального immutable spec.

`launch_update_supervisor` использует реальный execution domain данного Worker.
Payload не может передать Job, executable, roots или PID для запуска. Production
launch требует фиксированного installed worker path и откладывается при неизвестном
внешнем Job; breakaway отсутствует. Test-only private вызов остаётся внутри
существующего validation Job, не меняет его и не является production policy.

Existing содержит сохранённую историю; физическая живость наблюдается отдельно
через supervisor-status. Повтор сначала читает updater journal по key/payload, без owner claim и новых
preflight. При живом Active supervisor его owner lock занят, но duplicate всё равно
возвращает сохранённый update/launch. Другой payload того же key — конфликт.
Сохранённый Requested без launch тоже только наблюдается: повтор не создаёт процесс.

## Отдельный канал

Worker открывает отдельный endpoint своего generation: SID/DACL и kernel peer PID
на Windows, private Unix socket/UID на Debian. Обычные local RPC, WebSocket,
readonly extensions и MCP не dispatch-ят эти команды. Канал сериализует запросы;
блокирующие проверки выполняются вне async reactor. При завершении service worker
дожидается уже начатого callback: отмена async task не отменяет `spawn_blocking`.

Каждый запрос проверяет frozen worker tuple, actual supervisor PID/birth и один
SQLite snapshot updater installation/parent/launch/owner. Требуются Active, тот же
owner UUID/epoch и текущая installation scope. Worker читает updater DB readonly;
updater по-прежнему не открывает worker journal. Общей транзакции между ними нет.

Параметры worker-side lease формирует worker: source root берётся из его frozen
local config, baseline/target — из принятого UpdateRequest, список проектов — из
фактического пересечения зарегистрированных workspace. Caller не сообщает blockers,
свободные места, проекты или результат physical scan.

| Команда | Результат |
| --- | --- |
| Observe | Сохранённая lease либо её отсутствие; это диагностика, не новое право записи. |
| BeginSource | Durable SourcePending и startup guard до ACK. |
| Advance | SourceHeld и verified release через CAS/receipts; drain/closure допускаются только при parent Draining. |

Ответ — типизированный Snapshot или Error; lease и ошибка не возвращаются
одновременно. Snapshot может быть исходным receipt, а не текущей lease. Error не
доказывает отсутствие уже принятого эффекта: нужно Observe исходного ID. Большой
immutable launch spec не дублируется в forwarding reply; сохраняется 32-KiB bound.

Supervisor forwarding принимает только command, а worker binding, ID и epoch берёт
из своего journal. Caller с тем же JSON, но другим OS PID, не получает lease.
Handoff сохраняет короткий 3-second deadline; control имеет общий 45-second deadline,
из которого внутреннему вызову worker оставлено 42 s. Соединение может ждать busy
pipe до отправки; после отправки команда автоматически не повторяется. При lost ACK
нужно Observe исходного ID и сохранить прежние command ID/payload.

## Capacity, настройки и завершение

AcquireSource проверяет global work capacity в той же IMMEDIATE transaction, которая
фиксирует SourceHeld/receipt. SourceHeld и следующие active phases занимают один
work slot; SourcePending — нет. Ordinary task/import/workflow claims считают этот
slot в своих transactions. Update grant и task claim не могут забрать последнее
место одновременно. Released освобождает slot вместе с lease. Crash/timeout его
не освобождает. При отсутствии общего лимита поведение остаётся без этого лимита.
UI объясняет этот учёт рядом со счётчиком работ и показывает отдельную update lease.

Named-resource policy пока приводит к явному defer, без launch/new grant. Перед
настоящей сборкой нужен frozen update resource plan; обходить build/cache/interactive
пулы нельзя. В этом пакете нет compiler/file effects. Readonly image/source preflight
не является разрешением на сборку. [Следующее продолжение](fresh-source-control.md)
вынесло baseline/release I/O из Store/process registry mutex: admission barrier и
accepted-owner check предшествуют scan, fresh authority/revision и atomic work grant
следуют за ним. Native gate этого продолжения ожидается.

Новые source lease/grant требуют parent Requested/Preparing. Drain/closure/physical
confirmation требуют committed updater Draining intent после candidate verification.
Release блокируется в SourceApplying, Building, ValidatingCandidate и при cutover:
сначала нужен доказанный cleanup/исход через будущий driver. Исторический receipt
остаётся наблюдением. Этот gate не симулирует готовый candidate ради positive drain:
backend drain/quiesce покрыт прежним worker-coordination gate, новая связь проверяет
отказ преждевременному закрытию.

Изменение worker config, включая bytes при том же JSON, или updater config блокирует
новые grants. История/Observe доступны; Release проверяет старый pinned source root
и полный baseline/target manifest, поэтому может освободить прежнюю lease даже при
ожидающем перезапуска config. Восстановление не переносит права на новую конфигурацию.

Bound supervisor Stop сначала получает fresh Observe worker. Пока lease не Released
либо её отсутствие/освобождение не подтверждено, Stop отклоняется; оба journals и
startup barrier сохраняются. До Active worker grant невозможен, поэтому Ready можно
остановить без этого обращения. Source/install/shutdown команды здесь отсутствуют.

## Проверки и следующие зависимости

Новые локальные tests покрывают реальный helper process/IPC, forged peer, stale
build/context/generation/config/epoch, duplicate при занятом owner lock, ignored
receipt writes, config changes, global capacity race, premature drain rejection/release и Stop
при held lease. Windows harness добавляет отдельную worker-control группу и
checksummed `update-control-proof.json`; happy path использует настоящий распакованный
пакет WinBridge readonly. Установленный worker служит только transport тестов.

Readonly source-driver adapter добавлен в [следующем блоке](fresh-source-control.md).
Следом: compiler ownership/cleanup, автоматическое source/build выполнение, named
resource plan и exact shutdown с ACK/physical exit. Затем U2–U4, UI notifications и единственный bootstrap.
