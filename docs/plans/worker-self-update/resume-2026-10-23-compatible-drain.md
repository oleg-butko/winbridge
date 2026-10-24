# Передача контекста: обязательная проверка совместимости перед drain

Дата сессии: **2026-10-23, America/New_York**. UTC в именах прогонов может
относиться к следующему дню. Это актуальная точка продолжения после
`d82871b`; исторические ACTIVE/NEXT в других документах не являются заданиями
к повтору. Пользователь попросил завершить начатые проверки, сохранить
подробное состояние и сделать коммит. Новый Windows-прогон schema17
**не отправлялся**, незавершённых проверочных задач на момент закрытия нет.

## Цель и фактическая готовность

Разрабатываем самообновление установленного Windows worker из проверенных
исходников: подготовка и сборка, безопасная остановка, замена постоянного runtime,
запуск новой версии, проверка её связи и уведомление пользователя. Ручное
обновление нужно только при bootstrap либо конкретной неисправности.
[Основной план U0–U4](README.md); архитектурный переход A4 временно отложен.

| Блок | Что уже подтверждено | Что остаётся |
| --- | --- | --- |
| U0 / внутренний U1 | Source verification/transaction, управляемая сборка, artifacts, health, supervisor/worker coordination, drain и exact shutdown; отдельные native gates. | Не выдавать внутренние команды за публичное самообновление. |
| U2.1 | Consistent final backup после exact exit; positive и fault gates. | Сохранять связь backup с original custody при следующих шагах. |
| U2.2 replacement | Actual Main schema15: перенос всего набора, manifest последним, FilesMoved22 → VerifyingLocal; четыре fault cases. | Activation, local health, controller confirmation, оставшиеся сбои полного cutover. |
| Support discovery / schema16 | Actual queries, frozen policy/context, durable support FSM, actual Main actor18, lost ACK/duplicate, actual15→16 и refusal старого exe. | Это отдельные доказательства, сами по себе не разрешение остановить worker. |
| Production drain / schema17 | Реализованы decision ledger, atomic outbox binding, actor19/view11; локальные тесты и Windows cross-check прошли. | **Actual Main19 + actual16→17/refusal не запускались.** Положительный сквозной production admission тоже ещё не доказан. |
| U3 | Требования desktop/tray/popup и согласованного закрытия определены. | Реализация обновления desktop и автоматические UX/fault gates. |
| U4 | План public API/bootstrap определён. | Installed end-to-end auto-update, CLI/MCP, доставка результата и инструкция recovery. |

Текущий source: **updater journal17 / worker journal16 / supervisor19 / view11**.
Старые views1–10 и wire actions остаются прежними. Public install выключен.
Установленный worker не обновлялся этим кодом; весь updater ещё не готов.

## Что реализовано в этом коммите

[Техническое описание](compatible-drain.md) дополняет эту инструкцию.

- Новый `CompatibleDrainRecord` связывает решение с точным support attempt,
  revision, domain-separated SHA256, owner/epoch и parent revision. Terminal
  Checked с blockers, Unsupported и Indeterminate сохраняются как Blocked;
  незавершённый query не превращается в окончательный запрет.
- Blocked не меняет root и не создаёт drain intent/commands. Admitted должен
  сохраняться в той же транзакции, что root BeginDrain и прежний drain outbox.
  FK, immutable triggers и readback предотвращают частичную запись; ignored
  write откатывает транзакцию.
- Новый actor entry `DrainCompatibleCandidate` / `ObserveCompatibleDrain`
  использует один blocking slot и отвечает после durable admission. Duplicate
  читает прежнее решение; сохранённый JSON и cold reader не создают permit.
- Допуск сохраняется только при текущем owner/executor/schema. Положительный
  bridge удерживает artifact/config/executor pins через admission, проверяет
  source/live lease/health/cleanup/compiler/worker; consuming driver повторяет
  проверки перед RPC. Trusted Rust API не является пользовательским RPC.
- После сохранения production decision прежний v13 не может обойти Blocked
  или переобозначить старый outbox как production. Старые внутренние сценарии
  без выбранного нового route сохранены для совместимости. Будущий публичный
  update обязан пользоваться новым route, а не выбирать legacy при отказе.
- Добавлены schema17 migration и synthetic downgrade fixtures. Обновлены strict
  Rust DTO, JSON Schema и generated TypeScript. Нового пользовательского UI пока нет.
- Подготовлен общий native gate с явным `--compatible-drain` /
  `-CompatibleDrainGate`: Main19 Blocked/duplicate/legacy rejection и отдельная
  actual16→17 миграция consistent копии настоящей истории.
- Счётчик BeginDrain вынесен из `cfg(test)` в `cfg(any(test,
  feature="update-validation"))` вместе с точкой инкремента: native fixture
  действительно считает запросы, а не читает константный ноль.
- Заключительный review исправил cold reader нового native harness: production
  mode должен читать view11 через `compatible_drain_snapshot`, а не frozen
  view10, в котором поля compatibleDrain нет. Старый mode продолжает читать10;
  добавлена проверка выбранной schema. Без исправления тест упал бы на unwrap
  после cleanup, даже если сам production route работал правильно.

Сейчас обе реальные стороны объявляют backup15/preparation16/replacement17,
а activation/controllerConfirmation/desktopUpdate отсутствуют. Поэтому
ожидаются **шесть blockers**: missing_contract + driver_not_implemented для
каждой из трёх возможностей. Blocked в следующем gate — правильный результат.
Не добавлять вымышленные версии контрактов ради положительного теста.

## Карта кода

| Файл / каталог | Для чего нужен |
| --- | --- |
| `apps/update/src/compatible_drain.rs` | Typed decision, digest, validation, physical preflight, passive view. |
| `apps/update/src/journal/compatible_drain.rs` | Schema17, current authority, immutable decision, scoped graph readback. |
| `apps/update/src/journal/drain.rs`, `drain.rs` | Atomic root/outbox/link и binding в памяти permit; сверка перед delivery/receipt/uncertain callbacks. |
| `apps/update/src/preparation.rs`, `supervisor.rs` | Blocking bridge, client19, один work slot, durable ACK, abort coordination. |
| `crates/protocol/src/update/compatible_drain.rs`, `update.rs` | Новые actions/view11; no authority booleans. |
| `apps/update/src/compatible_drain/tests.rs`, `journal/compatible_drain/tests.rs` | Model3 + SQL4, в том числе synthetic migration/fencing/rollback. |
| `crates/protocol/tests/update_compatible_drain.rs` | Строгий контракт19, неизвестные поля/состояния запрещены. |
| `apps/worker/src/update_control.rs` | IPC19 rejection regression и validation-only BeginDrain instrumentation. |
| `apps/worker/src/update_control/validation/candidate_support.rs` | Native Main: support → production request → Blocked → duplicate → legacy rejection. |
| `apps/worker/tests/update_shutdown/candidate_support.rs` | Actual Main fixture, normal IPC, cold history, image identities, cleanup. |
| `apps/update/tests/compatible_drain_migration.rs` | Windows-only actual16→17 на Backup API копии и отказ actual old16 exe. |
| `scripts/test-update-support-actor.ps1` | Один cache lease, сборка, выбор групп, logs/ready publication. |
| `scripts/validate-update-support-actor-native.py` | Submit/observe same task, source/reference pins, independent proofs и negative mutations. |
| `apps/update/src/runtime_replacement/physical.rs`, `driver.rs` | Original resident RuntimeReplacementGuard; отправная точка для activation. |
| `apps/worker/src/runtime_guard.rs` | Startup maintenance refusal до открытия рабочего журнала. |
| `apps/update/src/updater_support.rs`, `crates/core/src/update/updater_compatibility.rs` | Объявление реальных driver contracts и frozen policy. |

## Завершённые локальные проверки

Логи находятся в ignored `.winbridge/update-preparation/`; они не входят в Git.

| Проверка | Результат / лог |
| --- | --- |
| `CARGO_INCREMENTAL=0 cargo test --workspace --locked` | **688 passed, 0 failed, 16 ignored**; `compatible-drain-workspace-tests.log`. |
| Новые model/SQL tests | **7/0**; `compatible-drain-model-ledger-tests.log`. Они проверяют модель и SQLite, не Windows effects. |
| Реальный локальный IPC19 без support | **1/0**; `compatible-drain-worker-tests.log`: rejection, root/outbox неизменны, frozen view10. |
| Linux all-target Clippy | Passed; `compatible-drain-linux-clippy.log`. Packages update/worker/protocol, feature update-validation, `-D warnings`. |
| Windows MSVC all-target Clippy | Passed; `compatible-drain-final-windows-clippy.log`; после исправления cold reader повторно passed в `compatible-drain-close-windows-clippy.log`. Те же packages/features, включая новый native harness. Это не выполнение Windows программы. |
| Protocol с `--features jsonschema` | **51/0**; `compatible-drain-final-jsonschema.log`. |
| Generated Rust/TS contracts | Generation и `npm run check:contracts` passed; `compatible-drain-final-contract-check.log`. |
| TypeScript / UI | `npm run typecheck`, `npm run build:ui` passed. Прежнее Vite warning о chunk >500kB остаётся, ошибок сборки нет. |
| Python observers | Actor3 + support7 passed; py_compile passed. |
| Formatting / docs | `cargo fmt --all -- --check`, `git diff --check`, `npm run check:scaffold` passed;258 Markdown files/links. |

Старый actor16 observer после расширения повторно проверен с `--verify-run`
на прежней successful задаче: **23 native tests, 21 ready files дважды,
28 negative mutations**. Новой задачи это не создаёт. Лог:
`compatible-drain-close-reference-recheck.log`.
Закрывающая машинная сводка без private config/tokens:
`.winbridge/update-preparation/session-close-20261023-compatible-drain.json`.

## Последние настоящие Windows gates — не запускать повторно

| Gate | Task / run | Подтверждено |
| --- | --- | --- |
| [Support actor18](windows-candidate-support-actor-gate-2026-10-23.md) | `adf48103-c02b-40e6-9442-6d089882c48c` / `20261023-233438-99674bc1` | succeeded/0;23 tests/6 groups/8 commands;21 files twice/28 mutations; support Checked5/history6, normal IPC, lost ACK/duplicate, owned abort. Drain не запрашивался. |
| [Support16 metadata/query/migration](windows-candidate-support-gate-2026-10-23.md) | `f88c0185-6665-424c-8e07-92d0867612ad` / `20261023-224925-0eae733c` | succeeded/0;25 tests/7 groups;13 files twice;36 query +22 migration mutations; actual15→16/refusal. |
| [Main15 replacement](windows-runtime-replacement-main-gate-2026-10-23.md) | `665e7e2f-e734-49cf-ae87-d83498a29cbd` / `20261023-163606-15675844` | succeeded/0;60 tests/12 groups;20 files twice/44 mutations; FilesMoved22, VerifyingLocal; activation нет. |
| [Replacement faults15](windows-runtime-replacement-faults-gate-2026-10-23.md) | `63232320-8bf6-4826-b13c-315ead0b4433` / `20261023-173625-e4eea0a6` | succeeded/0;60 tests/12 groups;32 files twice/68 mutations; occupied directory, foreign handle, ignored receipt, owned crash до receipt. |

Первый actor18 run `20261023-231817-d6b1374c` /
task `fe719061-08e8-4f5a-a0a7-6c6b225dd3a5` был terminal failed/1:
тест отправил обычный Info в control pipe (`binding.endpoint`) вместо normal
worker endpoint из instance context. Исправлено в `db0105a`, guards не ослаблялись;
19 failed ready files сохранены. Успех из таблицы — отдельная clean попытка.
Не переигрывать failed namespace. В observer затем исправлена shadowed variable
`expected_root` (`d82871b`); выполнен readonly recheck успешной задачи.

### Exact pins для следующей миграции

Источник настоящей schema16 истории — successful actor run из первой строки.
Verified pin:
`.winbridge/workflow-validation/20261023-233438-99674bc1/support-actor-verified.json`.

- Producer `db0105a135256f93d30e273c449fdd937bdc776d`.
- ZIP `winbridge-20261023T233334Z-db0105a-8e9bd9e8.zip`.
  SHA256 `cfaa944add1d1562998855cb84b61c7d5b6dcfd9f4d54a23674d7c0b4bff0aa3`.
- Source manifest SHA256 `2d8eeedc781aa5a6ef23705c1e3fb2b76b850f79c4c949e9e91fed25d13da59b`.
- Target `7186bb652a6148b2866dc966a055f041de2d67a9`;
  ZIP `winbridge-20261023T224917Z-7186bb6-59eeb744.zip`,
  SHA256 `94ed85726985c32ee6678e9bc1d1c013c4f9813df2be6fd7206a7642cf695a49`.
  Target manifest `e4740f078caf2743768e78c90246df3b5bdb308fd69195821ae03ae000ff3484`.
- Report SHA256 `43aa6217c5bdd86f596389f53e34d4bd1918f4556b05976dfafb8bf8f1ab5ba8`.
- Proof SHA256 `3158cc0b9213ca20661e679a508dad380aabe813efcd6391c28f85eb02b09682`.
- Update ID `a738cc69-29b7-4fbc-a042-e2620df3d671`, support attempt
  `c84ee635-98d6-480a-86e5-d8cb1d24eb45`, revision5/history6, owner epoch2.

Shared proof/report:
`/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-233438-99674bc1/`.
Original local runtime/DB16/image paths берутся из exact proof, а не выбираются
по имени последней папки. Сохранять original DB/WAL/images и local validation
directories: новый тест делает **consistent копию**, не мигрирует оригинал.

Readonly команда для старого gate, без нового submit:

~~~bash
python3 scripts/validate-update-support-actor-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261023T233334Z-db0105a-8e9bd9e8.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261023T224917Z-7186bb6-59eeb744.zip \
  --cache-run 20261017-061527-f4639971 \
  --verify-run 20261023-233438-99674bc1
~~~

## Окружение на момент закрытия

- Debian repository `/home/user1/winbridge`, shared `/mnt/hgfs/shared`
  соответствует `K:\VMs\shared`. Source ZIPs: `WinBridge/sources`.
- Controller PID3922, `ws://192.168.44.203:1143`, конфиг
  `.winbridge/controller-snapshot.local.json`, IPC
  `.winbridge/controller-session/controller-local-development.sock`.
  Лог `.winbridge/update-preparation/controller-resume-20261023.log`.
- Installed worker **d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2**,
  build1792486531570951100, manifest
  `835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241`.
  Generation `8f8d1fb9-e5b1-4984-a17e-d067db46268e`.
  Повторный Info: connected, activeTaskCount0, maintenance/recovery errors=null,
  recoveryCoverage.complete=true. Ручное обновление пока не требуется.
- Постоянные Windows пути: `K:\_proj-26\winbridge\src` и
  `K:\_proj-26\winbridge\runtime`. Их не изменяли новым gate.
  Изолированный проект: `K:\_proj-26\winbridge\workspaces\snapshot-fixture`.
- Retained native cache:
  `...\snapshot-fixture\workflow-validation\20261017-061527-f4639971\test-build-cache`.
  PS runner удерживает один exclusive cache lease.
- Последние измерения: Debian свободно около25.17GB; Windows после завершённого
  actor16 gate **30,656,147,456 bytes**. Windows значение историческое, перед
  новой сборкой измерить заново. Ни Windows, ни Debian каталоги не очищались.
- Desktop не нужен; тестовых UI-окон не было. Desktop можно закрыть,
  установленный worker оставить работать. Новых background native/Cargo runs нет.

PID, IP, generation, disk и подключение **перепроверять после перезапуска VM**.
Не запускать второй controller, если первый уже слушает. Если его нет:

~~~bash
./target/debug/winbridge-controller \
  --config .winbridge/controller-snapshot.local.json serve
~~~

Не менять локальный config вслепую. Если после перезагрузки отсутствует HGFS или
worker offline, сначала read-only диагностика, а не правки firewall/mount/services.
Не печатать целиком private gate input, tokens, submission/EncodedCommand или config.

## Как продолжить: конкретная последовательность

1. Прочитать AGENTS, README, верх handoff/roadmap, этот документ и
   `compatible-drain.md`; выполнить `git status --short`. Начать с проверки
   связи и диска, затем review нового17 пути и harness. Локальные проверки
   завершены; повторять их при новых изменениях/сомнениях, не ради цикла.
2. Нужны **два distinct clean schema17 source exports с built UI**: producer
   и ограниченная candidate fixture. Ни старый target7186/schema16, ни грязный
   ZIP не подходят: health и policy требуют matching schema17.
   Можно экспортировать этот checkpoint как fixture, затем после полезного
   review/fix/docs коммита — producer. Это тестовая fixture, не готовая версия
   для установки пользователю. Команда экспорта из чистого дерева:

   ~~~bash
   python3 scripts/export-source.py \
     --output-directory /mnt/hgfs/shared/WinBridge/sources --include-built-ui
   ~~~

   Сохранить оба exact ZIP/manifest/commit pins. Перед export убедиться, что
   `web-dist` соответствует source; при необходимости `npm run build:ui`.
3. Сообщить пользователю: нужен **только установленный worker**, desktop
   необязателен, тестовых окон не будет. Измерить свободное место на обоих
   хостах: менее6GiB — новые сборки остановить. Затем один новый isolated run:

   ~~~bash
   python3 scripts/validate-update-support-actor-native.py \
     --archive /mnt/hgfs/shared/WinBridge/sources/NEW-PRODUCER17.zip \
     --target-archive /mnt/hgfs/shared/WinBridge/sources/NEW-FIXTURE17.zip \
     --cache-run 20261017-061527-f4639971 \
     --compatible-drain \
     --reference-pin .winbridge/workflow-validation/20261023-233438-99674bc1/support-actor-verified.json
   ~~~

   NEW-* — placeholders, заменить реальными exported именами. **Эта команда
   ещё не выполнялась.** Не запускать PS вручную с неизвестными SourceDirectory:
   Python сам доставит/распакует source и подготовит exact arguments.
4. Сразу сохранить run/Task ID. Ожидаются **33 tests /10 groups /12 commands**:
   прежние22 prerequisites + model/SQL7 + contract1 + mailbox1 + actual
   migration1 + Main1. Проверить terminal succeeded/0 и не только Cargo summary.
   Main должен сохранить Blocked с прежним root, duplicate той же decision,
   legacyFallbackRejected=true, beginDrainCount0, source lease SourceHeld/rev1,
   shutdown0, normal IPC и unchanged images. После explicit fixture abort —
   cold history без permit. `productionDrainEnforced=true`, но runtimeApplied=false.
5. Отдельно проверить migration proof: Backup API copy16→17, epoch+1, migration
   backup16, все прежние support/history rows неизменны, gateRows0, fresh
   admission из старого context запрещён. Actual old16 init/status должны
   отказать17 без DB/nonempty WAL изменений. Original reference bytes сохраняются.
   Observer проверяет ready files дважды и должен отвергнуть **32 Main +19
   migration mutations**; не подменять proof вручную.
6. При observation timeout читать **тот же** accepted Task ID/run. Для recheck
   добавить `--verify-run RUN` к той же команде со всеми mode/reference flags.
   Metadata/request сохраняются до submit; lost ACK восстанавливается по exact
   key/payload, а не новым submit. При terminal failure сохранить artifacts;
   после обычного исправления harness использовать новый clean commit/export
   и новый namespace. Не переигрывать потенциально исполненный OS effect.
7. После gate записать exact proofs/hashes/границы в новый отчёт, обновить
   handoff/roadmap и commit. Только затем продолжить activation из **original
   resident RuntimeReplacementGuard**: новый intent/receipt/одноразовый permit,
   PID-before-resume, state/instance/file handoff, ранний maintenance guard,
   local health и controller confirmation. Saved FilesMoved не восстанавливает
   custody после crash. Дальше remaining U2 faults → desktop U3 → public/bootstrap
   U4. Для локальных внутренних gates можно продолжать изолированные fixture
   сценарии; production policy не ослаблять из-за ещё отсутствующих U3 contracts.

## Ограничения, которые нельзя потерять

Один внешний эффект на durable intent; unknown outcome → Indeterminate, без
автоповтора. JSON/report/receipt не заменяют OS handles и fresh authority.
Raw lifecycle не может выставить FilesVerified/LocalHealthVerified/
ControllerConfirmed. Старые контракты заморожены, новые вводятся отдельными
версиями. Working runtime не трогать до isolated gates/bootstrap.

`CARGO_INCREMENTAL=0`, переиспользовать основной target; не редактировать Rust
во время Cargo. При отсутствии cargo добавлять `/home/user1/.local/bin` в PATH
только команды. Не удалять controller binary, journal/config/reports/Windows
validation dirs. Обычные harness fixes/retry пользователь уже разрешил выбирать
самостоятельно. При проблеме **установленного** worker либо необходимости его
обновления остановить зависящие Windows действия и сообщить точную причину.
Services/firewall/mount/global settings без отдельного разрешения не менять.
