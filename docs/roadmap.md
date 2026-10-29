# Постепенная реализация и расширение

**Same-Store release/ReleasePending source,2026-10-29:** internal worker transaction
сохраняет schema19 shutdown history и освобождает exact lease только после readback/
COMMIT; Existing — данные, не новый permit. Новый pending watchdog допускает exact
SQL/mode gap, mutations закрыты. Исправлен Store/mode race; native checkpoints
проверяют registries/Empty domain без инверсии lock order.144 worker tests/3 ignored,
9 новых; Linux/MSVC all-target Clippy/fmt passed. [Контракт и продолжение](plans/worker-self-update/finalization-worker-release-2026-10-29.md).
Source19/23; mutable V8 decoder/marker/factory/Active/keeper ещё нет. Windows этого
кода пока не запускали; installed source48/gen946 connected/error=null прежний.
Рабочий worker оставить запущенным, desktop не нужен, public install off.

**Pending finalization receiver data source,2026-10-29:** отдельный readonly
scope после root Committed сверяет same resident command/entry/child/keeper,
current updater owner/coordinator/schema и exact unconfirmed decision. Старые
V5/V6 admission не меняются; query не создаёт mode/OS custody/send permit.
15 targeted tests/2 новых, Linux/MSVC updater all-target Clippy passed.
[Контракт и продолжение](plans/worker-self-update/finalization-ledger-2026-10-29.md).
Source19/23, V8 handler/physical factory/Active/keeper ещё не включены; working
worker прежний, desktop не нужен, public install off.

**Receiver config pin handoff source,2026-10-29:** лишний File из outer native
receive больше не держится весь срок service: передан V4 receiver и закрыт после
File-ID check уже удерживаемого LocalService pin. Verification/quarantine по-прежнему
readonly.7 lifetime tests/2 новых, Linux/MSVC worker all-target Clippy passed;
Windows FileShare assertions ещё не исполнялись — включить в следующий V8 gate.
[Ревью и следующие границы](plans/worker-self-update/keeper-finalization-review-2026-10-29.md). Native22→23 ниже остаётся
frozenfcc49dd; этот runtime refactor в нём не проверялся. Source19/23, V8/Active/
physical factory/keeper conversion не включены; installed worker прежний,
worker оставить, desktop не нужен, public install off.

**Native retained22→23 passed,2026-10-29:** run20261029-202033-a2132bf8,
task18e1625b-d2bc-44d8-9b0b-949446ab685c succeeded0, producerfcc49dd.
Actual Windows copy/backup/old22 init+status refusal/cold20 passed;4 result files
verified twice,34 negative audits +142 reference audits; повторное чтение
идентично, reader7b6230c/SHA pinned в verified-after-reader-review.json.
Native task выполнен один раз. Первоначальный reader перепутал view.build (historical compiler record)
с producer BuildIdentity; исправлен со сверкой всей projection с pinned Main29,
7/24 portable tests passed. Перезапуск Windows не нужен. Исходный Python runner
завершён1 на ошибке reader; standalone fixed audit passed, это не worker failure.
[Результат и продолжение](plans/worker-self-update/finalization-migration-2026-10-29.md).Source19/23, root остаётся
AwaitingController/local7/lifetime Confirmed; новых finalization rows0, live grants0.
Installed0.2.0/source48/gen946 прежний/active0/errors=null; worker оставить,
desktop можно закрыть, окон не было. Далее physical factory/V8/worker Active/
keeper/two generations/U2–U4; public install off.

**Retained22→23 gate подготовлен,2026-10-29:** actual Main29 reference7ca206f→944a39c,
fixed Finalization22To23, consistent copy/old22 init+status refusal/cold20/no-grants.
[Сценарий и команда](plans/worker-self-update/finalization-migration-2026-10-29.md).23 portable reader tests/MSVC Clippy passed;
Windows parser/storage preflightdce79220 passed, old22 image прежний,17.8GB free.
Native migration пока не запускали; далее clean source export/один fenced task/
independent reread, затем physical factory/V8/Active/keeper/U2–U4. Installed worker
прежний, desktop не нужен, public install off.

**Finalization foundation source,2026-10-29:** updater23/worker19. Root Committed
и ReleasePending intent связаны одной SQLite transaction; pending блокирует
следующий admission, Known-runtime serve отказывает до worker Store. Proposed V8
не advertised; physical factory/receiver/delivery/Active/keeper conversion ещё нет.
[Контракт, review, проверки и продолжение](plans/worker-self-update/finalization-ledger-2026-10-29.md).
Full workspace999/17 ignored +после review13 targeted/4 CLI startup =1000 distinct;
Linux/MSVC all-target Clippy/fmt/scaffold passed. Actual Linux CLI22→23/backup/
old22 refusal passed; Windows23 gate ещё не выполнялся. Main29 ниже — frozen19/22.
Далее actual retained Main29 copy22→23, затем original factory/V8/release/Active/
keeper/two generations/U2–U4. Installed0.2.0/source48/gen946 прежний/active0/errors=null;
worker оставить, desktop не нужен, public install off.

**Main29 Windows passed,2026-10-29:** run20261029-155738-d847446d,
task2cd973db-9769-4695-b601-9d75344ab0ad, producer7ca206f→target944a39c.
235 native test executions/54 commands/67 files twice; independent audit и
readonly reread совпали. [Результат и pins](plans/worker-self-update/main29-native-2026-10-29.md).
Два authenticated V7 чтения после original expiry подтвердили current session
того же child/Job/Store; foreign peer/spoof/version отказали, bytes/history прежние.
Первая попытка772361c4 остаётся failed: test adapter28→20 исправлен и проверен.
Runtime204 archived; runner/test controller завершены0. Installed0.2.0/source48
прежний/active0/errors=null; рабочий worker оставить, desktop можно закрыть.
Далее durable finalization/root/release/Active/permanent keeper/two generations/U2–U4.
Public install off. Ниже — исторические checkpoint, их running статусы superseded.

**Main29 retry running,2026-10-29:** run20261029-155738-d847446d,
task2cd973db-9769-4695-b601-9d75344ab0ad, producer7ca206f→target944a39c.
Run-info `.winbridge/update-preparation/main29-204-20261029-155738-d847446d/run-info.json`,
log `.winbridge/review-2026-10-29/resident-main29-retry-run.log`, runner304946/tool2998,
test controller304947/.203:1144/state `/tmp/wb29-506_1vey`. Production131887 прежний.
Предыдущая попытка772361c4 terminal failed из-за test adapter28→20; cleanup доказан.
Новый run **не pass**, не повторять submit: читать тот же task до terminal/reread.
Working0.2.0/source48 прежний, desktop не нужен, worker оставить; public install off.

**Main29 завершился failed, не pass,2026-10-29:** run20261029-151031-f024760d/
task772361c4.231 предварительная native проверка прошла. Same child11644 вошёл
в lifetime, позднее reconciliation завершилось; первый Internal28 response
отвергнут **native test helper**, в его schema switch отсутствовал28→20.
V7 outcome не сохранён/не объявлять доказанным. Helper вынесен в общий portable/
native adapter,3 регрессии/58 Python/Linux+MSVC Clippy passed; retry parser46ab6ada passed.
Далее clean producer/автоповтор, нового native pass пока нет.
Cleanup taskc8326230 passed: slot204 processes0/worker EXE absent,18.8GB free;
failed runtime archived10 entries, installed0.2.0/source48/gen946 прежний/active0/
errors=null. Test controller302634 и runner302633 отсутствуют, повтор старого
submit запрещён. [Подробности](plans/worker-self-update/resident-native-gate-2026-10-29.md).
Рабочий worker оставить, desktop не нужен; public install off.

**Main29 запущен, пока не завершён:** run20261029-151031-f024760d,
task772361c4-ae8d-4ebe-8a6b-28fbce1df0fb, producerfcee32a→target944a39c.
Run-info `.winbridge/update-preparation/main29-204-20261029-151031-f024760d/run-info.json`,
log `.winbridge/review-2026-10-29/resident-main29-run.log`, runner302633/tool session6497.
Test controller302634/.203:1144/state `/tmp/wb29-3rsanhqa`, production131887 прежний.
Native supervisor/worker build passed, resident model checks начались. Не повторять
submit: читать тот же task/context/key до terminal + independent reread.
Installed0.2.0/source48 прежний; desktop не нужен, working worker оставить.

**Main29 source подготовлен,2026-10-29:** actual original keeper/current V7
connection после lost Ready/expiry, foreign OS peer/JSON spoof/version refusal,
unchanged snapshots и cold20 включены в отдельный native gate. Пока **не запускали**.
[Сценарий, target944a39c, команда и проверки](plans/worker-self-update/resident-native-gate-2026-10-29.md).
983 workspace/17 ignored/126 groups, Linux/MSVC Clippy/fmt,58 Python checks passed;
Main27/Main28 reread identical. Windows parser/slot/storage preflight9a18f07c passed,
slot204 свободен,20.3GB free, installed0.2.0/source48 прежний. Дальше clean producer
export/один Main29/independent reread, затем finalization/root/release/Active/
permanent keeper/U2–U4. Public install off, working worker оставить, desktop не нужен.

**Resident actor28/view20 source,2026-10-29:** отдельный readonly query/client/
mailbox подключён поверх V7, original guard/query/correlation сохранены. Ошибка
чтения не переписывает V6 history; observed — датированный факт, не commit/release.
[Контракт, проверки и Main29](plans/worker-self-update/resident-actor-2026-10-29.md).
Workspace982/17 ignored +1 новый bound-mailbox test =983 distinct passed;
Linux/MSVC all-target Clippy/contracts/typecheck/fmt прошли. Native V7 пока нет.
Исправлен service_started для pending lifetime; views1–19 не получают новых полей.
Следом Main29 на distinct target944a39c/stable runtime-test-204, затем durable
finalization/root/release/Active/permanent keeper/U2–U4. Source19/22 прежние,
public install off; installed worker0.2.0 не меняется, desktop не нужен.

**Readonly resident V7 source,2026-10-29:** actual OS peer связан с original live
keeper; current accepted frame/link проверяется после expiry/reconnect, отдельно
от historical V5/V6. Compact header+SHA полного projection и local readonly Info
predicate сохраняют bounded wire; strict query не меняет mode/journal/permissions.
[Контракт, source checks, disk cleanup и продолжение](plans/worker-self-update/resident-observation-2026-10-29.md).
978 distinct Linux tests/17 ignored (workspace969 +9 дополнительных worker validation).
Linux/MSVC all-target Clippy/fmt/scaffold прошли; нового native V7 gate пока нет. Worker19/
updater22/actor27/view19 прежние, public install off. Следом actor/current-view и
actual native V7 target, потом durable finalization/keeper/Active/U2–U4.
Working0.2.0/source48c834c/active0/errors=null прежний; desktop не нужен.

**Physical custody source отделена,2026-10-29:** private `ProcessCustody` владеет
original child/Job/channel/captures/readers, verification pins остаются в guard.
Drop запрашивает original tree cleanup до отпускания pins, без detach/unsafe/
новых grants. [Структура, проверки, ограничения](plans/worker-self-update/process-custody-owner-2026-10-29.md).
972 workspace tests/17 ignored/126 groups, Linux/MSVC all-target Clippy/fmt/diff
passed.3 новых IO tests проверяют concurrent1MiB drain/64KiB cap, move, error,
Unicode EOF. Нового native gate на refactor нет: Main28 относится к0426a13.
Source19/22/private6/actor27/view19 прежние; permanent conversion/root commit/
release пока отсутствуют. Следом typed OS-authenticated resident control/current
connection/finalization intent, затем consuming keeper/Active/new full native/
two generations/U2–U4. Working worker прежний, desktop не нужен, public install off.

## История предыдущих checkpoint

**Main28 Windows passed,2026-10-29:** run20261029-091025-be7fc543/task393a8003,
producer0426a13 → target90fc5b4.213 native tests/50 commands/63 files twice,
24+27+37+15+10 negative audits/4 markers; independent readonly reread identical.
[Результат и pins](plans/worker-self-update/main28-native-2026-10-29.md).
Actual header4/body0, Issued→Indeterminate→Confirmed после original expiry,
same child22096/Job/Store/authority, no private resend/cold adoption, closed roots
подтверждены. Runtime204 archived, test controller247140 exited0, installed0.2.0/
gen946f1128 прежний/active0/errors=null. Desktop можно закрыть, worker оставить.
Следом [physical custody/keeper finalization](plans/worker-self-update/keeper-finalization-review-2026-10-29.md):
выделить owned handles из verification pins; old stream после partial Ready
retire, не переиспользовать; отдельный authenticated control и root/release/Active
FSM, затем two generations/U2–U4. Public install off. Main28 terminal, не replay.

**Main28 запущен, ещё не завершён:** run20261029-091025-be7fc543,
task393a8003-e229-4b41-93c7-b65eea2fa992; clean producer0426a13 → target90fc5b4.
[Команды, source pins и границы](plans/worker-self-update/lifetime-ready-loss-2026-10-29.md).
Наблюдать ту же задачу/процесс, не resubmit. Run-info:
`.winbridge/update-preparation/main28-204-20261029-091025-be7fc543/run-info.json`,
log `.winbridge/review-2026-10-29/ready-loss-main28-run.log`.
Runner PID247139, test controller247140 на.203:1144, state `/tmp/wb28-x7geapm4`.
Production controller131887 и installed worker прежние. Пока нет terminal +
independent reread, native partial-Ready не объявлять подтверждённым. Worker
оставить, desktop/окон нет; public install off. В следующем goal turn проверить
живой runner/task, при terminal проверять сохранённый результат readonly.

**Main28 source подготовлен,2026-10-29:** exact-scope `update-validation` hook
оставляет private Ready body непрочитанным; original guard/Job/channel/window
сохраняются. Native fixture ждёт durable Indeterminate и original expiry, затем
один normal IPC подтверждает тот же command с revision2. [Реализация, проверки, gate](plans/worker-self-update/lifetime-ready-loss-2026-10-29.md).
969 workspace tests/17 ignored,52 Python tests, Linux/MSVC Clippy/fmt/diff passed.
Main27 readonly reread unchanged; его normal proof не переименовывается в fault.
Final PS/storage/slot preflight passed: task52def8ef, slot0/K:22.9GB; installed
worker прежний/idle. Source19/22/private6/actor27/view19, public install off.
**Actual Main28 пока не выполнен:** clean export → one native run → independent
reread, затем permanent keeper/commit/scoped release/two generations/U2–U4.
Рабочий worker оставить, desktop и окна не нужны. Старые gates terminal, не replay.

**Main27 Windows passed,2026-10-29:** merge5cbc7b0, run20261029-000947-c9ad6d6f/
taskdf6803ac.208 native tests/49 commands/60 files twice/24+27+37+10 negatives;
terminal succeeded0 и independent reread identical. [Pins, доказательства, далее](plans/worker-self-update/main27-native-2026-10-29.md).
WIP observation fix подтверждён: same child13156/Store/authority/Job и original
custody сохраняются после original240s, duplicate/no replay/cold NotResident
проверены; root AwaitingController/local7, commit/release закрыты. Reader review
добавил6 coherent key-alias refusals;5 focused Python tests passed. Source
worker19/updater22/private6/supervisor27/view19. Следом **private Ready loss fault**
(в Main27 false), затем permanent keeper role/commit/scoped release/two generations/
U2–U4. Main27 terminal, не replay ради продолжения. Installed0.2.0/gen946f1128
прежний/active0/errors=null; test controller231033 exited0, runtime204 archived.
Desktop можно закрыть, рабочий worker оставить; public install off.

**Ревью .203 ветки lifetime-runtime-204,2026-10-28:** commits90fc5b4/2a0bee6/49b6d7b
проверены вместе с пользовательским WIP. [Разбор и продолжение](analysis/updater-lifetime-runtime-204-review-2026-10-28.md).
964 workspace tests/17 ignored и45 Python regressions passed; TS/UI/fmt/scaffold,
Linux/MSVC all-target Clippy и generated contracts прошли. Исходный Main27 run20261028-224638-3e951af3 failed: entry confirmed, но
позднее наблюдение вызывало V5 Issued-only reader. Fix использует original owned
lifetime snapshot до/после normal IPC; регрессионный тест passed. Для Windows
подтверждения нужен один новый Main27 с fixed supervisor, distinct V6 target90fc5b4,
stable runtime-test-204 и test controller.203:1144. До terminal passed + independent
reread native V6 не объявлять завершённым. Installed worker прежний; preflight
PID2532/slot processes0/24GB свободно. Worker оставить, desktop/окон нет.

2026-10-28, .204: [supervisor27/view19](plans/worker-self-update/lifetime-actor-2026-10-28.md)
подключает original lifetime entry и readonly reconciliation. Source готов к
новому Main27/19+22; native gate ещё не выполнен. Затем private Ready fault,
keeper role/final commit/scoped release/U2–U4. Старые checkpoints ниже исторические.

2026-10-28, .204: [lifetime V6 source runtime](plans/worker-self-update/lifetime-runtime-2026-10-28.md)
продолжает original admission: same-child receiver, typed quarantine, single-send
и normal IPC observation; roots/dispatch закрыты. Остаток original deadline теперь
соблюдается и в раннем verification IO. Source19/22/private6/actor26/view18;
actor27/view19 + fresh native gate, keeper role/commit/scoped release и U2–U4 впереди.
Windows runtime в этом source шаге не заменяется, native proof не объявлен.

**Lifetime original admission/preparation — source checkpoint:** consuming
original guard → durable Issued → повторный current custody/readback; readonly
receiver data и strict proposed private6 packets добавлены. [Границы, ревью, далее](plans/worker-self-update/lifetime-admission-2026-10-28.md).
950 workspace tests/17 ignored, Linux/MSVC Clippy и contracts/fmt/scaffold passed.
Implemented private5/actor26/view18 прежние; receiver6, sender, typed quarantine,
normal IPC, actor27/view19 и native positive gate — следующий незавершённый шаг.
Далее keeper role/commit/scoped release/U2–U4; public install off. Installed
worker неизменён, новый Windows run не выполнялся.

**Actual21→22 copy gate passed:**1 native/4 files twice/26+61 negatives,
readonly reread identical. [Результат и границы](plans/worker-self-update/retained21-to22-2026-10-28.md).
Original controller/local7/root/rows сохранены; epoch2→3→4/backup/old21 refusal
подтверждены. Reader sessionId type fix проверил тот же native result,46 Python
regressions passed. Installed worker прежний. Следом fresh consuming original
lifetime admission/receiver → keeper/commit/scoped release/U2–U4. Public install off.

**Retained21→22 harness готов к native run:** fixed profile, archived Main26
controller/local7/root, consistent copy, backup/epoch/old21 refusal и24 observer
mutations. [План](plans/worker-self-update/retained21-to22-2026-10-28.md).
MSVC/16 Python и Windows syntax/image/storage preflight прошли; actual migration
ещё не проверена. Public install off. Следом clean export/run/reread, затем fresh
lifetime admission/receiver, keeper/commit/release и U2–U4.

**Lifetime ledger22:** immutable intent/current/history, CAS/causal readback
и readonly full-graph queries реализованы.11 SQL tests/946 workspace passed,
17 ignored. [Границы и продолжение](plans/worker-self-update/lifetime-ledger-2026-10-28.md).
Writers только cfg(test), private5/actor26/view18 прежние, public install off.
Следом actual21→22 consistent-copy/refusal → fresh original admission/receiver →
supervised keeper/commit/scoped release/U2–U4. Native22 runtime пока не проверен.

**Lifetime DTO/FSM:** compact contract и чистые causal transitions реализованы;
6 focused/935 workspace tests passed,17 ignored. [Контракт и продолжение](plans/worker-self-update/lifetime-contract-2026-10-28.md).
Никаких новых advertised support/private/schema версий или runtime permissions.
Следом updater22 ledger/readback → fresh original admission → actual retained21
copy/refusal gate → same-child supervised runtime/normal IPC → commit/release/U2–U4.

**Main26 passed:**175 tests/46 commands,57 files twice/24+27+10 negatives,
3 actual markers; independent readonly reread совпал. [Pins/результат](plans/worker-self-update/main26-native-2026-10-28.md).
Same child/authority/new session, lost ACK и cold no replay подтверждены; original
240s не продлены. Installed0.2.0 здоров/прежний, desktop не нужен. Root
AwaitingController/local7; lifetime/commit/release/public install ещё закрыты.

**Lifetime OS spike passed:**2 cases/73 files twice/36 negative audits,
readonly reread identical. [Evidence и scope](plans/worker-self-update/windows-lifetime-spike-2026-10-28.md).
Installed worker прежний. Следом compact DTO/FSM → ledger/admission → same-child
receiver/quarantine → final commit/scoped release/U2–U4. OS helper proof не даёт
production keeper/capabilities, original startup window и Job limits прежние.

2026-10-28/latest: supervisor26/view18 реализованы в source;927 workspace/17 ignored,
Linux/MSVC Clippy/TS/UI passed. Old views frozen, Observed отделён от current
connection и cold history. Следом full native Main26/stable204/SDK21; distinct
target b149eed сохранён. Runtime пока не проверен, whole updater не завершён.

2026-10-28/latest: same-child network/private5 + independent readonly normal IPC
реализованы в source, workspace921/17 ignored +26 focused controller tests,
Linux/MSVC Clippy passed. Новый runtime на Windows пока не исполнен. Следом
supervisor26/view18 и отдельный Main26 в stable204, затем Job lifetime/commit/
release и U2–U4. Installed worker не меняется; public whole update закрыт.

2026-10-28: source updater schema21 содержит controller ledger/read-only graph,
10 SQL tests/workspace914/17 ignored/MSVC Clippy passed. Production fresh admission
и same-child network пока отсутствуют. [Actual20→21 copy gate](plans/worker-self-update/retained20-to21-2026-10-28.md)
пройден:taskd5a75306/4 files twice/21+34 negatives, installed0.2.0 неизменён.
Следом original owner/receiver/actor и native full run; public install закрыт.

Original controller preparation реализована в source: consuming native factory,
current local7/root/owner/nonce/window guards, pinned configured topology и
data-only receiver snapshot.269 updater tests/4 ignored и MSVC Clippy passed;
network send/private5 receiver/actor26/view18 и native runtime proof ещё впереди.

2026-10-28: [controller-session DTO/FSM foundation](plans/worker-self-update/controller-session-activation-2026-10-28.md)
реализована и reviewed:10 focused tests,255 updater/75 protocol, Linux/MSVC
Clippy/TS/generated contracts passed. Выбранные endpoint/normal link/projection
связаны с тем же LocalVerified7/service/authority. На этом checkpoint network send
и ledger ещё не были подключены; далее ledger21 выше. Actor25 прежний.

2026-10-28,.203: [controller0.2.0 установлен](releases/controller-0.2.0-debian203-2026-10-28.md)
после review remote f069697 и18 actual controller tests. Worker0.2.0/две real VM
связи/прежняя история подтверждены; config неизменён. Следом updater подтверждение
новой controller session, затем Job lifetime/commit/release и U2–U4.

2026-10-27, .204: [post-install review](analysis/multi-controller-post-install-review-2026-10-27.md)
устраняет привязку native migration tests к installed0.1.1, удаляет superseded model
integration target и объединяет controller diagnostics metadata lookup. Реальные
multi-controller/old-schema guards сохранены; runtime deployment не меняется.
Новые PS reference параметры пока требуют будущей native проверки, не считать
этот review новым Windows gate. Updater продолжать по основному плану ниже.

2026-10-27, .204: **worker0.2.0 установлен пользователем; controller.204 обновлён
до0.2.0**. [Actual deployment двух Debian подтверждён](releases/controller-0.2.0-debian204-2026-10-27.md):
обе связи.203/.204 connected, один прежний worker generation/incarnation,10 tasks
сохранены, recovery errors=null. Runtime config.204 и controller.203 не менялись.
Общая readonly диагностика теперь доступна на.204; updater остаётся отдельной работой.

2026-10-27, .204: **[пакет worker0.2.0 для ручной установки](releases/worker-0.2.0.md)
готов**. Повторно проверены сохранённые native gate/evidence и ZIP; Windows runtime
не менялся. Далее пользователь устанавливает normal worker по постоянному пути
и topology.203+.204; на момент упаковки подключение двух разных Debian ещё не проверялось.
Завершение updater/controller session/Job lifetime/commit/release — отдельный этап.

2026-10-27: [Main25 после merge](plans/worker-self-update/main25-after-multi-controller-2026-10-27.md)
**полностью подтверждён**:140 native tests/41 commands/52 files twice,24 core+10
marker negative audits,3 original barriers и отдельный cold worker17→19.
Архив1000 files/152 directories проверен readonly; installed runtime не менялся.
[Actual retained19→20 copy gate](plans/worker-self-update/retained19-to20-2026-10-27.md)
также пройден:4 files twice/21 negatives/old reference60 negatives, original
history/backup/old19 refusal проверены. Следом controller session того же resident
child, Job lifetime/commit/release; U2–U4 остаются впереди.

2026-10-27: **multi-controller слит в main** после [ревью](analysis/multi-controller-merge-review-2026-10-27.md).
895 workspace/17 ignored, Linux/MSVC Clippy,3 browser scenarios, повторный readonly
audit Windows4 peers/5 tasks/16 negatives/15 files twice прошли. Runtime остаётся
одним; source journal19/updater20, установленный worker0.1.1 не меняется.
Следом actual Main25 profile19/20 в отдельном постоянном runtime-test-204 через.203.
Ниже сохранены прежние feature/checkpoint статусы.

2026-10-27: выполняются [Main25 profiles и изолированная worker17→19 migration](plans/main25-schema-profiles-2026-10-27.md).
Main ba9557b переносится в feature; profiles сохраняют17 и добавляют19 без ослабления
schema/authority checks. Windows slot204 отдельный, installed worker не обновляется.
После Windows работы пользователь переключит worker обратно на.203.

## Multi-controller: текущая ветка .204

2026-10-27: [main обновлён доba9557b, новое сравнение](plans/multi-controller-review-ba9557b.md).
Main25/archive/cache fix совместимы по архитектуре; новый preflight требует Links
adapter, а native audit требует явного переноса worker17→19 schema expectation.
Подготовлен полный новый integration patch;894 workspace/update-validation tests,
17 ignored, Clippy/MSVC/contracts/TS/UI passed. Ветки не слиты, Windows не запускался.

2026-10-27: подтверждён source/runtime сценарий3/4 controller, общий limit constant
и универсальный test harness. [Проверки и границы](plans/multi-controller-runtime.md#три-и-четыре-controller--2026-10-27).
Native Windows gate19 и интеграция с main остаются отдельными шагами.

2026-10-27: [review и сравнение с main c4b7aea](plans/multi-controller-review-2026-10-27.md).
Подготовлен source patch для общего startup и readonly service с topology.
Проверочное объединение879/0/17ignored, Clippy/MSVC/contracts/TS/UI passed;
ветки не слиты, Windows gate19/updater migration остаются впереди.

В `feat/multi-controller` (.204): [прямые сессии двух controller](plans/multi-controller-runtime.md).
Git coordination отменена. Pure sessions/origin, WorkerSettings topology и actual
transport подключены; Linux model и два реальных loopback WebSocket соединения
проверены с одним Worker/Store (187 passed /5 штатно ignored, Clippy/MSVC check).
Readonly список связей и RPC counters подключён отдельным контрактом/панелью;
scope/old-worker/offline проверены на Chromium + Linux worker. Durable creator
attribution (schema18, workflow inheritance, readonly API/task UI) добавлена:
819 workspace +3 protocol tests и actual Linux17→18 proof прошли. Затем добавлены
cancellation actor/propagation/readonly UI в schema19:829 workspace +3 protocol,
actual Linux17/18→19 proof и Chromium прошли. 2026-10-27 добавлены три readonly
CLI/MCP query wrappers с pinned context и отдельным IPC envelope: два Linux
controller, реальные CLI/MCP subprocesses, running-query/no-cancel, stale scope и
old-component refusal проверены (37 controller/72 protocol tests). Native Windows
gate19 ещё впереди. Windows переключает пользователь вручную;
установленный0.1.1 и его config здесь не меняются.

2026-10-26: по новой задаче пользователя выполняются
[review/merge performance-ветки и выпуск worker0.1.1](plans/run-speed/review-merge-2026-10-26.md).
Ветка слита в main: CPU fix и build profiles проверены, release harness исправлен. Worker0.1.1
выпущен: run20261026-182213-6cd81f3e succeeded,140 Rust +6 native policy,
actual old-controller RPC и source final readback passed. Постоянный runtime-test
path/port1144/900s approval budget проверены; ZIP готов к ручному применению.
Installed d052 ещё не заменён; дальше actual installed verification после ответа
пользователя, затем normal-service updater plan. Public updater/multi-controller
не включаются.

## Updater: checkpoint main ba9557b

2026-10-27: [Main25 native scenario](plans/worker-self-update/main25-2026-10-27.md)
подключён в source: pre-admission firewall wait, same-child service, IPC/history/pause,
cold exit и archive под original lease. Первый actual run:136 prerequisites passed,
setup отказал на Cargo hard link до Main; fix cross-checked и native regression1/0
прошёл с2 files twice, test slot сохранён.
Разовая консервация закрытого slot ожидает отдельного одобрения; positive Main25 впереди.

2026-10-27: [runtime archive](plans/worker-self-update/runtime-archive-2026-10-27.md)
имеет29 native policy checks; retirement hook cross-checked. Full Main25/wiring ещё впереди.

2026-10-27: [FixtureLayout/private input2](plans/worker-self-update/fixture-layout-2026-10-27.md)
подключён в source; workspace843/0, Linux/MSVC Clippy. Native layout5+2/3 files twice/12 negative audits подтверждён; archive/full Main25 впереди.

2026-10-26: [activation runtime reservation](plans/worker-self-update/activation-runtime-reservation-2026-10-26.md)
прошёл6+11 native checks. Full Main25/archive/layout ещё впереди; production не менялся.

2026-10-26: performance-ветка слита в main, worker0.1.1 выпущен и **установлен
пользователем, actual build/hash/history проверены**. [Проверка и продолжение](plans/worker-self-update/service-ledger-2026-10-26.md).
Source updater20 содержит intent/history, original-guard admission, shared Worker/
readonly IPC и trusted V4 sender/receiver с независимым parent normal-IPC probe.
Actor25/view17, resident service states и атомарные activation7/LocalVerified +
root AwaitingController реализованы. Source workspace833/0. Следом stable
activation runtime/network fixture → actual Main25/retained19→20.
[Checkpoint](plans/worker-self-update/local-service-actor-2026-10-26.md).
Native component gate15429a4:16+14 tests/3 files twice, independent verification
и readonly recheck; это не actual same-child V4/Job/Main25. Старый1f653b2/13+13
сохраняет лишь свой historical library scope.
Public updater/multi-controller не включаются.
Установленная0.1.1 не заменяется.

## Завершено: ускорение прогонов и выпуск 0.1.1

2026-10-26: [локальный worker0.1.1](releases/worker-0.1.1.md) с idle CPU fix и
startup/version diagnostics: Windows release/compatibility gate пройден, ZIP опубликован.
[Несколько Debian controller](plans/multiple-debian-controllers.md) — отдельная
будущая фича; конфигурацией0.1.1 один worker к двум controller не подключается.

2026-10-26 [маркировка validation storage](validation/validation-storage.md):
37.66GiB измерены readonly; tmp- для новых временных work dirs, storage guide,
inventory CLI и legacy-path compatibility. Очистка и перенос caches/references
в новую раскладку не выполнялись.

2026-10-26 дополнительный bugfix: [worker idle CPU](plans/run-speed/idle-worker-2026-10-26.md).
Полная история читалась каждые25ms. Pending-index queries/diagnostics и isolated
Windows CPU experiment завершены:99.19→1.51% одного ядра на synthetic history,
52 native tests passed/5 ready files twice. Затем worker0.1.1 выпущен и установлен;
проверка build/hash/history завершена в checkpoint выше.

2026-10-26 новая цель: [следующий раунд](plans/run-speed/round2-2026-10-26.md).
Раунд завершён: desktop rebuild ordinary release118.5625→private local opt0
41.7945s(64.75%),25 ready files twice, role/source/cleanup passed. Production argv
unchanged. Full gate с новым builder:201 tests/56 files twice/78 negative audits
passed. Command sum21.409→20.266min observational. Warm feature matrix не требует
перекомпиляции; дальнейшие варианты — dev/test codegen/symbols и точный SDK target set.

Исторический контекст отдельной perf-ветки до слияния: пользователь 2026-10-25
поручил продолжать эксперименты до существенных
результатов в отдельной ветке; другой агент продолжает updater в `main`.
[План оптимизации и E1 A/B эксперимент](plans/run-speed/README.md): сначала
opt-level1 при сохранённых safety checks; затем полный native gate и build
batching по результатам. E1: median65 tests36.165→20.6745s,42.83% быстрее,
но первая сборка дороже; [измерения и v2](plans/run-speed/results-2026-10-26.md).
Updater эта ветка не развивает.

2026-10-26 итог: v2(core/protocol/serde_json opt1, updater0) ускоряет paired65-test
suite45.65%; full E2 **201 Windows tests/56 files twice/78 negative audits passed**.
Ветка содержала профили и инструменты эксперимента; теперь они слиты в `main`. Полные команды
заняли21.41min против прежних24.10min, это observational сравнение. Следующий
кандидат E3 — стоимость feature variants/rebuild по Cargo fingerprint/timings.

## Текущий checkpoint — writer24 подтверждён 2026-10-25

[Актуальное продолжение](plans/worker-self-update/resume-2026-10-25-writer-verified.md)
и [review/диагностика](plans/worker-self-update/review-2026-10-25-latency.md).
Run20261025-232746-26bad6d7/taska49ade1f-4e35-4eff-84ac-3b8d21c0a9ea
**succeeded/0:193 tests,56 ready files twice,44+16+18 negative audits**.
Same-child journal writer19/17/actor24/view16/private3 проверен, обе actual migrations
прошли. Verifier исправлен и тот же report проверен readonly; replay не было.
Новые admissions240s разрешены пользователем; старые120s intent не меняются.

Первый незавершённый сценарий — [normal local service/reconciliation](plans/worker-self-update/local-service-activation.md)
в том же child/Store/lock без повторного owner epoch. Затем новая controller session
→ durable Job lifetime transfer → commit/scoped maintenance release → remaining
U2 faults/CLI/MCP/opt-in → U3 desktop → U4 actual installed update → возврат к A4.3.
Schema20 format/readback добавлены; private4/actor25/view17 ещё не реализованы.
Source data foundation для service включает strict DTO/binding, pure FSM и
журнал и consuming admission/connection transfer preparation. Service4 driver/
receiver/normal Worker/actual IPC впереди.
Public install выключен; worker0.1.1 применён вручную, новый updater20 не установлен.
Старые NEXT/ACTIVE ниже исторические. Точные source/task/proof pins и окружение
192.168.44.204 — историческое окружение writer gate; текущее .203 в новом checkpoint.
Terminal gate не повторять без новой причины.

## История предыдущих checkpoints: не текущие задания

2026-10-24: [combined native95 actor21/read binding запущен](plans/worker-self-update/windows-activation-files-gate-2026-10-24.md),
task01d7bf76-6c53-427c-8cd9-81d8f8798962/run20261024-153856-fe2e4f76,
producer74e8839. Наблюдать тот же task; proof ещё нет. Public install off,
installed runtime unchanged. После audit/review продолжать launcher/receiver/state.

2026-10-24: whole-set read binding library и **actor21/view13 добавлены**, пока
без native proof. Worker/updater schemas16/18 прежние, activation/public install off.
Workspace716/0 (16 ignored), legacy87/0 readonly recheck passed. Final checks,
clean producer export и combined native95 (`--activation-files`) впереди, затем
launcher/private receiver/state handoff и remaining U2/U3/U4. [Activation plan](plans/worker-self-update/runtime-activation.md).


2026-10-24: consuming one-file **read handoff прошёл native4/0**,
taskade260bf-de0a-436b-9920-589eb26ee86c/run20261024-143036-03cf8cbb,
producerd4c27a3,5 ready files twice. [Pins и свойства](plans/worker-self-update/runtime-activation.md).
Следом whole-set original-guard/journal binding, затем actor/launcher/state handoff.
Schemas18/16/20/view12 прежние; public install off, installed d052 unchanged.

2026-10-24: physical bridge начат с consuming one-file read handoff; Windows
all-target Clippy passed. Готовы4 native sharing/rebind/failure tests и короткий
`--runtime-read-handoff-only` gate, ещё не выполненный. Whole-set original-custody
binding и launcher/state handoff впереди; schema/capability/public install не менялись.

2026-10-24: **[native18 preparation/migration87/0 gate пройден](plans/worker-self-update/windows-activation-preparation-gate-2026-10-24.md)**,
26 ready files twice/72 Main +12 migration mutations и readonly recheck. Actor20
metadata preparation проверен в original supervisor; actual17→18/refusal passed.
Observer исправлен под tagged FilesVerified. Следом physical file-handle bridge,
launcher/private receiver/state handoff, local/controller/lifetime, remaining
U2/U3/U4. Public install off, installed d052 unchanged; старые pending ниже — история.

2026-10-24: [native18 preparation/migration task запущен](plans/worker-self-update/windows-activation-preparation-gate-2026-10-24.md),
`c1675a54-b250-4fa9-b2d3-66c653033180` / run20261024-111115-9272d0e2,
producerb12ecc6. Наблюдать ту же task; результата пока нет. Expected87 tests/16
групп, installed runtime unchanged, public install off. После terminal audit/review
продолжать physical activation/state handoff; старые NEXT ниже — история.

2026-10-24, продолжение после daa88da: combined **native18 preparation/migration
harness добавлен**, actual run ещё впереди. Actor20/ledger18 review без важных
ошибок, Linux/MSVC Clippy/Python4+9 passed. [Activation plan](plans/worker-self-update/runtime-activation.md)
содержит режимы и следующий порядок. Public install/activation capability off;
installed runtime не менялся. Проверки harness не являются native success proof.

2026-10-24, checkpoint: **supervisor20/view12 activation preparation** подключён
к ledger18; worker schema16 прежняя. Это metadata, не launch capability.
[Подробное продолжение](plans/worker-self-update/resume-2026-10-24-activation-preparation.md)
содержит итоговые проверки и карту следующей работы. Native17 readonly recheck
пройден; workspace713/0 (16 ignored), focused28/0, protocol-jsonschema53/0,
Linux/MSVC Clippy/contracts/TypeScript/UI/fmt/scaffold262 также прошли.
Combined actual18 preparation/migration harness/gate ещё впереди.
Execution/state handoff/local/controller/lifetime, remaining U2/U3/U4 не готовы;
public install off, installed d052 unchanged. Старые статусы ниже — история.

2026-10-24: [activation ledger18](plans/worker-self-update/runtime-activation.md)
добавлен: graph-bound reader, immutable history/CAS/readback и guarded metadata
preparation с transient nonce (не create permit). SQL6/model22, workspace709/0
(16 ignored), обе final Clippy passed. Actor20/view12, actual17→18/refusal,
actual owned preparation и execution driver ещё впереди; activation support=None.

2026-10-24: [activation intent/startup FSM](plans/worker-self-update/runtime-activation.md)
реализованы как data model:10 новых regressions, workspace703/0 (16 ignored),
обе Clippy passed. No-process отделён от unknown, late observations сохраняются
без новых grants. Fresh permission/ledger/OS driver ещё отсутствуют; activation
support=None. Следом schema18/graph-bound admission и physical handoff.

2026-10-24: [production drain17 actual Main/migration gate](plans/worker-self-update/windows-compatible-drain-gate-2026-10-24.md)
**пройден33/0**,27 ready files twice/38+19 mutations и readonly recheck.
Taska4f17a0d terminal succeeded/0, не resubmit. Root/SourceHeld сохранены при
Blocked, BeginDrain0, duplicate/legacy rejection и actual16→17/refusal прошли.
Следом activation intent/FSM/ledger и original-custody execution по плану ниже,
затем U2 faults/U3/U4. Public install off; whole updater не готов.

2026-10-24: [activation source model](plans/worker-self-update/runtime-activation.md)
добавлена отдельно: exact completed replacement digest/owner/generation и
installed worker/manifest,5 новых model tests +1 projection/обе Clippy passed.
Это metadata, не launch permit. Schema17/native gate остаются отдельной задачей;
следом activation intent/FSM/ledger/fresh original-custody admission и driver.

2026-10-24: второй native17 run тоже failed после успешного health10: main
stack overflow сохраняется при heap boxing. Package244/0 и обе Clippy отдельного
owner thread8MiB прошли; далее fresh namespace. Migration/refusal в обоих
прогонах подтверждены, полный Main gate ещё открыт.

2026-10-24: [первый native17 run](plans/worker-self-update/windows-compatible-drain-first-attempt-2026-10-24.md)
прошёл32 preliminary tests и actual16→17/refusal (23 ready files twice,
19 negative mutations), но Main завершился stack overflow до health/support.
Heap allocation mailbox future добавлен, supervisor tests8/0 и обе all-target
Clippy прошли; затем fresh native
namespace с новым producer. Production Main gate не закрыт, installed d052 unchanged.

2026-10-23 checkpoint по просьбе пользователя: [required production drain17/19](plans/worker-self-update/compatible-drain.md)
реализован, workspace688/0 (16 ignored), model/SQL7/IPC1/jsonschema51,
обе all-target Clippy/contracts/TS/UI/Python/docs passed. Immutable decision
и atomic outbox link блокируют legacy fallback после production decision.
Combined actual33 Main17 + migration16→17/refusal **не запускался**.
[Подробный порядок продолжения](plans/worker-self-update/resume-2026-10-23-compatible-drain.md):
два clean distinct exports17 → native gate/proof audit → activation/U2 faults/U3/U4.
Public install off, установленный worker не менялся. Нижние checkpoints — история.

Продолжение review17: независимый observer усилен (exact decision/view scope,
UUID/time/reason и fabricated-authority mutations), Main audit теперь38 +migration19.
Old16 readonly recheck28/Python3+7 passed; combined33 native gate ещё не запускался.

2026-10-23: [support actor18 Main gate23/0](plans/worker-self-update/windows-candidate-support-actor-gate-2026-10-23.md)
пройден,21 ready files twice/28 mutations. Lost ACK/duplicate/normal IPC/abort
подтверждены. Следом production support→drain/schema17/actor19 и migration/Main
blocked-request gates, затем activation/U2/U3/U4. Public install off.

2026-10-23: [support actor18/view10](plans/worker-self-update/candidate-support-actor.md)
добавлен, workspace679/0 и обе Clippy/TS/UI passed. Actual Main support gate
подготовлен; production drain binding ещё не подключён. После native проверки
продолжить новым production entry с atomic evidence link, затем activation/U2/U3/U4.
Ниже сохранены предыдущие checkpoints; public install off.

2026-10-23: [candidate support schema16](plans/worker-self-update/candidate-support.md)
и [Windows25/0 metadata/query/migration gate](plans/worker-self-update/windows-candidate-support-gate-2026-10-23.md)
завершены:13 ready files twice,36+22 mutations, actual15→16/refusal.
Local workspace674/0, обе Clippy/TS/UI passed. [Новая инструкция продолжения](plans/worker-self-update/resume-2026-10-23-candidate-support.md):
versioned actor/pre-drain binding → native Main → activation → remaining U2/U3/U4.
Actor/Main нового support driver ещё не проверены; public install off.
По просьбе пользователя текущие проверки закрыты и сохранена передача контекста.

2026-10-23: [updater support native8/0 gate](plans/worker-self-update/windows-updater-support-gate-2026-10-23.md)
пройден,9 ready files twice/36 mutations. [Pure typed compatibility policy](plans/worker-self-update/updater-compatibility.md)
добавлена: default107/schema108 core/protocol tests, обе Clippy/TS passed.
Это foundation, не durable gate. Следом versioned intent/journal-bound probe/
pre-drain admission (schema16 при необходимости, actual15→16/refusal), затем
activation и U3/U4. Installed d052 unchanged, public install off, цель active.
Исторические checkpoints и terminal задачи ниже не повторять.

2026-10-23: проверки завершены по просьбе пользователя. [Четыре replacement faults](plans/worker-self-update/windows-runtime-replacement-faults-gate-2026-10-23.md)
прошли60 native tests/12 groups,32 ready files twice/68 negative mutations.
Positive gate тоже повторно сверён; оба task terminal succeeded/0, активных
native/Cargo runs нет. [Актуальный handoff](plans/worker-self-update/resume-2026-10-23-replacement.md)
задаёт порядок продолжения: native updater support query и durable compatibility
до drain → activation/local health/controller confirmation → оставшиеся U2 faults
и U3/U4. Query пока local/cross only, public install off; whole updater не готов.
Исторические NEXT/ACTIVE ниже не являются заданиями к повтору.

2026-10-23: [actual Main15 replacement gate](plans/worker-self-update/windows-runtime-replacement-main-gate-2026-10-23.md)
**пройден60/0**: original custody, manifest-last/FilesMoved22, trusted FilesVerified,
post-crash history и maintenance startup refusal.20 ready files twice/44 negative
proof mutations. Следом fault matrix и activation, U3/U4. Public install off;
полный updater ещё не готов. Старые WIP/ACTIVE ниже — исторические точки.

2026-10-23: отдельный actual Main15 replacement gate подготовлен в native test /
PS/Python runner и independent verifier. Cross-check и9 Python tests passed;
Windows исполнение ещё впереди. Public install выключен.

2026-10-23 WIP: [replacement driver/actor v17/view9](plans/worker-self-update/runtime-replacement-actor.md)
добавлен: admission-before-ACK, original custody, manifest-last IO и trusted
FilesVerified → VerifyingLocal. Updater15/worker16 без новой миграции.
Actual Main positive/fault replacement gates и activation ещё впереди;
локальные/cross проверки не означают готовый updater. Public install off.
Ниже — исторические точки; текущий порядок продолжения в указанной инструкции.

2026-10-23 WIP: [consuming custody/rename library](plans/worker-self-update/runtime-replacement-custody.md)
добавлена, local workspace648/0 и обе Clippy passed. Библиотека удерживает original
process/state/instance lock и выполняет только сохранённые one-attempt moves с
physical receipt checks. Actor/driver/trusted root transition и positive native
replacement ещё впереди; public install остаётся off.

2026-10-23 WIP: [replacement ledger schema15](plans/worker-self-update/runtime-replacement-ledger.md)
добавляет immutable current/history и fresh original-guard admission, без rename
callback/actor. Package216/0, workspace647/0 и обе Clippy passed.
[Native15 metadata11/0](plans/worker-self-update/windows-runtime-replacement-ledger-gate-2026-10-23.md)
пройден,6 ready files twice; [actual retained14→15/refusal](plans/worker-self-update/windows-replacement-migration-gate-2026-10-23.md)
тоже пройден1/0,6 ready files/27 negative mutations. Positive original-custody
admission/consuming guard и rename/activation ещё впереди. Старые14 gates ниже
не переносить на новую schema.

2026-10-23: [три actual preparation faults](plans/worker-self-update/windows-runtime-preparation-faults-gate-2026-10-23.md)
пройдены40 tests/10 groups,24 ready files twice,48 negative mutations. Review
исправил недостающую сверку preimages в Python verifier,7 tests passed. Lightweight
replacement [model gate12/0](plans/worker-self-update/windows-runtime-replacement-model-gate-2026-10-23.md)
producer7f3a952 также пройден,6 ready files twice; ledger/custody/rename/
activation и U3/U4 ещё впереди. Установленный worker не менялся, public install off.

2026-10-23: цель возобновлена; [actual preparation faults](plans/worker-self-update/runtime-preparation-faults.md)
реализуются отдельным gate (occupied destination, changed candidate, owned crash
до receipt). Cross-check/all-target Windows Clippy прошли; native execution впереди.
Public install отключён, installed d052 worker не менялся.
Следующий data-only replacement record добавлен отдельно:6 local tests/обе Clippy
passed; native production rename/ledger/custody ещё впереди. Модель не входит в
выполняющийся immutable preparation fault producer.

2026-10-23: проверки завершены по просьбе пользователя; следующий агент начинает
с [подробной точки продолжения](plans/worker-self-update/resume-2026-10-23.md).
Оба последних Windows tasks terminal succeeded/0; повторно сверены22 ready files
и полные proofs, новых runs нет. Первый незавершённый блок — actual preparation
fault matrix, затем consuming custody/rename/activation, U3/U4. Public install отключён.

2026-10-23: [actual13→14 migration/refusal gate](plans/worker-self-update/windows-preparation-migration-gate-2026-10-23.md)
пройден1 integration test/6 ready files/19 negative mutations на реальном Verified-backup13
graph. Следом actual preparation fault matrix; installed worker unchanged.

2026-10-22 WIP: [preparation driver/actor v16/view8](plans/worker-self-update/runtime-preparation-actor.md)
добавлены; local workspace636/0, Clippy/TS/UI и
[actual Main40/0 gate](plans/worker-self-update/windows-runtime-preparation-main-gate-2026-10-22.md)
пройдены. Actual13→14/refusal закрыт2026-10-23; fault matrix впереди.
Это создание независимых files, не rename/activation/public install.

2026-10-22, продолжение: [U2.2 preparation library](plans/worker-self-update/runtime-preparation.md)
добавляет data FSM/ledger schema14. Targeted local10/0, package199/0 и обе Clippy passed;
[native model/ledger10/0](plans/worker-self-update/windows-runtime-preparation-gate-2026-10-22.md).
Затем разработаны copy driver/actor и пройден actual gate выше; consuming
custody/rename/activation впереди. Public install остаётся отключённым.

## Историческая точка перед началом U2.2 — 2026-10-22

[Подробный handoff](plans/worker-self-update/resume-2026-10-22.md) сохраняет эту
контрольную точку; текущий прогресс U2.2 указан выше. Тогда U0/internal U1 и U2.1
были закрыты; U2.2 имел file/FSM
primitives и manifest planner. **Первый незавершённый блок:** immutable proof
binding и durable preparation/replacement ledger, затем consuming custody и file
driver, activation, U3 и U4. Public update отключён. Windows-прогоны завершены;
старые записи «gate выполняется» ниже — исторические. Повторно отправлять их не надо.

## История контрольных точек

2026-10-22: [U2.1 final backup](plans/worker-self-update/windows-state-backup-faults-gate-2026-10-22.md)
завершён: happy/migration/refusal +23-test four-case fault gate,31 ready files,
18 negative proof mutations. [SDK/model gate9/0](plans/worker-self-update/windows-runtime-file-moves-gate-2026-10-22.md)
и manifest plan подготовили основу U2.2. Следом durable preparation/replacement
ledger и consuming custody/file driver, затем activation. Public update/U3/U4 не готовы.

2026-10-22: [runtime moves/FSM gate](plans/worker-self-update/windows-runtime-file-moves-gate-2026-10-22.md)
пройден9 native tests. Immutable full backup fault run203814-bc4258b9 выполняется;
U2.1 ещё открыт. Manifest plan — data-only; ledger/custody/driver/activation впереди.

2026-10-22: цель возобновлена. [U2.2 data FSM и review-план](plans/worker-self-update/runtime-replacement.md)
начаты без IO/installation authority (6 pure tests и обе Clippy passed).
Первый U2.1 fault gate выявил race fixture до admission; readonly ожидание
retirement исправлено, новый native repeat впереди. Отдельный four-case final backup fault runner/verifier
реализован; native execution ещё впереди. U2.1 остаётся открытым; затем U2.2
replacement/activation. Installed worker обновлять сейчас не требуется.

2026-10-21: internal U1 завершён, U2.1 final backup реализован до supervisor
v15/view7 (updater13/worker16). [Native success22 tests и migration12→13](plans/worker-self-update/windows-state-backup-gate-2026-10-21.md)
пройдены. Fault scaffold прошёл обе Clippy; отдельные runner/verifier и native
fault gate ещё впереди, U2.1 остаётся открытым. Replacement/activation/U3/U4 и
public update не готовы. Пользователь поставил цель на паузу и попросил handoff:
[точная инструкция следующей сессии](plans/worker-self-update/resume-2026-10-21.md).
Установленный worker не менялся, ручное обновление сейчас не требуется.

2026-10-20: сквозной [candidate health gate](plans/worker-self-update/candidate-health.md)
пройден —13 native tests /4 actor cases /9 groups, actual candidate build и
atomic acceptance. [Durable drain/quiesce](plans/worker-self-update/drain-quiesce.md) прошёл26 native tests,
12 groups и четыре full actor cases. Следующий U1 блок —
[exact shutdown](plans/worker-self-update/exact-shutdown.md). Самообновление U1–U4 ещё не завершено; installed runtime не менялся.

В следующем блоке реализованы data FSM и updater schema11: immutable intents,
Issued-before-delivery, append-only receipts/history и atomic root quiescence.
Добавлены consuming driver/supervisor v13 и view5: ожидание принятой очереди,
readonly reconciliation потерянного ACK и coordinated abort. 563 workspace tests,
15 завершающих worker tests, обе Clippy проверки, TS и UI build проходят;
объединённый native gate пройден (A28/C3b, отдельно от более позднего readonly
исправления статуса отмены). Shutdown/install этим не разрешаются.

Для exact shutdown добавлены [контракты и два журнала](plans/worker-self-update/exact-shutdown.md):
updater schema12 и worker schema16, immutable receipt/history, однократная выдача
разрешения после commit. Trusted exit adapter требует held OS handles и instance lock;
raw ShutdownVerified запрещён, runtime replacement пока отсутствует.
[19 native primitive tests](plans/worker-self-update/windows-shutdown-primitives-gate-2026-10-20.md)
пройдены; их область — отдельные process/state/ledger примитивы.
Результаты полного actual Main сценария приведены ниже.
Worker control v6 реализован: receipt-before-ACK и stop через existing loop;
584 workspace tests прошли для control v6. Cold readonly receipt verifier добавлен
и прошёл 11 проверок точек входа и [4 native reader tests](plans/worker-self-update/windows-shutdown-reader-gate-2026-10-21.md);
consuming supervisor v14/view6 реализован: 588 workspace tests и обе all-target
Clippy проверки прошли. Он сохраняет OS guard после driver retirement; combined
[actual Main stop/ACK/exit gate](plans/worker-self-update/windows-shutdown-main-gate-2026-10-21.md)
пройден: один integration test /4 полных случая /17 checksum artifacts.
[Supplemental delayed-lock/pre-exit-crash cases](plans/worker-self-update/windows-shutdown-supplemental-gate-2026-10-21.md)
также пройдены: два случая /13 checksum artifacts. Внутренний U1 цикл завершён.
Следующий блок — [U2.1 final consistent backup](plans/worker-self-update/final-state-backup.md)
после exact exit перед replacement. Public installation/activation и U2–U4 ещё
открыты. Raw FilesVerified/local health/controller confirmation закрыты до
появления соответствующих trusted drivers; 166 updater tests и обе Clippy проходят.

Правило каждого этапа: маленький рабочий сценарий → реальные события → понятный UI → проверка сбоев → обновление handoff. Подготовка архитектуры не считается выполнением runtime функции. Сетевую или файловую часть можно развивать независимо от UI, но этап закрывается только с видимым результатом для пользователя.

## Самообновление U0–U4 — 2026-10-17

В U1 добавлен [durable runtime baseline](plans/worker-self-update/runtime-baseline.md):
updater schema 8, atomic intent/receipt с parent transitions и запретом source effects
пока baseline pending. Baseline library / supervisor v7 прошли [175 native tests / full build / actual
Main и pins](plans/worker-self-update/windows-runtime-baseline-gate-2026-10-19.md).
[Preimages library](plans/worker-self-update/runtime-preimages.md), schema 9,
добавляет immutable intent/receipt и create-only пять copies до source effects;
[Library native gate](plans/worker-self-update/windows-runtime-preimages-gate-2026-10-19.md)
прошёл 189 tests/full build/Main/pins. [Actor v8](plans/worker-self-update/runtime-preimages-actor.md)
прошёл [193 native tests / full build](plans/worker-self-update/windows-runtime-preimages-actor-gate-2026-10-19.md); health/shutdown/install остаются впереди. Добавлен [fresh-state rehearsal](plans/worker-self-update/worker-rehearsal.md): четыре local tests passed, native gate pending; durable health approval ещё не готов. [Первый rehearsal native run](plans/worker-self-update/windows-worker-rehearsal-gate-2026-10-19.md) остановился на WAL Access denied в старом cancel fixture, добавлена bounded revalidation. [Compatibility v3/v9](plans/worker-self-update/worker-compatibility.md) реализован, native pending. Installed runtime не обновлялся.
[Schema-7 live control gate](plans/worker-self-update/windows-runtime-control-gate-2026-10-18.md)
прошёл: 165 native tests / 19 groups, full build и actual Main/pin lifetime.


По просьбе пользователя продолжение A0–A9 временно отложено.
[Анализ auto-update](analysis/worker-self-update/README.md) одобрен; уточнён
[план U0–U4](plans/worker-self-update/README.md). U0 завершён:
readonly source preflight, Update FSM и build-only staging прошли
[19 native tests и полную Windows сборку четырёх exe](plans/worker-self-update/windows-preparation-gate-2026-10-17.md).
Следующий U1 — durable source apply, updater supervisor и maintenance; полное
самообновление ещё не реализовано, bootstrap впереди после isolated gates.
[U1 journal/CAS/owner lock и bounded ZIP staging](plans/worker-self-update/durable-preparation.md)
реализованы и прошли [42 native tests/actual ZIP/candidate build](plans/worker-self-update/windows-durable-preparation-gate-2026-10-17.md).
[Worker source lease/drain/quiesce/startup guard](plans/worker-self-update/worker-coordination.md)
реализованы в trusted adapter с readonly UI; 393 local tests и
[52 native Windows tests / полный candidate build](plans/worker-self-update/windows-worker-coordination-gate-2026-10-17.md)
passed. [Source transaction](plans/worker-self-update/source-transaction.md) реализована
в trusted library: source lease, preimages, durable intents/receipts, no-clobber moves,
manifest-last и readonly crash inspection. 405 local tests и
[65 native Windows tests / полный candidate build](plans/worker-self-update/windows-source-transaction-gate-2026-10-17.md)
passed; source block завершён.
[Supervisor handoff](plans/worker-self-update/supervisor.md) реализован как trusted
library с отдельным authenticated IPC и updater schema 3; 413 local tests и
[76 native tests / полный candidate build](plans/worker-self-update/windows-supervisor-gate-2026-10-17.md)
прошли. [Worker launcher/control](plans/worker-self-update/worker-control.md)
добавлен как trusted API: exact binding, duplicate-first и supervisor-only IPC;
SourceHeld атомарно резервирует work slot, named resources пока откладывают grant.
417 local tests и [80 native tests / полный candidate build](plans/worker-self-update/windows-worker-control-gate-2026-10-17.md) passed.
[Fresh SourceControl](plans/worker-self-update/fresh-source-control.md) добавлен:
process-authenticated Info и source scan без общего mutex; 26 targeted regressions
passed; [82 native tests / candidate build](plans/worker-self-update/windows-fresh-source-gate-2026-10-17.md) прошли. Следом compiler ownership и exact shutdown.
[Managed compiler](plans/worker-self-update/compiler-driver.md) реализован как trusted
library: intent/PID/Job/resume, output и cleanup, updater schema 4. 429 workspace tests
и 10 итоговых compiler tests passed;
[95 native tests / candidate build](plans/worker-self-update/windows-compiler-gate-2026-10-18.md) прошли.
[Atomic compiler launch](plans/worker-self-update/atomic-executable.md) закрывает
CreateProcess/Job gap; [102 native tests / candidate build](plans/worker-self-update/windows-atomic-launch-gate-2026-10-18.md)
прошли. Source/build/install orchestration и
публичное включение не готовы; U1 целиком открыт.
[Source admission](plans/worker-self-update/source-admission.md) разделяет durable
Applying и одноразовый file driver permit; 26 integration tests и Windows cross-check
passed; [109 native tests / candidate build](plans/worker-self-update/windows-source-actor-gate-2026-10-18.md) прошли.
[Source preparation actor](plans/worker-self-update/source-actor.md) связывает ZIP,
source grant и source driver через internal v2; 438 workspace tests и Linux/Windows
Clippy passed; [109 native tests / candidate build](plans/worker-self-update/windows-source-actor-gate-2026-10-18.md) прошли. Build/install и public capability ещё отключены.
[Compiler admission](plans/worker-self-update/compiler-admission.md) добавляет fresh
Intent/one-use permit до dispatch; 31 integration tests и Linux/Windows Clippy passed,
native rollup пройден.
[Compiler actor/abort/output/artifact identity](plans/worker-self-update/windows-artifact-identity-gate-2026-10-18.md)
прошли 133 native tests / 15 groups, полный Rust/desktop build и четыре exe role
queries. Далее durable compiler-report/artifact-set binding и exact shutdown;
U1 целиком и public install ещё открыты.
[Compiler report binding](plans/worker-self-update/compiler-report-binding.md)
добавляет updater schema 6 и atomic completion/report receipt, не меняя legacy
wire shapes. 467 workspace tests и Linux/Windows Clippy passed;
[139 native tests / полный build](plans/worker-self-update/windows-report-binding-gate-2026-10-18.md)
прошли. Следующий блок — durable artifact-set/old runtime guards.
[Exact artifact set](plans/worker-self-update/candidate-artifacts.md) реализован
как trusted readonly verifier и atomic receipt/FSM driver, updater schema 7.
477 workspace tests и Linux/Windows Clippy passed; native gate ещё впереди.
Artifact receipt не разрешает health/install; mailbox и old runtime guards остаются.
[Artifact actor v5](plans/worker-self-update/artifact-actor.md) подключён; два
explicit fixture tests и 477 workspace tests passed. Native 149 tests/build/query
passed, overall gate failed из-за final PowerShell variable; readonly actual-file
диагностика passed, variable исправлена с portable regression. Combined native
rollup впереди; old runtime guards, health/shutdown и U2–U3 ещё не готовы.
[Known installed set](plans/worker-self-update/installed-set.md) reader/pins добавлен;
startup/control/publication ещё не подключены. [Abort read snapshot](plans/worker-self-update/abort-read-snapshot.md)
исправляет false Corrupt при concurrent commit. 482 local tests passed; Windows
artifact fixture staging исправлен после отказа extra-file guard, repeat впереди.
[Combined native gate](plans/worker-self-update/windows-artifact-actor-gate-2026-10-18.md)
clean 48b8d21 пройден: 156 tests / 16 groups, full build/four actual queries,
exact-file inspection, artifact actor и old-reader refusal. 36 ready files
проверены. Native blocks выше завершены; весь U1 остаётся открытым. Следом
normal worker startup/live identity и baseline до SourceApply, затем health/shutdown.
[Early startup guard](plans/worker-self-update/runtime-startup.md) подключён
в normal Main до journal; readonly verify-runtime и actual-main negative tests
добавлены. Positive actual Windows startup gate впереди. Publication/live identity/
baseline acceptance и весь U1 остаются открытыми.
[Actual startup/pin lifetime gate](plans/worker-self-update/windows-runtime-startup-gate-2026-10-18.md)
пройден: 159 tests / 17 groups, full candidate build, readonly Main и отдельный
actual serve/пять pins/owned idle stop. Bootstrap/install не включены. Далее
live identity/control и durable baseline до SourceApply; U1 остаётся открытым.
[Shared runtime identity DTO](plans/worker-self-update/runtime-identity-contract.md)
перенесены в protocol с exact-byte tests и generated TS, без wire/schema changes.
Control v2/live identity и durable baseline до source effects остаются следующими.
[Live runtime control v2 / supervisor v6](plans/worker-self-update/runtime-live-control.md)
реализованы как trusted readonly API, без public installation. 490 local tests и
Linux/Windows Clippy passed; positive native fixture/combined gate впереди.
Далее durable old-runtime baseline до SourceApply. U1 не завершён.
[Использование и ограничения](guides/worker-update-preparation.md).
A4.3 installed gate остаётся открытым. После самообновления вернуться к прежнему
архитектурному плану.

## Архитектурный переход A0–A9 — новая задача 2026-10-13

Этапы 0–7 ниже завершены и сохраняются как история. Пользователь поручил
объединить Actor Model + FSM, consistency/extensibility и consistency techniques,
повторно проверить план, зафиксировать его и начать выполнение.
[План и зависимости](plans/architecture-transition/README.md),
[повторное ревью](plans/architecture-transition/review.md).

- [x] A0: [frozen legacy contracts/journal tests и ADR инвариантов](plans/architecture-transition/baseline.md); 14 новых/171 workspace tests passed, Windows cross-check tests passed. Runtime не меняется.
- [x] A1: [Task FSM через прежний wire API](plans/architecture-transition/task-fsm.md); 182 workspace tests и [66 native Windows tests](plans/architecture-transition/windows-task-fsm-gate-2026-10-13.md) passed, runtime/wire/schema совместимы.
- [x] A2: [durable transitions](plans/architecture-transition/durable-transitions.md); task revisions/CAS A2.1 завершён (197 workspace/[107 native tests](plans/architecture-transition/windows-task-cas-gate-2026-10-13.md)); [A2.2 domain CAS/atomic links](plans/architecture-transition/domain-transitions.md) завершён (213 workspace/[125 native tests](plans/architecture-transition/windows-domain-cas-gate-2026-10-13.md)); [A2.3.1 authority/attempt tokens](plans/architecture-transition/ownership-context.md) завершены (229 workspace/[141 native tests](plans/architecture-transition/windows-ownership-gate-2026-10-13.md)); [A2.3.2 durable leases](plans/architecture-transition/project-resource-leases.md) завершены (245 workspace/[157 native tests](plans/architecture-transition/windows-resource-leases-gate-2026-10-13.md)); [A2.4.1 physical process reconciliation](plans/architecture-transition/physical-reconciliation.md) завершён ([164 native tests](plans/architecture-transition/windows-reconciliation-gate-2026-10-13.md)); [A2.4.2a journal inspection](plans/architecture-transition/journal-inspection.md) завершён ([176 native tests](plans/architecture-transition/windows-inspection-diagnostic-2026-10-13.md)); controlled archive/new-pair admission завершён ([197 native tests](plans/architecture-transition/windows-recovery-pair-gate-2026-10-14.md)).
- [x] A3: [discovery/read model](plans/architecture-transition/discovery.md); A3.1 завершён (286 workspace, 24 Rust/TS cases, [203 native tests](plans/architecture-transition/windows-discovery-gate-2026-10-14.md)); [A3.2a foundation](plans/architecture-transition/readiness-foundation.md) завершён ([39 native tests/desktop check](plans/architecture-transition/windows-readiness-gate-2026-10-14.md)); [A3.2b runtime producer](plans/architecture-transition/paired-runtime.md) actual paired/restart gate passed; [A3.2c UI projection](plans/architecture-transition/readiness-projection.md) [actual installed pair/projection gate](plans/architecture-transition/windows-projection-pair-2026-10-15.md) завершён; A3.3 read model — [48 native tests/desktop gate](plans/architecture-transition/windows-operation-pages-gate-2026-10-15.md) пройден; [actual installed read-model gate](plans/architecture-transition/windows-operation-pages-installed-2026-10-15.md) завершён.
- [x] A3.3: [согласованные страницы операций](plans/architecture-transition/operation-pages.md), A3.2 [actual pair/projection gate](plans/architecture-transition/windows-projection-pair-2026-10-15.md) завершён; A3.3 native candidate прошёл 48 tests/desktop compile, actual installed новый контракт проверен; A3 завершён.
- [ ] A4: global resources, очередь и fairness для нескольких проектов; первый пакет [A4.1 work slots](plans/architecture-transition/work-scheduling.md) прошёл [57 native tests/desktop check](plans/architecture-transition/windows-work-scheduling-gate-2026-10-15.md); [actual installed pool/MCP/UI gate](plans/architecture-transition/windows-work-scheduling-installed-2026-10-15.md) завершён, A4.1 закрыт. [A4.2 backlog/pause](plans/architecture-transition/queue-control.md) завершён: 337 workspace tests, [69 native tests/desktop compile](plans/architecture-transition/windows-queue-control-gate-2026-10-16.md) и [actual installed queue/MCP/UI gate](plans/architecture-transition/windows-queue-control-installed-2026-10-17.md) прошли. [A4.3 resource sets](plans/architecture-transition/resource-sets.md): 347 workspace tests и [80 native tests/desktop compile](plans/architecture-transition/windows-resource-scheduling-gate-2026-10-17.md) прошли, новый installed контракт ожидает update.
- [ ] A5: local actor pilot и постепенная миграция owners.
- [ ] A6: Run/Step и contract-tested one-shot executor/report.
- [ ] A7: outbox + inbox для нужного durable receiver (условный этап).
- [ ] A8: два независимых потребителя и automatic paired native gates.
- [ ] A9: расширение FSM/контрактов; дальнейшие изменения по доказанной потребности.

A0, A1, A2.1, A2.2, A2.3.1, A2.3.2 и A2.4.1 завершены;
A2.4.2a завершён: local/browser и
[176 native Windows tests](plans/architecture-transition/windows-inspection-diagnostic-2026-10-13.md) passed;
следующий пакет A2.4.2b — controlled restore/new incarnation/admission. До native gates process/runtime
изменения не считаются готовыми. Пользователь выбрал
[новую пару/журнал с архивом](plans/architecture-transition/restore-admission-options.md).
[A2.4.2b.1 request fence](plans/architecture-transition/fenced-requests.md) завершён:
271 local tests, browser gate и [186 native tests/desktop compile](plans/architecture-transition/windows-fenced-request-gate-2026-10-13.md) прошли.
Archive preparation и physical cutover [b.2/b.3](plans/architecture-transition/recovery-pair.md)
завершены: 280 local tests, cross-checks/browser и
[197 native tests/desktop compile](plans/architecture-transition/windows-recovery-pair-gate-2026-10-14.md) passed.
Полный A2 закрыт после финального ревью; далее A3 discovery/read model.

## Этап 0 — подготовка репозитория

- [x] Отдельный Git repository, Rust workspace и границы будущих модулей.
- [x] DTO для recipe/direct task и события, CLI help/describe.
- [x] UI prototype с fixtures: timeline, параметры, output, успех/ошибка/потеря свежести.
- [x] Принятые локальные решения, HGFS схема, UX, ADR и handoff.
- [x] Lockfiles и воспроизводимые проверки.

Результат этапа 0: сборка Rust/UI и contract tests проходят; на момент его завершения runtime отсутствовал. Текущее состояние следующего этапа описано ниже.

## Этап 1 — одна команда и реальный журнал на Windows

Статус 2026-10-05: этап завершён по [Windows отчёту и пяти подтверждённым native UI шагам](done/stage0-7/stage1/windows-gate-2026-10-05.md). Worker объявляет implementedCapabilities `task_submit`, `task_cancel`; на момент коммита этапа 1 controller оставался scaffold. Настройки validation и desktop находятся рядом с exe, отчёты — в shared. Дополнительно исправлено сохранение формы команды при повторном открытии окна; этот UI fix проверен Rust storage tests, browser integration и дополнительно подтверждён пользователем на Windows вместе с dev:ui.

- [x] Runtime config loader: один project, разрешённые recipes/direct, относительный cwd, timeout. Версионирование и actionable validation errors.
- [x] Windows worker выполняет команду в локальном fixture project. Direct executable одновременно читает stdout/stderr; recipe использует тот же service.
- [x] Durable task ID, accepted до ACK, idempotency key/payload check, локальное task/output хранилище. State transitions и recovery записываются явно.
- [x] User-scoped named pipe и Tauri shell; desktop только вызывает worker и читает read model. SID + WinBridge instance ID.
- [x] UI показывает источник, executable, массив аргументов, cwd, timestamps, output и exit code; реальные данные через adapter.
- [x] Управляемое дерево процессов, timeout и cancel. Закрытие UI не останавливает worker.

Критерий: пользователь выполняет fixture команду, видит успех и ошибку, отменяет длительный process tree; дубликат запроса не создаёт второй процесс. Сбой после запуска виден как interrupted/indeterminate без молчаливого повторения. Windows проверка обязательна; Linux cross-compile её не заменяет.

## Этап 2 — Debian управляет Windows по локальной сессии

Статус 2026-10-05: [настоящий Windows runtime gate](done/stage0-7/stage2/windows-session-gate-2026-10-05.md) пройден: шесть групп, включая Debian→Windows execution, cancel, dedup и reconnect с тем же PID. Подключение и три новых native UI наблюдения подтверждены пользователем; этап 2 завершён. Адреса VMnet8: VM 192.168.44.203, host 192.168.44.1. [Windows инструкция](validation/windows-session-validation.md).

- [x] Plain `ws://` listener на выбранном локальном IP Debian, Windows outbound connection; никаких TLS/certificates/relay.
- [x] Входящие IP не блокируются; источники активности записываются в JSONL с ротацией и видимыми ошибками записи (решение 2026-10-05).
- [x] Handshake, wire version, реализованные capabilities, instance/generation IDs, request/response correlation, bounded message sizes.
- [x] CLI для submit/get/cancel и подписки на события через controller. Controller не выдаёт собственный статус за подтверждение исполнения worker.
- [x] Worker сохраняет задачу перед подтверждением; reconnect восстанавливает её по прежнему ID и события по sequence.
- [x] UI отдельными состояниями показывает controller online/offline, последнее подтверждение задачи, очередь и возможные действия.
- [x] Постоянный Windows runtime из локального settings JSON; повторная подготовка сохраняет configs/форму/state, диагностика различает TCP/WebSocket/Hello.

Критерий: агент Debian запускает задачу в Windows fixture project, получает output; разрыв соединения не останавливает процесс, reconnect возвращает результат, повтор key не дублирует исполнение. Неверная версия и timeout имеют понятную причину в UI.

## Этап 3 — готовый snapshot через VMware Shared Folders

Статус 2026-10-06: [Windows runtime gate пройден](done/stage0-7/stage3/windows-snapshot-gate-2026-10-06.md), 19 actual import/build/run/cancel/junction checks. Пользователь подтвердил imports/preview/build/failure/history в native UI. По его замечаниям добавлены shared package select, сохранение delivery form и понятный отказ сборки непроверенного snapshot; 48 Rust tests и реальная UI integration проходят. Этап 3 завершён по решению пользователя: UI исправления приняты, подробная ручная проверка отложена. Это принятие не обозначается как дополнительный выполненный Windows UI test.

- [x] Общие Rust/TypeScript DTO manifest/ready и bounded metadata preflight: Windows пути, case collisions, file/directory conflicts, hashes/size syntax и byte counts.
- [x] Выбор mapping `/mnt/hgfs/shared` ↔ `K:\VMs\shared` и отдельного Windows workspace root.
- [x] Capture точного commit SHA из Git objects; dirty working tree исключён.
- [ ] Immutable dirty snapshot с явной маркировкой — отдельное последующее расширение.
- [x] Namespace, `.part`, manifest, ready marker, RPC import и локальный verified staging.
- [x] Policy/actual-byte/link guards и Windows compile для path/size/hash/traversal/collision/reserved names/incomplete transfers.
- [x] Настоящий Windows runtime gate: import/build/run/cancel/NTFS junction, 19 checks.
- [x] Пользователь принял новые UI исправления; подробные ручные наблюдения отложены по его решению.
- [x] Workspace lease: импорт и сборка одного поколения не пересекаются. Runtime configs и пользовательские данные сохраняются отдельно.
- [x] UI preview состава snapshot, bytes/files, progress и причины отказа, адреса обоих представлений shared folder.

Критерий: build запускается из verified Windows snapshot; потеря mount или обрыв записи не ведёт к сборке неполных исходников. Повтор import не создаёт другую задачу. Не изменяются файлы вне namespace/зарегистрированного workspace.

## Этап 4 — первый полный цикл разработки

Статус 2026-10-06: этап завершён. Автоматический Windows прогон `20261006-163840-a97da6e3` подтвердил 41 native test, native desktop check и полный import/build/artifact/start/IPC/log/dedup/stop на отдельном Netwatch clone. [Доказательства](done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md), [изоляция](validation/netwatch-validation.md). Editable profiles/drafts, output filters и artifact UI проверены browser integration; подробные ручные Windows UI наблюдения отложены пользователем. Пользователь обновил runtime до 523fc5b; source commit и features подтверждены read-only запросом 2026-10-07.

- [x] Workflow fixture: выбрать SHA → publish/import → build → start → stdout readiness → log tail.
- [x] Принятый профиль и step IDs в journal; ошибка сборки блокирует запуск следующего шага, dedup не создаёт повторный процесс.
- [x] Process registry для приложений, отдельно от одноразовых build tasks; явный stop и indeterminate recovery.
- [x] Log read/tail: byte offset, UTF-8, truncation/rotation/prefix rewrite, page/UI квота. Task output сохраняется в journal; файл лога читается у приложения.
- [x] UI workflow steps/timeline, ссылки на build/start tasks, app status и связанный лог; принятие request не считается завершением.
- [x] Автоматические Windows regression через старый fixed worker, isolated source/build/state и checksum-verified shared report.
- [x] Netwatch profile с отдельными config/logs/IPC и фактическим health/PID probe; production экземпляр не использовался.
- [x] Отдельные editable именованные profiles/drafts, фильтрация output и artifact catalog; Rust storage и browser integration прошли.

Критерий: на примере Netwatch агент получает стабильный snapshot, собирает на Windows, запускает его, читает лог и видит все шаги в UI. Падение приложения и провал readiness показываются отдельно от успешной сборки.

## Этап 5 — внимание пользователя и нормальная ежедневная работа

Дополнение 2026-10-10: исправлены ложная dirty configuration после undo проекта,
неограниченное ожидание закрытия и no-op popup hide. [Диагностический desktop log](guides/desktop-diagnostics.md)
и [42 Windows tests + два native UI запуска](done/stage0-7/stage5/windows-desktop-diagnostics-gate-2026-10-10.md)
проверены автоматически; это исправления завершённого этапа, отдельный manual gate не требуется.

План и порядок реализации согласованы 2026-10-07: [объяснение этапа 5](done/stage0-7/stage5/stage5-plan.md).
Первый сценарий — читаемый PowerShell и долговечный список внимания с acknowledgement.

Первый сценарий реализован: [журнал внимания и границы](guides/attention.md). Task outcomes,
dedup и acknowledgements сохраняются у worker; UI раскрывает EncodedCommand и
предлагает task links/explicit acknowledgement. [Tray/popup/sound/preferences](guides/companion.md)
реализованы и [прошли автоматический Windows gate](done/stage0-7/stage5/windows-companion-gate-2026-10-07.md):
21 core + 9 attention tests, два native UI запуска, 16 verified artifacts.
[Первый прерванный прогон](done/stage0-7/stage5/windows-companion-attempt-2026-10-07.md) остаётся историей;
исправлены обозначение тестового окна, защита close и публикация отчёта.
Workflow/app attention реализованы по [плану](done/stage0-7/stage5/workflow-attention-plan.md) и
[прошли автоматический Windows gate](done/stage0-7/stage5/windows-workflow-attention-gate-2026-10-08.md):
19 backend tests, actual popup → точный workflow → явный UI ACK, 12 verified artifacts. История/диагностика реализованы, [автоматический Windows gate](done/stage0-7/stage5/windows-history-gate-2026-10-08.md) пройден. [MCP adapter](guides/mcp.md) также прошёл actual Windows gate; projects/connection settings и согласованная output retention также прошли [native gate](done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md).
[Автоматические Windows проверки](done/stage0-7/stage5/windows-attention-gate-2026-10-07.md): 7 native tests
прошли через установленный worker без повторения завершённого workflow gate.

[Ревью](done/stage0-7/stage5/review-attention-2026-10-07.md) исправило fresh running после failed result commit.
Сценарий [tray/popup/sound/preferences](done/stage0-7/stage5/tray-plan.md) завершён; план был зафиксирован
коммитом `bbba964` до реализации. План workflow/app attention зафиксирован `aeae684`
до реализации; сценарий завершён. [История/диагностика](guides/history-diagnostics.md) добавляет keyset pages, старые IDs, keyboard actions, redacted editable task report и storage usage; [Windows gate](done/stage0-7/stage5/windows-history-gate-2026-10-08.md) пройден: 13 tests, actual UI и 17 artifacts. [MCP](done/stage0-7/stage5/mcp-plan.md) и settings/output retention также завершены.

- [x] Attention event → Tauri popup/tray badge/встроенный звук. Dedup, rate limit, quiet hours, severity.
- [x] Открытие нужной task page из уведомления, acknowledged state; pending attention при закрытом companion.
- [x] Workflow/application attention: typed targets, единый сигнал родительского workflow, атомарная запись и native target navigation gate.
- [x] История task/import/workflow/application, поиск/фильтры/keyset pages, точные старые IDs, быстрые действия и keyboard shortcuts.
- [x] Projects/connection settings, навигация разделов/controls с клавиатуры и защита незаписанных правок.
- [x] Editable task diagnostic с redaction, default omission, create-only save и проверкой readback/SHA-256.
- [x] Output paging/quota, recovery без UI и read-only размеры журнала.
- [x] Согласованная output retention policy и настройки: preview/atomic cleanup, protected targets, durable results и opt-in automatic cleanup. Metadata/history/dedup не удаляются.
- [x] [MCP stdio adapter](guides/mcp.md): 30 typed tools над теми же RPC/handler; contract, actual Unix lost ACK и [Windows gate](done/stage0-7/stage5/windows-mcp-gate-2026-10-08.md) прошли.

Критерий: пользователь понимает, что сделал агент, получает уместный сигнал, открывает причину и может продолжить работу. Рутинные разрешённые действия не требуют повторяющихся confirmations.

## Этап 6 — скорость и объёмы после измерений

Статус 2026-10-10: этап 6 завершён [automatic chunk/resume gate](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md). Первый сценарий: [воспроизводимый baseline](done/stage0-7/stage6/stage6-plan.md) существующей доставки,
Windows import/build, HGFS tree/archive и объёмов output/journal. План зафиксирован
после ревью этапа 5; выбранные расширения теперь измерены и проверены.

- [x] [Baseline HGFS tree/archive и time-to-build](done/stage0-7/stage6/performance-baseline-2026-10-09.md) с fixture параметрами: 3 повтора, 59 actual Windows операций, output/control/cancel/storage evidence.
- [x] [Первая измеренная оптимизация capture](done/stage0-7/stage6/performance-capture-gate-2026-10-09.md): bounded Git batch, 13.58× в 9 matched pairs, identical hashes и 21 Windows operation без изменения wire/ready.
- [x] [Incremental full/delta](done/stage0-7/stage6/windows-delta-gate-2026-10-09.md): три fresh Debian → Windows one-file commit; ZIP 2 367 994→193 bytes, publication/import timings и actual build/output/control/cancel/attention.
- [x] [Delta от verified snapshot](guides/snapshot-delta.md): schema 2/pinned base/local source cache, full target verification, CLI/MCP/UI; 40 native tests и cross-host gate прошли. Старые поколения сохраняются; удаление отражается новым составом.
- [x] [Пакетное чтение output и виртуализация строк](done/stage0-7/stage6/windows-output-gate-2026-10-10.md): bounded pages/cursors/tails, не более 28 DOM rows; 11 Windows tests, три native IPC пары, hashes/dedup/restart/control и browser integration.
- [x] [Chunk/resume больших объектов](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md): manifest-pinned 1 MiB blocks, explicit import после cancel/crash, exact Debian pins и offline build; 39 native tests, shared ZIP bytes на 75% меньше.
- [x] Оценена необходимость SocketDelivery/дальнейшей list virtualization: текущие gates не выявили необходимости. Shared resume и bounded output/history достаточно для измеренных сценариев; другой adapter и дополнительная виртуализация выбираются при новых данных.

Критерий: измеряемое улучшение выбранного сценария, отсутствие регрессии cancel/attention latency, корректное восстановление после прерывания. Не менять транспорт ради обещания абстрактной скорости.

## Этап 7 — каркас для следующих проектов

Первый вертикальный сценарий реализован: [общие recipe profiles](guides/project-profiles.md),
статический CommandHandler/runner, metadata и общий JournalTimeline. Task origin и
workflow recipe definitions сохраняются до ACK; queued workflow с изменённым
рецептом не начинает новые import/build/start. [Automatic Windows gate](done/stage0-7/stage7/windows-profiles-gate-2026-10-10.md)
passed: 68 native tests, five verified artifacts; 152 workspace tests и full
browser integration проходят. [Второй прикладной сценарий](done/stage0-7/stage7/windows-projects-gate-2026-10-10.md)
прошёл: isolated Netwatch и WinBridge используют общий профиль, 59 native Rust
tests и оба application cycles, 15 verified artifacts. 153 workspace tests;
**этап 7 завершён 2026-10-10**.

- [x] Статические handler interfaces runner/workspace-sync закреплены [контрактами](guides/project-integration.md) и проверены на двух реальных проектах; ACK/journal/processes остаются у worker.
- [x] Project profiles и reusable recipes; общий UI renderer событий и capability metadata.
- [x] Второе приложение: WinBridge worker рядом с isolated Netwatch, без изменений transport/RPC и копирования task service; actual IPC/log/artifacts/managed stop/restart подтверждены.
- [x] Необходимость SDK/package versioning оценена: оба consumer используют статический workspace contract, отдельный SDK не нужен. Динамические plugins и другие ОС — отдельные запросы.

Критерий: следующий проект подключается конфигурацией/модулем, сохраняет journal/cancel/recovery и получает понятный UI.

Этап 5 завершён 2026-10-09: [Windows settings/retention gate](done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md)
подтвердил 36 backend tests, два native UI запуска и 19 artifacts.
Пользователь выбрал manual restart для config и включаемую automatic output cleanup;
история/ключи/attention/snapshots/reports остаются. Все сценарии этапа 5 прошли
автоматические Windows gates; новый ручной gate не назначается.

## После основного roadmap

Этапы 0–7 завершены. Новый цикл A0–A9 описан в начале этого документа.
Следующий проект подключается по [готовой инструкции](guides/project-integration.md)
через registry/recipe profile и прежний workflow/journal/cancel/recovery. Native
evidence и актуальный source archive находятся в handoff. Новые возможности
выбираются по следующей пользовательской задаче: dirty snapshots, source-cache
cleanup или другой delivery adapter остаются отдельными расширениями с собственными
критериями. SDK/plugins без потребителя не вводить. Старые snapshots/reports/history
не очищать; после reboot заново проверять связь и build identity.

### U1: compiler mailbox и coordinated abort — 2026-10-18

[Реализация и границы](plans/worker-self-update/compiler-actor-abort.md): внутренние
v3 build / v4 abort, config 3, updater journal 5. Compiler Intent до ACK, один
execution permit; source/Job/output proof до worker Release. Runtime apply,
public update и bootstrap не включены. Native gate этого блока ещё не выполнен.


## U1: compiler output и исправление PowerShell paths — 2026-10-18

[Bounded output](plans/worker-self-update/compiler-output.md): typed readonly byte
pages, File ID cursor, live/terminal distinction и retained SHA proof. 18 compiler
regressions, explicit PowerShell actor и 8 worker-control tests passed locally.
Linux/Windows Clippy и contracts/typecheck passed для output; финальная проверка
path fix и native повтор ещё впереди.

Первый native run clean d2c7fdd `20261018-093814-ca0a18e6` прошёл 124 tests,
но compiler actor завершился exit 1; gate НЕ пройден, candidate build не выполнен.
Отдельная Windows диагностика `f5420c13-3c16-4836-bd70-7a5f662d3dda` подтвердила:
PowerShell принимает K:\… и отклоняет эквивалентный \\?\K:\… с
AuthorizationManager check failed. Fixed resolver теперь передаёт tool paths без
verbatim prefix, сохраняя canonical cwd/source identity и запрет UNC/devices.
Failure fixture сохраняет bounded stdout/stderr и делает coordinated abort.
Незавершённый validation Task `37321e75-f4fa-49bf-ad2b-ed4265dbc2f9` отменён явно;
worker подтвердил cancelled, idle 0, complete coverage и прежний generation.
Installed source 1f3aa9f / runtime/config/journal не менялись; bootstrap не нужен.
Автоматический повтор разрешён пользователем. A4 остаётся на паузе.

## U1: artifact identity/probe и абсолютные tool paths — 2026-10-18

[Контракт](plans/worker-self-update/artifact-identity.md). Четыре exe получают
--artifact-info; static marker проверяется до запуска legacy desktop. Bounded
Job/stdout/stderr query не читает config, не пишет state, не разрешает installation.
Build-only script проверяет четыре role/source identities внутри compiler tree
и сверяет SHA с payload. Durable artifact-set receipt/health/shutdown ещё впереди.
11 preflight tests и worker/fixture metadata checks passed; Linux/Windows Clippy
all-targets passed. Native проверка desktop/query и полный повтор ещё впереди.

Второй native run 6ae7382 `20261018-095232-7c0952a3` завершился failed/1
штатно с report.ready.json: 20 passed tests, один новый unit test выявил допустимый
для общего prefix helper drive-relative `K:relative`. Tool resolver теперь
отдельно требует is_absolute; schema-3 PowerShell executable тоже.
Windows free before/after 25,076,011,008 / 25,038,508,032 bytes; installed worker
idle 0, прежний generation, recovery/maintenance errors нет. Первый run и
PowerShell AuthorizationManager diagnosis сохранены ниже. Gate ещё НЕ пройден.

U1 native 06211fe: 133 tests / 15 groups и полный Rust/desktop compile passed;
финальная identity-проверка отказалась от Cargo hard link. Create-only payload
copy перенесён перед probes; hard-link isolation portable test passed. File ID
review и UTF-8 wrappers также добавлены. Полный gate и U1 ещё открыты.

## U1: health journal и supervisor v10 — 2026-10-20

[Health-driver](plans/worker-self-update/candidate-health.md): schema 10, fresh
one-use intent, managed config/rehearsal helpers, immutable PID/cleanup, atomic
CandidateAccepted и abort после quiescence. Внутренняя реализация проверяется;
сквозной native health gate и public update/exact shutdown/install ещё впереди.
U1 не помечается завершённым.

Сквозной actor run20261020-103249-4030e35e выявил несовместимость rehearsal
с собственным строгим health-контрактом: обычный readiness banner попадал в
stderr. Обе проверки завершались exit0, но approval был правильно отклонён.
Исправляется вывод пробного worker, добавлена проверка actual exe; повтор gate
впереди. [Drain/quiesce](plans/worker-self-update/drain-quiesce.md) — следующий
запланированный блок после health gate, ещё не реализация.

Подготовительный контракт следующего блока: supervisor v11 / worker control v4
читает immutable command receipt без повторной мутации. Local547 tests/doctests,
Linux/Windows Clippy и UI checks passed; native v11/v4 ещё требуется. Это не
durable drain driver и не разрешение остановить установленный worker.

Run20261020-161358-d2e84c59 прошёл три отмены, но выявил неверный pre-transition
guard внутри уже post-transition atomic acceptance hook. Исправлен точный новый
parent proof; SQLite regression проверяет rollback чужих bindings. Ревью также
исправило compiler-slot deadlock при Draining/capacity1. Local549 tests/doctests
и Linux/Windows Clippy passed; объединённый native повтор всех новых проверок
и четырёх health actor cases ещё требуется. U1 остаётся открытым.

Supervisor v12 / worker control v5 подготавливает logical drain observation с
queue pause/revision и coherent DB snapshot. Local551 tests/doctests, Clippy и UI
checks passed; native v12/v5 ещё требуется. Нулевые счётчики не подтверждают
physical quiescence. Durable drain driver и exact shutdown остаются впереди.


2026-10-27: в feature готовится worker0.2.0 с topology1–4 и schema19.
[Native multi-controller gate](plans/multi-controller-native-2026-10-27.md) подготовлен;
Windows proof/release пока не объявлять. Main25 profiles используют отдельные pins,
installed0.1.1 остаётся прежним. Новая версия не включает public updater install.


2026-10-27: [Windows multi-controller gate](plans/multi-controller-native-2026-10-27.md)
пройден на0.2.0/source48c834c:4 independent controllers,5 tasks, native cold17→19,
independent16 negative audits/15 files twice. User разрешил новые TCP ports;
installed0.1.1 не заменялся. Source19 Main25 и выпуск ZIP ещё впереди.
