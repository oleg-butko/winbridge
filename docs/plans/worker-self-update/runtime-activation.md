# Activation после проверенного replacement

2026-10-24. **Execution driver ещё не реализован; read binding21 gate failed.**
Текущий source updater18/worker16/supervisor21/view13. Preparation20/actual17→18
[прошли87/0](windows-activation-preparation-gate-2026-10-24.md), one-file read
adapter прошёл4/0. Whole-set factory/actor21 добавлены, однако
[новый Main gate](windows-activation-files-gate-2026-10-24.md) terminal failed/exit1:
94 preliminary passed/1 Main failed, во время read-pin handoff истёк preparation
deadline120000ms. Positive Bound/cold proof отсутствует; миграция отдельно принята.
Сначала timings/исправление этого перехода и positive95-test gate в новом
namespace, затем launcher/private receiver/state handoff. Execution capability
и public install off. Все начатые проверки завершены, installed runtime unchanged.
[Актуальный checkpoint и проверки](resume-2026-10-24-activation-files.md),
[общий U0–U4 план](README.md),
[original custody](runtime-replacement-custody.md),
[resident replacement actor](runtime-replacement-actor.md).

## Результат, который нужно получить

После FilesMoved22/VerifyingLocal запустить **новый установленный** worker из
того же постоянного runtime. Новый worker должен проверить loaded set/config,
получить прежний stateDirectory, пройти миграцию/reconciliation и соединиться
с controller, сохраняя запрет новых задач до подтверждённого commit. Supervisor
после успеха может завершиться: работающий новый worker должен сохраниться.
Это отличается от прежнего rehearsal на новом пустом journal.

Не выдавать наличие FSM или нового DTO за реализованный activation contract.
`UpdaterDriverContracts::implemented_windows()` пока сохраняет activation=None.

## Первый элемент модели реализован отдельно

`apps/update/src/activation.rs` содержит strict `ActivationSource`: плоскую
привязку к полному FilesMoved record через domain-separated canonical SHA256.
Она сохраняет installation/set, owner/epoch/launch, ожидаемый parent revision,
config/shutdown/backup digests и **installed** worker/manifest evidence.
Candidate или displaced paths не подставляются. Нет копии всей цепочки
preparation/history, live observation, permits, файловых или процессных эффектов.

`validate` сравнивает сохранённые данные с exact retained replacement. Это
проверка исторической связи; она не доказывает текущий VerifyingLocal root,
владение original guard или готовность capabilities. Всё это обязан проверить
будущий admission в собственной транзакции и physical bridge.

Пять новых model regressions прошли вместе с одним прежним projection test:
неполный/unknown prefix, изменённые owner/generation/hash/image, digest всего
наборa включая retained fixture, exhausted revision и неизвестные authority
fields. Linux/MSVC all-target Clippy update/worker passed; логи
`.winbridge/update-preparation/activation-source-*.log`.
Эта модель добавлена **после** export628c0f6 и не входит в текущий native17 run.
На момент Source model commit updater schema оставалась17; ledger18 добавлен
позже и описан ниже. Actual activation proof отсутствует.

## Intent и FSM запуска: модель реализована

`apps/update/src/activation/lifecycle.rs` добавляет frozen `ActivationContext`,
`ActivationIntent`, typed receipts и `ActivationRecord`. Все проверки/переходы
получают retained RuntimeReplacementRecord и сверяют плоский source binding.
Context сохраняет предыдущий worker, supervisor, logical journal scope,
config/file identities, ожидаемые schemas/protocol, timeout и **только SHA256**
будущего channel nonce. Actual nonce не сериализуется в эти данные.

| State | Подтверждённые данные |
| --- | --- |
| Prepared | Намерение подготовлено, OS launch permission отсутствует. |
| Creating | Intent создания; исход OS call ещё не подтверждён. |
| Created | Сохранены actual process stamp и собственный scoped Job. |
| ResumeGranted | Разрешение записано до resume; это ещё не Ready. |
| ReadyForState | Тот же PID сообщил exact loaded set/manifest/image/config. |
| HandoffIssued | Intent передачи instance/state; новый owner ещё не подтверждён. |
| StateOwned | Подтверждены тот же PID, прежние journal/lock IDs и новый generation. |
| LocalVerified | Совпали frozen schemas/protocol, loaded identity и normal local compatibility. Это не whole update commit. |
| CreationFailed | Подтверждённый no-process outcome; сохранять его может только будущий trusted creation observer. |
| Indeterminate | Последний подтверждённый prefix сохранён, новых внешних действий нет. |

Revision идёт по одному причинному шагу. Duplicate create/resume/handoff,
перезапись nonce/context/receipts, чужой process/Job и nested uncertainty
отклоняются. Схемы сравниваются с frozen context, а не с будущими глобальными
константами executable.

Deadline запрещает **новые внешние grants**, но поздний PID/Ready/fact не
теряется: его можно сохранить и остановиться без resume/handoff. Тайм-аут
не превращается в «процесс не создан». При будущей реализации ledger/root
transition поздний local fact также не должен автоматически объявлять весь
update успешным: это отдельное решение live driver с его monotonic deadline.

Только metadata: нет launch/state-open/commit callbacks, RPC, fresh permit или
новой capability. Будущий schema18 reader обязан загрузить связанные shutdown,
health и supervisor rows и сверить **весь Context**, включая config/state/schemas,
с этими authoritative records; одного `record.validate(replacement)` недостаточно
для fresh admission. JSON нельзя использовать как источник текущих полномочий.

Проверки:10 новых lifecycle regressions (всего16 в activation filter, включая
5 Source tests и прежний projection), workspace703/0 (16 ignored), Linux/MSVC
all-target Clippy update/worker passed. Логи `activation-lifecycle-*.log` в
`.winbridge/update-preparation/`. Actual native activation ещё не выполнялась.
Следом ledger/schema18 и original-custody admission, затем physical launcher,
private worker receiver и handoff. Controller/lifetime/global commit остаются
отдельными следующими блоками; support activation пока=None.

## Activation ledger18: graph-bound preparation

`apps/update/src/journal/activation.rs` добавляет intents/current/history с FK
к runtime replacement, immutable identity/terminal triggers, causal CAS и
readback. Нет production API, принимающего готовый record/receipt для записи;
raw append доступен только synthetic SQL tests до реализации guarded driver.

Reader в одной SQLite transaction проверяет полный retained replacement,
историю activation и настоящий FilesVerified edge: owner/epoch, Replacing →
VerifyingLocal, соседние revisions, request и timestamp. Context заново строится
из shutdown, verified backup, health/config/rehearsal и supervisor. Их digests
совпадают с preparation proofs. Старые данные проверяются по frozen schemas,
а не переоцениваются как свежий допуск для текущей версии.

Windows-only `prepare_activation` требует original resident guard, текущий
Active supervisor/owner/epoch, actual self PID и точный VerifyingLocal root.
Guard/files/anchors проверяются до и после записи. Сохраняется только Prepared,
root не меняется, process/job/state не создаются. Дубли возвращают Existing
без нового transient nonce. Fresh `ActivationPreparation` не Clone/Serde;
nonce доступен trusted caller только в памяти, в journal находится его SHA256.
Это подготовка аутентификации, **не create/resume/state-transfer permit**.

Schema17→18 использует существующий consistent migration backup/epoch fencing.
Synthetic downgrade fixtures сначала удаляют три новых dependent tables.
SQL6/model22 прошли: ignored admission/history writes, rollback, stale CAS,
immutable history, cold no-repeat и synthetic17→18. Workspace709/0 (16 ignored),
final Linux/MSVC all-target Clippy update/worker passed. Логи `activation-ledger-*.log`.
Actual17→18 migration/refusal и actual original-custody preparation **не запускались**.

## Supervisor20/view12: подготовка подключена

PrepareActivation/ObserveActivationPreparation принимаются только через exact
Active bound supervisor/token/revision. Caller не передаёт record/nonce/paths;
worker command в запросе запрещён. View12 читает graph в согласованном snapshot,
live custody задаётся только resident signal. Frozen views1–11 не меняются.

Единственный blocking slot перемещает тот же Box original replacement guard в
`prepare_activation` и возвращает его actor при успехе и обычной ошибке. Fresh
preparation/nonce остаются в памяти supervisor; durable Prepared до ACK, lost ACK
не освобождает guard и не запускает новую подготовку. Duplicate возвращает
существующие данные. Ни actor, ни DTO пока не умеют create/resume/handoff;
activation support по-прежнему=None. Actual positive preparation branch проверен
[Main18 gate87/0](windows-activation-preparation-gate-2026-10-24.md).

### Следующая вертикаль

1. Экспортировать clean source18 с built UI после commit
   как ограниченную fixture. Не использовать17 ZIP как candidate18.
   Выполнено для7f6a029: `winbridge-20261024T103055Z-7f6a029-3e94038a.zip`,
   pin `.winbridge/update-preparation/activation-fixture18-export.json`.
2. Supervisor20/view12 реализован, local/cross результаты в checkpoint выше.
   Перед native gate проверить вместе actor/ledger: durable Prepared до ACK,
   тот же Box original guard, resident nonce, duplicate и cold no-adoption.
3. Дополнить actual replacement Main fixture отдельным preparation-only mode:
   explicit internal legacy route до FilesMoved → capture20 → duplicate → cold
   read. Проверить root/files/state неизменны после capture, launch0. Ordinary
   startup остаётся blocked. Не включать public install или advertised activation.
4. В тот же native gate включить actual17→18/refusal на новой consistent копии
   retained successful17 history из run20261024-085627-3a106020 (verified pin в
   workflow-validation того же run). Original DB/WAL/images не менять. Observer
   должен сохранить отдельный explicit18 mode, сохранив readonly17 recheck.
5. После native review — consuming file/state handoff, launcher и private worker
   receiver, guarded callbacks и actual startup. Ни Prepared, ни cold view не
   создают права продолжать после потери original owner. Все последующие пункты
   общего плана ниже остаются незавершёнными.

### Combined native18 harness и пройденный gate

`apps/worker/tests/update_shutdown/activation_preparation.rs` продолжает настоящий
Main replacement scenario: original supervisor получает20 после FilesMoved;
Prepared читается из journal до чтения ACK, ответ намеренно не потребляется.
Observe/duplicate проверяют ту же запись, original lock, неизменность root/files/
worker DB/WAL и launch/handoff count0. После закрытия owned test Job холодный
читатель получает ту же историю без physical custody; ordinary startup отказан
существующей replacement fixture. Production19 policy не отключается: этот
внутренний fixture route не создаёт production decision и не объявляет capability.

`apps/update/tests/activation_migration.rs` проверяет настоящую историю successful17
run20261024-085627-3a106020 на новой SQLite Backup API копии:17→18, backup17,
owner fencing, все прежние rows/support/Blocked decision и пустые activation
tables. Старый actual17 exe должен отказать init/status18 без DB/WAL writes;
original DB/WAL/images не мигрируются и не заменяются.

Runner `scripts/test-update-health-actor.ps1 -ActivationPreparationGate` объединяет
эти проверки с прежними replacement/backup regressions; ожидается не менее87
native tests/16 групп. Observer
`scripts/validate-update-activation-preparation-native.py` требует две distinct
clean schema18 ZIP и verified17 reference pin. Он сохраняет request/meta до
submit, поддерживает `--verify-run`, проверяет ready files дважды, exact commands
и actual proof, затем отрицательные мутации. Независимый
`scripts/update-activation-preparation-proof.py` сверяет source/context/health
digest/FilesVerified edge, совпадающие copies и отсутствие новых прав у cold view.

Четыре Python evidence tests, прежние9 runner tests, Linux/MSVC all-target Clippy
прошли; focused Rust filter28/0 также passed. Команда
реального gate после commit/export (SOURCE18 — новый producer, не fixture7f6a029):

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/SOURCE18.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T103055Z-7f6a029-3e94038a.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json
~~~

При observation timeout добавить `--verify-run SAVED_RUN` и наблюдать прежний
Task ID. Без `--verify-run` создаётся новый namespace/task. Gate87/0 теперь пройден; exact producer/read-only команда в отчёте выше.
Installed worker only, desktop не нужен,
тестовые UI-окна не появляются; установленный runtime не меняется.

## Начата physical bridge: однофайловый read handoff

`crates/platform/src/runtime_files/read_handoff.rs` добавляет consuming
`OwnedRuntimeFile::into_read_pin_checked` и non-Serde `RuntimeReadPin`. Переход
сначала удерживает read handle с разрешением DELETE-sharing и запретом WRITE,
проверяет исходные bytes/hash/File ID, освобождает только move handle и открывает
строгий no-write/no-delete pin. После этого повторно сверяются path/File ID/hash
обоих handles. Подмена файла с идентичными bytes не принимается. Checkpoint
проверяется до acquisition, до release, после release и под final pin.

Ошибка до release удерживает original move owner; после release — тот же
readonly file object с запретом WRITE. Последний может разрешать rename/delete
и намеренно **не возвращается как успешный pin или повторный move owner**.
Caller обязан сохранять failure вместе с остальными directory/state/instance
guards и запрещать launch. Ошибка хранит owner в Box; первый Clippy выявил
128-byte Err, это исправлено без отключения lint.

Linux/MSVC all-target Clippy platform/update/worker passed. Четыре настоящих
Windows теста пройдены (success/before-release failure/after-release failure/
same-bytes foreign File ID). Для короткой проверки добавлен
режим `validate-update-shutdown-primitives-native.py --runtime-read-handoff-only`;
он выполняет только эти primitives в новом namespace и не повторяет полный
87-test Main/candidate build. Schema/action/capability не меняются.

Native task `ade260bf-de0a-436b-9920-589eb26ee86c`, run
`20261024-143036-03cf8cbb`, producer `d4c27a3abe99c664cfdd0dc86d1d3a3799bbfaab`
terminal succeeded/exit0:4 tests,5 ready files проверены дважды. ZIP
`winbridge-20261024T143017Z-d4c27a3-dd063fec.zip`, SHA256
`15f47e64e13e597f814002a32df031ae0716ebced34545f271d11db347a25fe0`, manifest
`6bdab813da54a93cca1681dfcc327136d7ae3b97a5476afca006a4022151a771`.
Report SHA256 `9331130d7598641c01a465be24ef20e0e75196903a45d754d0cd36c601a94163`,
test log SHA256 `2a5f1f663f0be3379f100697fcd9aa21888448326c60277aba1c300ff47b4746`.
Verified pin `.winbridge/update-preparation/activation-read-handoff-native-verified.json`,
observer log `activation-read-handoff-native.log` в том же каталоге.
Windows free27,557,306,368→27,689,910,272 bytes, Debian после audit16,529,096,704.
Cleanup не выполнялся, installed d052/generation прежние/active0/errors=null.
UI-окон не было, desktop можно закрыть, installed worker оставить. Active runs нет.

На момент primitive gate это была однофайловая операция. Следом реализованы
whole-set bridge под original guard, journal/current-owner binding и actor21,
описанные ниже; их positive Main gate не пройден. Только после полного exact
readback и положительного gate можно приступать к отдельному create admission.

## Whole-set library/actor21: Main обнаружил истечение deadline

`runtime_replacement/activation_files.rs` потребляет свежий non-Serde
ActivationPreparation и **тот же Box original guard**. `require_activation_prepared`
сверяет nonce digest, Prepared0, root VerifyingLocal/source parent revision,
owner/epoch, actual supervisor self PID и полный сохранённый graph. Окно подготовки
дополнительно ограничено live monotonic deadline; JSON и коррекция wall clock не
обновляют его. Повтор старой metadata не создаёт новое окно.

Исходные exited-process/state/instance guards, displaced move handles и anchors
остаются внутри результата. Пять installed move handles переводятся в строгие
read pins; после полного readback создаётся PinnedInstalledSet и сверяются все
File IDs, manifest/set/runtime identity. Любая ошибка удерживает pending/strict/
residual handles в закрытом ActivationFilesFailure; из ошибки нельзя получить
успешный guard. Это не create permit. `require_current` относится только к
Prepared admission; будущая creation factory должна потреблять guard и перейти
к отдельному typed process owner, не ослабляя проверку Prepared ради следующих фаз.

Actor component `supervisor/activation_files.rs` использует одну typed машину
Idle → Binding → Bound/Failed, единственный общий blocking slot и Scope check.
Internal21 BindActivationFiles/ObserveActivationFiles возвращают view13.
Первый ACK обозначает **volatile resource binding**, а не разрешение внешнего
эффекта; исходный Prepared уже durable. Duplicate наблюдает тот же процесс.
Cold snapshot показывает NotBound/heldFiles0/originalStateHeld=false и не
реконструирует handles. Views1–12 не получают нового поля.

Native Main harness расширен флагом `-ActivationFilesGate`, observer —
`--activation-files`. Mode сохраняется в metadata: при readonly recheck его
нельзя добавлять к старому preparation-only run или удалять из нового files run.
Старый87/0
`--verify-run` по-прежнему проходит из текущего checkout. В новом mode ожидаются
не менее95 native tests: прежний Main/actual17→18 gate, monotonic timer test,
4 read-handoff primitives, strict21 DTO и actual IPC21 rejection. Сам Main после20
вызывает21 до дополнительных тяжёлых file observations и проверяет strict read
refusal BEFORE, successful whole-set read AFTER, write/delete denial, сохранённый
instance lock/root/history/DB/WAL, duplicate и cold no-adoption. Новый independent
`update-activation-files-proof.py` дополняет source/context/migration audits.

Локальные проверки нового source: workspace716/0 (16 ignored), final Linux/MSVC
all-target Clippy, protocol-jsonschema55/0, generated contracts/TypeScript/UI build,
fmt/diff/scaffold263 passed. Python5+9 tests passed. Логи `activation-files-*` в
`.winbridge/update-preparation/`. Из clean producer74e8839 выполнен native95
mode; архив fixture18 остаётся7f6a029, источник migration — successful17 pin.
Task01d7bf76-6c53-427c-8cd9-81d8f8798962/run20261024-153856-fe2e4f76 terminal
failed/exit1:94 preliminary passed,1 Main failed по live deadline в read-pin
handoff.27 ready files twice, actual17→18 audit12 passed; полного Main proof нет.
[Диагностика и pins](windows-activation-files-gate-2026-10-24.md).

Первая следующая работа — измерить Prepared→Bind: тяжёлые observations и
duplicates в harness, graph/custody/hash preflights driver и per-file checkpoints.
Номер файла/источник расхода бюджета пока неизвестны. Перестановку assertions
или оптимизацию повторного чтения подтвердить измерениями, сохраняя authority/
nonce/FileID проверки. Старый deadline не продлевать; изменение политики сроков
требует явного контракта. После исправления — новый clean producer/namespace и
positive gate; failed task не replay. Execution до этого не начинать.

Подключены только read handles. Private worker receiver, suspended launcher,
state handoff, controller/lifetime и actual update остаются незавершёнными;
activation capability=None, public install off. Positive whole-set branch пока
не доказан: Native87 относится к producerb12ecc6 (preparation20), Native4 —
кd4c27a3 (one-file adapter); новый Main21 failed. Проверки source — в handoff.

## Ограничения фактического кода

- `RuntimeReplacementGuard` удерживает original exited process/state/instance
  custody, anchors и OwnedRuntimeFile handles. Новый cold reader их не получает.
  `FilesMoved` — исторический факт, а не permission на повторный запуск.

- `LockedShutdownState` удерживает exclusive worker.lock. Обычный Worker::open
  не сможет получить тот же lock до явной передачи. `Worker::open` также дважды
  проверяет maintenance marker перед Store::open; эту защиту сохраняем.
- `OwnedRuntimeFile` запрашивает DELETE и запрещает чужих writers/deleters;
  обычные runtime pins открываются с другим sharing mode. Перед стартом нужно
  превратить move custody в обычные read pins, сравнив exact file IDs/hash/set.
  Простое `drop(guard)` слишком рано теряет остальные права и наблюдения.
- `InfrastructureChild::spawn` сегодня не suspended и не является готовым
  activation launcher. `ManagedChild` обеспечивает suspended→Job→resume,
  но его обычная lifetime/cleanup семантика не подходит постоянному worker.
  Нельзя изменить поведение всех task/probe children ради нового сценария.

По [документации Microsoft о Job limits](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_limit_information)
KILL_ON_JOB_CLOSE завершает связанные процессы при закрытии последнего Job
handle. Значит, «parent завершился успешно» и «child продолжает работать»
требуют отдельного проверяемого lifetime handoff. Это вывод для нашего launcher,
а не свойство существующего `InfrastructureChild::detached()`.

## Последовательность реализации

1. **Модель и binding.** Отдельный typed activation intent/record/result,
   distinct attempt ID, root VerifyingLocal revision, owner/epoch/launch,
   replacement attempt/revision/hash, exact target manifest/set/image/config,
   old worker state identity и ожидаемые journal/protocol schemas. Ссылки/digests
   связывать с immutable rows; не вкладывать ещё одну полную копию всей цепочки
   source→build→health→replacement в каждый новый message.
2. **Ledger и fresh admission.** Updater schema18, backup/fencing migration17→18,
   current/history/outbox с CAS/readback. Admission только из живого original
   guard после проверенного FilesMoved, не из JSON. Один non-Clone/non-Serde
   permit. Новые wire versions/views вводятся отдельно; старые формы frozen.
3. **Файловая передача.** Под guard проверить все пять текущих файлов и anchors,
   подготовить совместимые промежуточные read handles, затем убрать DELETE
   access move handles и получить final installed-set read pins. Любое окно
   открытия закрывается повторной exact identity/hash проверкой до launch.
   Ошибка удерживает сохранившиеся ресурсы/barrier и запрещает продолжение;
   произвольный чужой файл по прежнему пути не принимается.
4. **Один suspended launch.** Отдельный activation launcher вне старого worker
   execution domain, fresh owned Job и ограниченный capture. Durable Creating
   до OS call; actual PID/birth/session/host + Job receipt до resume grant.
   Failed creation и unknown creation — разные состояния, неизвестный исход
   не запускает второй процесс. Не передавать nonce в command line/stdout.
5. **Ready до state writer.** Новый private activation entry worker проверяет
   собственный loaded runtime, точный config и actual supervisor process.
   Из отдельного аутентифицированного канала он подтверждает готовность;
   обычный `serve` по-прежнему получает maintenance refusal. До передачи
   instance lock не открывать journal writer и не подключать scheduler.
6. **State handoff.** Durable HandoffIssued → проверенный release только своего
   instance lock → acquisition того же file ID новым процессом. Старые state
   directory/journal/runtime anchors и original exited process сохраняются
   столько, сколько допускают sharing rules. Новый in-process startup permit
   создаётся только этим handshake, не десериализуется и не доступен обычному
   CLI флагу «ignore guard». На worker стороне сохранить отдельную durable
   квитанцию нового владельца; при добавлении таблицы — worker schema17 и
   actual16→17/refusal gate, не side channel в случайной metadata.
7. **Local verification.** Exact new build/process/instance/incarnation/schema,
   ownership/recovery/resource reconciliation, normal IPC, сохранённые user
   pause и настройки. Новые roots всё ещё блокируются maintenance. Только
   trusted receipt переводит root в AwaitingController; raw lifecycle запрещён.
8. **Controller confirmation и lifetime.** Подтвердить настоящую новую session
   и scoped generation/build, не один доступный TCP порт. До финального commit
   сделать отдельный durable handoff постоянного child lifetime. Рекомендуемый
   адаптер изменяет только fresh owned activation Job после intent и проверяет
   результат через QueryInformationJobObject; не трогает task/domain Jobs,
   DACL, breakaway policy или системные настройки. По
   [SetInformationJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-setinformationjobobject)
   текущие limits сначала читаются, затем меняется нужная часть и выполняется
   readback. Такой адаптер ещё нужно реализовать и проверить, это не готовая API.
9. **Commit и maintenance release.** Commit допускается после всех receipts;
   worker очищает только свой marker/lease, сохраняя независимую user pause.
   Потерянный ACK сверяется тем же command ID. Раздельно показывать install
   outcome, текущую доступность процесса и доставку уведомления/отчёта.

Updater schema18 уже реализована; worker17 остаётся возможным будущим изменением
при добавлении durable receipt, не объявленной capability. Уточнять номера и
compatibility policy при имплементации по фактическому HEAD.
Supervisor остаётся владельцем одного blocking slot. Heap boundary после
stack overflow сохраняется; тяжёлые physical preflights не переносить в mailbox.

## FSM и восстановление

Минимальная причинная цепочка: Planned → Creating → Created → ResumeGranted →
ReadyForState → HandoffIssued → StateOwned → LocalVerified → ControllerVerified →
LifetimeHandedOff → Committed. Это проект состояний, не текущий wire enum.
Каждая external delivery имеет собственный Operation/Command ID и saved intent.
Unknown creation/resume/handoff/detach/commit удерживает Indeterminate с точным
последним известным prefix. Saved receipt не воссоздаёт live custody/child.

Cold reader только наблюдает и объясняет manual recovery. Не открывать old worker
на мигрированном журнале и не делать автоматический downgrade из backup.
После ошибки updater не удаляет guard или частичные данные ради повторного запуска.

## Автоматические gates

- Same-key/different-payload, stale root/owner/epoch/process, changed config/set,
  busy instance lock, ignored SQL write, atomic history/readback rollback.
- Actual schema17→18 updater и, если добавлен ledger worker,16→17 worker на
  consistent копиях настоящих данных, с backup/history/old-exe refusal.
- Actual Main positive: original guard → launch → state handoff → reconciliation
  → local IPC → controller confirmation → release; данные и user pause сохранены.
- Child не открывает DB writer до разрешения и не принимает задачи до commit;
  чужой CLI/process/replayed nonce не создаёт startup permit.
- Crash до/после каждого OS effect и receipt; no duplicate spawn/resume,
  unknown handoff не освобождает чужой lease, старый worker не запускается.
- Parent выходит после commit, новый worker сохраняется и выполняет fixture;
  crash до lifetime handoff убирает принадлежащий тесту child/Job tree.
- Ready files дважды, независимый verifier и negative proof mutations;
  disk measurements обоих хостов и exclusive cache lease. Не создавать UI окон.

После этого остаются полный U2 fault coverage, desktop U3 и public/bootstrap U4.
Только после их настоящих drivers/gates можно убрать production blockers и
проверить положительный новый support→drain маршрут end-to-end.
