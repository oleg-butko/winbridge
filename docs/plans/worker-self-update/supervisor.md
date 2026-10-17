# U1: отдельный supervisor и передача владения

2026-10-17. Реализованы внутренние функции запуска после [source transaction](source-transaction.md).
Внешний update capability, source driver IPC, worker shutdown и runtime install
остаются отключены. Этот блок проверяет независимый процесс и его передачу;
он ещё не подключён к production worker launcher.
[Native Windows gate](windows-supervisor-gate-2026-10-17.md) прошёл: 76 tests /
12 groups, actual source smoke, 19 checked artifacts и build четырёх exe.

Launcher использует updater journal, OS owner lock и текущий epoch. Сначала
сохраняется immutable launch intent; затем запускается проверенная копия установленного
updater из собственного `runtime/update/<id>/supervisor`. Она не входит в заменяемый
runtime set. Stdout/stderr направлены в отдельные create-only файлы, не в pipes
родителя. Supervisor не использует ManagedChild и не назначается worker execution
Job. Обычные задачи сохраняют прежние Job limits; breakaway не включается.

До запуска и после CreateProcess проверяется отсутствие launcher/child в указанном
worker execution domain. Наследуемые внешние Jobs не покидаются и не изменяются.
В Windows gate внешний Job тестового транспорта остаётся владельцем всей проверки;
доказательство относится к отдельному domain тестового worker. Production launcher
не должен считать это доказательством возможности покинуть произвольный внешний Job.
`supervisor-status` показывает отдельный `externalJobObserved`: это наблюдение,
а не разрешение покинуть Job. До production install policy должна отложить запуск
в неподтверждённом внешнем окружении. [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
[IsProcessInJob](https://learn.microsoft.com/en-us/windows/win32/api/jobapi/nf-jobapi-isprocessinjob).

Новый процесс сначала ожидает durable spawn receipt с точным PID/creation identity,
проверяет собственный image/config/scope и launch nonce, получает следующий updater
owner epoch. Он публикует Ready только после открытия отдельного user-scoped IPC.
Parent проверяет actual server process; server получает actual client PID через
ОС. Handoff разрешён только исходному parent process с точной creation identity.
Observe/Stop разрешены владельцу capability того же пользователя; Stop имеет
дополнительный no-effects/Requested guard.
Уникальный для запуска nonce хранится в journal только как hash и не выводится в отчёты.

Ready не разрешает source/runtime effects. До handoff parent владеет process handle
и может завершить только этот процесс при ошибке. Перед отправкой handoff parent
отказывается от kill-on-drop: потерянный ACK после принятия не должен убить нового
владельца. Child сохраняет Active под собственной authority до ACK. Active supervisor
сохраняет owner lock после parent exit; source/install команды пока отсутствуют.

Незавершённые launch intent/spawn и неизвестный handoff не повторяются. Повторный
запрос читает сохранённую историю, не создаёт второй процесс. До handoff ожидание
ограничено; самостоятельное завершение child до принятия effects записывается как
известный no-effects outcome. После handoff автоматического TTL takeover нет.
Readonly status связывает launch ID/update ID/process/epoch/phase и не подменяет
физическую недоступность сохранённым Active. Stop этого блока допустим только пока
parent update остаётся Requested и файловые действия не начинались.

Updater schema 3 добавляет immutable launch evidence и CAS/receipts. Schema 1/2
мигрируют под свободным owner lock, с verified consistent SQLite backup и fencing
epoch. Source status schema 2 остаётся доступным readonly без миграции. Старый
schema-2 updater должен отказать на schema 3 без записи.

Проверки: lost ACK/duplicate payload, stale epoch, ignored DB writes, неверный
peer/nonce/image/config, crash до spawn receipt/до handoff, child после parent exit,
один owner и сохранение запрета запускать supervisor из управляемого domain.
Реальные Windows тесты — в отдельных runtime; установленный worker только транспорт.
Дополнительный smoke использует настоящий распакованный пакет WinBridge: проверяет
его manifest до/после Ready → Active → Stop и измеряет время подготовки. Исходники
этого пакета не меняются.

## Код и диагностика

- `crates/platform/src/infrastructure.rs`: отдельный process handle, birth/host/session,
  pre/post domain checks, kill-before-handoff и detach-before-send. Linux — development
  adapter; production гарантии Job проверяются на Windows.
- `crates/transport/src/supervisor.rs`: отдельный framed IPC, 3-second request/read/write
  deadlines, bounded DTO, same-user Unix credentials или SID/DACL named pipe.
  Windows перед отправкой ждёт busy instance в пределах того же 3-second deadline;
  повторной отправки команды нет. Отдельный native тест удерживает занятый pipe
  и проверяет ожидание без отправки данных. Windows сверяет kernel client/server PID; disconnect до отправки сообщения
  не завершает listener. [Client PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid),
  [Server PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid).
- `apps/update/src/journal/supervisor.rs`: launch spec, immutable phase receipts,
  CAS/epoch, single active supervisor, source-effect admission и atomic stop +
  parent FailedBeforeCutover. История не удаляется; новые requests не обходят
  незавершённый launch, даже если parent lifecycle уже terminal.
- `apps/update/src/supervisor.rs`: sealed PreparedSupervisor, verified helper copy,
  receipt-gated startup, Ready/Active и authenticated Observe/Handoff/Stop.
  `supervisor-internal` — внутренний режим текущего проверенного image, не команда
  обновления для агента. Старт без своего receipt/PID/nonce отвергается до owner claim.

Public CLI: `--config <runtime/winbridge-update.local.json> journal supervisor-status <id>`.
Один SQLite snapshot содержит installation/parent/launch; physical process и Job
наблюдаются отдельно. `journalEpochCurrent` означает совпадение epoch в БД,
не доказательство живого IPC. `physicalObservation` не подменяет phase. Source/install
capability здесь всегда выключена. Nonce не появляется в shared proof/логах; временная
capability в crash tests передаётся только через приватный test fixture и удаляется
вместе с ним. Stdout/stderr сохраняются в helper directory независимо от parent.

## Ревью и следующие зависимости

413 workspace tests passed; повторены updater tests после уточнений. Clippy,
workspace/tests Windows cross-check, TS contracts/typecheck/build:ui проходят.
Ревью вынесло полное хеширование исходников и image перед Ready из короткого handoff RPC,
защитило listener при disconnect до идентификации peer, отделило saved Active
от физической доступности, добавило exact birth и atomic parent/launch stop.

Handoff проверяет текущие bindings, но не выдаёт разрешение на файловые эффекты.
Перед будущим source grant/apply потребуется свежая полная проверка исходников.

Source changes/runtime install/shutdown в новом IPC отсутствуют. Следующий пакет:
worker adapter выбирает свой реальный execution domain, связывает supervisor peer
с current build/config/incarnation и выдаёт source/drain/quiesce/shutdown grants.
Перед тяжёлыми source/build действиями нужен атомарный учёт work/resource capacity
у worker; свободный slot по readonly Info не даёт reservation. Compiler processes
принадлежат supervisor, их cleanup подтверждается до quiesce/cutover. Затем U2–U4,
production environment policy, GUI notifications и единственный bootstrap.
