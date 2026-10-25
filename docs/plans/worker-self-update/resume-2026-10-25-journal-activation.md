# Updater: передача контекста после завершения проверок

Дата: **2026-10-25, America/New_York**. Это авторитетная инструкция для следующей
сессии без истории чата. Она заменяет прежние NEXT/ACTIVE, включая
[checkpoint до Main23](resume-2026-10-25-state-handoff.md). Пользователь поручил
закончить текущие проверки, описать точную границу реализации и сделать коммит.

**После checkpoint36f9895 добавлены Rust Main24 и независимый writer verifier.**
Они прошли source/cross/local проверки; нового Windows writer-прогона нет.
После Main24 добавлены PS/observer writer mode и реальные миграционные fixtures.
Они прошли source/local/cross проверки; Windows execution/PS parse ещё впереди. Ожидающих
Cargo/native/observer runs нет. Receiver pin остаётся `7fd8135`/target `1a2149f`.
Самообновление целиком **не готово**, public install остаётся выключенным.

Первое следующее действие: закончить **combined native writer gate** для уже
готового receiver `1a2149f`: clean distinct producer/export, Windows PS syntax
preflight и один автоматический прогон с actual updater18→19/worker16→17/refusal. Обновлять установленный worker для этого
сейчас не требуется. Подробности, ограничения и критерии ниже.

**Native update:** [первый actual run](windows-journal-writer-gate-2026-10-25.md)
terminal failed (run203011-cffafea9/taskabf1250f). Preliminary41 passed, actual
migration/Main24 не достигнуты: неверный SQL column в fixture. Исправлена projection
command/record, portable real-Store regression1/0 и MSVC passed. Нужен новый clean
fixed producer/export/run; первый не replay. Подробный отчёт содержит ZIP/proof
pins, parse-only success и cache lineage. Installed worker исправен/idle.

## 1. Что реализовано и что ещё не доказано

Независимые версии текущего source: **updater journal19 / worker journal17 /
supervisor24 / consistent view16 / private physical2 + writer3**. Writer DTO
command/receipt имеют schema1. Номер private contract не равен номеру DB schema.

| Часть | Реальная готовность |
| --- | --- |
| U0 / внутренний U1 | Проверка и применение исходников, build, candidate rehearsal, drain/quiesce и exact shutdown реализованы; прежние native gates пройдены. |
| U2.1 | Consistent final backup и fault gates пройдены. |
| U2.2 physical | Preparation/replacement, один private child, Ready и передача ему прежнего instance lock подтверждены на Windows для updater18/worker16/private2. |
| Worker journal activation | Реализованы exact old-header CAS + immutable receipt в одной транзакции, schema17 и обычная inactive16→17 migration с backup. Source/local/cross checks прошли. |
| Existing-state opening | `ActivationJournal` удерживает Store и тот же acquired lock; opening без CREATE/migration/обычного begin_authority, одноразовая попытка и retained failure custody реализованы. |
| Writer pipeline | Updater19 intent/history, consuming native driver, worker private3 handler и supervisor24/view16 реализованы. Rust Main24, independent verifier, PS/observer mode и migration fixtures добавлены; actual gate ещё не выполнен. **Настоящего Windows writer proof ещё нет.** |
| Activated normal service | Reconciliation, normal local IPC, новая controller session и передача lifetime ещё не реализованы. |
| Завершение обновления | Commit/scoped release, оставшиеся U2 faults/management, U3 desktop и U4 installed self-update впереди. |

Advertised `activation`, `controller_confirmation`, `desktop_update` остаются
`None`. Сохранённая Confirmed receipt не означает готовый сервис или успешное
обновление целиком. Данные не восстанавливают kernel custody и startup permission.
Installed runtime/config/journal не заменялись.

## 2. Что сделано в последнем блоке

Последовательность commits позволяет восстановить развитие без истории чата:

| Commit | Содержание |
| --- | --- |
| `f8cf372` | Strict writer DTO, чистая модель и frozen authority fingerprint. |
| `e3a4a3d` | Worker17: atomic header/receipt, old-owner fencing, migration/backup. |
| `f5dcec1` | Existing-state connection под прежним lock, retained failures, запрет close-time WAL checkpoint. |
| `768422d` | Updater19 graph-bound intent/history, private3 frames, consuming driver и worker handler. |
| `1a2149f` | Supervisor24/view16, consuming actor, быстрый ACK/busy, local mailbox/legacy checks. |
| `7fd8135` | Exact pin clean receiver `1a2149f` для будущего native gate. |

Worker сверяет unchanged accepted shutdown и Quiesced lease, затем в одной
IMMEDIATE transaction делает CAS прежнего header: тот же incarnation, epoch+1,
новый private authority ID; сохраняет immutable command/hash/receipt и проверяет
readback. Fresh permit означает только принятую DB-транзакцию. Duplicate получает
прежнюю receipt без нового permit/epoch. Ошибка commit означает outcome unconfirmed;
автоматического повторного эффекта нет. Maintenance не снимается, обычные записи,
admin operations и обычный release остаются закрытыми.

Открывающий owner не извлекает Store или lock наружу. До записи проверяется exact
existing schema17/WAL/accepted shutdown, сохраняются первоначальные process/runtime/
state pins и исходное временное окно. Одна попытка помечается до транзакции; даже
известный rollback не превращает тот же owner в повторно используемый grant.

Updater factory в одном snapshot связывает current StateOwned6, полный graph и
old authority из verified final backup. Политика пока **same worker schema**.
Durable Issued0 предшествует одной private send; затем возможны Confirmed1 или
Indeterminate1→Confirmed2 как поздний факт. Key/command/deadline неизменяемы.
Partial write/EOF/timeout не разрешают повтор. Worker private3 сверяет actual
parent/runtime/lock, updater snapshot и original monotonic deadline, делает CAS,
возвращает authenticated receipt и удерживает Store/lock/data permit. Normal
scheduler, local IPC и controller ещё не стартуют.

Actor24 потребляет только original resident Owned guard через один blocking slot.
Один correlation key может появиться до durable Issued: сам key не доказывает
admission. Duplicate start только наблюдает. Completion проверяет exact key/intent/
physical receipt и graph-bound Confirmed; owner/failure сохраняются при любом исходе.
Cold view — NotResident, liveness unknown. Views1–15 остаются frozen.

### Существенные результаты review

- Найден и воспроизведён close-time эффект SQLite: последняя ещё не активированная
  RW connection при Drop checkpoint-ила/удаляла исходный WAL. RO/RW opening теперь
  до первого DB query устанавливает и проверяет `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE`.
- Receipt validation повторяет intrinsic command validation; согласованно
  пересчитанный SHA не скрывает неправильные IDs/scope/epoch/process/file pins.
- Authority fingerprint использует общий frozen V1 helper и golden vector;
  private authority ID не экспортируется.
- Поздняя writer receipt связывается с immutable StateOwned6, даже если текущая
  activation уже Indeterminate7. Для fresh send progressed prefix не подходит.
- Перед private write явно запрещено нулевое remaining startup window.
- Actor24 start/busy не делает тяжёлый recursive status audit перед grant.
  Это важно для оставшихся примерно10s из прежнего original120s окна.
- Local mailbox тест требует свежий standalone updater binary. Ранее stale image
  давал EOF; explicit build перед тестом исправил сценарий, production checks не ослаблялись.

## 3. Законченные проверки и evidence

После36f9895 добавлен source Main24: original child/lock, один writer command,
actual worker receipt/header observations, stale cached owner refusal, все прочие
worker tables сохранены, duplicate/legacy/cold/actual Job exit. Fast branch сразу
передаёт24 после StateOwned; тяжёлый physical-only audit до него не выполняется.
Отдельный create-new trace перед private send существует только в explicit clean
validation fixture. Marker сам не доказывает delivery; он связан с обеими receipts.

Дальше source wiring завершено: `--activation-writer` / `-ActivationWriterGate`,
strict19/17 archives и private3, отдельный reference из actual physical132,
actual18→19/refusal и worker inactive16→17/active16 refusal fixtures. Updater
migration сохраняет всю physical history и возвращает Existing data без fresh
permit; это не обязано быть ошибкой. Worker fixture запускает actual old16 CLI
в отдельном instance, связывает task/output/dedup с DB, сохраняет read pins копий
PE и проверяет consistent backup/старый16 refusal. Весь installed runtime прежний.
Source checks: Python24/0, Linux/MSVC all-target validation Clippy и legacy132
readonly regression passed. PS в Debian отсутствует, syntax preflight выполнить
на Windows до heavy build. Latest logs `writer-migrations-*-final.log`,
`writer-migrations-legacy-recheck.log`. Эти проверки не native migration proof.

Предыдущие Main24 проверки: portable validation hooks **4/0**, Python preparation/process/
state/writer **5+3+5+6=19/0**, Linux/MSVC workspace all-target validation Clippy,
fmt/scaffold/diff passed. Shared physical prefix verifier выделен без ослабления
legacy16/18; старый actual132/0 снова прошёл readonly recheck (45 files twice,
44+40+12 mutations). Latest logs `writer-main24-*.log`, summary
`writer-main24-source-checks.json`. Default workspace781/0 ниже — baseline1a2149f,
после test-only изменений полный workspace повторно не запускался.

Review исправил новую fixture: baseline требует **Exited**, а не только ACK
старого worker. Python fixtures проходят JSON roundtrip, чтобы независимые
наблюдения не менялись вместе через object aliasing. Validation marker связывается
с build того же sender/lock-boundary, source pins совпадают с Main; buildId разных
сборок не обязан совпадать. Новых wire contracts/публичных действий не добавлено.

Для функционального source `1a2149f` pipeline уже завершён. При закрытии заново
сверены machine summary, существование логов и суммарные Rust test results;
полные неизменившиеся сборки повторно не запускались.

| Проверка | Результат |
| --- | --- |
| Полный Rust workspace | **781 passed / 0 failed / 16 ignored** |
| Focused updater writer | **19 passed / 0 failed** |
| Реальный local mailbox24 | **1 passed / 0 failed**, внутри14 invalid scope/version/token/extra-command requests без journal effects |
| Standalone updater с update-validation | Build passed до mailbox test |
| Linux all-target validation Clippy, warnings=errors | Passed |
| MSVC all-target validation Clippy, warnings=errors | Passed; это cross-check, не Windows execution |
| Generated contracts, check:contracts, TypeScript, UI build | Passed |
| Rust fmt, scaffold/Markdown links, git diff check | Passed |
| Закрывающий readonly recheck прежнего Windows Main23 | **Passed:132 tests,45 files twice,44 replacement +40 state +12 migration mutations rejected** |
| Закрывающий recheck clean receiver ZIP | Passed:813 paths/sizes/SHA, exact archive/manifest/source/schema/marker pins |

Логи source pipeline: `.winbridge/update-preparation/actor24-*.log`, summary
`actor24-checks.json`. Закрывающие generated evidence в том же каталоге:
`checkpoint-20261025-close.json`, `checkpoint-20261025-info.json`,
`checkpoint-20261025-physical-recheck.log`. Эти private/runtime файлы не коммитить.

Предшествующие focused Store activation12/0 и opening8/0 описаны в
[opening report](activation-journal-opening.md); они входят в последующие workspace
проверки и не добавляются к781 как отдельные новые тесты. SQLite tests используют
реальные транзакции/locks, но часть process/graph metadata synthetic. Они не
доказывают actual Windows writer, настоящий old16 executable migration или сервис.

При изменении соответствующего кода воспроизводить проверки последовательно:

~~~bash
export PATH=/home/user1/.local/bin:$PATH
export CARGO_INCREMENTAL=0
cargo build -p winbridge-update --features update-validation --locked
cargo test -p winbridge-update --lib writer --locked
cargo test -p winbridge-worker --lib activation_writer_v24 --features update-validation --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --features winbridge-worker/update-validation --locked -- -D warnings
cargo clippy --workspace --all-targets --features winbridge-worker/update-validation --target x86_64-pc-windows-msvc --locked -- -D warnings
cargo run --locked -p winbridge-protocol --example export-types --features typescript
npm run check:contracts
npm run typecheck
npm run build:ui
cargo fmt --all -- --check
npm run check:scaffold
~~~

Перед heavy build сначала fresh измерение **обоих** дисков≥6GiB. Не редактировать
Rust во время Cargo; не запускать конкурирующие feature/profile builds.

## 4. Актуальная карта кода

| Файл | Что читать/менять |
| --- | --- |
| `crates/protocol/src/update/activation_writer.rs`, `activation_writer_view.rs` | Writer DTO и view16. |
| `crates/protocol/src/update/activation_receiver.rs` | Frozen physical2, отдельные private writer3 frames/marker. |
| `apps/update/src/activation/writer.rs`, `writer_record.rs`, `writer_view.rs` | Intrinsic validation/hash, delivery FSM, cold projection. |
| `apps/worker/src/store/activation.rs`, `activation_tests.rs` | Worker atomic CAS/receipt и migration preflight/backup. |
| `apps/worker/src/store/activation_open.rs`, `activation_open_tests.rs` | Store+same-lock owner, single attempt, no-close-checkpoint. |
| `apps/worker/src/store.rs`, `state_backup_export.rs` | Schema17 и frozen public old-authority stamp. |
| `apps/update/src/journal/activation_writer.rs`, `activation_writer/tests.rs` | Schema19 factory/history/readback, same snapshot backup binding, child snapshot. |
| `apps/update/src/runtime_replacement/activation_writer.rs` | Original guard, Issued before one send, receipt/uncertainty и retained custody. |
| `apps/update/src/runtime_replacement/activation_process.rs` | Original process/Job/channel и physical state handoff. |
| `apps/worker/src/activation.rs` | Действующий private3 writer handler; normal service пока отсутствует. |
| `apps/update/src/supervisor/activation_files.rs`, `supervisor.rs` | Actor24, fast dispatch/busy, one slot/key, completion и resident projection. |
| `apps/update/src/journal/supervisor.rs` | Consistent snapshot16, graph-bound writer view. |
| `apps/worker/src/update_control.rs` | Actual local mailbox24 regression. |
| `apps/worker/tests/update_shutdown.rs`, `update_shutdown/activation_state.rs`, `update_shutdown/activation_writer.rs` | Main24 source: fast same-child dispatch, SQL/header/fencing checks и cold actual exit; ignored native test ещё не выполнен. |
| `apps/update/tests/activation_migration.rs` | Existing actual updater17→current migration fixture; нужен новый actual18 reference. |
| `scripts/test-update-health-actor.ps1` | Writer flag, explicit18/16 references, exact ignored Main24/migration tests, inline minimum selection checks, restored test env. |
| `apps/update/tests/activation_writer_migration.rs`, `apps/worker/tests/journal_activation_migration.rs` | Новые actual old executable migration/refusal fixtures; пока cross-only. |
| `scripts/update-activation-writer-reference.py`, `update-activation-writer-migration-proof.py` | Verified old132→18/16 references и independent migration audit18 mutations. |
| `scripts/validate-update-activation-preparation-native.py` | Debian observer/submit/verify-run; старые exact18/16 assumptions. |
| `scripts/update-activation-preparation-proof.py`, `update-activation-state-proof.py`, `update-replacement-proof.py` | Shared strict scope/physical/replacement checks; explicit19/17 path, legacy defaults прежние. |
| `scripts/update-activation-writer-proof.py`, `test-update-activation-writer-native.py` | Новый writer verifier и synthetic component regressions; top-level actual proof пока отсутствует. |

Более подробные component contracts: [opening](activation-journal-opening.md),
[ledger/private3](journal-writer-runtime.md), [actor24](journal-writer-actor.md).
Нижние статусы исторических документов относятся к указанным там commits.

## 5. Receiver готов; harness producer ещё не создан

Использовать **target** из clean source commit
`1a2149f88407fd9c33d101e12d07d7c4b64599d3`, с built UI:

| Pin | Значение |
| --- | --- |
| ZIP | `winbridge-20261025T163918Z-1a2149f-8b8c082c.zip` |
| ZIP SHA256 | `abc74d43ee4e43f0b60a86f0cd3bc9dd35679fcf8b6f010cb1ca3c5c72573c1b` |
| Manifest SHA256 | `b27808d7f52abd9fdbb4c5fe01e078493950d22e5f5a46c8b59b992bb453bed1` |
| Payload | 813 files /13011755 bytes |
| Contracts | updater19/worker17/actor24/view16/physical2/writer3 |

ZIP лежит в `/mnt/hgfs/shared/WinBridge/sources/`. Local export/pin:
`.winbridge/update-preparation/actor24-receiver-{export,pin}.json`.
Все manifest paths/sizes/SHA повторно сверены при закрытии.
**windowsExecutionVerified=false**: это source fixture, не готовый native binary proof.

После реализации harness нужен **другой clean producer commit/export**.
Старый target1370d6b содержит worker16/private2 и не подходит same-schema writer gate.
Не использовать нынешний legacy runner с новой19/17 ZIP, просто ослабив проверку
schema: `require_runtime_schemas` пока намеренно требует worker16. Новый режим
должен строго различать версии; старый `--verify-run` сохранить для старых proofs.

## 6. Первый следующий вертикальный сценарий

### Main24: продолжить тот же child, а не создать новый

1. Rust input `activationWriter`, writer result kind и ignored test
   `actual_main_activates_one_journal_writer_without_service_or_replay` **добавлены**.
   PS runner/observer **подключены**: `-ActivationWriterGate` / `--activation-writer`.
   Их настоящего Windows результата пока нет.
   Existing modes23/22/21 оставить доступными. Новый Rust test принимает только
   согласованные writer/state/process/files/preparation flags; install не включает.
2. Обычный путь: старый worker shutdown → final backup → preparation/replacement
   → Prepare20 → Bind21 → Launch22/Ready → Handoff23/StateOwned.
3. **Сразу после StateOwned отправить24**, без промежуточных тяжёлых audits.
   `activation_state::execute_writer` уже использует общий быстрый путь и новую
   ветку до многочисленных physical-only audits; старый `execute` сохранён.
   Проверить реальное время в native run; assertions не заменяют actual proof.
4. `SupervisorChild::activation_writer(start)` возвращает
   `Result<Option<UpdatePreparationView>>`. **None — только volatile dispatch ACK
   или busy**, не durable Issued и не success. Stable observe после retirement
   возвращает Some16. Старый `activation_process::call` требует Some и без нового
   helper для24 непригоден. Новый writer helper None принимает; потерянный ACK и
   duplicate source assertions добавлены, native proof ещё нужен.
5. Original startup window **120000ms не продлевать**. В physical132 Saved
   StateOwned был109925ms после admission, observed107679ms: оставалось около10s.
   Helper wall timings имеют другой origin. Если нужно ускорить validation probes,
   сделать отдельный быстрый trace mode; не отключать live/scope/window checks.
6. После receipt закончить live observations, остановить изолированный test Job
   и дождаться actual signalled process/empty Job/освобождения того же lock.
   Тяжёлые independent audits выполнить после этого; public serve не запускать.

### Миграции: реальные прежние executables, отдельные доказательства

- **Updater18→19/refusal.** Взять consistent copy successful18 history из
  physical132 run `20261025-100933-d8a50807`. Новый reference должен связать exact
  old18 image/descriptor/source/ready/report/history hashes и terminal task.
  Reference **подготовлен из проверенного Main23**, helper:
  `scripts/update-activation-writer-reference.py`. Composite kind
  `journal_writer_migration_reference`, отдельные inner refs18/16. Old image
  source1370d6b — target старого report, не его producer7b270c6. Local file
  `.winbridge/update-preparation/writer-migration-reference18-16.json`, SHA256
  `bd1e71f1181236e4b8e3c4f140e8b716abe3c8908c10d11651132252c25f3772`.
  Старый `activation_schema17_reference` нужен
  legacy17→18 и не становится18 сменой поля schema. После19 старый executable18
  должен отказать без writable изменений; проверить backup, историю и fencing.
- **Worker inactive16→17/refusal.** На actual old16 executable получить журнал с
  tasks/output/dedup keys; мигрировать consistent copy новой17, проверить
  сохранность данных/backup/authority и отказ старого16 после17.
- **Worker active16 отказ.** Отдельно получить настоящий accepted-update/shutdown
  old16 и подтвердить, что обычный new17 open отказывает до writes/backup.
  Production old16 пока не умеет закончить self-update/release accepted shutdown:
  нельзя искусственно «освободить» его историю и назвать actual successful16 flow.
  Synthetic released-history preservation уже проверяется unit tests.

### Acceptance нового независимого verifier

Новый verifier связывает actual SQL observations обоих journals, immutable
command/hash/receipt и prefixes. Synthetic tests проверяют delivery component,
а не actual Main graph. В combined run вызвать top-level verify/audit; проверить:

- Тот же child PID/birth/host/session/generation, state lock, loaded file IDs,
  runtime/config/owner/attempt; один launch, один private send и один epoch increment.
- Worker header имеет прежний incarnation/epoch+1/new authority fingerprint;
  receipt атомарна header. Cached old authority fenced, lease Quiesced и accepted
  shutdown неизменны; root остаётся VerifyingLocal, normal service не объявлен.
- Updater Issued/Confirmed принадлежат exact command/worker receipt/StateOwned6/
  verified final backup. Duplicate, lost ACK и cold read не дают replay/новый owner.
- Saved Confirmed остаётся фактом после actual child exit; cold NotResident и
  unknown liveness не превращаются в readiness. Test Job/locks освобождены реально.
- Согласованные tamper/mutation scope/epoch/PID/birth/file IDs/hash/history,
  invalid bool/nullable observations и FILETIME отвергаются. Secret/nonce/token/
  serialized permissions не допускаются в отчёт. Сохранить прежние physical checks.

После source review/checks — clean producer commit/export, fresh Info/disk/cache checks и **один combined Windows run**, включая реальные migration/refusal cases.
Сохранить request/key/taskId до наблюдения; при потере сессии сверять тот же task,
не запускать второй «на всякий случай».

### Команда нового combined gate после clean producer export

Сначала проверить PS syntax на Windows без исполнения script; здесь pwsh нет.
Сам gate требует только installed worker, не открывает desktop. Target receiver
остаётся1a2149f; old reference pin теперь **Main23**, не прежний compatible-drain17.

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/NEW-CLEAN-PRODUCER.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T163918Z-1a2149f-8b8c082c.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261025-100933-d8a50807/activation-preparation-verified.json \
  --activation-writer
~~~

`NEW-CLEAN-PRODUCER.zip` — placeholder, пакет ещё не экспортирован. Сохранить
returned run/taskId. Для восстановления тот же argv плюс `--verify-run RUN`;
не повторять submit. Metadata по-прежнему `activation-preparation-run.json`,
но key prefix `native-activation-writer-`. Independent observer проверит Main24,
обе migration proofs, exact commands/test counts, hashes/ready files twice и
negative mutations. Ожидается39 test groups/42 commands; actual counts взять
из результата, не выдавать minimum за выполненные tests. Native proof ещё нет.

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

## 8. Окружение и состояние при закрытии

После source Main24 fresh Info **2026-10-25T19:30:01Z** подтверждает тот же
installed d052/build/generation/session, connected/active0, maintenance/recovery
errors=null, coverage.complete=true. Evidence `writer-main24-post-info.json`.
Disk query task `27610f43-0022-45cd-9ae8-b16ef30513bc` succeeded/0:
Windows13764911104 / Debian25153089536 bytes перед source checks; receipt
`writer-gate-disk-20261025T191227Z-6fe5b46f.json`. Debian после checks25141014528.
Cleanup не выполнялся, нового native writer task нет. Desktop можно закрыть,
installed worker оставить. Следующие пункты также сохраняют исходный closing36f
и environment pins; historical disk measurements не заменяют fresh admission.

- Debian repo `/home/user1/winbridge`; shared `/mnt/hgfs/shared` соответствует
  `K:/VMs/shared` на Windows. Source/build `K:/_proj-26/winbridge/src`, stable
  runtime `K:/_proj-26/winbridge/runtime`; firewall paths остаются прежними.
- Isolated validation project `snapshot-fixture`, root
  `K:/_proj-26/winbridge/workspaces/snapshot-fixture`. Installed runtime не target
  этого native gate. Windows validation directories не удалялись.
- Controller PID3922 (parent3921), `ws://192.168.44.203:1143`; socket
  `.winbridge/controller-session/controller-local-development.sock`, config
  `.winbridge/controller-snapshot.local.json`, log
  `.winbridge/update-preparation/controller-resume-20261023.log`.
- Installed worker source `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`, generation
  `8f8d1fb9-e5b1-4984-a17e-d067db46268e`, buildId1792486531570951100,
  manifest835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241.
- Fresh Info **2026-10-25T16:52:47Z**: connected, activeTaskCount=0,
  maintenanceError=null, journalRecoveryRequired=null, recoveryCoverage.complete=true.
  Session `0d55d6c6-ad2e-4423-b2d8-53326ab7d14a`, controller generation
  `cf0d5370-6cd0-463f-b671-974879aa6299`. Physical task и disk task перепроверены
  как terminal succeeded/0. Новых Windows tasks при закрытии **не отправляли**.
- Последнее actual Windows disk measurement: task
  `71d3d949-57a9-4a2e-9530-8a8805d6975b`, succeeded/0,
  Windows13696045056 bytes, Debian25193771008 bytes перед actor24 checks;
  receipt `actor24-disk-20261025T160724Z-6de358fe.json` в update-preparation.
  Debian при закрытии25151401984 bytes. Это исторические измерения, не fresh
  admission для следующей сборки. При закрытии heavy build и cleanup не требовались.
- Reusable Windows cache run `20261017-061527-f4639971`, подкаталог
  `test-build-cache`. Проверить saved cache receipt, брать **один exclusive lease**.
- Cargo/native/observer processes завершены; работающий controller оставлен.
  **Desktop можно закрыть, installed worker оставить.** Тестовых UI окон не было.

После reboot сначала выполнить AGENTS.md session reads/git status, проверить
listener/свежий Info; PID/адрес выше не считать вечными. Не запускать второй
controller. Если listener отсутствует, запуск из repo:

~~~bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
~~~

Для readonly IPC использовать `call` из `scripts/validate-workflow-native.py`:
`info`, `get_task` с сохранённым taskId, `read_output` с afterSequence.
Не выводить полный config/request/private nonce/token; печатать выбранные safe
identity/status fields. Живой/unknown task сначала reconcile; **никогда не replay**.

Перед каждым новым Windows run сообщить пользователю: достаточно installed
worker, desktop не нужен, текущий runtime-only gate не открывает UI. После terminal
явно сообщить, какие окна можно закрыть и какой worker оставить. При неожиданном
поведении installed worker или необходимости его ручного обновления остановить
зависимые Windows действия и объяснить причину. Обычные isolated build/test
исправления и повтор после terminal исхода уже разрешены; не спрашивать снова.

Не менять system/services/firewall/mount/global config. Windows reports/validation
каталоги не очищать. Debian unused reproducible caches можно чистить только без
active build; не удалять running controller binary, runtime/config/journal/reports.
Использовать CARGO_INCREMENTAL=0 и основной target. В этом closing task исходный
worktree был clean, исходники восстанавливаются Git; private generated evidence
не входит в commit. Для будущих важных unversioned/user-dirty файлов соблюдать
codex-backup/secret/SQLite consistent backup правила из AGENTS.md.

## 9. После native writer gate

1. Normal service/reconciliation под fresh writer; normal IPC подтверждает exact
   process/build/generation/context/schema, user pause/settings сохраняются.
   Maintenance не открывает roots/dispatch до trusted commit.
2. Настоящая **новая controller session/build/generation**, отдельно от local
   health. Тестовый controllerUrl=None не является controller confirmation.
3. Durable intent/readback и отдельная передача lifetime свежего owned activation
   Job, затем commit/scoped lease release. Не менять чужие task Jobs или firewall.
4. Оставшиеся U2 fault/crash/recovery/reconciliation сценарии и понятные CLI/MCP
   status/cancel/opt-in. Неизвестный исход не разрешает автоматический restart.
5. U3: desktop form-save/exit ACK, обновление desktop при необходимости, понятные
   popup/tray/sound success/failure notifications с учётом запущенного UI.
6. U4: bootstrap только если installed worker действительно требует новой
   capability, затем actual self-update постоянного runtime с автоматическим
   health/result и конкретной manual recovery при ошибке. До этого public install off.
7. После U4 вернуться к архитектурному A4.3 по основному долгосрочному плану.

Это план будущих контрактов, а не уже существующие CLI actions. Номера сверять
с новым HEAD. Сначала доказать isolated happy path и recovery, затем открывать
public install. Общую цель updater не отмечать завершённой по этому checkpoint.
