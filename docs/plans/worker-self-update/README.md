# Самообновление: план реализации U0–U4

2026-10-17. Пользователь одобрил [анализ](../../analysis/worker-self-update/README.md)
и поручил детализацию и реализацию. Архитектурный переход A4 временно отложен.
Самообновление не считается готовым до U4. Каждому пакету нужны автоматические
проверки; ручная работа на Windows — первоначальная установка новой capability.

## Постоянные пути и границы

- Source/build: `K:\_proj-26\winbridge\src`, основной `target` переиспользуется.
- Installed: `K:\_proj-26\winbridge\runtime`; worker/desktop не запускаются из staging.
- Staging: `runtime\update\<id>\payload`; прежние binaries в том же update directory.
- Update store: `runtime\update-state\updates.sqlite`, независимый от worker journal.
- ZIP: `K:\VMs\shared\WinBridge\sources`; отчёт: `WinBridge\updates\<id>`.
- Config updater рядом с exe: `winbridge-update.local.json`; реальные roots и
  worker config задаёт local policy, payload не может назначить installation paths.
- Без изменения services, startup, firewall, TLS и установки global dependencies.
- Не изменять рабочий Windows runtime до isolated U1–U3 gates и bootstrap.

Compiler actor / [coordinated abort](compiler-actor-abort.md): internal v3/v4,
config 3, journal 5. Отмена до cutover проходит Aborting → FailedBeforeCutover
только после source/compiler/worker release proof; неизвестный исход остаётся
ManualIntervention. Candidate verification и runtime replacement ещё впереди.

## Контракты и данные

U0 вводит отдельные DTO source manifest/plan и UpdatePhase. Legacy RPC/AttentionTarget
и их schema hashes неизменны. Сетевой update capability пока не объявляется.
Dry-run CLI имеет собственный versioned результат; `canApply` означает только
отсутствие source conflicts, а не разрешение остановить worker или заменить exe.

Source ownership: предыдущий manifest + actual file SHA. В новом плане для каждого
пути указаны Add/Replace/Remove/Keep, old/new identity. Неизвестные исходники и
локальные правки дают blockers, отсутствующий baseline — BootstrapRequired.
Кэши, локальная конфигурация и runtime data исключены. Никаких удалений в dry-run.
U1 source apply использует собственные durable intents/receipts до файловых эффектов;
реализовывать его как вызов dry-run плюс незащищённый цикл копирования нельзя.

U1 добавляет UpdateRequest (caller key, target archive/hash, expected installed
build/config/scope), UpdateRecord (ID, revision, FSM, process/file evidence) и
maintenance token (update ID, journal incarnation, epoch, phase). Duplicate lookup
предшествует новым admission checks; другой payload того же key — KeyConflict.
Update store: `updates`, `transitions`, `file_intents`, `file_receipts`,
`process_attempts`, `delivery_receipts`; одна активная операция установки.
Записи сохраняются до ACK. Updater/worker имеют разные БД: идемпотентный handshake
и reconciliation заменяют несуществующую общую транзакцию.

## FSM и разрешения

| Phase | Допустимые действия | Условие выхода |
| --- | --- | --- |
| Requested/Preparing | Pin/copy/verify package, source plan; source apply только после U1. | Полный manifest, source/config/resource guards. |
| SourceApplying | Только после source lease: durable preimage/intent → файл → receipt. | Полный manifest/readback; неизвестный partial source → ManualIntervention. |
| Building | Управляемая сборка и staging; старый worker обслуживает другие проекты. | Build exit 0, source readback, artifact hashes. |
| CandidateVerified | Isolated native smoke/contract/migration test без production effects. | Candidate proof привязан к actual source/binaries. |
| Draining | Запрет новых roots, children принятого workflow разрешены. | Нет незавершённых roots/apps; user pause не отменяется. |
| Quiesced | В одной authority transaction закрыть dispatch; затем physical scan. | Нет OS owners/in-flight effects; final consistent DB backup и preflight. |
| Stopping | Save ACK desktop, exact-process exit; worker shutdown ACK/exit. | Instance lock и заменяемые файлы освобождены. |
| Replacing | Per-file intent → replace → hash/receipt; запуск заблокирован. | Весь binary set соответствует manifest. |
| VerifyingLocal | New worker запускается с guard до миграции/scheduler. | Version/config/schema/incarnation/IPC/reconciliation корректны. |
| AwaitingController | Bounded проверка actual handshake/contracts, без нового update. | Подтверждение exact new/new либо manual intervention. |
| Committed | Снять только свой maintenance, сохранить user pause, финальное уведомление. | Install outcome, UI readiness и report delivery учитываются раздельно. |
| Deferred/FailedBeforeCutover | Нет новых update effects; известный старый runtime доступен. | Новый update только новым осознанным запросом. |
| ManualIntervention | Не менять state и не запускать произвольный старый exe. | Наблюдения и точные ручные recovery steps. |

Нельзя решить безопасность по `active_jobs == 0`, одному PID или отсутствию pipe.
Resume проверяет barrier после claim; scan идёт после закрытия dispatch.
После падения updater guard сохраняется, ручной запуск worker его проверяет до
Store::open. Тайм-аут не доказывает, что процесс завершён. После migration/новых
effects downgrade journal автоматически запрещён; backup служит восстановлению.

## Пакеты и критерии готовности

### U0 — readonly preflight и build staging

- [x] Отдельные strict DTO, manifest/path/size/hash/clean-source проверки и pure FSM.
- [x] `winbridge-update`: verify-source и source plan CLI, local config, JSON blockers.
- [x] Build-only script с disk measurement, locked build и create-only staging,
  stdout/stderr, manifest/readback; не вызывает runtime copy/launch/config creation.
- [x] Tests: local edits/deletions/unknown paths, case/path collisions, symlink,
  hardlink, protected data, bad manifest/hash, deterministic plan и cutover failure.
- [x] Portable PowerShell stage tests и реальный CLI dry-run на exported source.
- [x] [Automatic native Windows tests и полный build/staging четырёх exe](windows-preparation-gate-2026-10-17.md): 19 tests, native junction guard, source readback и actual artifact identity прошли.

U0 не применяет source plan, не выключает процессы, не объявляет remote capability.
Первоначальный build verifier можно собрать вручную при bootstrap; дальнейший
автоматический pipeline использует уже установленный проверенный updater.

### U1 — source transaction, supervisor и maintenance

- [x] [Независимый update journal](durable-preparation.md): atomic acceptance/transitions,
  CAS/idempotency/receipts, OS owner lock + epoch, readonly inspection и crash tests.
- [x] Bounded ZIP verification/create-only local source staging без extra/duplicate
  entries и без изменения рабочего src.
- [x] [Native journal/ZIP preparation gate](windows-durable-preparation-gate-2026-10-17.md):
  42 tests, actual ZIP verification, script guards и полный candidate build passed.
- [x] [Worker source/drain/quiesce coordination](worker-coordination.md): durable lease/CAS,
  separate user pause, pre-migration startup/recovery guard и readonly UI. Local 393
  tests и [52 native Windows tests / полный candidate build](windows-worker-coordination-gate-2026-10-17.md)
  passed. Shutdown/IPC не готовы.
- [x] [Source transaction](source-transaction.md): trusted driver, create-only preimages,
  intent/receipt, no-clobber moves, full manifest verification и readonly crash inspection.
  405 local tests и [65 native Windows tests / полный candidate build](windows-source-transaction-gate-2026-10-17.md)
  passed. Source lease/SourceApplying обязательны до записи; automatic replay/rollback отсутствуют.
- [x] [Supervisor library handoff](supervisor.md): guarded launcher, durable receipts и
  отдельный IPC; 413 local tests и [76 native tests / полный candidate build](windows-supervisor-gate-2026-10-17.md)
  прошли. Source/install/shutdown команды отсутствуют.
- [x] [Worker launcher/control](worker-control.md) реализован как trusted API;
  [80 native tests и candidate build](windows-worker-control-gate-2026-10-17.md) прошли. Реальный execution domain, fixed runtime и external Job guards,
  immutable binding и supervisor-only IPC. SourceHeld атомарно занимает work slot;
  named-resource policy пока откладывает grant. Breakaway запрещён. Публичное включение,
  source/build orchestration и production shutdown остаются следующими задачами.
- [x] [Fresh SourceControl](fresh-source-control.md): readonly process-authenticated
  Info и source scan без общего mutex реализованы; 26 targeted regressions и
  [82 native tests / candidate build](windows-fresh-source-gate-2026-10-17.md) прошли. Автоматический source/build driver пока не подключён.
- [x] [Managed compiler](compiler-driver.md) реализован как trusted library:
  intent/PID/Job/resume, output и verified cleanup; updater schema 4. 429 workspace
  tests, 10 итоговых compiler tests и
  [95 native tests / candidate build](windows-compiler-gate-2026-10-18.md) passed. Compiler
  orchestration и coordinated artifact/abort driver пока не подключены.
- [x] [Atomic executable adapter](atomic-executable.md) для compiler: Job назначается
  при CreateProcess, bounded handle inheritance и CRT argv;
  [102 native tests / candidate build](windows-atomic-launch-gate-2026-10-18.md) passed.
- [x] [Source admission перед dispatch](source-admission.md): one-use fresh commit
  permit, повтор только observe; 26 integration tests и Windows cross-check passed,
  [109 native tests / candidate build](windows-source-actor-gate-2026-10-18.md) passed.
- [x] [Source preparation actor](source-actor.md): ZIP/grant/apply driver и
  отзывчивый internal v2 mailbox; 438 workspace tests и Linux/Windows Clippy passed,
  [109 native tests / candidate build](windows-source-actor-gate-2026-10-18.md) passed.
  Compiler actor и coordinated abort добавлены следующим блоком.
- [x] [Compiler admission до dispatch](compiler-admission.md),
  [compiler actor/abort](compiler-actor-abort.md), [output](compiler-output.md) и
  [artifact queries](artifact-identity.md):
  [133 native tests / полный build](windows-artifact-identity-gate-2026-10-18.md)
  passed. Durable candidate approval/install остаются впереди.
- [x] [Compiler report binding](compiler-report-binding.md): updater schema 6,
  atomic compiler completion/report receipt и readonly exact-byte verification;
  [139 native tests / полный build](windows-report-binding-gate-2026-10-18.md)
  passed; public install ещё не включён.
- [x] [Exact artifact set / atomic receipt + FSM](candidate-artifacts.md) и
  [mailbox v5](artifact-actor.md): updater schema 7;
  [156 native tests / full build/four queries/exact-file proof](windows-artifact-actor-gate-2026-10-18.md)
  passed. Artifact receipt не заменяет worker health/approval.
- [ ] Old runtime baseline/preimages, candidate health/rehearsal и approval driver.
  [Known installed-set reader](installed-set.md) и abort snapshot прошли тот же
  native gate. [Early startup guard](runtime-startup.md) подключён; три actual-main
  negative tests passed locally, положительный native gate впереди. Publication,
  authenticated live identity и durable baseline acceptance остаются впереди.
- [ ] Worker admission/drain/quiesce/shutdown/startup guard во всех effect paths;
  local config edits/cleanup/recovery тоже согласуются с maintenance.
- [ ] Отдельный IPC с user SID/DACL; old/new journals не меняют update authority.
- [ ] Native fault gate: updater жив после worker exit; нет второго supervisor,
  accepted tasks не обрываются, app/unknown owner блокирует update, lost ACK и
  падение между handoff receipts не разрешают преждевременный replace.

### U2 — same-schema apply и agent tracking

- [ ] Binary manifest, same-volume replace и verified prior set, exact exit/lock.
- [ ] New worker verification mode и controller-pinned confirmation.
- [ ] Installation-level management contract/CLI/MCP/status с opt-in local policy;
  произвольные archive/runtime paths запрещены; legacy readonly transport прежний.
- [ ] Native faults: locked exe, disk full, partial rename, crash после каждого
  file intent/receipt, manual launch при mixed set, lost reconnect/confirmation.
- [ ] Rollback только до нового journal owner/effects и при доказанной целостности;
  update заканчивается ошибкой, автоматически повторно не устанавливается.

### U3 — desktop и миграции

- [ ] Native update notification и save-before-exit с process ACK/exit timeout.
- [ ] `--background` без main/popup flash; повторный background launch не поднимает
  существующее окно. Единственный tray, updater показывает статус при worker offline.
- [ ] Visible/tray/absent desktop; сохранение drafts, quiet hours, error report link.
- [ ] Consistent backup и effect-free migration rehearsal на актуальной копии,
  поддерживаемые schema transitions в release metadata; old binary refusal.
- [ ] Native crash migration/desktop save/tray failure, unchanged config/history/keys.

### U4 — bootstrap и installed update

- [ ] Один согласованный ручной bootstrap worker/desktop/updater; source baseline
  принят и подтверждён. User auto-update policy включена для WinBridge packages.
- [ ] Следующий real update агент запускает автоматически, без ручного Ctrl+C,
  распаковки или rebuild. Проверить stable paths/actual versions и бизнес-сценарий.
- [ ] Отчёт доказывает все этапы, исходы UI/delivery отдельно; ошибку можно разобрать
  без работающего worker. Написать понятную инструкцию ручного восстановления.
- [ ] Обновить guides/architecture/readiness и вернуться к installed A4.3 gate.

## Повторное ревью до реализации

Уточнены исходные риски: `-PrepareOnly` не build-only; updater нельзя выполнять как
обычный task; updater itself не входит в заменяемый комплект работающей копии;
две SQLite не образуют общую transaction; DB backup делается после final quiesce,
а rehearsal до этого лишь предварительный; пользовательская pause/config pending
не снимаются обновлением. Source plan не даёт разрешения apply. Опасный apply
остаётся заблокирован до U1–U3 gates. Стоимость основной части — lifecycle/fault
recovery, а не копирование файлов. Новая actor library для U0–U4 не требуется.

[Compiler actor и безопасная отмена до установки](compiler-actor-abort.md) — U1 internal v3/v4, config 3, journal 5; native gate пройден.

[Bounded compiler output](compiler-output.md): trusted readonly API; native проверка пройдена вместе с compiler actor и PowerShell path fix.

[Artifact identity и ограниченные probes](artifact-identity.md): readonly query четырёх exe и compiler-tree проверка;
[133 native tests / полный candidate build](windows-artifact-identity-gate-2026-10-18.md) пройдены, runtime installation не включена.
