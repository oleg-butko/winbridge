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

## Fresh helper files и consuming admission

Source19/25: bootstrap/files.rs добавляет FreshUpdater::from_managed_build. Caller
предоставляет trusted registered build root; canonical image path должен лежать в
нём. Actual bounded artifact query проверяет updater role/OS/arch/clean source commit+
manifest, точный SHA/identity/bytes и полную cleanup query tree. Это stateless managed
query, не новый coordinating actor. Build-cache source может быть hardlinked; copied
fixed helper/config/output должны иметь single link. Новый updater не берётся из
старого installed runtime: copy_new_checked читает pinned fresh image с owner
checkpoints и создаёт новый update/<id>/supervisor/winbridge-update.exe.

File staging требует accepted Requested0/current owner, actual calling worker stamp,
loaded worker config hash/domain host/session, correct generation endpoint и actual
runtime/update-state directory. Последняя проверка не выводится из scope header:
consistent копия SQLite сохраняет header, но не права писать другой runtime. Новый
portable regression проверяет эту границу. Configs read-pinned/<=64KiB, fixed local
paths, baseline source verified до staging. Новый helper directory create-only;
частичная подготовка сохраняется, не overwrite/replay/cleanup автоматически.

Image/updater config/worker config pins запрещают write/delete; stdout/stderr read
pins позволяют keeper writer, но не rename/delete. Все пять identity distinct,
single-link/current paths/hashes проверяются заново. Private token не сериализуется
в intent, вместо него SHA. PreparedBootstrapFiles создаёт согласованные intent/spec
с поздним preparedAt, чтобы тяжёлая copy/query не съедала fresh3s observation.

Windows-only admit потребляет этот original object и non-Clone KeeperProbeCapture,
проверяет own_state до/внутри/после SQL, получает opaque original COMMIT. own_state —
trusted worker callback, обязан проверять реальную normal mode/domain witness и
не ждать Store-lock под SQL-lock. Worker integration callback пока отсутствует.
Failure содержит original file/process pins/known fact, не превращается обратно в
fresh prepared object; post-COMMIT failure явно сообщает known commit/no coordinator.
Existing возвращает только historical data. Никакого CreateProcess/нового wire action
этот участок не добавляет, public update capability остаётся выключенной.

Max2: incoming reservation — данные для будущего actual keeper creator. У current
keeper должен сохраняться query-only predecessor handle из его coordinating phase;
physical creator проверяет positive exit до CreateProcess. Client reservation может
быть prepared без физического ancestor-exit доказательства, но не является spawn
permit. Не доверять saved PID/boolean из JSON. Это уточнение места physical guard,
не снятие invariant. Missing handle/query error запрещают создание третьего.

Проверки:446 updater tests/7 ignored,15 focused bootstrap tests, Linux/MSVC updater
all-target Clippy -D warnings/fmt/diff passed. Python readers9 tests (files/probe/
exclusion), files observer24 negative mutations. Новый ignored Windows case
bootstrap::files::native::fresh_helper_and_config_pins_allow_only_the_intended_file_access
строит real fresh updater, query/copy проверяет при отдельном старом installed
sentinel; native file sharing/write/replace/output pins retirement. Worker state
synthetic: это не actual normal-mode/keeper admission/full updater workflow. Перед
native result не считать эти Windows guarantees подтверждёнными исполнением.
Driver run-update-bootstrap-files-native.py, readonly verify-update-bootstrap-files-native.py.
Debian free before package10,258,866,176/after9,515,409,408bytes; Windows/shared
15,806,816,256bytes. Source schema25 unchanged, Windows directories не удалялись.

Далее сразу основной effect path: minimal keeper launch envelope из этих pins,
actual original-child auth, fixed paths/args, query-only exclusion BOTH original
activation Job и old task domain, retained predecessor positive exit/max2. Один
in-memory creation attempt, detach до start marker/ACK, отдельное Observe после
unknown. P0 не открывает SQL/future Root. Worker записывает exact Spawned receipt,
освобождает owner; новый actor получает authority и handoff. Logical worker и physical
spawner различаются. Затем startup parent-handle переносится до нового Keeper;
old Keeper выходит по known next ready+old worker exited+same Job empty+readers done.
Normal direct-in-external-Job worker guard сохраняется. Для same-SDK использовать
existing supervisor protocol; stable future-SDK ready/worker bridge и migration
с epoch jump остаются отдельным обязательным gate до U4.

## Native fresh-helper file-sharing unit — passed

Sourceec0885e,run20261030-213510-ab939497,
task797e7899-1649-4f65-b749-ad7dbe8ec914. Original source export создан18:44UTC;
после возобновления goal actual task отправлен21:35UTC, не replay. Archive
winbridge-20261030T184417Z-ec0885e-1368cd2d.zip,
SHA030650bca9895b47d90036903ac8e98a279b86e85b7d16f3065bb11ae86f862e.

Actual clean updater --artifact-info queried, fresh helper SHA equal; отдельный
old installed sentinel остался неизменным. Exe/updater config/worker config writes
blocked, updater config atomic replace blocked, intended output append allowed/
output rename blocked; после Drop pins config write+same replace succeeded. Root0
unchanged, supervisor/reservation отсутствуют, coordinator не создан. Worker state
synthetic: liveAdmissionProved=false/bootstrapCreationProved=false. Consuming
admission/normal-mode/keeper launch не считать проверенными этим unit.

4 ready files verified twice/24 negative audits. Report SHA256
e903ce75e9a92cada9221cda02e0b775d67c5c8d04d7ce1e9111c8542f0541fe;
proof6448d09da211f5713465c1cf9d0b1cd6f7296403c8d6234d914d0f64b5682073.
Cargo build59.710s, selected unit command73.041s (включая compilation).
Windows15,741,677,568→15,475,306,496bytes. Metadata
`.winbridge/update-preparation/bootstrap-files-20261030-213510-ab939497/`;
log `.winbridge/keeper-stage-review-204/bootstrap-files-native-run.log`.
Installed worker unchanged; оставить запущенным, desktop не нужен/окон нет.
Windows directories не удалялись. Следующий effect: actual keeper one-attempt
launch с query-only outside BOTH Jobs и actual ancestry exit, Spawned/ready/handoff.

## Dual-existing-Job physical launcher

Platform InfrastructureChild::spawn_excluding_jobs получает exact retained local
parent handle и два разных existing Job names. Parent PID должен быть std::process::id;
live birth/host/session проверяются PinnedProcess. Оба query-only require_outside_job
выполняются до OS spawn; после него exact child и тот же parent проверяются снова.
Missing Job/query error — failure, а не outside. Не вызывает ExecutionDomain::open
или Job creation из wire identity, не меняет limits/не ставит breakaway. На ошибке
после CreateProcess owned child очищается; это не разрешение automatic retry.

Existing spawn(domain) сохраняет свой прежний guard и post-check; общий private
spawn_process содержит прежний Command/create/stamp/owned cleanup. Новый метод
возвращает kill-on-drop owner, не делает detach сам. Future keeper обязан сохранить
one-attempt state ДО вызова, detach только после positive boundary checks и ДО
one-time start/ACK. Никаких public worker update commands этот primitive не добавляет.

Windows unit keeper_launch_requires_two_existing_jobs_and_an_exact_local_parent
проверяет foreign parent/alias/missing Job rejection, real child outside обеих Jobs,
owned cleanup/exact exit. Он compiled MSVC, ещё не запускался. Linux infrastructure
3 tests/1 ignored и Linux/MSVC platform all-target Clippy passed. Logs
`.winbridge/keeper-stage-review-204/bootstrap-dual-exclusion-tests.log`,
`bootstrap-dual-exclusion-linux-clippy.log`/`bootstrap-dual-exclusion-final-windows-clippy.log`.
Native execute/wire launch всё ещё обязательны. Два Job — activation и old task
scope, не число agents. Главное продолжение: keeper minimal launch envelope/hash,
actual peer→one retained creation attempt→independent observation/Spawned/ready/handoff.
Kepr должен хранить predecessor process handle с actual startup и positive exit
для third generation. Stable future-SDK bridge/max2/full two workflow/U2–U4 не готовы.

## Keeper-launch1: сервер и одна попытка

2026-10-30/.204, source worker19/updater25. Новый independent local envelope
`KeeperLaunchRequest` содержит original keeper binding и одну из Start/Observe/
ConfirmReady. Start несёт только новые update/launch IDs, hashes/bytes/file IDs,
private startup token, excluded Job, historical predecessor identity и исходное
preparedAt. Пути/argv/runtime/SQL Root из запроса не принимаются: creator использует
fixed runtime/update/<id>/supervisor/winbridge-update.exe, fixed updater config и
stdout.log/stderr.log. Debug скрывает token; fingerprint использует его SHA.

Только closed keeper с actual original child/Job допускает creator. Его local
in-memory slot резервируется до preflight/OS boundary; exact duplicate возвращает
прежний status даже после истечения budget. Иной payload, известный отказ или
неопределённый исход не освобождают slot. Статусы Failed/Indeterminate/Exited
сохраняются; никакого recreate по saved data. Observe/ConfirmReady не создают
процесс. Обычный coordinator и незавершённый retirement отказывают без SQL effects.
Все DTO decode идут из original bounded bytes, routing Value не удаляет duplicate
fields перед typed validation. Реальный IPC test проверяет cold/duplicate/mixed
frames, неизменность root/supervisor и exclusive owner.

Creator pin-ит fixed image/config и output identities, проверяет predecessor exit,
actual peer и исходный budget, использует platform spawn_excluding_jobs для двух
EXISTING Jobs и переводит child в detached до ответа/любого Spawned marker.
Query/access error остаётся отказом/unknown, limits и breakaway не меняются.
Created ещё не Ready: worker должен проверить exact process, записать Spawned,
освободить original owner и получить Ready от того же нового coordinator.
ConfirmReady принимается только для того же сохранённого process/ticket hash,
после повторной physical outside проверкой обоих Jobs. Keeper пишет один bounded
launch event с phase/process/deadline/reason, без token и всего запроса.

Для max2 новый coordinator при incoming bootstrap удерживает query-only kernel
handle своего previous keeper, пока тот жив; проверяет immutable incoming scope.
Этот handle переносится через writer retirement в закрытый keeper, не сохраняя
SQLite, authority, config/image pins. Последующий creator требует exact expected
identity и positive exited на этом удерживаемом handle. None подходит только для
первого coordinator без incoming delegation. Missing/reused PID не значит exit.
Новый Windows unit проверяет живого/завершённого owned predecessor, changed identity
и отсутствие pin; helper readiness не позволяет принять zero-selected-tests.
Unit пока compiled-only, не native evidence. Bootstrap startup использует остаток
исходного preparedAt+10s, с проверками до owner и Ready. Новый процесс не получает
новые10s; existing ordinary supervisor flow сохраняется.

Проверки:452 updater tests/7 ignored, включая5 machine tests и29 supervisor
tests/2 ignored с actual local IPC; Linux/MSVC all-target Clippy, generated
contracts/TypeScript/scaffold passed. Пока НЕ проверены native actual keeper CreateProcess, ancestor gate,
полный новый child handshake и два updater workflows. Client к server ещё не
подключён: нужен consuming original PreparedBootstrap, ровно один Start, сохранение
его ресурсов при unknown, только Observe после потери ответа, exact Spawned/owner
release/Ready/ConfirmReady. Затем старый keeper должен выйти только после known
successor, actual original worker exit, same Job.empty и readers finished; idle
polling добавлять нельзя. Future-SDK journal bridge и worker own-mode integration
остаются отдельными незавершёнными шагами. Установленный worker не менялся.


Отдельный OS-unit profile `run-update-keeper-launch-primitives-native.py` выбирает
ровно production dual-Job launch unit и exact predecessor handle unit. Новый
namespace/report kind сохраняет прежние17 observer tests и старые gate selections.
Reader проверяет exact commands/test names/one passed/no ignored, source/task scope,
неизменность installed worker и два чтения ready files;12 deliberate mutations
rejected,3 synthetic reader tests passed. Это тесты системных примитивов, не actual
keeper bootstrap/client/SQL owner handoff; вывод сохраняет все широкие proof flags
false. В этом profile нет окон, сети или изменений рабочего runtime. Проверка
места и retained-cache lease выполняются перед компиляцией на Windows.


### Windows OS-unit результат

Sourceff3307efe1f5b1e7204fc3f463f2d5d776bbdb38,
archive winbridge-20261030T220946Z-ff3307e-bdd42bdd.zip,
SHA8ab6fd0e0d89069420c709c0ef6847ce895a1856a6a179c42d8f22a11d546312,
manifestc1a82d5c369405250a574d5c92d81c93c56c3804f54fff68dbe07cbbcf10531b.
Task4713f975-c011-411d-961d-a4d6b1928281/run20261030-221011-8c68630d passed;
3 report/log files проверены дважды,12 negative audits отвергнуты. Report
0245f0a4c13bd2e7db230f5117a5144e606c06924ab6ff0551123111a0cfb26c.

Оба production OS primitives действительно выполнены на Windows: exact local
parent/two existing Jobs/one create/positive helper exits; retained ancestor handle
live→exited с отказом для changed/missing identity. Это не actual closed-keeper
Start request/SQL Spawned/Ready и не два updater workflows. Reader сохраняет
bootstrapCreationProved/keeperRuntimeProved/liveAdmissionProved=false. Task не replay.

Замеры: task146.052s; Cargo commands33.604s и97.610s, tests0.10s и0.39s. Таким
образом131.214s внутри Cargo, из них только0.49s в test bodies;14.838s вне Cargo
включают snapshot extraction/verification/reporting и process wrappers. Логи Cargo
показывают пересборку workspace crates при сохранённых сторонних dependencies.
Не следует относить всё это время к медленному тестовому сценарию.
Windows свободно15,214,280,704→15,215,120,384bytes. Installed worker source48/genf9
не изменился/active0/errors=null. Helpers завершены, окон нет, desktop не нужен,
рабочий worker оставить. Отчёты и Windows validation directories сохраняются.

При следующем consuming client отдельно измерить full file revalidation через
existing artifact hash counters: original keeper capture3s включает несколько
проверок image/config pins вокруг SQL COMMIT. Нельзя продлевать capture/startup
после неизвестного исхода либо пересоздавать Start при expiry. Прежде чем менять
проверки, получить timings/bytes/calls именно настоящего admission/handoff.


## Клиент original attempt и передача owner

2026-10-30/.204, same-SDK source19/25. Это внутренний consuming API, ещё не
подключённый к worker admission/UI или mixed-version journal bridge.

Порядок вызовов:

1. `PreparedBootstrap::begin_launch(j, owner, own_state)` потребляет original
   files/capture/known reservation; Existing rows не имеют этого метода. Binding
   и private token проверяются против immutable reservation, paths фиксированы.
2. `BootstrapLaunchAttempt::send_start(&mut journal, owner, own_state)` проверяет
   original3s capture и фиксирует start_attempted до первого await/write. Future
   заимствует resident attempt: timeout/cancellation сохраняют все pins/flag и
   unconfirmed reason. После unknown — `observe()`, не новый Start. Наблюдение
   использует original handles, не refresh3s и не SQL writes.
3. Ответ проверяет version/request/launch/ticket hash/original expiresAt, canonical
   birth/host/session и exact new process outside обоих existing Jobs. Terminal
   results не возвращаются в Created. Exited metadata не открывает мёртвый PID и
   не разрешает handoff. Query failures не считаются outside.
4. `publish_spawned` имеет один write attempt. Отдельный writer проверяет original
   root0/Intent/owner/directory внутри IMMEDIATE transaction,2 physical checkpoints,
   actual COMMIT/readback и выдаёт opaque `BootstrapSpawnedCommit`. Повтор при
   existing receipt отказывает до callback. Generic advance_supervisor не может
   заменить original COMMIT фактом из saved data. Ошибка хранит attempt/owner;
   unconfirmed write не повторяется. Known fact сохраняется до поздних checks.
5. `release_owner` потребляет именно original known fact после повторной проверки
   scope/process/files и original preparedAt+10s. При отказе returns retained owner
   и attempt; новый owner не приобретается от имени старого. При успехе выдаёт
   `BootstrapSession` и только затем drop original owner-lock.
6. `wait_ready_and_confirm` требует exact Ready revision2/process/spec/epoch+1 и
   receipt timestamp до original10s deadline. Та же retained process Arc передаётся
   в SupervisorHandle, без reopening PID/ownership reconstruction. ConfirmReady
   идёт старому keeper; unknown reply можно прочитать через `observe_keeper()`.
7. `handoff` использует прежний one-shot Supervisor API; unknown reply выясняется
   через `observe_handoff()`. `finish` разрешён только после known Active и live
   original process, затем снимает original preparation pins. Returned observed
   handle не убивает независимый coordinator при Drop. Direct owned path сохраняет
   прежний cleanup/detach; следующие source actions идут через прежние API.

Новый PhaseTrace пишет admission/begin/start/publish/release elapsed и process-wide
hash calls/bytes/ns deltas. Эти counters могут включать concurrent hash work,
не заменяют проверки/authority. Прерванная borrowed future оставляет trace outcome
interrupted_or_unfinished, а команду — unconfirmed, не unsent/success. Token и whole
wire request не логируются. Capture3s и startup10s не продлеваются.

При ревью исправлены две важные границы: перенос delegated child больше не
пересоздаёт pin по PID; opaque Spawned COMMIT отделён от generic existing receipt.
Compile-time проверка actual futures на Send нашла !Sync SQLite reference во
вложенном async preflight; SQL checks вынесены до I/O future. Runtime/owner/epoch
проверки ради Send не ослаблялись.

Проверки:460 updater tests/7 ignored,22 bootstrap +1 exact Spawned guard;5 новых
reply/unknown/terminal/identity tests и3 настоящих SQLite tests. Actual deferred-FK
COMMIT failure и оба checkpoints откатывают Spawned/receipt без fact, duplicate
write не rerun checkpoints, owner/directory/current-receipt changes отвергаются.
Linux/MSVC all-target Clippy и fmt/diff passed, direct supervisor IPC regression
сохраняется. Windows actual client/admission/Start→Spawned→Ready пока НЕ proved.
Native probe unit усилена: expired3s capture отклоняет fresh operation, но исходные
kernel handles остаются пригодны для readonly continuation. Нужен новый isolated
run для этого изменения, не replay прежнего passed task.

Дальше нужен intrinsic Active-worker factory: candidate historical keeper binding
может служить только nomination, сверенной с actual loaded activation scope и
новым authenticated probe, не saved custody. Own-mode callback должен быть
nonblocking при SQL checkpoints; нельзя брать Store/registry lock с ожиданием под
journal writer. Fresh updater предварительно строится managed build внутри
registered project до source/runtime changes. Затем native actual client handshake
и controlled old keeper exit только при known successor + original root exit +
same Job.empty + finished readers, без idle polling. Two real workflows/max2,
future-SDK migration/bridge и U2–U4 остаются отдельными gates; install off.


### Windows original-handle continuation

Source07941873d4abf618304bfb5622939f77e9714517,
archive winbridge-20261030T224435Z-0794187-a39fac40.zip,
SHA394fa6874fc919a90aaaa2df3245fc7af9ee29f431656eaa5fa768dff04f30e5,
manifest9b43a270a7a45c02869495e2d02f407b4fa7ee680be4fb817c6df66c429feb9b.
Task3efde89e-958c-44ce-9668-bbade0535cbf/run20261030-224456-f079f929 passed:
3 ready files twice/25 negative audits. Native helper проверила оба условия:
expired original3s capture запрещает fresh use; те же original handles остаются
живыми/in the same Job/outside the same Job для readonly continuation. Marker
с finished helper/empty Job/readers сохраняет прежний independent audit.
Reportd81bb95c057b62a10a0d8a680d075b3c4d21ef9cbef6601198efc34d634fc60c,
proof0a70220d0d3ff27f648faa5de7fbca7a0cc6f3f46e8990df2c3c51e88a9fcc5f.

Scope остаётся OS peer/Job/capture, synthetic role history; unit не вызывает новый
consuming bootstrap client, его SQL Spawned/owner release или Ready/Handoff.
keeperRuntimeProved/bootstrapCreationProved=false. Повторять этот task запрещено.
Task127.391s/Cargo109.137s/test3.38s (3.05s — intentional capture expiry wait).
Windows15,066,087,424→15,069,876,224bytes; installed source48/genf9 unchanged,
active0/errors=null. Окон нет/закрывать нечего; установленный worker оставить,
desktop не требуется. Отчёты/Windows validation directories не удалялись.

Следующий независимый вертикальный участок — controlled old keeper exit. Нужна
monotonic known-successor confirmation (не потерять её, если successor позже exited),
positive original root exit по удерживаемому kernel object, same owned Job.empty
и завершённые readers. Пока root жив, ждать kernel exit + IPC без периодического
polling. После root exit допустим отдельный drain wait; ready JoinHandle нужно
удалять, иначе select по completed future создаст новый idle spin. Query/Wait error
не означает exit. Drop старого keeper не должен убить detached successor.
После этого — intrinsic worker factory/nonblocking own-mode witness/managed fresh
helper build и настоящий Native client/два workflow; public install всё ещё off.


## Controlled old keeper exit

2026-10-30/.204, source worker19/updater25. Старый closed keeper выходит только
при одновременном выполнении четырёх условий: known original successor Ready,
positive original root exit, empty same owned Job и finished/joined оба original
output reader. Пока root жив, loop ждёт IPC и kernel process signal, без timer/SQL/
hash polling. На root exit completed JoinHandle удаляется; polling завершённого
future мог бы вызвать новый spin. Drain timer250ms включается только после actual
root exit и successor confirmation; без неё keeper остаётся resident даже при
empty Job/EOF. Ошибка wait/query — unknown с сохранением ресурсов, не exit.

`PinnedProcess::wait_for_exit` — блокирующий WaitForSingleObject(INFINITE) на том
же query/SYNCHRONIZE handle, вынесенный в bounded blocking owner task. Guard pin-ит
original worker до close writer; custody и watcher разделяют Arc на SAME object.
Никаких PID lookup после exit. KeeperStatus.worker_alive читает retained kernel
signal; original_custody_held подтверждает identity/ownership независимо от того,
жив root или нет. Original Job остаётся owned до final empty observation/Drop.

Known-successor flag монотонный: subsequent Exited у successor не стирает уже
подтверждённый Ready. Перед exit owned ManagedChild даёт original exit/reap, тот же
Job даёт positive empty; оба уже finished reader join-ятся без ожидания живого
процесса. Reader panic помечает output incomplete, факт joined сохраняется после
удаления JoinHandles. Завершающая проверка Job.empty не отделена await от close.
Drop detached successor закрывает только query/std child handles, не убивает его.

Логи original_worker_exit_observed/original_keeper_drain_observed/exit и bounded
unknown reason показывают задержку. Drain flags пишутся при изменении; idle logs
не добавлены. Изменения не открывают SQLite и не удерживают updater authority/
config pins в closed keeper. Нет breakaway/изменения Job limits/future-schema read.

Проверки:475 updater/platform tests/8 ignored; existing machine/reader tests усилены
для monotonic Ready и early-reader-join отказа. Linux/MSVC all-target Clippy passed.
Windows dual-Job unit усилена: kernel wait не возвращается у live child, после
owned cleanup завершается на same object, все helper roots exited. Full exit-loop
unit подготовлена, ещё не выполнена: confirmed successor должен пережить actual
old keeper OS exit; root exit при live leaf не закрывает Job; unconfirmed successor
удерживает keeper даже после empty Job/EOF, затем fixture вручную завершает только
свой owned keeper. Idle samples до root exit и после completed wait без confirmation
проверяют отсутствие >25% одного CPU на sample. Это диагностический sample, не
универсальный performance benchmark.

Новая fixture имеет synthetic role history и synthetic Ready claim, но использует
production closed loop/actual IPC peer/held kernel objects/Job/readers/OS exits.
Она НЕ доказывает SQL retirement, bootstrap creation, new client handshake или
полный updater workflow; broad proof flags остаются false. Не включать public install.
Native input/root/reports фиксируются в отдельном namespace keeper-exit; helpers
не используют сеть/GUI, installed worker остаётся прежним. Native script выбирает
ровно production platform dual-Job/wait unit и ignored keeper exit-loop unit.

Runner/verifier используют общий update-lifetime-native-profiles.py: прежние пять
scopes/counts/switches/meta keys сохранены, mutually exclusive selection проверяется
до RPC.27 Python reader/profile tests passed,40 deliberate exit proof mutations
rejected. После refactor4 прежних native reports (probe/files/launch/exclusion)
проверены дважды без Windows RPC/replay, hashes остались прежними. Новый independent
reader связывает2 cases/status snapshots/idle samples с production exit trace,
не путает manual fixture stop с natural exit. Нужен новый isolated Windows run.

Далее intrinsic Active-worker factory/nonblocking own-mode witness, managed fresh
updater build и реальный client Start→Spawned→owner→Ready/Handoff. После этого
нужны два настоящих workflow/max2, cold/fault matrix и future-SDK bridge/U2–U4.
Unconfirmed root exit намеренно не даёт новый launch/role adoption; cold restart
и reconciliation не объявляются реализованными по этой OS fixture.
