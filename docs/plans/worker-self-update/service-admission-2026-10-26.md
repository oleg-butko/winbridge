# Подготовка локального сервиса: admission и передача журнала

2026-10-26. Продолжение [journal20](service-ledger-2026-10-26.md) и
[плана normal local service](local-service-activation.md).

## Реализовано в исходниках

Parent/updater:

- `ActivationProcessGuard::prepare_local_service_blocking` потребляет исходный
  guard процесса, Job, канала и file pins. Требуются физический StateOwned6,
  подтверждённый writer, текущий owner/root/supervisor и original startup window.
- `admit_activation_service` фиксирует intent/current/history в одной write
  transaction. Проверки живого владельца выполняются до записи и перед commit.
  `ActivationServicePermit` создаётся только после подтверждённого commit.
- Повтор ключа возвращает только Existing record; другой ключ получает KeyConflict.
  Ни Issued из холодного журнала, ни late fact не дают новый permit.
- `ActivationServicePreparation` удерживает тот же parent guard и свежий permit.
  Метод **не отправляет service4 command**, не запускает другой child и не
  объявляет LocalVerified. При ошибке guard остаётся в failure; возможный Issued
  помечается Indeterminate, если его удалось однозначно прочитать и связать.
- Проверка writer/backup переиспользует уже проверенный graph в той же transaction;
  рекурсивный replacement/backup audit повторно не выполняется ради этого чтения.

Child/worker:

- `ActivationJournal::prepare_local_service` потребляет сам журнал и Fresh
  `JournalActivationPermit`. Подмена команды/receipt/header/lease отвергается.
- `PreparedServiceJournal` удерживает **тот же SQLite connection и полный
  LockedShutdownState**. Нет Store::open, migration, begin_authority, второго lock
  или извлечения File handle. Epoch/generation/Quiesced lease не изменяются.
- Проверки authority/receipt/accepted shutdown/lease выполняются в согласованном
  worker snapshot и повторяются после последнего внешнего checkpoint.
- При ошибке Fresh поглощён, failure удерживает журнал и state pins; нельзя
  получить из него reusable grant. Успешный контейнер пока тоже не открывает
  обычный IPC, не запускает reconciliation или scheduler.

Это trusted Rust library, не RPC/CLI для произвольных JSON. Serialized records
остаются данными. Schema20/worker17 и supervisor24/view16 прежние; private service4,
actor25/view17, advertised activation и public install **ещё не включены**.

## Проверки и Windows-прогон

Portable tests проверяют rollback при потере custody до/после записей, отказ
commit, повтор во всех delivery phases, потерю Fresh при ошибке, сохранность lock/
bytes/epoch/receipt и смену header прямо на последнем checkpoint.

Новый режим автоматического сценария:

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/EXPORTED_SOURCE.zip \
  --service-admission-only \
  --update-preparation-reuse-run 20261017-061527-f4639971
```

Он проверяет retained owner task/report/cache pin, берёт прежний compiler lease,
создаёт новые source/tmp/state/report, сохраняет request до dispatch с fsync и
использует fenced request2. Группы: `service-history-native` (13 SQL/migration
tests) и `service-transfer-native` (6 file-lock/SQLite transfer tests). Независимый
verifier проверяет точные argv/log names, native scope, counts и compiled build
identity (commit/manifest) из pinned source;
пропущенные/ignored/failed tests и bool вместо exit code отвергаются.

Это **19 fixture checks на Windows**, а не actual same-child service startup
или Main25. Нельзя записывать их как доказательство normal IPC, runtime guard
admission в полном replacement graph или actual retained19→20/refusal.
Source checks: workspace **813 passed / 0 failed / 17 ignored**, Linux и MSVC
all-target validation Clippy, fmt/scaffold291/diff-check; Python verifier5 passed.
Первый workspace run остановился на существующем event_log rotation test с
WouldBlock при повторном open. Его отдельный повтор и затем полный workspace
прошли; причина временной блокировки не установлена, код логгера не менялся.
Логи `.winbridge/update-preparation/service-admission-{workspace,workspace-repeat,
log-recheck,clippy-linux,clippy-windows}.log`. Native fixture run завершён; точные pins приведены ниже.

Preflight task `25d7d666-ff67-4504-b120-c3430d68285b` succeeded/0. Debian
16,952,639,488 bytes free; Windows K: 27,557,597,184 bytes free. Actual worker
0.1.1/source59249e6, SHA536d8637…, generation936909c6… прежние. Evidence:
`.winbridge/update-preparation/service-admission-preflight-20261026T192839Z-9de87cb6/`.

## Windows fixture gate завершён

Run **20261026-155845-0e79d392**, task
**f7c5320a-bb89-47ba-ba13-d7c27048d8e1**, succeeded/0:
**13 SQL/migration + 6 connection/state transfer tests**, все без ignored/failed.
Три ready artifacts дважды проверены; compiled commit/manifest обоих test binaries
совпали с archive. Это positive fixture proof; original-guard admission в полном
replacement graph и настоящий service startup остаются будущим Main25.

| Pin | Значение |
| --- | --- |
| Producer | `f145b62e7494dde0e94a6453af136da941e082a8` |
| Archive | `winbridge-20261026T195736Z-f145b62-e2c1db04.zip`, 875 source files |
| ZIP SHA-256 | `252f8e1b426c888c84c4d7916ce7c393145420ecdac5a00e680366abffefa02e` |
| Manifest SHA-256 | `059828d9d265dedb65e84d18465dcc97832f2051cea8f78ce8bb746f9f3c6d2e` |
| report.json | `6fde0592e7c148402d7ab0ce76cd9f938ef54977dc303944da5015604bf50e31` |
| service-history-native.log | `9010919c99f963976a6ce89f521205da9c5f0b23129931960b177c21a17c3e77` |
| service-transfer-native.log | `74958059a92fa76c8c803c19111661a2f5499e920e56986c6248f83df1863061` |

Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261026-155845-0e79d392`.
Local `.winbridge/workflow-validation/20261026-155845-0e79d392/` содержит исходный
request, fenced-request, terminal task/output, worker proof и
`service-admission-verified.json`. Observer log:
`.winbridge/update-preparation/service-admission-native.log`.
Session51244 завершён/exit0; ни один тестовый worker/desktop не запускался.

Compiler cache действительно переиспользован из original owner
`20261017-061527-f4639971`, report SHA
`22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`.
Группы исполнялись заново. Windows свободно до/после: 27,414,011,904 /
26,898,288,640 bytes; Debian после: 16,728,076,288 bytes. Очистка не выполнялась.
Installed worker source59249e6/generation936909c6… connected/active0/errors=null,
не заменялся. Controller5103 оставлен. Desktop не нужен; worker оставить работать.

В Debian driver добавлен readonly recheck без rebuild/submit. Он повторно принят
на этом же terminal run; original reports/requests не перезаписываются, локальная
observation получает новое имя. Команда продолжения из новой сессии:

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261026T195736Z-f145b62-e2c1db04.zip \
  --service-admission-only \
  --verify-service-admission-run 20261026-155845-0e79d392
```

Заключительные изменения driver относятся к readonly наблюдению, directory fsync
перед будущими dispatch и отображению cache receipt. Rust/PS producer этого gate
остаётся **f145b62**; новые helper changes не выдаются за новый Windows run.

## Следующая работа

1. Service4 DTO/private receiver и consuming Worker factory из PreparedServiceJournal.
   Использовать общую инициализацию Worker, сохранив config/domain/witness checks,
   recovery, user pause и maintenance Quiesced. Нельзя добавлять normal Store open.
2. Отделить bind обычного IPC/readiness от network::connect: local-only service
   не открывает controller session. Private Ready не считается готовым сервисом.
3. Parent проверяет настоящий IPC, PID сервера, exact runtime/config/build/context/
   generation/schema и прежние tasks/output; только затем service confirmation и
   LocalVerified. Перед send обязательны service4 capability и повтор live checks.
   Потеря ACK/timeout не разрешают повторить команду или создать child.
4. Actor25/view17 и новый isolated Main25, actual retained19→20/refusal и faults.
   Затем controller session/lifetime transfer/commit/release, remaining U2/U3/U4.

Текущий установленный worker не заменяется. Для тестов оставить его работающим;
desktop не нужен, новых UI окон или сетевого test worker этот режим не создаёт.
