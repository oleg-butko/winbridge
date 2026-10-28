# Следующий сценарий: controller session после LocalVerified

2026-10-28. Main25 и actual retained19→20 пройдены; worker0.2.0 установлен,
controller.203/.204 доступны. Продолжение U2 без нового installed worker до тех
пор, пока существующий worker служит исправным исполнителем изолированных gates.
Public install/whole update остаётся выключенным.

## Текущая реализация

**Main26 harness подготовлен, ещё не выполнен.** Exact native test
`actual_main_connects_same_verified_worker_to_controller_without_commit_or_replay`
добавляет V5 после прежнего local-service prefix, теряет только volatile dispatch
ACK, проверяет same PID/authority/root/history и cold NotResident/no replay.
PowerShell `-ActivationControllerGate` требует source и target19/21 до build;
old Main25 flags/profiles остаются прежними. Shared local-prefix verifier получает
фиксированные kind/schema от каждого gate, никогда не переписывает reported SDK.
Actual Main25 был заново проверен readonly после refactor:52 files twice,
24+10 negative audits/3 native markers passed, новых Windows команд не было.

Новые scripts: `run-update-controller-main-native.py`,
`verify-update-controller-main-native.py`, `update-activation-controller-proof.py`.
Runner требует ровно approved `K:\_proj-26\winbridge\runtime-test-204`, фиксирует
fenced request/ACK, оставляет controller при unknown task acceptance и после
terminal останавливает только свой known test controller. Existing rotating
activity log сохраняется после stop, чтобы доказать actual NEW session даже
короче interval status polling, без дополнительных sleeps/window renewal.
Installed worker/context/build должен остаться прежним. Native Rust MSVC check
прошёл без bundled SQLite (на Debian нет MSVC lib.exe для C); на Windows gate
собирает bundled штатным native toolchain. Python synthetic tests покрывают27
controller mutations, типы/unknown counters и actual activity event binding;
они не являются native evidence. Full Main26 и fault matrix остаются впереди.

После clean producer commit/export запустить:

```bash
python3 scripts/run-update-controller-main-native.py \
  --source-archive <новый-clean-producer21.zip> \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261028T094607Z-b149eed-a3228347.zip \
  --cache-run 20261017-061527-f4639971 \
  --worker17-reference 'K:\VMs\shared\WinBridge\releases\20261026-182213-6cd81f3e\winbridge-worker.exe' \
  --worker17-reference-sha256 536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712 \
  --test-runtime-directory 'K:\_proj-26\winbridge\runtime-test-204' \
  --controller-config .winbridge/controller-snapshot.local.json \
  --controller-binary .winbridge/controller-runtime-updates/20261028T071841Z-203/winbridge-controller-0.2.0
```

Перед submit сообщить: installed worker0.2.0 оставить, desktop закрыт, UI окон
нет; test networking только stable204/отдельный controller1144. Проверить≥6GiB
на Debian/Windows и pinned PS parser. Preflight firewall900s до admission,
original window240s неизменен. При unknown acceptance не replay: сохранённые
request/key/task ID — единственный источник продолжения. Legacy runtime-test
по-прежнему не трогать. По успеху reread independent proof, review→commit,
затем Job lifetime/whole commit/release/U2–U4.

**Supervisor26/view18 реализованы в source.** Typed connect/observe используют
одного resident Serviced owner, отдельный StartingController и одноразовый key;
completion сохраняет того же guard либо original failure custody. Старые actions
и view1–17 не получают нового поля; cold view18 показывает NotResident и saved
fact без текущего projection/link/permission. Live phase называется Observed:
это факт независимого normal IPC наблюдения с resident owner, а не обещание,
что соединение никогда не разорвётся. Actual current links остаются отдельной
диагностикой worker. Root всё ещё AwaitingController, physical7 без изменений.
Нет caller URL/topology/command/receipt в новом supervisor request; default
selection — pinned configured primary. Public updater capabilities по-прежнему
не объявляют activation/controller whole-update readiness.

Review добавил exact source owner/epoch/launch/config/schema checks при completion,
сверку snapshot saved receipt с resident observation и сохранение old local
service view после network phase.927 workspace/update-validation tests passed,
17 штатно ignored; cold real supervisor/14 invalid envelopes, unknown fields,
cold view и12 completion mutations покрыты. Final Linux/MSVC all-target Clippy,
generated TS/typecheck/UI build/scaffold313 прошли. Новый actor/native Windows
runtime пока **не исполнен**; следующий шаг — отдельный Main26 harness/run.

Для distinct target уже зафиксирован clean b149eed/private5/schema19+21:
shared `WinBridge/sources/winbridge-20261028T094607Z-b149eed-a3228347.zip`,
archiveSHA `cb3ec31a823be380a21caf170b26c80764b0070200307724cb0c2b68a46b817d`,
manifestSHA `fff5fb118c61b186ec9ec6dd0927fa091710be4a59ed5f96e3451471ba59c8b0`.
Экспорт сделан из detached worktree
`.winbridge/source-exports/controller-target-b149eed`, main WIP не подменялся.
Producer Main26 должен быть новым clean source после native harness commit;
source/target разные, оба updater21/worker19. Не использовать старый20 SDK как21
и не выдавать новый run за historical Main25. Installed worker48c834c прежний;
desktop не нужен, networking только из approved stable runtime-test-204.

**Same-child network/private5 реализованы в source.** Consuming preparation
передаёт один ControllerCommand по прежнему authenticated pipe; receiver связывает
exact controller intent с original RunningLocalService и запускает обычный manager
на том же Arc/Store/lock/generation/authority. FSM NotStarted→Attempted→Running
не разрешает retry даже после ошибки checkpoint.1–4 endpoints работают независимо;
close останавливает и join-ит network до закрытия IPC/освобождения state.
Новый readonly normal IPC возвращает Waiting/Observed/Rejected; Observed содержит
actual compatibility, full accepted projection и atomic current link/attempt.
Parent независимо проверяет normal IPC/PID/Info/frame после private Ready;
original240s не обновляются. Controller ledger Confirmed не меняет root local7/
AwaitingController, не передаёт Job и не открывает dispatch/maintenance.

Review: нельзя начинать сеть после stop/завершения IPC; нельзя использовать frame,
полученный до затратной проверки runtime, после смены ticket. Обе ситуации закрыты.
Ожидание projection не хеширует EXE и не перечитывает полный updater graph в каждом
цикле; poll100ms, expensive verification после появления accepted frame. Это важно
после прежнего CPU regression. Metadata fixtures не выдают native proof.
Debian tests1–4 actual TCP attempts/real IPC + unchanged authority/history/queue/
closed dispatch/duplicate refusal/network join прошли; ещё2 cases проверяют
pre-effect topology/checkpoint failure и stopped IPC. Workspace921/17 ignored
passed; после него26 focused controller tests (включая дополнительный normal IPC
probe refusal) passed. Final Linux/MSVC workspace all-target/update-validation
Clippy passed; evidence `.winbridge/review-2026-10-28/controller-network-*`.

**Новый Windows runtime пока не исполнялся.** Private5 marker теперь объявляется
из source receiver; installed48c834c остаётся private4 и не менялся. Следом
typed supervisor26/view18, где saved controller fact отделён от current residency,
и отдельный Main26 в stable slot204. Прежние Main25/retained20→21 не replay-ить.
Job lifetime/whole commit/release и U2–U4 остаются впереди. Ниже сохранены
foundation checkpoints и план guards, а не заявление готовности всего updater.

Добавлены strict DTO в protocol/update/activation_controller.rs и TypeScript
bindings. Pure activation/controller связывает frozen physical6/writer/service
с exact LocalVerified7, сохраняет original deadline и typed history
Issued→Confirmed либо Issued→Indeterminate→Confirmed.10 focused tests прошли.
Это первый data foundation checkpoint: network send/runtime/SQL ledger/capability
тогда ещё не добавлены, updater20/actor25/view17/service4 оставались прежними.
Следом добавлен ledger21/read-only graph (workspace914/17 ignored,10 SQL cases),
см. [actual20→21 результат](retained20-to21-2026-10-28.md): native copy gate пройден.

Следом добавлена **original guarded preparation**: native consuming
`prepare_controller_blocking` сохраняет child/Job/channel/files, pin конфигурации
и единственный Issued permit после подтверждённого SQL commit. Fresh factory
сверяет exact LocalVerified7/atomic AwaitingController edge, owner/epoch/current
supervisor/nonce/schema21 и original monotonic window до/после записи. Same key
возвращает только Existing data; другой selection/key — KeyConflict. На этом
checkpoint sender/network/private5 receiver ещё не были реализованы, marker5 не advertised:
старый child отвергается до intent. Actor25/view17/service4 и installed worker
прежние; public install закрыт. Подготовка отдельно не означает подключение.
269 updater library tests passed/4 ignored;13 controller SQL/root cases включают
18 root-edge подмен и rollback при завершении window до/после INSERT. MSVC
workspace/all-target/update-validation Clippy passed. Новый original preparation
на Windows ещё не исполнялся: следующий runtime gate должен проверить весь graph,
не только SQL primitives или уже пройденную миграцию с empty controller tables.

Review foundation выявил, что projection сам по себе не содержит Welcome nodeId:
добавлена отдельная проверка current normal-IPC link/attempt/node/session и
lastConnectedAt. Full frame проверяется отдельно по digest/sequence/build/context.
В test matrix есть stale reconnect, coordinated graph/config drift и чужой peer;
никакой новый connector или public permission из этих данных не создаётся.

Проверки foundation:255 updater library tests/4 штатно ignored и75 protocol
all-target tests (включая frozen legacy wire/schema); после дополнительного
positive secondary-endpoint case все10 focused tests passed. Linux/MSVC workspace
all-target/update-validation Clippy прошёл, TypeScript/generated contracts/fmt/
scaffold312 также. Windows исполнения нового runtime ещё нет: runtime отсутствует.
Исходные controller.203 f069697 и worker48c834c продолжают работать независимо от
изменяемого build cache. Evidence `.winbridge/review-2026-10-28/`.

Следующий source шаг — journal21 ledger: три immutable controller tables с FK к
service record, exact graph/readback и CAS history; old schemas возвращают None.
При fresh admission проверять AwaitingController/root revision=source.parent+1
и original local7/nonce/owner (старый activation::owned_current принимает только
VerifyingLocal и здесь неприменим). Duplicate key + другая frozen selection —
KeyConflict; Existing не выдаёт permit. Для history читать actual physical6 и
local7 из immutable history, не восстанавливать их из более поздней строки.
Нормальный open20→21 — consistent backup/epoch fence; synthetic old fixtures
удаляют новые пустые таблицы перед relabel, actual old20 берётся из retained
Main25 archive. Frozen receiver scope20 после migration не получает grant21.

## Выбранные решения

- Подключить обычный network manager к **тому же** resident local-service Worker.
  Config, Store, lock, loaded PID/birth/build, incarnation/generation и committed
  authority остаются прежними; повторный open/migration/begin_authority запрещён.
- Использовать существующий validated ControllerReadinessProjection как ответ
  controller на actual normal Info. Worker уже проверяет ticket, session,
  controller generation, свой build/generation/context и sequence. Новый сетевой
  протокол/отдельный callback listener не нужен. Readiness views/verifications
  в projection не дают permission и не должны автоматически включаться.
- Сохранить exact1–4 configured endpoints и отдельный confirmation endpoint.
  По умолчанию выбирается configured primary; trusted policy может явно выбрать
  другой configured ID. Selection замораживается в command и сверяется с config
  SHA. Payload не добавляет адреса и не меняет primary. Другие связи работают
  независимо; их offline не подменяется global connected и не блокирует
  подтверждение явно выбранной связи. Это не новая граница доверия по IP.
- Intent до effect, ровно один network-start dispatch под original resident
  guard. Все reconnect attempts остаются внутри одного command/original window;
  наблюдение не посылает команду заново. Успех требует свежего connectedAt после
  issuedAt, current ticket и принятого projection именно выбранной связи.
- Receipt содержит immutable service/local hashes, тот же compatibility/authority
  и stamp controller/provider/session/context/sequence плюс SHA full projection.
  Независимый gate сохраняет исходный frame для проверки digest. Timestamp и JSON
  сами по себе не доказывают текущую residency/freshness: нужны live owners.
- Late receipt разрешает уточнить сохранённую неопределённость, не возобновляет
  deadline, не запускает cold network и не даёт commit/release. Roots, dispatch,
  admin и user pause сохраняют прежние границы.

## Последовательность

1. Strict DTO + pure command/receipt/typed FSM. Никакого advertised capability,
   runtime send, DB schema bump или public endpoint на этом шаге.
2. Review foundation и tests:1–4 endpoints, exact config selection, deep scope,
   duplicate/uncertainty/late fact, stale/foreign projection и неизменные старые DTO.
3. Durable ledger и оригинальное guarded admission: предполагаемый updater21,
   empty new tables при actual retained20→21 copy migration/old20 refusal.
   Номера пока проектируемые, не выдавать их за реализованные контракты.
4. Same-child consuming network start/normal IPC observation с bounded original
   deadline, rollback владения при fail и proper join network tasks при close.
   Только original local-service owner; никакого конструктора из archived JSON.
5. Trusted sender/receiver/actor (предполагаемые private5/actor26/view18); frozen
   прежние версии. Parent независимо читает actual normal IPC/selected controller
   projection и проверяет тот же PID/build/config/authority/session.
6. Новый full native run в stable runtime-test-204, controller.203:1144, отдельные
   instance/state/report и старый retained worker17 reference (не installed0.2.0).
   До admission actual network preflight с900s firewall budget; original startup
   window240s не продлевается. Сохранённые Main25 не replay-ить.
7. Review→fix→Job lifetime transfer→whole update commit/scoped maintenance release.
   U2 faults/management, U3 desktop, U4 installed self-update и A4.3 сохраняются.

Controller receipt не вызывает старый root event ControllerConfirmed сразу:
он ведёт AwaitingController→Committed, тогда как Job lifetime ещё не передан.
До отдельного trusted commit сохранять root AwaitingController, physical
LocalVerified7 и закрытые roots; новая controller ledger/view показывает только
свой факт. Не добавлять physical8 или менять frozen старые view DTO ради этого.

## Проверки

Portable модель отделяет valid data от live grant; core подтверждает LocalVerified7
из exact frozen StateOwned6/writer/service. Подмены owner/process/build/config/
authority/session/selected endpoint/projection sequence отклоняются. Неверный hash
full frame, старый connected timestamp и другая связь не становятся подтверждением.
При uncertainty нет automatic replay; новые keys не продлевают исходное окно.
SQL tests проверяют одну транзакцию полного graph и immutable history. Runtime
tests — same Arc/Store/lock/epoch,1–4 sessions, stale tickets и startup cancellation.
Native gate — actual controller Info/projection, lost ACK/cold observe/no replay,
сохранённые tasks/output, закрытые roots и завершение всех owned Jobs.
