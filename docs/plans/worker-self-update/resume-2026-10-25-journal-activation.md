# Updater: инструкция продолжения после завершения проверок

**Новое поручение после этого checkpoint:** [review/latency](review-2026-10-25-latency.md).
Пользователь явно разрешил240s для новых admissions. Прежние требования не менять120s
ниже больше не применяются к новым attempts; старые frozen intents не продлеваются.
Положительный новый writer gate пока не получен. Debian теперь192.168.44.204.

**2026-10-25, America/New_York.** Авторитетный checkpoint для сессии без истории
чата. Пользователь поручил закончить текущие проверки, записать результаты и
дальнейшие шаги, сделать коммит. Новые этапы реализации при закрытии не начинались.
Эта инструкция заменяет прежние NEXT/ACTIVE, включая
[предыдущий checkpoint](resume-2026-10-25-state-handoff.md).

## 1. Результат закрытия

Combined Windows run **20261025-205856-79e1b7c0** завершён **failed/exit1**:
**190 тестов прошли в 38 группах; Main24 — 1 failed**. Обе actual migrations
(updater18→19/refusal, worker16→17/active16 refusal) прошли; independent audit
отверг18 подмен. Final writer observe получил `Activation preparation deadline
expired; no new execution or adoption`. **Положительного Main24/writer proof нет.**
Все проверки/observer завершены; после terminal повтор не запускался. Первый следующий шаг —
разобрать задержки activation и исправить путь, сохранив original120s и fences,
затем повторить combined gate из нового clean producer. Только после полного
success переходить к normal service/controller/lifetime.

- Task `f762c5a2-0cf8-474a-8582-45d4bd6c74c1`: failed/exit1; observer exit1.
- Выбраны42 commands/39 test groups: 41 commands exit0, Main Cargo exit101.
  53 ready files дважды сверены по bytes/SHA. Overall report passed=false.
- Ready set не содержит `candidate-health-actor-proof.json` или writer-before-send
  marker. Успешные writer CAS/epoch/receipt **не доказаны**. Отсутствие marker само
  по себе также не доказывает отсутствие эффекта в SQLite.
- Fresh Info **2026-10-25T21:25:12Z**: installed d052/generation прежние,
  connected/active0/errors=null, recoveryCoverage.complete=true.
- Readonly Windows task `b871ac45-a58f-4e8a-8e2b-19ebbd7dd24f` succeeded/0:
  по PID+creation FILETIME проверено, что old test worker17920,
  supervisor17048 и replacement worker22864 **завершились**. Это не отдельный
  Job-empty proof; для failed Main такой successful artifact не создавался.
- Native report disk: Windows13228408832→12158017536 bytes; Debian при закрытии
  25040994304 bytes. Cleanup не выполнялся, Windows directories сохранены.
- Local closing receipt `.winbridge/update-preparation/writer24-session-close-20261025.json`;
  process check `writer24-close-processes-20261025T212430Z-0ec63e6d.json` рядом.
  Active Cargo/native/observer проверок при передаче нет; controller оставлен.

**Полное самообновление ещё не готово.** Public install выключен; advertised
`activation`, `controller_confirmation`, `desktop_update` остаются `None`.
Установленный Windows worker/runtime/config/journal не заменялись. Desktop можно
закрыть, **установленный worker оставить работать**. Новых UI окон не было.

## 2. Что уже реализовано

Независимые версии source: **updater journal19 / worker journal17 /
supervisor24 / consistent view16 / private physical2 + writer3**.
Writer command/receipt DTO имеют schema1. Эти номера обозначают разные контракты.

| Часть | Граница готовности |
| --- | --- |
| U0 / внутренний U1 | Source preflight/apply, build staging, candidate rehearsal, supervisor, drain/quiesce и exact shutdown реализованы; прежние native gates пройдены. |
| U2.1 | Final consistent backup после exact exit и fault gates пройдены. |
| U2.2 physical | Same-volume replacement, private child Ready и передача прежнего instance lock подтверждены Main23 на updater18/worker16. |
| Worker17 journal | Exact old-header CAS + immutable receipt в одной IMMEDIATE transaction; backup перед inactive16→17 migration; active16 refuse. |
| Existing-state opening | ActivationJournal удерживает Store и тот же acquired lock; без CREATE/migration/обычного begin_authority; одна попытка, retained failure. |
| Writer pipeline | Updater19 graph-bound intent/history, consuming native driver, private3 receiver, actor24/view16 и combined native harness реализованы. Результат actual gate — в разделе 1 и отдельном отчёте. |
| Обычный сервис нового worker | Reconciliation, normal local IPC, новая controller session и передача lifetime **ещё не реализованы**. |
| Завершение self-update | Commit/scoped release, оставшиеся U2 faults/management, U3 desktop и U4 installed update **впереди**. |

Confirmed writer receipt доказывает принятую DB-транзакцию, но не готовый сервис
или обновление целиком. Сохранённые данные не восстанавливают kernel custody,
startup permission, живой Job или право повторить private send.

## 3. Коммиты, review и source-проверки

| Commit | Изменение |
| --- | --- |
| f8cf372 | Strict writer DTO, модель и frozen authority fingerprint. |
| e3a4a3d | Worker17: atomic header/receipt, old-owner fencing, backup/migration. |
| f5dcec1 | Opening существующего Store под тем же lock, retained failures, запрет close-time WAL checkpoint. |
| 768422d | Updater19 intent/history, private3 frames, consuming driver/receiver. |
| 1a2149f | Actor24/view16, один blocking slot/key, fast ACK/busy и mailbox regression. |
| 7fd8135 | Clean receiver1a2149f export/pin. |
| a9babb9 | Rust Main24, before-send marker и независимый writer proof verifier. |
| 0d5d0e3 | PS/observer writer mode, реальные updater18→19 и worker16→17/refusal fixtures. |
| 772b00a | Исправлена SQL projection frozen shutdown record; portable real-Store regression. |

Существенные исправления и invariants, которые нельзя потерять:

- SQLite при Drop последней connection мог checkpoint-ить/удалять прежний WAL.
  RO/RW activation opening устанавливает и проверяет
  `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` до первого DB query.
- Intrinsic validation повторяется при receipt validation; пересчитанный SHA не
  скрывает неверные IDs/scope/epoch/process/file pins. Authority fingerprint V1
  общий и frozen; private authority ID не экспортируется.
- Worker атомарно сохраняет тот же incarnation, epoch+1, новый private authority
  ID и immutable receipt. Старый cached owner fenced. Duplicate возвращает
  receipt без нового permit/epoch. Commit error означает unconfirmed outcome.
- Opening owner помечает одну попытку до транзакции; даже rollback не делает его
  reusable grant. Store/lock наружу не извлекаются.
- Issued0 сохраняется до одной private send; затем Confirmed1 либо
  Indeterminate1→Confirmed2 как поздний факт. Partial write/EOF/timeout не разрешают
  повтор. Поздняя receipt связана с immutable StateOwned6, не с mutable current tip.
- Actor24 consumes original resident Owned guard, сохраняет owner/failure.
  **None start/observe — только volatile ACK/busy**, не admission/успех.
  Stable Some16 ожидается после retirement. Cold — NotResident/liveness unknown;
  views1–15 frozen без новых полей.
- Main baseline требует Exited старого worker, а не только shutdown ACK.
  Сразу после StateOwned передаётся24; тяжёлые audits идут после receipt/Job exit.
- Original monotonic startup window **120000ms не продлевается**. У старого Main23
  StateOwned наступал примерно на110s; remaining window мало. Test marker не
  заменяет delivery и связан с обеими actual receipts.
- Schema16 хранит shutdown в `worker_shutdown_receipts.command,record`, не
  `receipt`. Общая readonly projection сверяет typed command/receipt/update index;
  её portable regression открывает настоящую Store/schema17.
- Updater migration повторно видит terminal replacement как **Existing data**;
  это не fresh permit и не обязано быть Err. Проверяется сохранение всего history.

Evidence source-проверок (generated, не коммитить):

| Проверка | Результат / граница |
| --- | --- |
| Workspace baseline1a2149f | 781 passed /0 failed,16 ignored; focused writer19, actual mailbox24 1. |
| Main24 hooks/verifier | portable hooks4; Python19; Linux/MSVC workspace all-target validation Clippy passed. |
| PS/observer/migrations0d | Python24 (prep5/process3/state5/writer7/migration4); Linux/MSVC all-target validation Clippy passed; old132 readonly recheck passed. |
| SQL fix772 | real-Store projection regression1; MSVC worker all-tests validation Clippy; fmt/scaffold278/diff passed. |

Полный workspace после test-only изменений повторно не запускался; baseline781
не выдавать за новый native result. Logs в `.winbridge/update-preparation/`:
`actor24-checks.json`, `writer-main24-source-checks.json`,
`writer-migrations-final.json`, `writer-migrations-{linux,msvc}-final.log`,
`writer-migration-query-{regression,msvc}.log`. Native результаты отдельно ниже.

### Повторяемые source-команды

При изменении Rust выполнять последовательно, с fresh disk admission перед
компиляцией; не менять Rust-файлы во время живого Cargo. Ниже source-checks,
**не Windows execution**. Exact native commands уже frozen внутри PS script.

```bash
export PATH=/home/user1/.local/bin:$PATH
export CARGO_INCREMENTAL=0
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features winbridge-worker/update-validation --locked -- -D warnings
cargo clippy --workspace --all-targets --features winbridge-worker/update-validation --target x86_64-pc-windows-msvc --locked -- -D warnings
python3 scripts/test-update-activation-preparation-native.py
python3 scripts/test-update-activation-process-native.py
python3 scripts/test-update-activation-state-native.py
python3 scripts/test-update-activation-writer-native.py
python3 scripts/test-update-activation-writer-migration-native.py
npm run check:scaffold
```

Для полного default workspace использовать `cargo test --workspace --locked`
по необходимости изменения; ignored native tests отдельно запускает approved PS
сцена с real refs. Python filenames содержат дефисы: запускать файлы явно;
`unittest discover` в этой структуре их не нашёл (0 tests не считать успехом).

## 4. Native прогоны и exact pins

Полный [отчёт двух writer-прогонов](windows-journal-writer-gate-2026-10-25.md)
содержит причины ошибок/результат, hashes, storage и итоговый scope.
**Никогда не replay terminal run; observation timeout не повод создать новый key.**

### Первый writer run — failed, сохранён

Run `20261025-203011-cffafea9`, task `abf1250f-9876-435c-83ea-672c0462d8b0`.
Producer0d5d0e3; terminal failed/exit1. Preliminary41 passed; SQL projection error
возник до миграции/old16 server/Main24. Исправлена в772b00a. Failed namespace не
использовать как successful reference и не менять.

### Повторный writer run — результат в разделе 1

- Run **20261025-205856-79e1b7c0**.
- Task **f762c5a2-0cf8-474a-8582-45d4bd6c74c1**.
- Producer **772b00ac8a5f4b5a0081cd1014bfc6c65b5b2e5b**.
- ZIP `winbridge-20261025T205458Z-772b00a-e7ad6653.zip`.
- ZIP SHA256 `e780292bde894f437c69c810a9e8a9307596c338923af4d515aa75e8918e25a1`.
- Manifest SHA256 `9bb24397934eae13c2a25a8fca3835ae4acc3172be3303b03efd8974071faba3`.
- Payload824 files; clean built-UI source package.
- Shared report `/mnt/hgfs/shared/WinBridge/workflow-validation/20261025-205856-79e1b7c0/`.
- Windows report `K:/VMs/shared/WinBridge/workflow-validation/20261025-205856-79e1b7c0/`.
- Local metadata `.winbridge/workflow-validation/20261025-205856-79e1b7c0/activation-preparation-run.json`.
- Observer log `.winbridge/update-preparation/writer24-native-retry.log`.

Receiver — **1a2149f88407fd9c33d101e12d07d7c4b64599d3**, same-schema19/17,
actor24/view16, physical2/writer3. ZIP
`winbridge-20261025T163918Z-1a2149f-8b8c082c.zip`, SHA256
`abc74d43ee4e43f0b60a86f0cd3bc9dd35679fcf8b6f010cb1ca3c5c72573c1b`,
manifest `b27808d7f52abd9fdbb4c5fe01e078493950d22e5f5a46c8b59b992bb453bed1`.
Source pin не заменяет native execution proof. Installed d052 для этого gate
служит только обычным транспортом исполнения изолированного сценария.

Composite migration reference получен из **успешного physical Main23**:
`.winbridge/workflow-validation/20261025-100933-d8a50807/activation-preparation-verified.json`.
Helper заново проверяет old132 report/proof/ready bytes; отдельные refs18/16
используют actual old images **target1370d6b**, не producer7b270c6.
Reference SHA256 `bd1e71f1181236e4b8e3c4f140e8b716abe3c8908c10d11651132252c25f3772`.
Нельзя подставить сюда legacy compatible-drain schema17 pin.

Cache **20261017-061527-f4639971/test-build-cache**: owner report SHA256
`22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`;
owner taskac5724f1-c38a-4a50-a446-f9b57ae5473b terminal failed, четыре compiler
commands exit0. Проверены ready bytes и task scope. Cache — только compiler
cache, не inherited test success; одна exclusive lease. Receipt
`.winbridge/update-preparation/writer24-cache-verified.json`.

PS native parse-only taskbb8e0890-89c6-4f9a-b6ee-6550bd4bbaec succeeded/0:
Parser.ParseInput без syntax errors, ActivationWriterGate присутствует,
**scriptExecuted=false**. Script SHA256
`b2b1566d7d40f273dad9c6f20a1af5703bb6a17c931a4ed326fa65a3ab06bad6`.
772 ZIP entry имеет те же байты, поэтому syntax evidence reused. Это не runtime
proof; при следующем изменении script выполнить новый native parse.

### Readonly повторная проверка того же run

Команда **не запускает Windows gate заново**. Сначала get_task по сохранённому ID.
При отсутствии local metadata восстановить её из retained evidence/оригинального
request, а не отправлять новую task. Не публиковать private request/config/token.

```bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T205458Z-772b00a-e7ad6653.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T163918Z-1a2149f-8b8c082c.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261025-100933-d8a50807/activation-preparation-verified.json \
  --activation-writer --verify-run 20261025-205856-79e1b7c0
```

Для failed run команда не создаст positive proof. Сначала разобрать конкретную
ошибку в отчёте, исправить source, выполнить необходимые проверки и commit/export;
только затем distinct new run/key после fresh Info/storage admission.

### Первая задача следующей сессии: устранить истечение окна активации

Ошибка возникает при stable observe в
`apps/worker/tests/update_shutdown/activation_writer.rs:212`: actor вернул
`driver_error`, а не Confirmed receipt. Source-путь writer сначала pin/capability
и `require_custody`, затем admit Issued, потом private send. Общая ошибка live
window создаётся в `apps/update/src/activation.rs`. **Это ориентир для review,
не замена отсутствующему final SQL readback.** Не запускать старый failed child.

Читать в shared report:

- `shutdown-main-pipeline.log`: Ready **90237ms**, StateOwned **120352ms**.
  Эти elapsed имеют origin `execute_inner` **до** baseline/admission; нельзя
  напрямую приравнивать их к original startup age.
- `None-supervisor-stderr.log`: structured activation timing, `bind_files`
  занимает43883ms, после bound remaining66324ms. Нужна диагностика также
  launch/handoff/observe/admission, а не только writer send.
- `None-activation-state-before_release.json` и `...after_release.json`:
  actual lock false→true на том же directory/journal/lock, revision5/HandoffIssued.
  Их observedAt−admittedAt =95691ms/102882ms. Это partial physical observations,
  не полный positive StateOwned/writer proof.

Рекомендуемая последовательность исправления:

1. Readonly разобрать retained timing/graph, при необходимости добавить bounded
   clean-fixture timing вокруг launch/handoff/observe/writer admission. Сверить
   monotonic origin с admittedAt, отдельно длительность RPC и durable work.
2. Найти повторные expensive graph/hash/status/validation действия на исходном
   owned пути. Перенести только избыточную диагностику за receipt/Job exit либо
   переиспользовать доказанно неизменяемую часть snapshot под теми же read pins.
   Любая оптимизация требует review TOCTOU/owner/epoch/file/lock/deadline checks.
3. **Не увеличивать120s, не обновлять admittedAt, не повторять send, не убирать
   необходимые live checks и не подменять Unknown успехом.** Если безопасной
   оптимизации недостаточно, оформить явное новое архитектурное решение вместо
   скрытой смены контракта; текущее принятое окно остаётся прежним.
4. Portable regressions + relevant Linux/MSVC checks; clean commit/export.
   Receiver1a2149f можно переиспользовать только если его код/contracts остаются
   совместимыми с исправлением. Если меняется receiver, нужен новый distinct
   target export/pin; старые clean pins не переименовывать.
5. Fresh Info/disks/cache lease, объявить worker-only/no UI и новый unique run.
   Не использовать failed journal/namespace как новый runtime/reference.
   Принять только полный Main24 + обе migrations + independent audits/ready bytes.
   После success обновить этот checkpoint и только затем перейти к пунктам3–8 ниже.

## 5. Карта кода

| Файлы | За что отвечают |
| --- | --- |
| `crates/protocol/src/update/activation_writer*.rs`, `activation_receiver.rs`, `supervisor.rs`, `preparation_view.rs` | DTO, intrinsic checks, private3, actor24/view16. |
| `apps/update/src/activation/writer*.rs` | Delivery FSM и cold projection. |
| `apps/worker/src/store/activation{,_tests}.rs` | Worker atomic CAS/receipt/migration. |
| `apps/worker/src/store/activation_open{,_tests}.rs` | Store+same-lock owner, single attempt, no-close-checkpoint. |
| `apps/worker/src/store.rs`, `state_backup_export.rs` | Worker17, frozen authority stamp. |
| `apps/update/src/journal/activation_writer.rs`, `activation_writer/tests.rs` | Updater19 factory/history/readback и graph binding. |
| `apps/update/src/runtime_replacement/activation_writer.rs`, `activation_process.rs` | Original guard, Issued before one send, retained child/Job/channel. |
| `apps/worker/src/activation.rs` | Private receiver физического handoff/writer; normal service отсутствует. |
| `apps/update/src/supervisor/activation_files.rs`, `supervisor.rs`, `journal/supervisor.rs` | Actor24 slot/completion/resident view и consistent16 snapshot. |
| `apps/worker/tests/update_shutdown{.rs,/activation_state.rs,/activation_writer.rs,/replacement.rs}` | Actual Main24 от старого shutdown до writer/Job exit, cold/duplicate/fencing. |
| `apps/update/src/runtime_replacement/activation_process_validation.rs` | Explicit clean fixture create-new marker перед send; не production permission. |
| `apps/update/tests/activation_writer_migration.rs` | Actual18→19/refusal, immutable old physical history, no fresh permit. |
| `apps/worker/tests/journal_activation_migration.rs`, `tests/support/journal_migration_queries.rs` | Actual old16 CLI/task/output/dedup, inactive migration, active refusal и точная SQL projection. |
| `scripts/test-update-health-actor.ps1` | Combined writer39 groups/42 commands, exclusive build cache, proof/report publication. |
| `scripts/validate-update-activation-preparation-native.py` | Submit/observe/verify-run; strict new19/17 mode и старые frozen modes. |
| `scripts/update-activation-writer-reference.py`, `update-activation-writer-migration-proof.py` | Old132→18/16 reference, independent migration audit18 mutations. |
| `scripts/update-activation-writer-proof.py`, `test-update-activation-writer-native.py` | Main24 independent proof/audit и component regressions. |
| `scripts/update-activation-{preparation,state}-proof.py`, `test-update-replacement-proof.py` | Shared physical/replacement checks; explicit17 worker scope, legacy defaults16 unchanged. |

Component contracts: [opening](activation-journal-opening.md),
[ledger/private3](journal-writer-runtime.md), [actor24](journal-writer-actor.md).
Их старые строки NEXT/«не выполнено» исторические; состояние сверять с этим checkpoint.

## 6. Что делать в следующей сессии

1. Прочитать AGENTS.md и session docs, git status; затем этот checkpoint и native
   отчёт. Проверить, нет ли живого task/observer/Cargo. Не replay известные tasks.
2. Сверить итог последнего combined gate с actual task, ready hashes и independent
   verifier. Если есть failed участок — **сначала исправить именно его и получить
   полный положительный isolated gate**. Отдельные пройденные migrations не
   доказывают Main24. Не расширять original120s и не ослаблять scope/fencing.
3. После positive writer gate/review следующий вертикальный сценарий —
   **запустить normal local service/reconciliation в том же новом child** под
   committed writer и retained lock. Нельзя повторно normal-open Store и получить
   ещё один epoch/owner. Нужны отдельный typed контракт и durable transition,
   проверки exact process/build/generation/context/schema и чтение прежних tasks.
   Maintenance продолжает закрывать roots/dispatch; user pause/settings сохраняются.
4. Затем **настоящая новая controller session** с pinned build/generation/context.
   Local health и тестовый controllerUrl=None не являются этим подтверждением.
   Controller reconnect не повторяет accepted tasks. Потеря ACK/связи оставляет
   наблюдаемый неопределённый исход, а не разрешение запустить ещё один worker.
5. Затем durable lifetime-transfer intent/readback для свежего owned activation
   Job, подтверждённая передача lifetime, commit и **scoped** maintenance release.
   Job не должен убить успешно активированный worker при выходе updater; до
   подтверждения нельзя выдать cold data за живое владение. Чужие task Jobs и
   пользовательские pause/reasons не менять. Точные новые enum/версии спроектировать
   перед реализацией; перечисленное не является существующими CLI actions.
6. Остальные U2 faults/crash/reconciliation/management: locked exe, disk full,
   partial rename, crash по границам intent/receipt, mixed set, lost confirmation,
   stopped controller, stale updater/worker, duplicate keys. CLI/MCP/status и
   opt-in policy должны объяснять фазу, причину блокировки и manual recovery.
   После unknown нового owner/effects автоматический rollback/retry запрещён.
7. U3: save-before-exit desktop с ACK/actual exit; visible/tray/absent cases,
   обновление desktop при необходимости, background без flash/подъёма окна,
   один tray, понятные popup/sound success/failure и report при worker offline.
   Дополнить effect-free migration rehearsal и desktop failure gates.
8. U4: bootstrap только если установленной capability действительно недостаточно;
   затем actual self-update постоянного runtime без ручной распаковки/Ctrl+C/build,
   stable executable paths, версии/config/history/keys и бизнес-сценарий.
   Ошибка требует конкретной инструкции manual recovery. Только после U1–U3
   isolated gates и policy открывать public install. Затем вернуться к A4.3.

Для каждого следующего контракта: source DTO/FSM + old-view compatibility →
portable failure tests → актуальная Linux/MSVC проверка → clean commit/export →
один native gate через installed worker → independent evidence/review/docs.
Не подменять маленькие unit-компоненты доказательством полного self-update.

## 7. Прежний Windows physical proof: только читать, не resubmit

[Main23](windows-activation-state-gate-2026-10-25.md): terminal **succeeded/0**,
132 tests/32 groups/35 commands; run `20261025-100933-d8a50807`, task
`6bc1623a-c5c4-49ce-8511-cb46b43c7ece`. Дважды сверены45 ready files,
44 replacement +40 state +12 migration mutations rejected.
Это **updater18/worker16/actor23/view15/physical2**, actual updater17→18/refusal,
без writable activation/новой epoch. Закрывающий readonly recheck прошёл снова.

| Pin | Значение |
| --- | --- |
| Producer | `7b270c63e4db9e5195902fba026475c2768f58e1` |
| Producer ZIP | `winbridge-20261025T100857Z-7b270c6-9699c8da.zip` |
| Producer ZIP SHA256 | `ab507939bb9f220eff71da1ec6dfbf49b00318a16069e838a5c14e7149022ccf` |
| Producer manifest SHA256 | `0c2c1bfc6b878125ae609e0d7a07bde7fbd08ac03857ac89c209c73257317c43` |
| Target receiver2 | `1370d6b030e18a22266be1b02d24e4371eee01ac` |
| Target ZIP | `winbridge-20261025T092432Z-1370d6b-257867fc.zip` |
| Target ZIP SHA256 | `579728e29124e3bc914e22f070e3ab5634f68b17d0a0072d74a4387685230656` |
| Target manifest SHA256 | `2dc515ce88cb981a633c9b4acd4a1504d4fc2368d088fb73f3373068d3772f16` |
| Report SHA256 | `0a4df0af94b601e3ce94f9c0f03ee20ca0ff73a8edd05cd7853a3fc739c80cd9` |
| State proof SHA256 | `1b10d8d31b17b563cb37052a8178336ca1cbe80be6919fa467b8592f867bde13` |
| Migration SHA256 | `b68ebc494c19222778f429bbf3db52d739df1de20208199bb20bbca4d7862087` |

Shared reports: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261025-100933-d8a50807/`.
Local verified pin: `.winbridge/workflow-validation/20261025-100933-d8a50807/activation-preparation-verified.json`.
Original observer log: `.winbridge/update-preparation/state23-native-main.log`.

Команда **readonly проверки прежнего terminal run**, без submit новой задачи:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T100857Z-7b270c6-9699c8da.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T092432Z-1370d6b-257867fc.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-state --verify-run 20261025-100933-d8a50807
~~~

Reference здесь — successful production17 task
`a4f17a0d-4eea-4af2-8f2c-78bc6235c0af`. Он не reference для actual18→19.
Если локальные generated pins утрачены, сначала восстановить/проверить их из
retained shared reports и terminal task; отсутствие evidence не повод reexecute.

## 8. Окружение и правила безопасного возобновления

- Debian repo `/home/user1/winbridge`; shared `/mnt/hgfs/shared` ↔ `K:/VMs/shared`.
- Windows source/build `K:/_proj-26/winbridge/src`, stable runtime
  `K:/_proj-26/winbridge/runtime`; не менять firewall paths.
- Isolated project `snapshot-fixture`:
  `K:/_proj-26/winbridge/workspaces/snapshot-fixture/workflow-validation/RUN`.
  Это отдельный validation runtime, не installed target.
- Controller PID3922/parent3921, `ws://192.168.44.203:1143`, socket
  `.winbridge/controller-session/controller-local-development.sock`, config
  `.winbridge/controller-snapshot.local.json`, log
  `.winbridge/update-preparation/controller-resume-20261023.log`.
- Installed worker source `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`, build1792486531570951100,
  generation `8f8d1fb9-e5b1-4984-a17e-d067db46268e`, manifest
  `835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241`.
- Session `0d55d6c6-ad2e-4423-b2d8-53326ab7d14a`, controller generation
  `cf0d5370-6cd0-463f-b671-974879aa6299`. После reboot эти значения могут измениться;
  сверить fresh Info/listener, не запускать второй controller.
- До retry disk queryac04264e-6e15-45f9-a238-4803ff9e1326 succeeded/0:
  Windows13311016960 / Debian25047293952 bytes. Receipt
  `.winbridge/update-preparation/writer24-retry-space-20261025T205730Z-fcf0e2f9.json`.
  Исторический storage не admission следующей сборки.

Если listener отсутствует, из repo запустить:

```bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
```

Safe readonly IPC: импортировать `call` из `scripts/validate-workflow-native.py`,
endpoint — сохранённый socket; методы `info`, `get_task` с `taskId`, `read_output`
с `afterSequence`. Печатать выбранные identity/status/error поля, не полный config,
request, private nonce/token. Output читать bounded pages с sequence cursor.

Перед **каждым** новым Windows gate сообщить, какие worker/desktop нужны,
какие окна появятся и можно ли их трогать; после terminal явно сообщить, что
можно закрыть и какой worker оставить. Runtime writer gate требует только
installed worker, UI не открывает. При unexpected installed worker/manual update
остановить зависимые Windows действия, сообщить причину. Обычные isolated
build/test fixes и повтор после terminal разрешены, подтверждение не спрашивать.

Перед heavy build измерить оба диска: минимум6GiB. Rust: PATH включает
`/home/user1/.local/bin`, `CARGO_INCREMENTAL=0`, основной target, один Cargo/cache
lease. Debian unused reproducible caches можно чистить только без active build;
running controller binary не удалять. **Windows validation/report directories,
runtime/config/journal и пользовательские файлы не чистить.** В closing task
cleanup не выполнялся. System/services/firewall/mount/global config не менять.

Исходный worktree при закрытии clean; изменены только versioned docs.
Generated state/reports/config/DB/logs не коммитить. При следующем изменении важных
untracked/user-dirty файлов соблюдать codex-backup и secret/SQLite backup правила
AGENTS.md. Не менять `/home/user1/backup`.
