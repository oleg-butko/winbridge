# Следующий координатор через исходный keeper

**Native exclusion gate passed:** source5f543f0,
run20261030-112100-b8658b84/taskb446d8d3-06a8-4023-9912-d079f75e81d1.
3 exact tests passed: production pin membership/exclusion/exit и два OS helper cases.
74 ready files twice verified,36 OS-proof negative audits. Report
ba8de27d5e4d3b50b0c444866a130a8eae6e35d6c2a423ae1fd9bf44de1fac95;
two-generation proof90d129db6a576bffa41c44a78b50988f76eb1706ba727838384bf54108fd47a6,
keeper-loss proofc2e5487b1992ce35d4038e58b85c6da2b432e761b1e2e95cd584d1e0ed6c5378.
Windows14,119,886,848→14,822,199,296bytes; installed worker unchanged.
Helpers завершены, worker оставить, desktop не нужен/окон нет. Это OS boundary
proof с реальными helpers, не два полноценных updater workflows; durable bootstrap
и mixed-version протокол ещё впереди. Не replay ни failed, ни passed task.

2026-10-30/.204. Продолжение [keeper retirement](keeper-runtime-2026-10-30.md).
Это план и первый platform primitive, не реализованный launch protocol.
Installed worker source48/gen946 не меняется; public install off.

## Текущая граница

Main30/19/23 и actual23→24 copy migration прошли. Main31/19/24 passed:
run20261030-103611-811af539/taskb1cfa7b0-57b5-457e-8296-56c7629a2496,
producerd5a7bab→target830ccb9. Наблюдать тот же task, не replay.
Это отдельное доказательство закрытия writer/освобождения authority при живом Job;
73 files twice/19 keeper negative audits. Следующее поколение ещё не проверено.

Production `Worker::launch_supervisor` уже отказывает worker в external Job.
`InfrastructureChild::spawn` также проверяет собственный execution domain.
Activated worker находится в activation Job keeper, отличной от task execution
domain; проверки только последней недостаточно для делегированного запуска.
Существующий cold-worker отказ сохраняется. Нельзя добавлять breakaway или ослаблять
KILL_ON_JOB_CLOSE/limits. OS experiment двух поколений — вспомогательный опыт,
он не заменяет durable admission и настоящих updater actors.

## Первый реализованный примитив

`PinnedProcess::require_outside_job` query-ит конкретную существующую Job через
тот же exact retained process handle, проверяя liveness до и после наблюдения.
Положительная и отрицательная проверки используют общий query primitive. Missing
Job, access denied, invalid name, exited process и unsupported platform дают ошибку,
не положительное outside evidence. Ни Job, ни процесс не создаются; query handle
немедленно закрывается, limits и полномочия не меняются. Результат не является
разрешением запуска и не сериализуется как capability.

Existing Windows exact-pin test усилен: contained child отвергается как outside,
parent принимается outside только live existing Job; missing/invalid Job и exited
pin отвергаются. OS spike дополнительно проверяет actual parent до CreateProcess
и actual next child после него вне original Job и old task domain. Независимые
низкоуровневые member observations остаются вторым oracle.

Linux infrastructure3 tests/1 ignored и Linux/MSVC platform all-target Clippy passed.
Windows execution нового primitive пока не выполнялся. Новый explicit gate:
`run-update-bootstrap-exclusion-native.py`, reader
`verify-update-bootstrap-exclusion-native.py`. Он выбирает exact production-pin test
и два OS cases, имеет отдельные namespace/report kind/idempotency key. Старый
lifetime-spike default по-прежнему требует ровно2 commands. Selector3 tests и прежние
8 OS reader tests passed; synthetic tests не являются native evidence.
Запускать gate после завершения Main31/освобождения cache. Installed worker оставить,
desktop не нужен;3 локальных tests без новых окон и сети, hashed EXE только local OS.

## Следующий вертикальный сценарий

1. Разделить logical current worker и physical spawner keeper, сохранив старые
   SupervisorSpec/handshake semantics. Новая durable delegation имеет собственный
   versioned контракт/ledger; cold rows никогда не создают live owner или retry.
2. Bootstrap принимает только исходный actual worker peer (PID/birth/host/session,
   membership, loaded generation/context/Active binding), не JSON PID. Фабрика
   запроса принадлежит live worker с freshly admitted update; project command не
   может отправить произвольный вне-Job spawn. Используется pinned trusted updater
   установленного runtime, фиксированные args/config/working directory; нет general
   path/argv API. First round проверяет два поколения с тем же SDK.
3. Старый keeper после writer close не открывает SQLite и не принимает новую
   updater authority. Minimal bootstrap version независима от будущего journal
   format. Одна durable admission и один in-memory creation attempt. Unknown create
   или потеря ответа не дают повторного spawn; readiness выясняется у того же actor.
   Независимость child устанавливается до отправки одноразового start/handoff.
4. До CreateProcess actual keeper обязан быть вне original activation Job и old
   task domain; actual child проверяется там же до acceptance. Query errors остаются
   unknown. Новый координатор получает следующий owner epoch через настоящий OS-lock,
   а current worker подтверждает handoff как logical parent.
5. Одновременно максимум один coordinating actor и два keeper. Это ограничение
   поколений процессов, не числа Debian-контроллеров. В ledger нужны immutable
   связи поколений; live parent keeper проверяется actual pinned handle. Занятый
   предыдущий keeper запрещает создание третьего, без silent adoption/cold restart.
6. Старый keeper выходит только после known ready следующего actor, actual old
   worker exit, positive empty того же original Job и завершения readers. Exit root
   не заменяет Job.empty. Keeper loss закрывает именно свой original Job; новая
   цепочка вне него должна выжить. Повторные команды только наблюдают тот же attempt.

## Смена версии updater до U4

Same-SDK round не доказывает обновление на будущую SQLite schema. До публичного
включения нужна явная стратегия: fresh updater image/descriptor negotiation и
стабильный versioned worker bridge, который не заставляет старый worker/keeper
интерпретировать будущую внутреннюю schema. Нельзя убрать exact-schema проверки
механически, объявить future SDK совместимым по одному номеру или переложить это
на JSON receipt. Новая schema требует consistent retained-copy/backup/old refusal
proof. Native mixed-version gate обязателен отдельно от двух same-SDK поколений.
После этого остаются U2 fault matrix, U3 UI/attention и U4 actual installed update.

## Первый exclusion run: исправлен setup exact-pin test

Source09328f6, task473a983b-bb4c-4985-8dae-05f5a2b05aa0,
run20261030-111128-d13660f4 failed на первом test после0.13s; два OS cases не запускались.
2 ready files twice verified, report12601e60cdfbf13faafdbaea84467858fdf6ed1a4eb6814d22853711d921244c.
`ManagedChild::prepare(None)` создаёт unnamed Job и намеренно не выдаёт recoverable
ProcessIdentity. Existing test затем unwrap-ил её перед membership assertions.
Setup исправлен на уникальный named Job. Сам query primitive и строгие проверки
не менялись, MSVC platform all-target Clippy passed. Это не положительный native
proof; требуется новый isolated run/key. Старый task не replay.
Installed worker unchanged; no windows, Windows directories не удалялись.

## Выбранное направление fresh updater и устойчивого bridge

Первый same-SDK delegation round проверяет lifetime, но не решает будущую schema.
Рекомендуется запускать свежий updater, построенный из нового pinned snapshot,
до изменений пользовательского source/runtime. Старый worker может выполнить этот
stateless build как обычную управляемую задачу, проверить SHA/manifest и frozen
bootstrap descriptor v1 в своём execution domain. Только после успешного build
создаются durable update/delegation intent и independent coordinating actor.
Ошибочный build оставляет работающую установку без изменений.

Old keeper знает только actual current-worker peer, исходный Job и стабильный
bootstrap envelope: fixed helper location, image/config identities, hashes,
logical parent/physical spawner, correlation/one attempt. Он не открывает новый
journal и не проверяет future schema через старую библиотеку. Fresh updater сам
выполняет проверенную миграцию и получает следующий owner; stable ready/handoff и
worker-control bridge не должны заставлять old worker читать будущую SQLite.
Глобальный epoch после migration может увеличиться дважды (migration invalidation
и new-owner acquisition); нельзя подменять это старым parentEpoch+1 либо переписывать
исторические owner fields. Ready нужно проверять через versioned live bridge,
связанный с actual child, preaccepted root и migration result.

Нужен отдельный фиксированный bootstrap query/handshake, а не расширение frozen
SupervisorSpec или снятие exact-schema guards повсюду. Public updater capability
пока закрыта. First same-SDK round и native mixed-version migration/control round —
разные обязательные gates. Старый installed source48 ещё не реализует этот bridge;
его обновление понадобится перед реальным U4, после изолированных проверок.

Очередность source работы: immutable delegation binding/ledger и known-commit
permit → actual worker-to-keeper peer/one creation → stable child handoff/ownership
→ known-empty retirement/max2 → fresh-updater/worker bridge и mixed-version gate
→ auto workflow/U2 fault matrix/U3 UI/actual installed U4. Только выполненные
сценарии помечать готовыми; bootstrap effect сейчас не подключён.

## Journal-only intent model

`apps/update/src/bootstrap.rs` добавлен без schema bump (source19/24).
BootstrapIntent — сериализуемые данные будущего ledger, не wire request и не permit.
Он связывает предыдущий KeeperBinding/selection hash, новый Requested root0,
logical worker/physical keeper, свежего owner, clean updater build/source pin,
image/config/output identities и old task domain. Проверяется причинная связь с
immutable KeeperSelection/Confirmed receipt, текущими installation/scope/build/
loaded worker config/source baseline и порядком clock/epoch. Updater config и
worker config — разные файлы, их hashes не приравниваются.

Helper names фиксированы: `update/<updateId>/supervisor/winbridge-update.exe`.
Payload не содержит произвольные executable/argv/runtime path или plaintext token.
Canonical intent hash — только ключ данных. validate/deserialize/helper_components
не создают process/Job/authority; live factory обязана заново проверять actual peer,
filesystem pins, known COMMIT и отсутствие занятых предыдущих keeper.

5 portable tests passed, включая causal check с настоящими SQLite keeper rows
(synthetic process metadata),14 malformed bindings и6 historical rebinding/epoch/
clock cases. Linux/MSVC updater all-target Clippy passed до дополнительной проверки
opaque archive basename; после неё5 tests/fmt/diff passed. Нативного delegation
execution/admission, нового journal table или advertised bootstrap capability нет.
Далее атомарная reservation/supervisor intent, immutable payload/idempotency,
non-Serde known-commit permit и только затем actual keeper creation/handoff.

2026-10-30 после пользовательского restart установленный worker по-прежнему0.2.0/
source48c834c, новый generationf9daa176-4125-4ee2-ad0d-aa704f59815b. Обе связи.203/.204
connected, primary203, journal incarnation5a8eb173 прежний, active0/errors=null.
Исторические native reports выше правильно фиксируют generation946 до restart.

## Atomic reservation — source19/25

Schema25 добавляет immutable `bootstrap_reservations`: update ID, новый launch ID и
previous launch ID уникальны и связаны внешними ключами. Reservation содержит
Requested root0/BootstrapIntent и исходный supervisor Intent0. Общая transaction
primitive сохраняет Intent/receipt/reservation и проверяет readback в одной SQL
транзакции. Caller владеет COMMIT. Два live checkpoint до/после записи предназначены
для будущей consuming factory; текущие SQL tests используют synthetic process data.
Existing exact payload возвращается только как data, без checkpoint/spawn. Changed
payload/parent/owner/helper path отвергается. Нет сериализуемого creation grant.

Особое ревью: outgoing A→B ищется по previous_launch_id отдельно от incoming
reservation, создавшей A. Один OR по всем трём полям смешал бы поколения и мешал
B→C. Read API разрешает только update/child launch; regression сохраняет это правило.
Будущая factory должна проверить actual pinned предыдущего keeper, а не вывести
его exit из row. Максимум2 касается поколений, не Debian agents.

Повышение schema не применяется к установленному worker. Новый fixed profile
Bootstrap24To25 использует actual сохранённый Main31/24, сохраняет original selection
и весь исторический граф. Current journal header frozen независимо: global epoch3
после реального нового OS-owner, тогда как selected keeper остался epoch2. Нельзя
их приравнивать или менять retained records. Новый backup и обе old24 init/status
refusal проверяются на отдельной consistent SQLite copy; fresh bootstrap rows=0,
cold view остаётся no-original-custody. Python reader сверяет exact frozen reference,
rejects24 mutations; синтетические тесты/shape compatibility не являются Windows proof.

Запуск: run-update-bootstrap-migration-native.py; readonly audit:
verify-update-bootstrap-migration-native.py. Исходный Main31 task не replay.
До получения native результата migration не объявлять проверенной на Windows.
Следующий шаг — non-Clone/non-Serde known-COMMIT result, удерживающий actual file/
process/Job pins и original attempt, затем actual authenticated worker→keeper,
одна попытка CreateProcess/child independence/стабильный ready handoff.

Проверки перед фиксацией source25:1033 workspace tests/17 ignored (default features),
8 focused bootstrap после последних assertions, Linux/MSVC workspace all-target
Clippy -D warnings, fmt/diff passed. Python migration observers13 tests passed;
новый reader rejects24 mutations и offline проверяет форму actual Main31 records.
Это последнее — только shape compatibility, не positive Windows migration proof.
Debian free before full run13,972,213,760bytes, после Clippy10,587,123,712bytes;
Windows/shared volume15,348,359,168bytes. Ничего в Windows не удалялось.

## Actual24→25 copy-only Windows gate — passed

Sourceb0947c3,run20261030-170706-bae5c263,
taskcf99be54-03ba-4b82-a431-cd0b3bd3cb0f. Source archive
winbridge-20261030T170646Z-b0947c3-8cc83ca5.zip,
SHAadde1e07d3a95d89837f5baddf14aff54ddbc8daeb3ce75e1c580a56023c6520.
Main31 actual keeper/history и current header3 сохранены; migration backup24
verified, new25 owner epoch4/cold observation5, historical keeper epoch2 не менялся.
Old24 init/status reject UnsupportedSchema без изменения copy; исходные journal,
artifact/manifest и установленный worker не менялись. Bootstrap table пустая,
cold view не создаёт custody. Это copy migration, не новый bootstrap execution.

4 ready files verified twice;24 migration negatives/29 independently rechecked
Main31 negatives. Report SHA256
d29af8305a867c6a3402893ce71b3bdbf9763d75b6484ddf1b94ec2f2a00fd9b;
proof8ec9c5990351d3c96901c3908624fd553f2cc75d16c6f4ca10e1470c02d35c01.
Cargo106.854s: build69s/test34.52s. Windows15,250,874,368→15,253,340,160bytes.
Metadata `.winbridge/update-preparation/bootstrap-migration-20261030-170706-bae5c263/`,
log `.winbridge/keeper-stage-review-204/bootstrap-migration-native-run.log`.
No task replay, windows none/desktop unnecessary; installed0.2.0/source48/genf9
оставить запущенным. Windows directories не удалялись.

## Known SQL COMMIT fact — следующий source25 участок

Internal `UpdateJournal::commit_bootstrap_reservation` получает итог New/Existing
в той же Immediate transaction. `BootstrapReservationCommit` (private fields,
без Clone/serde) создаётся только после собственного успешного COMMIT, проверки
того же directory/owner-lock и повторного readback. Duplicate/caller loss возвращает
Existing serialisable data, не восстанавливая original opaque fact и не выполняя
live checkpoints. Ошибка COMMIT не даёт typed success; будущая consuming factory
обязана сохранить исходные OS/file pins и не replay/adopt.

Review усиление: require_known_bootstrap проверяет не только immutable payload/
owner, но и точный Requested root0/исходный unstarted supervisor Intent. Историческая
reservation остаётся читаемой после progress; known fact нельзя использовать как
свежую подготовку после Spawned или перехода root в Preparing. Он вообще не является
разрешением запуска: actual peer/pins/Job exclusion/ancestor-exit/live capture и
consuming one-attempt factory ещё нужны. Никаких runtime callers/нового wire command
не подключено, current schema25 не меняется.

Новые portable SQLite проверки: caller теряет typed commit → duplicate только data;
настоящая deferred-FK ошибка в SQLite COMMIT после обоих checkpoints → ни known fact,
ни reservation/SupIntent; owner change, SupSpawned и rootPreparing отвергаются при
сохранённом opaque fact, а history по-прежнему читается. Synthetic process metadata
не доказывают Windows custody. Positive native24→25 gate выше относится к sourceb0947c3,
не к исполнению нового внутреннего COMMIT wrapper на Windows.

### Следующая физическая граница: actual current-worker peer

Existing keeper_control::validate использует legacy supervisor authorization:
первоначальный parent worker из before.spec. Это другой процесс, чем сохранённый
новый Active worker в KeeperBinding. Нельзя просто ослабить существующий authorize
или разрешить bootstrap по JSON PID/старому token. Новому минимальному handshake
нужен actual IPC peer PID, сравнение с original owned child (birth/host/session),
known original Active binding и свежая проверка реального Job. Retiring state/cold
rows не заменяют KeeperCustody. Новый contract не должен принимать future
UpdateRecord/SQLite schema/произвольные path/argv. Оба dispatchers должны проверять
DTO по original bounded bytes, сохраняя rejection duplicate keys.

Для max2 incoming reservation указывает предшественника, но сохранённого PID
недостаточно для positive exit. Следующий actor должен удержать полученный при
живом родителе query-only exact process handle до проверки его exit; missing PID,
PID reuse и query error не считаются завершением. Такой OS-only predecessor handle
не равен старым config/image verification pins, которые обязаны освободиться при
retirement. Продумать его перенос вместе с original physical custody до нового
Keeper, без SQLite/authority references. Создание вне original activation Job и
worker task domain по-прежнему проверяется до и после CreateProcess; production
worker direct-spawn guard остаётся включённым.

Проверки final known-fact участка:11 focused tests passed (1.91s), включая real
SQLite deferred-FK commit failure и freshness after Sup/root progress; Linux/MSVC
updater all-target Clippy -D warnings, fmt/diff passed. Это portable SQL tests;
new wrapper на Windows ещё не исполнялся. На installed worker нет новых task/changes.
Logs `.winbridge/keeper-stage-review-204/bootstrap-known-commit-freshness-tests.log`
и `bootstrap-known-commit-final-{linux,windows}-clippy.log`. Debian free10,507,022,336,
Windows/shared15,253,352,448bytes; без Windows cleanup. До runtime integration нужны
actual peer handshake/pins/known ancestor exit/one-attempt creator, затем native gate.

## Readonly current-worker probe1

Новый version-neutral KeeperProbeRequest/Response/Result находится в protocol,
TypeScript генерируется оттуда же. DTO имеет только version/request UUID/immutable
KeeperBinding. Binding v1 для wire заморожен; новые будущие Root/SQL models в него
не добавлять. No token/path/argv/exec or SQL schema. Legacy keeper-control остаётся
для old logical parent, probe — только actual newly activated original child.

Closed keeper host использует original bounded frame, typed decode сохраняет
rejection duplicate fields. В Retiring/cold actor probe не запускает закрытие writer
и не восстанавливает custody. Positive response требует exact in-memory binding,
actual IPC peer PID, original ManagedChild identity/Job, live birth/host/session и
keeper outside original activation Job. Он не открывает SQLite и не даёт spawn.

Client observe_original_keeper проверяет, что сам вызывающий процесс является
исходным worker, подтверждает actual IPC server PID, response correlation/binding/
clock и сохраняет query-only worker/keeper handles. Deadline3s отсчитывается ДО
подготовки/обмена и не обновляется ответом. require_current повторяет handle/Job/
clock checks. Capture non-Clone/non-Serde, не восстанавливается из JSON. Он ещё не
проверяет current worker mode/config/source pins/старый task domain или ancestor
exit — это обязанности consuming factory. Не считать observation готовностью к
creation или доказательством отсутствия другого обновления.

Portable dispatcher tests3 и supervisor23/2 ignored прошли, включая real IPC cold,
duplicate и mixed-contract rejection/root unchanged. Первый run увидел старый
support updater binary, EOF; rebuilt helper исправил fixture. Marker диагностика
сообщает cargo build -p winbridge-update --bin winbridge-update --locked при отсутствии
нового decoder. Marker — только feature diagnostic, не подмена source attestation.

Подготовлена ignored Windows unit
supervisor::keeper_probe::tests::native::original_child_probe_uses_real_peer_and_exact_handles.
Два actual process, отдельный named Job, IPC peer mismatch, changed binding,
duplicate decode, exact live capture и expiration. Read stdout/stderr concurrent;
child exited/owned Job empty проверяются. Test-only custody constructor использует
synthetic role history: unit не доказывает SQL retirement/normal Active/full updater
workflow и не создаёт bootstrap coordinator. Protocol roles и handles настоящие.
Parent OS process trace сверяется отдельно от result JSON,25 negative audits reader.
Driver run-update-keeper-probe-native.py, readonly verify-update-keeper-probe-native.py.
Пока native результата нет. Final1039 workspace tests/17 ignored, Linux/MSVC
workspace all-target Clippy -D warnings, contracts/TypeScript/UI/scaffold/fmt/diff
passed. Python6 tests passed (probe/exclusion readers); новый audit25 mutations. Installed worker не
обновлялся, public install/bootstrap action не включены. После unit — actual consuming
factory/known SQL fact+fresh probe+filesystem/domain/ancestor guards, one create/ready.

Логи final `.winbridge/keeper-stage-review-204/keeper-probe-workspace-tests.log`,
`keeper-probe-final-{linux,windows}-clippy.log`, `keeper-probe-final-contract-check.log`.
Debian free перед full check10,592,751,616bytes, после10,261,741,568bytes;
Windows/shared15,259,041,792bytes. Windows cleanup не выполнялся.

## Native current-child peer/Job/capture unit — passed

Source785b6e7,task5c0e520b-c366-4ebf-a1b6-51c7537799b1,
run20261030-175345-e76446dd. Archive
winbridge-20261030T175319Z-785b6e7-c1f8b81d.zip,
SHA90a7fb0efe2a373f1f03e8375293634fe42407316528e13f1c27a70e6e9367e7.
Actual parent18924/worker helper20468, exact birth/host/session/Job/IPC peer checks.
Wrong OS peer и changed binding/duplicate frame rejected; ровно один Observed,
retained capture истёк, child exit0, owned Job positive empty, readers joined.
Оба PID исторические helper processes, не установленный worker. Native role history
synthetic; new coordinator/SQL retirement/full normal-Active workflow не выполнялись.
keeperRuntimeProved=false/bootstrapCreationProved=false — эти границы сохранены.

3 ready files twice verified,25 negative audits, включая correlated process rebinding
отдельно от immutable server OS trace. Report SHA256
fa4c6c7fb1f936f512967e0429324ba5be778521323d97d1c9daac5cde9fec77;
prooff6716170fd7d4897a838e5b323906f9c0afc29e61ec7d8057dcea8654c75cf62.
Cargo105.322s, actual unit3.48s. Windows15,144,202,240→15,805,829,120bytes;
Windows directories не удалялись. Installed source48/genf9 unchanged/healthy/idle,
оставить запущенным; desktop не нужен, тестовых окон нет. Task/key не replay.
Metadata `.winbridge/update-preparation/keeper-probe-20261030-175345-e76446dd/`,
log `.winbridge/keeper-stage-review-204/keeper-probe-native-run.log`.

Следующий основной шаг: consuming physical factory связывает freshly built/pinned
updater helper/config/output files, worker's own live normal state/domain, known
bootstrap SQL COMMIT и fresh original keeper capture. Helper build должен совпадать
с новым requested source; existing supervisor::prepare копирует установленный
старый updater, поэтому использовать его без изменения здесь нельзя. Фиксированный
helper directory уже задан intent. Затем version-neutral delegation message только
от actual original child, один CreateProcess с original startup budget, detach до
one-time start/ACK, independent observation/Spawned receipt и ready handoff. Old keeper
не читает SQL/future Root. Current worker direct-in-external-Job guard сохраняется.
Max2 требует живого predecessor handle/positive exit, не PID lookup. После actual
same-SDK двух workflow — fresh future-SDK bridge/migration gate/U2–U4. Native unit
этой главы не подменяет ни один из этих оставшихся сценариев.
