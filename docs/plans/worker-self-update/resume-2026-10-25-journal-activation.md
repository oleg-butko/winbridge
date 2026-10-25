# Продолжение updater: journal19/17 и private writer3

Дата сессии: 2026-10-25, America/New_York. Этот checkpoint заменяет прежние
NEXT/ACTIVE и [инструкцию до Main23](resume-2026-10-25-state-handoff.md).
Он предназначен для агента без контекста чата. Пользователь поручил завершить
текущие проверки и закоммитить подробное состояние. Общая цель самообновления
остаётся незавершённой; public install включать пока нельзя.

**Продолжение после e3a4a3d/f5dcec1:** [opening library](activation-journal-opening.md)
и [ledger19/native writer driver/private3 handler](journal-writer-runtime.md)
реализованы. **Первый следующий шаг — supervisor actor24/view16**, затем combined
native19/17 writer gate. Нового native proof пока нет. Старый physical Main23
terminal и не resubmit; JSON receipt не разрешает обычный запуск или новое владение.

## 1. Реальная граница готовности

| Компонент | Состояние на этом checkpoint |
| --- | --- |
| Основной WinBridge | Установленный worker исполняет обычные изолированные Windows-проверки; его runtime не заменялся. |
| U0 / внутренний U1 | Source verification/apply, build, candidate rehearsal, drain/quiesce и exact shutdown реализованы; native gates пройдены. |
| U2.1 | Final consistent backup и fault gates пройдены. |
| U2.2 physical | Preparation/replacement, один private receiver, Ready и передача прежнего instance lock тому же child проверены на Windows. |
| Worker SQLite activation | **Реализован слой данных:** old-header CAS + immutable receipt в одной транзакции, worker schema17, миграция16→17 с backup. |
| Existing-state connection | `ActivationJournal` владеет тем же acquired lock и Store, отказывает без CREATE/migration, сохраняет failure custody и не повторяет попытку. Trusted library, без runtime entry. |
| Private writer | Graph-bound ledger19, consuming native driver и worker V3 handler реализованы. Supervisor actor24/view16 и native gate ещё не подключены. |
| Local IPC / controller / lifetime | Для activated receiver ещё не реализованы и не подтверждены. |
| U3 / U4 | Desktop update/notifications и настоящее installed self-update впереди. |

Номера независимы: **updater journal19 / worker journal17 / supervisor23 /
consistent view15 / private physical handoff2 + writer3**. DTO writer command/receipt
имеют schema1; private3 связывает их с authenticated channel; public RPC/capability не объявлены.
Старые wire/view shapes сохранены. Advertised `activation`,
`controller_confirmation`, `desktop_update` остаются `None`; public install off.

Native132/0 ниже относится к **worker16**, а новый SQL-код — к **worker17**.
Нельзя переносить доказательство physical handoff на writable activation или
выдавать updater migration17→18 за worker migration16→17.

## 2. Что сделано в текущем блоке

### Атомарная активация данных worker

`apps/worker/src/store/activation.rs` добавляет
`activate_journal_authority_checked(command, accepted_shutdown, checkpoint)`:

1. Проверяет intrinsic command envelope, exact shutdown scope, physical state
   identities, новый generation и точную worker schema17.
2. В coordinated IMMEDIATE transaction сверяет прежнюю authority и неизменяемую
   accepted shutdown receipt. Действующий lease должен оставаться Quiesced.
3. Для fresh command сравнивает public старый authority stamp с реальным
   header, вызывает live-check callback и проверяет исходное временное окно.
4. Делает CAS полного старого header: прежний incarnation, **epoch+1**, новый
   private authority ID. Проверяет affected rows и readback.
5. В той же транзакции вставляет command/hash/receipt в
   `worker_journal_activations`, перечитывает их, повторяет live/window checks,
   затем commit. Локальный cached header обновляется только после commit.

Receipt и header либо появляются вместе, либо откатываются вместе. Пропущенные
trigger-ом записи и ошибки checkpoint не дают acceptance. Ошибка commit
сообщает **outcome unconfirmed**: сначала чтение receipt/header, автоматического
повторного эффекта нет.

Fresh возвращает non-Clone/non-Serde `JournalActivationPermit`; **это только
факт принятой DB-транзакции**, не OS/startup/lifetime permit. Повтор возвращает
Observed с прежней receipt, без новой epoch/permit. Другой payload того же key
и второй command для того же update отклоняются. Cold read проверяет command
SHA, точную schema, accepted shutdown и authority history; владение ресурсами
из него не восстанавливается.

Maintenance lease и сохранённая остановка не снимаются. Обычные записи,
`begin_authority`, admin operations и обычный release остаются запрещёнными.
Расширять этот метод до запуска scheduler или общего «ignore maintenance» нельзя.

### Schema17, migration и recoverability

- Новая таблица с update primary key, unique command ID, command/hash/receipt
  и immutable/retained triggers. Header trigger дополнительно запрещает менять
  incarnation при live authority activation.
- `Store::open` делает readonly preflight до writable open старой15/16 или
  будущей schema. Незавершённый accepted update блокирует обычную миграцию.
- Для неактивной schema16 создаётся новый `journal-v16-<time>-<uuid>.sqlite`
  через SQLite backup API **до journal-mode/schema writes**. Проверяются schema16,
  quick_check и прежний authority. Backup file/parent синхронизируются.
- Backup handle удерживается до migration commit. FileID перепроверяется;
  Windows handle запрещает delete/rename. Дубликат байтов под новым FileID
  не принимается за прежний backup. При ошибке частичные файлы сохраняются.
- Миграция выполняется IMMEDIATE transaction с повторной проверкой backup и
  source authority; история, task dedup keys и accepted shutdown сохраняются.
  Backup здесь восстановительный артефакт, **не автоматическое разрешение downgrade**.
- Safe-path проверка идёт до `try_exists`: dangling symlink не разрешает создать
  журнал в постороннем месте. Путь должен быть абсолютным, без links/reparse.

Это обычная **неактивная16→17 migration**. Cross-schema takeover остановленного
активным updater worker16 ещё не поддерживается. Для первого writer gate нужны
изолированные baseline и target с worker17. Installed старый worker может быть
только транспортом запуска этого сценария; он не становится его activation target.

### Review и исправления

- Receipt validator теперь повторяет intrinsic command validation. Ранее
  согласованно пересчитанный SHA мог скрыть внутренне некорректный command.
  Проверяются IDs, hashes, old/physical scope, process birth, file IDs и окно;
  одного корректного SHA недостаточно.
- Вычисление authority fingerprint вынесено в общий Store helper, экспорт
  backup использует тот же frozen V1 prefix/tuple encoding. Golden vector
  предотвращает незаметную смену binding; private authority ID не экспортируется.
- Strengthened incarnation guard потребовал исправить **foreign-DB test fixture**:
  сначала тест подтверждает запрет live change, затем только внутри контролируемой
  fixture временно снимает и восстанавливает guard. Production guard не ослаблен.
- Старые migration fixtures теперь действительно удаляют новую17 таблицу перед
  искусственным downgrade. `CREATE IF NOT EXISTS` вместо строгой миграции не добавлен.
- В closing test обнаружен ложный выбор `-wal`/`-shm` вместо backup DB по общему
  префиксу. Теперь выбирается ровно один файл `.sqlite`; backup helper до этого
  уже проверял саму базу. Ошибка теста исправлена и повторно проверена.

## 3. Карта кода для продолжения

| Файл | Назначение |
| --- | --- |
| `crates/protocol/src/update/activation_writer.rs` | Три strict DTO: binding, command, receipt; Rust/JSON Schema/TS. |
| `apps/update/src/activation/writer.rs`, `writer_tests.rs` | Pure prepare/validate/hash, intrinsic envelope и receipt checks. |
| `apps/worker/src/store/activation.rs`, `activation_tests.rs` | Новый SQLite CAS/receipt, migration preflight/backup и реальные DB tests. |
| `apps/worker/src/store/activation_open.rs`, `activation_open_tests.rs` | Existing-state connection + actual lock в одном owner, одноразовый CAS и retained failures; no-close-checkpoint policy. |
| `apps/worker/src/store.rs` | Schema17 и порядок обычной миграции. |
| `apps/worker/src/state_backup_export.rs` | Публичный old-authority stamp через общий helper. |
| `apps/worker/src/store/archive.rs`, `shutdown.rs` | Existing backup FileID checks и strict accepted shutdown readback. |
| `apps/worker/src/activation.rs` | Живой authenticated receiver; сейчас до physical StateOwned и readonly inspection, **без writer**. |
| `apps/update/src/activation/channel.rs` | Frozen physical private2 и marker; writer handler отсутствует. |
| `apps/update/src/runtime_replacement/activation_process.rs` | Original resident process/Job/channel, consuming state handoff и retained failure custody. |
| `apps/update/src/journal/activation.rs` | Current/history graph schema18 до StateOwned6; writer ledger отсутствует. |
| `apps/update/src/supervisor/activation_files.rs`, `supervisor.rs` | Actor23, один blocking slot, живые guards отдельно от cold views. |
| `apps/worker/tests/update_shutdown/activation_state.rs` | Пройденный actual physical Main23; не writer gate. |
| `scripts/validate-update-activation-preparation-native.py`, `update-activation-preparation-proof.py` | Native observer и независимая проверка physical proof; будущий writer mode пока не добавлен. |

`prepare_command` принимает old authority argument, но сам **не доказывает**,
что он взят из нужного final backup. Будущая trusted factory должна в одном
updater read snapshot получить current StateOwned6, полный graph и verified
final backup `sourceAuthority`. SHA старого полного private header уже есть;
дополнительный RPC с экспортом private authority ID не нужен.

## 4. Завершённые и оставшиеся проверки

После следующего продолжения source теперь **updater19/worker17/private3**.
[Ledger/native driver/worker handler](journal-writer-runtime.md): **workspace776/0
(16 ignored), ledger6/channel8**, Linux/MSVC validation Clippy, fmt/contracts/
scaffold/diff passed. Final summary `writer-v3-final.json`, logs `writer-v3-*-final.log`
в `.winbridge/update-preparation/`. Actor24/view16 и actual native writer proof
пока отсутствуют. Окончательные проверки включают frozen-prefix late-fact fix.
Ниже сохранена история предыдущих component checkpoints.

После checkpoint e3a4a3d реализована [existing-state connection library](activation-journal-opening.md):
**focused8/0, workspace768/0 (16 ignored)**, Linux/MSVC validation all-target
Clippy, fmt/scaffold/diff passed. Last-connection WAL regression сначала
воспроизведена, затем исправлена и прошла. Логи `activation-open-*-checked.log`,
summary `activation-open-checked.json` в `.winbridge/update-preparation/`.
Для focused проверки: `cargo test -p winbridge-worker --lib store::activation_open::tests --locked`.
Это результаты f5dcec1; после него добавлены private writer3/ledger19. Native proof ещё нет.

Ниже результаты **исходного checkpoint e3a4a3d**, до opening component.
Его closing pipeline завершён без ошибок. Сырые логи локальные, в Git не включаются:
`.winbridge/update-preparation/close-writer-20261025-*.log`,
machine summary `close-writer-20261025-checks.json`.

| Проверка | Результат |
| --- | --- |
| Focused SQLite activation/migration | **12 passed / 0 failed** |
| Focused writer model | **7 passed / 0 failed** |
| Полный Rust workspace | **760 passed / 0 failed / 16 ignored** |
| Linux all-target validation Clippy, warnings=errors | Passed |
| MSVC all-target validation Clippy, warnings=errors | Passed |
| Rust fmt | Passed |
| Scaffold / Markdown links | Passed |
| Closing readonly physical Main23 recheck | **Passed**,132 native tests,45 files twice,44+40+12 rejected mutations |

SQLite tests проверяют реальные транзакции и archive/migration,
но process metadata в них synthetic. Они не доказывают физический exit, новый
Windows writer или работающий сервис. Windows-target Clippy — проверка кода,
не выполнение программы на Windows.

Команды закрывающего pipeline, последовательно с `CARGO_INCREMENTAL=0`:

~~~bash
export PATH=/home/user1/.local/bin:$PATH
export CARGO_INCREMENTAL=0
cargo test -p winbridge-worker --lib store::activation::tests --locked
cargo test -p winbridge-update --lib activation::writer --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --features winbridge-worker/update-validation --locked -- -D warnings
cargo clippy --workspace --all-targets --features winbridge-worker/update-validation --target x86_64-pc-windows-msvc --locked -- -D warnings
cargo fmt --all -- --check
npm run check:scaffold
~~~

Предыдущий DTO commit `f8cf372` отдельно прошёл contracts/TS/UI, protocol59/0
и workspace747/0. Текущий блок не меняет wire DTO shapes. Ранний SQL workspace757/0
предшествует последнему backup hardening и не заменяет closing прогон.

**Новых native writer17 и actual old16→17/refusal gates пока нет.** Для migration
нужно запустить настоящий старый16 executable на consistent копии, подтвердить
сохранность task/history/keys/header и отказ старой версии после17. Искусственная
локальная fixture не равна этому доказательству.

## 5. Проверенный physical gate: не повторять исполнение

[Main23 report](windows-activation-state-gate-2026-10-25.md) — terminal
**132/0**,32 groups/35 commands, task
`6bc1623a-c5c4-49ce-8511-cb46b43c7ece`, run `20261025-100933-d8a50807`.
45 ready files проверены дважды,44 replacement +40 state +12 migration
mutations отвергнуты. Observer exit0, реальная updater migration17→18/refusal.

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

Shared ZIPs: `/mnt/hgfs/shared/WinBridge/sources/`. Shared report:
`/mnt/hgfs/shared/WinBridge/workflow-validation/20261025-100933-d8a50807/`.
Local verified pin:
`.winbridge/workflow-validation/20261025-100933-d8a50807/activation-preparation-verified.json`.
Original observer log `.winbridge/update-preparation/state23-native-main.log`.

Безопасное повторное **чтение того же terminal run**, без новой Windows задачи:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T100857Z-7b270c6-9699c8da.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T092432Z-1370d6b-257867fc.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-state --verify-run 20261025-100933-d8a50807
~~~

Reference — successful production updater17 history task
`a4f17a0d-4eea-4af2-8f2c-78bc6235c0af`; не подменять failed audit или новой
историей. Main23 и прежние Main22/112 и lock7 terminal; не resubmit.

**Ограничение времени:** original startup window120000ms не увеличен.
StateOwned observed107679ms, saved109925ms после admission: остаётся около10s.
Writer intent/CAS должны уложиться в оставшееся **исходное** окно. Новый key,
message или wall-clock rollback его не продлевают. Тяжёлые independent audits
нужно выполнять после receipt, а live scope/deadline checks сохранять до эффекта.
Helper wall timings имеют другой origin; не смешивать их с admission-relative.

## 6. Следующий порядок реализации и критерии

1. **Existing-state constructor.** Открывать только существующий exact schema17
   journal под уже удерживаемым тем же child lock; без CREATE, миграции,
   обычного `begin_authority` или повторного acquisition. Retain state/config/
   runtime pins и actual parent/channel. Failure не теряет custody и не выдаёт
   второй owner. **Trusted library реализована после исходного checkpoint:**
   [opening/retained-lock tests и last-connection WAL fix](activation-journal-opening.md).
   Live callback ещё предстоит привязать к настоящему private child/graph;
   constructor сам не является authenticated runtime permission.
2. **Trusted writer admission/ledger — source реализован, schema19.** Взять old stamp из graph-bound backup,
   закрепить весь StateOwned6/config/schema/owner/attempt. Durable one-shot
   intent перед private command; retained immutable result/readback после.
   Updater19/backup/fencing migration добавлены; actual18→19/refusal впереди.
   Значение существующих schema18 rows не меняется.
3. **Private command реализован; actor следующий.** Добавлены contract/marker3,
   capability check до отправки, frozen physical2. Actor successor (следующий24/
   view16) потребляет original Owned guard через тот же blocking slot. Child
   проверяет actual lock, parent, loaded runtime и original monotonic deadline;
   вызывает atomic Store method с повторными live checks. Duplicate/cold —
   observation, не повтор send/CAS. Форма старых views остаётся прежней.
4. **Один combined native writer gate.** После source review/checks/clean commit
   экспортировать **новую receiver17 fixture**, затем distinct producer.
   Target1370d6b имеет worker16 и не подходит для нового same-schema gate.
   Main должен показать exact same child/lock, один epoch increment+receipt,
   lease всё ещё active, stale old writer fenced, lost ACK/dedup/cold read,
   late/foreign/tampered scope refusal, no replay after uncertain outcome.
   Добавить независимый verifier и actual updater18→19/worker16→17/refusal, не только synthetic
   downgrade. Existing physical/updater migration assertions сохранить.
5. **Local service/reconciliation.** Настоящий новый worker должен обслужить
   normal IPC с точными build/process/generation/context/schema и сохранить
   user pause/settings. Roots/dispatch остаются закрыты maintenance до commit.
6. **Controller + lifetime + commit.** Подтвердить настоящую новую controller
   session/build/generation, затем отдельно передать lifetime свежего owned
   activation Job с durable intent/readback. Тестовый URL=None не доказывает
   controller confirmation. Никаких изменений чужих task Jobs, firewall или
   системных политик. Только после trusted receipts commit и scoped lease release.
7. **Закончить U2 и U3/U4.** Fault/crash/recovery/reconciliation, понятные
   CLI/MCP status/cancel/opt-in; desktop form-save/exit-ACK, update/tray/success/
   failure notifications. После isolated gates подготовить bootstrap, если
   старому installed worker действительно нужен новый protocol, и только затем
   actual stable-runtime self-update. После U4 вернуться к архитектурному A4.3.

Это последовательность работ, не уже существующие CLI flags/actions. При
реализации уточнять номера по HEAD; не начинать расширять public install раньше
доказанного happy path и fault recovery.

## 7. Среда, безопасное возобновление и ручные действия

Debian repo `/home/user1/winbridge`. Shared `/mnt/hgfs/shared` соответствует
`K:\VMs\shared`. Windows source/build `K:\_proj-26\winbridge\src`, постоянный
runtime `K:\_proj-26\winbridge\runtime`. Installed runtime/config/journal не
заменялись, Windows validation directories не очищались.

Установленный worker commit `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`, generation
`8f8d1fb9-e5b1-4984-a17e-d067db46268e`. При closing disk query worker healthy,
active0 до запроса, maintenance/recovery errors=null. Disk task
`c088ade6-81c1-4f42-b296-57a9d148a33a` завершён succeeded/0. Receipt:
`.winbridge/update-preparation/close-writer-disk-20261025T141205Z-8fb3f591.json`.
Windows free13823733760 bytes, Debian перед checks25233608704 bytes.
Это измерение, а не гарантия свободного места после новой сессии.

Fresh post-check Info **2026-10-25T14:30:07Z** подтвердил прежние installed
build/generation, controller connected, activeTaskCount=0, maintenanceError=null,
journalRecoveryRequired=null, recoveryCoverage.complete=true. Session
`0d55d6c6-ad2e-4423-b2d8-53326ab7d14a`, controller generation
`cf0d5370-6cd0-463f-b671-974879aa6299`. Выборочная запись:
`.winbridge/update-preparation/close-writer-20261025-post-info.json`.
Debian free после исходных checks25226305536 bytes; cleanup не выполнялся.
Closing Cargo pipeline и readonly observer terminal/exit0; ожидающих проверок
нет. Новых native writer runs не отправляли. Desktop и тестовые UI-окна для
этого блока не запускались: desktop можно закрыть, установленный worker оставить.

Controller PID3922, `ws://192.168.44.203:1143`, IPC
`.winbridge/controller-session/controller-local-development.sock`, log
`.winbridge/update-preparation/controller-resume-20261023.log`. После reboot
PID/адрес и listener перепроверить; второй controller не запускать. Если listener
отсутствует, команда из repo:

~~~bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
~~~

Проверить fresh readonly Info до новых Windows действий. Не печатать полный
конфиг/request/token/private data. Для локального IPC можно импортировать
`call` из `scripts/validate-workflow-native.py`; scope не брать из старого Info.
Если installed worker offline — сначала восстановить связь, не заменять runtime
через произвольную ordinary task. При неожидаемом поведении installed worker или
необходимости его ручного обновления остановить зависящие Windows действия и
сообщить конкретную причину. Обычные isolated build/test fixes и retry после
подтверждённого terminal исхода разрешены без нового вопроса.

Для нового Windows gate нужен **installed worker; desktop не нужен**, UI-окон
для текущих runtime-only сценариев нет. Перед каждым запуском сообщать это
пользователю; после terminal явно разрешить закрыть тестовые окна и оставить
worker. Исходный unknown/live task никогда не replay: сохранить request/ID и
сначала сверить terminal status через `get_task`/тот же observer `--verify-run`.

После opening library новое readonly Info2026-10-25T14:51:03Z подтвердило тот же
installed generation/build/session, connected/active0 и отсутствие maintenance/
recovery errors. Evidence `activation-open-post-info.json`; disk task
`04d9b81a-35be-4ff0-80b2-b4c0c83ac1bf` succeeded/0, Windows13791174656 bytes,
Debian после checks25223417856 bytes. Новых native runs не было, Cargo terminal.

Перед heavy build измерять оба диска и записывать в отчёт, минимум6GiB. Использовать
`CARGO_INCREMENTAL=0`, основной target и **один exclusive cache lease**.
Windows reusable cache `20261017-061527-f4639971/test-build-cache`; перед повторным
использованием проверить saved cache receipt. Не запускать второй Cargo с другим
feature set параллельно и не редактировать Rust во время сборки. Не удалять
бинарник работающего controller, runtime/config/journal, reports и backup files.
Debian unused reproducible caches можно очистить при отсутствии build; Windows
directories без отдельного разрешения не чистить. В closing cleanup не понадобился.

Непросмотренных пользовательских правок в исходном worktree не было; текущие
исходники восстановимы из Git. Внешние private runtime данные и generated logs
в коммит не включаются. Для будущего изменения важного unversioned/dirty user
файла сначала `codex-backup` по AGENTS.md; backup directory не изменять.
