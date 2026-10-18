# U1: supervisor mailbox и автоматическая подготовка исходников

2026-10-18. Trusted supervisor связывает проверку ZIP, worker source grant и
одноразовый source driver. Runtime replacement, compiler orchestration и публичный
MCP update пока отключены. Этот блок не завершает U1.

## Политика и контракт

[Updater config schema 2](../../../examples/windows-update.config.json) добавляет
`sourcePackagesDirectory`, по умолчанию в примере `K:\VMs\shared\WinBridge\sources`.
Это read-only delivery input; журнал, staging и сборка остаются на локальном диске.
Путь выбирает local policy, запрос содержит только pinned archive filename/hash.
Schema 1 продолжает работать для прежних readonly/control сценариев; новая source
подготовка без schema 2 отклоняется до admission. Отсутствующий delivery directory
не мешает загрузить config; попытка подготовки возвращает конкретную ошибку.

Внутренние `PrepareSource`/`ObservePreparation` требуют contract 2, nonce,
Active worker-bound supervisor и exact scope/revision. Прежние v1 actions и ответы
сохраняют прежнюю JSON форму. Новые поля response отсутствуют в v1. Схемы журналов
не меняются: updater 4, worker 15; новые DTO не сохраняются в этих БД. Старый updater
не принимает config schema 2/новые actions. Remote capability не объявляется.

## Владение и порядок

Fresh parent Preparing commit предшествует ACK и dispatch. Потеря ACK не отменяет
принятую подготовку. Повтор на любой уже начатой phase возвращает только snapshot,
включая Preparing после сбоя; другого driver/ZIP stage не создаёт.

Supervisor передаёт journal и OS owner lock в один blocking thread, сохраняя ту же
owner epoch. Mailbox остаётся доступен. Readonly view читает parent/source/build и
число file receipts в одной SQLite transaction, отдельно сообщает driverActive,
время наблюдения и bounded driverError. Это данные о сохранённом состоянии,
не новое разрешение на source/build effects.

Driver проверяет frozen updater config до/после этапов, свободное место ≥6 GiB,
копирует pinned ZIP в create-only staging, получает SourcePending/SourceHeld через
process-authenticated worker control, готовит preimages и выполняет fresh
[SourceApplyPermit](source-admission.md). Fresh worker observations также сверяют
updater config; его изменение не меняет policy посреди source apply. Verified source
оставляет parent Building и worker SourceHeld. Установка binaries не выполняется.

Во время driver mutations/Stop отклоняются; Observe не ждёт файловую работу или
ответ worker. После ошибки SourceHeld/unknown barriers не снимаются автоматически.
Причина доступна в live view и supervisor stderr; после смерти supervisor нужны
saved journal/physical inspection и logs. Отсутствие driverError после перезапуска
не превращает неизвестную подготовку в успех. Coordinated abort/release — следующий
блок; raw Release при Building/SourceApplying по-прежнему запрещён.

## Проверки и ограничения

Шесть worker control tests passed, включая три новых: mailbox при задержанном worker
ответе, duplicate/Stop while busy; отсутствие policy до intent; повреждённый архив
без изменения source и без повторной попытки. Delay hook существует только в test
worker library. Actor/driver работают в настоящем отдельном updater процессе.
Test cleanup освобождает только проверенную fixture lease через trusted library;
это не production abort API. Source crash/admission и compiler tests включаются в
общий rollup. 438 workspace tests, Linux/Windows Clippy и tests cross-check passed.
Contracts generation/check, typecheck/build:ui/scaffold, 8 export tests, fmt и
Python/PowerShell syntax passed. Native rollup впереди.

Следом — managed compiler orchestration, artifact verification и coordinated
abort/release; затем exact shutdown, U2 cutover и U3 пользовательский companion.
Installed worker не меняется; ручной bootstrap нужен после isolated gates U1–U3.
