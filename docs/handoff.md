# Продолжение updater — writer24 подтверждён 2026-10-25

**Эта ветка занимается скоростью прогонов, 2026-10-25.** Текущее действие —
[оптимизация, E1 profile pilot](plans/run-speed/README.md). Пользователь поручил
продолжать эксперименты до существенных результатов в отдельной ветке/worktree;
другой агент может продолжать updater в `main`. Ниже — исходный checkpoint
этой ветки. Здесь не переходить к service/schema20 или U2–U4.

2026-10-26: [E1 результаты и текущий v2](plans/run-speed/results-2026-10-26.md).
65 Windows tests median36.165→20.6745s (42.83% быстрее),650 suite cases +2 policy
probes +130 exact diagnostic cases passed;25/131 ready files twice. Compiler
codegen overhead найден. Defaults неизменны; v2 task974c9b0c-86c4-42a2-a688-9f9e2fba26d5
завершился succeeded/0: median33.7865→18.3635s (45.65%),25 files twice,
650 suite cases +2 policy probes. Ветка задаёт v2 в Cargo.toml для полного E2;
`main`/installed runtime неизменны. Полного E2 proof ещё нет.

**Начать с [актуального checkpoint](plans/worker-self-update/resume-2026-10-25-writer-verified.md)**
и [review/latency](plans/worker-self-update/review-2026-10-25-latency.md).
Они приоритетнее исторических NEXT/ACTIVE ниже.

Run `20261025-232746-26bad6d7`, task `a49ade1f-4e35-4eff-84ac-3b8d21c0a9ea`:
**succeeded/0,193 Windows tests,39 groups/42 commands,56 ready files twice**.
Independent audits:44 replacement +16 writer +18 migration mutations rejected.
Original observer выявил ошибки verifier: reason expectations и Python1==True.
После исправлений тот же terminal run проверен readonly, без resubmit/replay.
Полный Main24 доказывает epoch+1/immutable receipt/fencing/same child+lock,
сохранность истории/Quiesced lease, actual child/Job exit и cold non-replay.

Source **updater19/worker17/supervisor24/view16/physical2+writer3**.
Новые startup admissions240s разрешены пользователем; старые120s intents неизменны.
Writer confirmed примерно через131.7s original startup age, remaining108.3s.
Source2413653, produceracf7123/target2413653. Code/proof pins и точная readonly
команда — в checkpoint. Старые failed runs не replay и не превращать в reference.

**Normal local service/reconciliation/controller/lifetime ещё не реализованы.**
Следом [normal local service](plans/worker-self-update/local-service-activation.md)
в том же child/Store/lock, без нового epoch → новая controller session → durable
Job lifetime transfer → commit/scoped release → remaining U2 → U3/U4 → A4.3.
Проектируемые20/private4/actor25/view17 не являются существующими capabilities.
Продолжение source после gate: service strict DTO/binding/pure FSM добавлены,
7 portable regressions passed. Journal20/admission/driver/receiver/normal IPC
ещё отсутствуют; код не создаёт fresh permits и не открывает Store/lock.
После foundation полный workspace791/0 (16 ignored), Linux/MSVC Clippy,
TypeScript feature/generated contracts/fmt/scaffold281 passed.
Public install/advertised activation/controller/desktop остаются выключенными.

Измерены основные затраты: Main730s включает target compiler216s; старый Main до
exit374s, backup13s, file preparation39s, replacement132s; binding44s включает
повторные SHA примерно2.23GB и graph validation. Надёжность важнее дальнейшей
оптимизации; guards сохранены. Readonly timing summary доступна в scripts.

Controller PID5312, ws://192.168.44.204:1143. Installed d052 generation
66c20413-63a9-4725-b4c1-f5d97cbfac51 connected/active0/errors=null. Config Windows
пользователь выбрал winbridge-worker.snapshot.local_204.json. Runtime не заменялся.
Native/Cargo/observer runs завершены. **Desktop можно закрыть; worker оставить.**
Перед heavy build fresh оба диска≥6GiB, CARGO_INCREMENTAL=0, один cache lease;
Windows directories/runtime/config/journal/reports не чистить. Пользовательские
настройки/паузы и границы компонентов сохраняются.

## История checkpoints

Записи ниже фиксируют прошлые source/gates. Их NEXT/ACTIVE и «ещё не реализовано»
не заменяют авторитетное состояние выше и новую подробную инструкцию.

## История checkpoint daa88da (до native18 harness/gate)

**Начать с [подробной инструкции следующей сессии](plans/worker-self-update/resume-2026-10-24-activation-preparation.md).**
Она содержит карту кода, результаты проверок, exact task/run/source/proof pins,
окружение, ограничения и последовательность продолжения. Пользователь поручил
закрыть текущие проверки и сохранить checkpoint. Updater целиком не завершён.
Нижние NEXT/ACTIVE, running/«ещё нет proof» — исторические записи; этот блок и
новая инструкция имеют приоритет.

Текущий source: **updater18/worker16, supervisor20/view12**. Flat source binding,
frozen startup FSM, graph-bound activation ledger и original-custody metadata
preparation подключены к внутреннему actor. Guard возвращается из единственного
blocking slot; plaintext nonce остаётся в resident non-Serde объекте. Duplicate
не создаёт новый nonce; cold read не восстанавливает custody. Старые views1–11
не получают поле activationPreparation. **Это подготовка, execution driver нет.**
Public install выключен; activation/controller_confirmation/desktop_update
support по-прежнему не объявлены реализованными.

Закрывающие проверки: workspace713/0 (16 ignored), focused28/0,
protocol-jsonschema53/0, Linux/MSVC all-target Clippy, generated contracts,
TypeScript/UI build, fmt/diff/scaffold262 прошли. Cargo/native runs завершены.
Подробности и логи — в новой инструкции. Debian осталось около15.4GiB, cleanup не было.
Последний настоящий Windows gate относится к **producer17/actor19/view11**:
[actual Main/migration33/0](plans/worker-self-update/windows-compatible-drain-gate-2026-10-24.md),
run `20261024-085627-3a106020`, task `a4f17a0d-4eea-4af2-8f2c-78bc6235c0af`.
Terminal succeeded/0,27 ready files twice,38 Main +19 migration mutations.
При закрытии выполнен новый readonly recheck, **без submit**. Этот gate не
проверяет последующие source/intent/FSM/ledger18/actor20. Старые tasks не resubmit.

**Следующий шаг:** реализовать combined native18 preparation/migration gate:
actual Main replacement fixture → capture20/duplicate/cold/read-only startup
refusal и actual17→18/refusal на consistent копии successful17 history. Нового
режима runner пока нет. Есть clean ограниченная fixture18 из7f6a029;
нужен distinct producer18 после harness commit. Дальше physical file/state
handoff, suspended launcher/private worker receiver, local/controller/lifetime
confirmation, remaining U2 faults, U3/U4. [Activation plan](plans/worker-self-update/runtime-activation.md).

Installed d052 подключён, active0, maintenance/recovery errors=null; runtime
не менялся. Active native tasks нет. Desktop можно закрыть, installed worker
оставить работать. Ручное обновление сейчас не требуется.

## История третьего native run: теперь terminal succeeded

**Наблюдать, не resubmit:** task `a4f17a0d-4eea-4af2-8f2c-78bc6235c0af`,
run `20261024-085627-3a106020`, producer `628c0f6` с dedicated owner stack8MiB,
fixture `efccfbe`. Observer exec82854; проверить live handle/get_task.
Лог `.winbridge/update-preparation/compatible-drain-native-owner.log`, metadata
`.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-run.json`.
Source ZIP `winbridge-20261024T085529Z-628c0f6-471d2793.zip`; target прежний
`winbridge-20261024T003539Z-efccfbe-b7a614ff.zip`; cache20261017-061527-f4639971,
`--compatible-drain --reference-pin .winbridge/workflow-validation/20261023-233438-99674bc1/support-actor-verified.json`.
При observation timeout повторять только readonly `--verify-run 20261024-085627-3a106020`
с теми же source/target/mode/reference. Ожидаются33 tests/38+19 mutations;
полного Main proof пока нет. Worker only/no UI, installed d052 unchanged.

Параллельно добавлена preparatory `ActivationSource` model, **не в native
producer628c0f6**: strict flat installed-generation binding, без authority/OS
effects.5 новых model tests +1 прежний projection, обе Clippy passed.
Новых schema/actions/capabilities нет; ledger/launch ещё не реализованы.
Карта и следующий порядок: [activation](plans/worker-self-update/runtime-activation.md).

## Предыдущие две попытки и последнее исправление

**Живых native runs сейчас нет, старые tasks не resubmit.** Второй
task21f28f01-3733-4fa7-9f31-6233d12c6327/run20261024-083044-f6ed4809
terminal failed/1. Health Verified10 прошёл, затем supervisor снова stack
overflow; timeout был следствием, переносить health admission пока нет оснований.
24 ready files twice,32 preliminary tests, actual16→17/refusal +19 mutations.
Readonly diagnostic task6c53e9d4 подтвердила main stack overflow после heap fix.
Main теперь исполняет supervisor на отдельном owner thread с8MiB stack;
package244/0 (6 ignored), Linux/MSVC Clippy passed. Test-only stderr получил
безопасные stage messages, final Linux/MSVC Clippy тоже passed. Следом clean
producer/export и fresh native gate. Installed d052 healthy/active0/unchanged.
План activation добавлен ниже как подготовка, driver не реализован.

## История второго native run: уже terminal, не текущая задача

**Наблюдать тот же task, не resubmit:**
`21f28f01-3733-4fa7-9f31-6233d12c6327`, run `20261024-083044-f6ed4809`,
producer `ddd5619` (heap mailbox fix), fixture `efccfbe`.
Observer exec session56910; PID/session перепроверить, authoritative статус
читать через controller get_task. Лог `.winbridge/update-preparation/compatible-drain-native-retry.log`.
Saved metadata/request: `.winbridge/workflow-validation/20261024-083044-f6ed4809/`.
На промежуточном наблюдении task running, затем terminal failed (см. выше).
Не считать gate законченным по наличию отдельных логов.
Installed d052 unchanged, worker only/no UI. Далее proof audit → review →
[activation](plans/worker-self-update/runtime-activation.md); это пока план, не driver.

## Предыдущая попытка: actual17 migration passed, Main stack overflow

[Первый actual17 run](plans/worker-self-update/windows-compatible-drain-first-attempt-2026-10-24.md)
20261024-003723-8bd37767 / task73081bc6-ea78-45e1-a79e-cc5f3d5a27c3
terminal failed/1.32 preliminary tests и actual16→17/refusal прошли,
23 ready files twice/19 migration mutations. Main упал до06-health/нового
production entry. Readonly diagnostic task5794c6a0 подтвердила stack overflow
главного потока isolated supervisor. Installed d052 healthy/active0/unchanged.
Heap constructor supervisor::run/run_inner добавлен; supervisor tests8/0
(2 ignored) и Linux/MSVC all-target Clippy update/worker passed.
После них clean producer export и новый namespace; old run не replay.
Проверки ниже закрывают предыдущий checkpoint, не actual Main17.

## Предыдущая точка: локальные проверки закрыты — 2026-10-23

**Начать с [подробной инструкции следующей сессии](plans/worker-self-update/resume-2026-10-23-compatible-drain.md).**
В ней карта кода, exact source/target/report/proof pins, окружение, команды
продолжения, критерии успеха и границы готовности. Она приоритетнее исторических
ACTIVE/NEXT ниже. Пользователь попросил завершить проверки и сохранить checkpoint.

[Required drain schema17/actor19/view11](plans/worker-self-update/compatible-drain.md)
реализован: immutable Blocked/Admitted decision, exact support binding,
atomic root/outbox/link и запрет legacy fallback после production decision.
**Локальные проверки завершены:** workspace688/0 (16 ignored), model/SQL7,
actual IPC19 rejection1, protocol-jsonschema51/0, обе all-target Clippy
update/worker/protocol, Rust/TS contracts, typecheck/UI build, Python3+7,
format/docs passed. Это не Windows исполнение нового route.

**Следующий шаг:** combined33-test actual17 Main + actual16→17/refusal gate.
Сценарий подготовлен, **ещё не запускался**, Task ID для него нет. Нужны два
distinct clean schema17 exports с built UI; schema16 run ниже используется
только как copy-only migration reference. После gate — activation/U2 faults/U3/U4.
Current source updater17/worker16/supervisor19/view11. Public install off,
полный updater ещё не готов; installed d052 не менялся.

Продолжение review: observer теперь сверяет полную passive decision/view11,
отдельный UUID, timestamps/reason и все duplicate/cold projections без authority
fields. Новый Main mutation audit38 (migration19); old16 recheck28 passed,
Python3+7/scaffold passed. Clean fixture17 экспортирована из efccfbe;
следом clean producer export и один actual combined gate.

[Actual Main support gate](plans/worker-self-update/windows-candidate-support-actor-gate-2026-10-23.md):
task adf48103-c02b-40e6-9442-6d089882c48c / run20261023-233438-99674bc1,
producerdb0105a, **terminal succeeded/0,23 native tests/6 groups/8 commands**.
21 ready files twice,28 mutations. Readonly verify-run passed после исправления
shadowed verifier variable. **Не resubmit**. Actual query Checked5/history6,
lost ACK/duplicate, normal IPC, owned abort и old file identities подтверждены.
Exact source/target/report/proof pins и readonly команда — в отчёте.

[Actor18/view10](plans/worker-self-update/candidate-support-actor.md) добавлен;
schema16/worker16 прежние, старые actions/views frozen. Local workspace679/0
(16 ignored), обе all-target Clippy, jsonschema/TS/UI, Python3+7 passed.
Первый native run20261023-231817-d6b1374c terminal failed/1 из-за normal Info
в control pipe; harness исправлен без ослабления guards.19 failed ready files
сохранены; это отдельная попытка, не replay. Full pins:
.winbridge/update-preparation/candidate-support-actor-native-first-failed.json.

Последний readonly recheck того же actor18 run прошёл:23 tests/21 files twice/
28 mutations. Drain в этом schema16 gate НЕ вызывался,
productionDrainEnforced=false. Не приписывать ему свойства будущего gate17.

Installed d052 unchanged/connected/active0, generation8f8d1fb9-e5b1-4984-a17e-d067db46268e,
maintenance/recovery errors=null. Controller3922 оставлен192.168.44.203:1143;
worker оставить, desktop можно закрыть, UI-окон не было.
Active native/Cargo runs нет. Ручное обновление worker сейчас не требуется.
Исторические ACTIVE/NEXT ниже не повторять.
[Полный предыдущий handoff](plans/worker-self-update/resume-2026-10-23-candidate-support.md)
сохраняет карту фундамента, окружение и ограничения; первый actor шаг в нём
теперь реализован и проверен. Форма следующего блока уточнена в actor doc выше.

## Предыдущая точка: updater checks закрыты

Начать с [подробного handoff для новой сессии](plans/worker-self-update/resume-2026-10-23-replacement.md).
В нём проверенные source/target/report hashes, код и инварианты, readonly команды,
окружение, ограничения и последовательность дальнейшей реализации. Этот документ
и указанная инструкция приоритетнее всех исторических ACTIVE/NEXT ниже.

- [Positive actual Main15 replacement](plans/worker-self-update/windows-runtime-replacement-main-gate-2026-10-23.md):
  task `665e7e2f-e734-49cf-ae87-d83498a29cbd`, run20261023-163606-15675844,
  terminal succeeded/0,60 native tests/12 groups,20 ready files twice/44 mutations.
- [Четыре actual replacement faults](plans/worker-self-update/windows-runtime-replacement-faults-gate-2026-10-23.md):
  task `63232320-8bf6-4826-b13c-315ead0b4433`, run20261023-173625-e4eea0a6,
  terminal succeeded/0,60 native tests/12 groups,32 ready files twice/68 mutations.
  Existing directory, foreign handle, receipt rejection, owned crash до receipt.
- При закрытии заново сверены оба full proofs,52 ready files дважды и source ZIPs.
  Observer завершён exit0; **активных native/Cargo runs нет, не resubmit**.
- [Readonly updater support query](plans/worker-self-update/updater-runtime-support.md)
  в commit d1b2bea: updater225/0 (6 ignored), обе all-target Clippy/TS/UI прошли.
  Она НЕ входит в native producer e567827; Windows query ещё pending.
- Updater15/worker16/supervisor17/view9. FilesMoved/VerifyingLocal проверены;
  **activation/local health/controller confirmation и U3/U4 не готовы**.
  Public install off, полный updater не завершён.

**Следующий блок:** native support query и durable candidate compatibility до drain,
затем authorized activation из resident guard, оставшиеся U2 faults, U3/U4.
Реализация остановлена на контрольной точке по просьбе пользователя; новая Windows
задача или ручное обновление установленного worker сейчас не требуется.

Controller3922 оставлен на192.168.44.203:1143; installed d052 unchanged,
generation8f8d1fb9-e5b1-4984-a17e-d067db46268e, connected/active0, errors=null,
coverage complete. Worker оставить, desktop можно закрыть, UI-окон не было.
Debian free26,487,005,184; Windows после gate35,630,841,856 bytes; очистки не было.
Все PID/session/disk перепроверять в новой сессии. Полная закрывающая сводка:
`.winbridge/update-preparation/session-close-20261023-replacement-verification.json`.

## История предыдущих точек: не текущие задания

# WIP: четыре actual replacement faults подготовлены

[Fault matrix](plans/worker-self-update/runtime-replacement-faults.md): occupied
displaced directory, foreign old-file handle, ignored retired receipt и owned
crash после OS retire до receipt. Новый mode `--runtime-replacement-faults` /
`-RuntimeReplacementFaults`, dedicated independent verifier/audit. Native fault
run пока не отправлен; **не объявлять матрицу пройденной**. Release path не имеет
fault barrier; он только в clean native Windows validation feature.
Linux/Windows all-target Clippy и production Windows check прошли; local updater/
protocol268/0 (6 ignored), Python9/0, scaffold246. Positive60/0 ниже — отдельный уже terminal gate.
Следом чистый commit/export и новый4-case native run (worker only, no UI), затем
ready/proof audit и activation. Updater целиком не готов, public install off.

# Current: actual Main15 replacement gate пройден — 2026-10-23

Начать с [проверенного gate](plans/worker-self-update/windows-runtime-replacement-main-gate-2026-10-23.md)
и [карты кода / дальнейших шагов](plans/worker-self-update/runtime-replacement-actor.md).
Task `665e7e2f-e734-49cf-ae87-d83498a29cbd`, run `20261023-163606-15675844`,
producer8457004 — **terminal succeeded/exit0**,60 native tests/12 groups,
20 ready files verified twice,44 actual-proof mutations rejected. **Не resubmit.**
Полный proof/pins: `.winbridge/update-preparation/replacement-main-native-verified.json`.
FilesMoved22, Root14→15 / VerifyingLocal; original custody=true до owned Job exit,
после него history остаётся FilesMoved/custody=false. Новое обычное startup
отвергнуто maintenance до DB effects. Worker/source/preimages/backup bytes сохранены.

**Нет текущих native/Cargo runs.** Installed d052 healthy/active0, unchanged
8f8d1fb9 generation, controller3922/192.168.44.203:1143. Desktop не нужен.
Updaterschema15/worker16/supervisor17/view9. Positive IO подтверждён, но **весь U2.2
не завершён**: следом actual fault matrix, activation/local health/controller
confirmation, U3/U4. Public install off. Ручное обновление installed worker пока
не требуется. Старые ACTIVE/NEXT ниже — история, не команды к повтору.

## История предыдущих попыток и commits

# ACTIVE: повтор Main15 после исправления test reader

Task `665e7e2f-e734-49cf-ae87-d83498a29cbd`, run `20261023-163606-15675844`,
producer `8457004c315fc5718e216fd0f364d5800a083eee`, source archive
`winbridge-20261023T163545Z-8457004-b8548a57.zip`. Task running; **не resubmit**.
Native observer: `.winbridge/update-preparation/replacement-main-native-retry.log`,
exact source/target pins: `replacement-main-native-active.json` там же.
Controller3922/192.168.44.203:1143, installed d052 прежний; worker only, без UI.
После terminal: ready files дважды, independent proof verifier, затем
`scripts/test-update-replacement-proof.py --proof SHARED_PROOF --pin ACTIVE_PIN`.
Это полная новая isolated попытка с тем же cache, не продолжение первого namespace.
58 прежних prerequisites + новый observer SDK regression + full Main дают ожидаемо
60 tests при полном success. Новый observer SDK regression уже native passed1/0:
совместимое чтение, rename, сохранение write/delete exclusion и возврат строгого
pin после освобождения owner. Полный Main retry ещё running; gate пока не объявлять
пройденным. Цель active.

# Current: первый Main15 run остановлен тестовым reader; исправлен сценарий

Task `cf49a541-0abf-4aa3-bb64-295faca99381` / run `20261023-160520-ee8053b8`
**terminal failed/exit1**, producer6ffc289. Не resubmit. 58 prerequisite native tests
прошли; actual Main достиг FilesMoved22 и VerifyingLocal. Следующий test reader
`inspect_image` несовместим с resident READ+DELETE handles (sharing violation32).
Это ошибка harness: production guard/строгие pins не ослаблялись. Добавлен test-only
reader с FILE_SHARE_READ|FILE_SHARE_DELETE и отдельный native SDK regression,
подтверждающий, что owner продолжает запрещать запись/чужое удаление.

17 ready files сверены дважды; итогового success proof нет. Exact pins/диагностика:
`.winbridge/update-preparation/replacement-main-native-first-failed.json` и
`replacement-main-native.log`. Source ZIP6ffc и retained runtime сохранить.
Installed d052 здоров, same generation8f8d1fb9, active0, maintenance/recovery errors=null.
Следом clean source export и **новый isolated run**, не повтор прежнего effect.
После success: независимый verifier и `scripts/test-update-replacement-proof.py`
(на actual proof), затем fault matrix и activation. Worker only, UI не нужен.

# WIP: отдельный actual Main15 replacement gate подготовлен

2026-10-23. Добавлены native integration helper/test, `-RuntimeReplacementGate` /
`--runtime-replacement-gate` и независимый verifier23 causal states. Windows
cross-Clippy прошёл, Python9/0; native run ещё не отправлен на момент этого commit.
Следом clean source export → изолированный positive Windows gate (worker only,
без UI), затем proof mutation review, faults и activation. Target schema15 fixture
5fbc1cf; public install off. Подробности в
[актуальной инструкции](plans/worker-self-update/runtime-replacement-actor.md).

# Updater: текущая точка — driver/actor v17, Windows replacement gate впереди

2026-10-23. Начать с [подробной инструкции продолжения](plans/worker-self-update/runtime-replacement-actor.md).
Добавлены blocking replacement driver, supervisor v17/view9, resident original
custody и trusted FilesVerified → VerifyingLocal. Схемы updater15/worker16 прежние.
**Новый actual Main replacement/activation ещё не проверены на Windows**; public
install выключен. Следом отдельный positive Main15 gate, затем faults и activation,
U3/U4. Workspace653/0,16 ignored; final schema2 CLI1/0, Linux/Windows all-target Clippy,
contracts/TS/UI, Python7+4 и scaffold244 прошли. Все проверки завершены; активных
Cargo/native runs нет. Логи и границы доказательств перечислены в новой инструкции.
Controller этой сессии запущен заново (PID3922); installed d052 connected, desktop
не нужен. Исторические PID/ACTIVE/NEXT и «не подключено» ниже относятся к прежним
commits и не являются текущим статусом. Завершённые Windows tasks не пересылать.

# Updater: preparation faults и replacement model проверены — 2026-10-23

## Current WIP: consuming custody/owned rename library

Добавлена [native custody/rename library](plans/worker-self-update/runtime-replacement-custody.md):
unique original Arc transfer с busy restoration, same process/state/instance lock,
five prepared +five old owned handles, fixed anchors, one-attempt directory/file IO
и guarded SQL success callbacks. Existing directory/unknown entries сохраняются;
failed acquisition/IO возвращают retained boxed custody. Schema15/16,v16/view8 прежние.
Local package217/0, workspace648/0,16 ignored и обе all-target Clippy passed.
**Actor/driver/trusted FilesVerified/actual positive replacement ещё не подключены.**
Новый Windows directory test только compiled, не run. Native/Cargo runs сейчас нет.
Следом blocking driver и supervisor новыйcontract/view, затем actual Main15 positive/
fault gates. До того не объявлять whole replacement проверенным. Installed d052
healthy/unchanged; public install off. Ниже готовая metadata/migration foundation.

## Current: replacement ledger schema15 metadata gate пройден

Добавлен [durable admission ledger](plans/worker-self-update/runtime-replacement-ledger.md)
и non-Serde fresh permit. Текущий source updater15/worker16, supervisor v16/view8
без новых wire actions. Package updater216/0,6 ignored; workspace647/0,16 ignored
и обе all-target Clippy прошли. [Native15 metadata11/0](plans/worker-self-update/windows-runtime-replacement-ledger-gate-2026-10-23.md)
пройден,6 ready files twice, task `e292eb2f-33b6-4d7f-b281-a954b5f24d63`/
run `20261023-091331-b6f35ac8`, producer5fbc1cf. Локальный result/pins:
`.winbridge/update-preparation/replacement-ledger-native-verified.json`.
**Нет активных native/Cargo runs, не повторять terminal tasks.** Дополнены synthetic downgrade
fixtures; предыдущие gates ниже относятся к14 и не подтверждают15.
Production callback rename-success/consuming guard/IO ещё отсутствуют. Следом
positive original-custody admission при подключении consuming guard. Installed
d052 прежний; ручное обновление не требуется.
[Actual14→15/refusal gate](plans/worker-self-update/windows-replacement-migration-gate-2026-10-23.md)
тоже пройден:task `11987f79-3786-4155-aa39-6277d74d6950`/run20261023-092901-d1c837e1,
producer aaf721f, succeeded/0.1 native test,6 ready files twice,27 mutations rejected.
Полные pins/proof: `replacement-migration-native-verified.json` в local namespace.
Integration test/runners (`--replacement-migration-only`)
и pin `replacement-schema14-reference-pin.json` из approved Main14 proof. Полный
original Prepared canonical SHA `a2b823d95c1a6cee157cbb8f9387df309d8dccca4eadec08b0c9877ee1638029`
связывает переносимые данные с прежним gate. Actual old14 init/status отказали15,
DB/WAL/reference сохранились, новая permission не появилась. Следом consuming
original custody/transition-aware guard, success callbacks/IO/actor, затем activation.

## Возобновление2026-10-23: actual preparation faults

Цель вновь active. Добавлены [три actual fault cases и feature-only barrier](plans/worker-self-update/runtime-preparation-faults.md),
новые PS/Python modes `RuntimePreparationFaults`/`--runtime-preparation-faults`.
Windows cross-check и обе all-target Clippy прошли; local preparation16/0,
Python7/0 и scaffold239 прошли после verifier review.
Native [fault gate прошёл](plans/worker-self-update/windows-runtime-preparation-faults-gate-2026-10-23.md):
task `50979d1f-6f38-4d0a-8f15-6e96a243222d`/run `20261023-081649-b9bf5a72`,
producerddce866, succeeded/0.40 tests/3 actual cases,24 ready files twice,
48 negative mutations rejected. Review усилил exact preimage receipt digest/IDs/
paths в verifier;7 Python tests passed. Original installed d052 healthy/unchanged.
Полный proof/pins: `.winbridge/update-preparation/preparation-faults-native-verified.json`.
**Не отправлять завершённую task повторно.** Controller3413, UI окон нет, public install off.
Следом — replacement ledger/fresh permit/consuming custody, rename driver и activation/U3/U4.
Параллельно добавлена [data-only replacement transaction](plans/worker-self-update/runtime-replacement.md):
`apps/update/src/runtime_replacement.rs`,6 local tests и обе all-target Clippy passed.
Exact preparation binding/retained-directory FSM +22 file steps, terminal uncertainty;
нет ledger/permit/custody/IO/public action. Эта модель НЕ входит в native producerddce866.
Её [lightweight Windows gate](plans/worker-self-update/windows-runtime-replacement-model-gate-2026-10-23.md)
тоже пройден:12 data tests/2 groups,6 ready files twice, task
`f1e772de-7a28-4ddd-9164-ed3c8a8434f8`/run `20261023-084849-45c8a2e5`,
producer7f3a952, succeeded/0. Полные pins:
`.winbridge/update-preparation/replacement-model-native-verified.json`.
**Нет текущих native/Cargo runs; обе task terminal, не resubmit.**
Текущие schemas14/16, supervisor v16/view8. Следом ledger/fresh original-custody
admission + consuming handoff, IO/actor/activation. Полный updater не готов.
Для следующего ledger: joined read exact Prepared record и его retained graph,
immutable intent/current/history и current owner/epoch/launch/root CAS; admission
проверяет original exit guard, backup и prepared files и выдаёт non-Serde permit
только один раз после commit. Existing/reopen не создают permit. После первого
rename original full-runtime guard уже неприменим: callbacks должны использовать
consuming transition-aware guard; не ослаблять прежний `ShutdownExitGuard::require`.
Новая schema потребует migration/read-only old-reader refusal и обновления synthetic
downgrade fixtures; native архивы14 и их gates не считать проверкой следующей schema.

## Зафиксированная точка закрытия перед возобновлением

**Начать с [подробного handoff 2026-10-23](plans/worker-self-update/resume-2026-10-23.md).**
Пользователь попросил завершить проверки и зафиксировать точку продолжения.
Дальнейшая реализация приостановлена до возобновления; updater целиком не завершён.
**Нет текущих native/Cargo проверок.** При закрытии readonly-повторно подтверждены
оба terminal succeeded/0, все22 ready files, source/target ZIP и полные proof
verifiers. Новый Windows run не запускался. Rust fmt и5 Python regressions прошли.

Продолжение2026-10-23: [actual13→14 migration/refusal gate](plans/worker-self-update/windows-preparation-migration-gate-2026-10-23.md)
пройден1 native test,6 ready files twice,19 proof mutations rejected. Task
4c252e55-f681-4f8f-87d2-05a18e410d28/run20261023-020641-5eca976a succeeded/0,
producer6286ad9. Реальный Verified-backup13 graph сохранён, epoch2→3, fresh permit
не выдан; actual old13 init/status отказывают14 без изменений DB/WAL. Original
reference и installed runtime не менялись. Local pins/proof:
`preparation-schema13-reference-pin.json`, `preparation-migration-native-verified.json`
в .winbridge/update-preparation. Следом actual preparation fault matrix.
[Actual Main preparation gate](plans/worker-self-update/windows-runtime-preparation-main-gate-2026-10-22.md):
40 native tests / 10 groups, task `d9805159-8804-486a-8aae-95ddb6e12e79`,
run `20261023-012740-9feb95b9`, succeeded/exit0. 16 ready files сверены дважды,
22 искажения actual proof отклонены. **Не отправлять готовую task заново.**

Текущий код: updater schema14 / worker16 / supervisor v16 / view8.
[Preparation library](plans/worker-self-update/runtime-preparation.md) и
[copy driver/actor](plans/worker-self-update/runtime-preparation-actor.md) реализованы:
полный baseline/preimages/candidate/health/shutdown/Verified-backup graph admission,
fresh non-Serde permit, directory/file intents до IO, пять independent copies,
manifest-last receipt, revisions0→12, same duplicate attempt. После owned Job exit
сохранился Prepared record, physical custody=false; history не разрешает replay.
Runtime/source/worker DB bytes не менялись; actual installed replacement не выполнялся.

Producer clean9a02ca9 / targetcee06a7, оба schema14/worker16. Exact archive,
manifest/report/proof SHA и storage приведены в gate report; полная локальная сводка
`.winbridge/update-preparation/runtime-preparation-main-native-verified.json`.
Предыдущий foundation gate cee06a7 (10 tests / 5 ready files) относится к v15/view7;
SDK/model9/0 и U2.1 happy/migration/fault gates также сохранены как отдельные proofs.
U0/internal U1/U2.1 завершены; **весь U2.2 и updater не завершены**.

Локально: workspace636/0,16 ignored. После review canonical config/runtime comparison
повторены16 preparation и2 directory tests, Linux/Windows all-target Clippy;
generated TS/contracts/typecheck/UI build, Python regressions и Windows all-target
validation-fixture Clippy прошли. Production disk threshold не ослаблялся: Debian
filesystem tests перенесены из малого /tmp tmpfs в ignored main target temp directory.

**Следом:** review → actual preparation IO fault matrix → consuming original
custody/rename ledger/driver → activation → U3/U4. Actual13 refusal закрыт выше.
Candidate cee06a7 — limited same-schema copy fixture, не production install target:
совпадение worker schema не доказывает весь updater feature set. До public cutover
добавить versioned candidate-updater compatibility check **до drain**.
Public install и raw FilesVerified/local health/controller confirmation отключены.

Installed Windows worker d052 прежний, connected/active0/no maintenance/recovery
errors; ручное обновление сейчас не требуется. Worker оставить, desktop можно
открывать/закрывать. Controller этой сессии PID3413, listener192.168.44.203:1143.
Окружение/restart/cache rules — в [новом handoff](plans/worker-self-update/resume-2026-10-23.md).
Ниже исторические записи; их прежние NEXT/ACTIVE/номера контрактов не актуальны.

## Передача контекста до возобновления U2.2 — 2026-10-22

**Начать с [подробной инструкции следующей сессии](plans/worker-self-update/resume-2026-10-22.md).**
В ней сохранены фактическая готовность, карта кода, отчёты/Task IDs/SHA, исправления,
локальная регрессия, восстановление окружения и последовательность следующих шагов.
Пользователь попросил закончить проверки и сделать коммит; новая реализация в этой
завершающей задаче не начиналась. Updater целиком не готов, public install отключён.

U0/internal U1 и **U2.1 завершены**. Последний fault gate: 23 native tests,
4 полных actual Main cases, 31 ready file; file/FSM gate: 9 native tests, 6 ready
files. Оба terminal succeeded/exit0. В завершающей задаче повторно сверены все
37 ready files и backup proof, без новых Windows задач. Номера контрактов:
updater journal13 / worker16 / supervisor v15 / view7.
Финальная локальная регрессия: **618 passed / 0 failed / 16 ignored**;
Rust fmt и 5 Python tests проходят. Причины ignored и логи указаны в handoff.

Основа U2.2 — data-only FSM, native file adapter и pure manifest planner.
**Следом:** immutable proof binding + durable preparation/replacement ledger →
consuming original OS custody → owned file driver/manifest-last → activation.
U3 desktop/migration/rollback и U4 bootstrap остаются обязательными.
Нельзя восстанавливать kernel custody из журнала или повторять неизвестный эффект.

Рабочий Windows worker d052 не изменялся: connected, active0, без maintenance/
recovery errors. Для будущих tests оставить worker; desktop можно открывать/закрывать.
Controller этой сессии PID3413; после restart проверить IPC и при необходимости
запустить заново по инструкции. Ручное обновление worker сейчас не требуется.

## Исторические контрольные точки

Ниже сохранена хронология разработки. Записи «Active», «pending», «повторить»
и прежние статусы относятся к моменту их создания. Они **не означают**, что сейчас
надо продолжать или заново отправлять перечисленные задачи. Актуален handoff выше.

## Active full backup gate; nine native file/model tests passed — 2026-10-22

**Не отправлять заново:** taskb3fd7ab5-c875-49e7-b961-2303a265ebc1,
run20261022-203814-bc4258b9, runner PTY8766. Log .winbridge/update-preparation/
state-backup-faults-observation-native.log. Producer clean6f060f0,
ZIP winbridge-20261022T203358Z-6f060f0-c3c330a0.zip,
SHAbfb2c1addbc14764a1a03ecf454b3e54450a486b86c3cb169ac9859d5ee16f9c.
Same target31fa/cache20261017-061527-f4639971. At latest observation22 base tests,
old-role build and candidate build passed; first full case shutdown/backup pending.
Goal active; installed d052 healthy, controller3413. No UI windows, worker only.
Observe same Task ID to terminal; timeout of observation never authorizes resubmit.

SDK task07e45433/run20261022-203417-4db1d2dc succeeded9/0,6 ready files verified twice.
[Evidence](plans/worker-self-update/windows-runtime-file-moves-gate-2026-10-22.md).
SDK primitive/data FSM native verified; whole replacement still NOT implemented.
Next data layer: installed_set::plan_replacement_manifest + two pure tests,
no IO/publication/approval. Final2/0 and both Clippy passed, including UUID-alias
rejection on review. Log runtime-manifest-plan-*. First full fault case now passed
including post-crash history; second actual Main case started. Do not assume remaining
cases passed until the full proof is verified.
These new manifest functions are NOT in immutable native producer6f060f0.

Next after full gate: inspect same task/proof, verify all ready files twice and
negative actual-proof mutations, update U2.1 readiness only if all4 cases prove it.
Then replacement immutable proof binding/ledger, fresh permit + consuming original
OS custody, file driver and activation. Public update and U3/U4 remain disabled.
Old target31fa is for backup validation; later schema/capability gates require
explicit matching old/new release archives. Do not reinterpret compiled schema13
helper as future-schema compatible. Prior Active/No-run entries are historical.

## No native run active; two fixes ready for clean repeat — 2026-10-22

Goal ACTIVE, installed d052 unchanged/healthy. Both preceding tasks TERMINAL:
- backup retry5a34314e/run20261022-200607-a60a7daf failed AFTER expected collision/
  same attempt/preserved destination assertions, during first post-Job readonly
  shutdown snapshot: SQLite disk I/O error. All retained report hashes verified.
  Diagnostic13146cba readonly journal status succeeds later, DB+WAL SHA unchanged;
  only SQLite SHM coordination changed. No backup replay or journal writer used.
- SDK3fc7949a/run20261022-202619-3d8f4020 failed2/3 with positive rename error87;
  old collision test accepted any error, so its pass was not proof of collision.

Changes ready: test-only 5s/25ms retry of SAME readonly snapshot only for Storage/
'disk I/O error'; Corrupt returns immediately. Preserve public pre-crash-proof.json
before owned Job stop and whitelist it. New native unit test validates retry selector.
No production restrictions changed. Backup gate now22 base tests +1 full four-case.
SDK move uses NULL RootDirectory/full path, all ancestor no-delete pins, fresh parent
ID check; no runtime fallback, one attempt. Collision requires80/183; Unicode/surrogate
pair filename and post-move sharing checked. SDK lightweight mode now9 tests (3 OS+6
FSM), without candidate/desktop rebuild. Combined Windows all-target Clippy passed.

NEXT commit/export current fixes, run SDK --runtime-file-moves-only first; after9/0
and ready verification run full --final-state-backup-faults with same31fa/cache.
Worker only, desktop unnecessary/no windows. Announce each Windows run and its end.
Do not delete retained failed dirs/reports. Never mark U2.1 done until four-case proof.
Following work: replacement binding/ledger/custody handoff → file driver → activation.
Previous Active entries below are HISTORICAL and must not cause task resubmission.

## Active retry + U2.2 handle adapter WIP — 2026-10-22

Goal ACTIVE. **Do not resubmit running task**5a34314e-e2e5-4302-80c5-3803df8b4646,
run20261022-200607-a60a7daf, runner PTY21211, log state-backup-faults-retirement-native.log
under .winbridge/update-preparation. Producer clean2fa306e,
ZIP winbridge-20261022T200548Z-2fa306e-e78efff5.zip,
SHA831cc6f460c3e70c0931e6a7ddaaf92e3c628141af44dc92131105af8f827081.
Same target31fa/cache20261017-061527-f4639971. Observe authoritative Task ID until
terminal; observation timeout is not permission to restart. No UI windows, installed
worker only; announce terminal/which worker to leave after run. Current controller3413.

Commits551f287 (data FSM/batch + reviewed runtime-replacement plan) and2fa306e
(retirement wait/marker read/verifier) are ready. Six pure model tests and both
Clippy passed. First failed task83efbf45 retained; previous entry below is history.

Parallel new uncommitted WIP: Windows-only crates/platform/src/runtime_files.rs +
lib export. Owned read+DELETE handle, no-write/delete sharing, source ID/bytes/hash,
relative destination dir handle, no-overwrite SetFileInformationByHandle rename,
one OS attempt per object. Three native tests defined, NOT run yet. SDK links in
runtime-replacement.md. Windows all-target platform Clippy passed after correcting SDK bool type; inspect
.winbridge/update-preparation/runtime-move-windows-clippy-corrected.log. No driver calls it,
no source/runtime files changed by adapter on either host. Next after compile:
review/native primitive check via approved runner, binding/ledger/custody handoff.
Prepared --runtime-file-moves-only / -RuntimeFileMovesOnly mode in existing
shutdown-primitives runners; no old reference is needed. Do not run concurrently
with the active four-case gate/cache. Three SDK tests remain native-unproven.
Do not claim adapter or U2 replacement native verified yet.

## U2.1 fixture retirement race corrected; retry next — 2026-10-22

Task83efbf45-c5df-4616-8929-086a7eeeb5df/run20261022-194847-ec2f7702 terminal failed1.
21 base tests passed, old roles and candidate built, first full case failed BEFORE
backup admission: Exited DB commit can precede shutdown driver retirement/custody.
No helper attempt executed. All11 ready files/hash checked; failed report retained,
SHA8b70cd131622a33e61cb400cddf274f230973f87a93c4f02af892065fa5ac2fe.
Windows62,294,671,360→58,604,429,312 bytes. Installed d052 healthy active0, no errors.
Test processes ended; desktop may open/close, leave installed worker running.

Fixture now performs bounded readonly ObserveStateBackup until idle+physical
custody+Replacing/no error BEFORE the first BackupWorkerState request. Fault proof
includes beforeAdmission, Python verifier requires it. Held marker reads tolerate
partial publication until the same deadline. No production precondition weakened,
no automatic replay of unknown effects. Windows all-target Clippy passed after fix.
NEXT clean export and repeat four-case isolated gate with same target31fa/cache.

U2.2 data-only per-file FSM + batch implemented, six pure tests and both all-target
Clippy passed. Explicit intent/receipt ordering, exact rename IDs, terminal uncertainty,
strict empty struct variants, fixed scope/independent roles/manifest-last. No IO,
ledger, guard handoff, supervisor apply or activation. See runtime-replacement.md.
No local Cargo/native currently active; previous Active entry below is historical.

## Active Windows fault gate — 2026-10-22

Goal active. **Не отправлять повторно:** task83efbf45-c5df-4616-8929-086a7eeeb5df,
run20261022-194847-ec2f7702, runner PTY80093. Наблюдать тот же Task ID через IPC;
PTY handle может исчезнуть после restart, timeout наблюдения не является terminal.
Log .winbridge/update-preparation/state-backup-faults-native.log. Producer clean5e456d4,
ZIP winbridge-20261022T194834Z-5e456d4-6bca4599.zip,
SHA7d78762c0dc19ed23ed05b102f036abf302b84fa33c5863f9b59a068e3687658.
Target31fa/cache20261017-061527-f4639971 прежние. Базовые21 tests пройдены,
old roles собраны, начался первый actual Main case; общий результат ещё не известен.
Installed d052 healthy, UI окон нет; desktop не нужен, worker оставить работать.

Параллельно начат U2.2 data-only per-file FSM в apps/update/src/runtime_exchange.rs
и review-план [runtime replacement](plans/worker-self-update/runtime-replacement.md).
Нет IO/ledger/authority/driver/activation. Первый pure test выявил unit-enum Serde
unknown-field gap, заменён на empty struct variants. Добавлена five-file batch
validation: manifest-last, independent IDs и immutable one-step advance. Pure suite6/0;
final Linux/Windows Clippy выполняются. Controller текущей сессии PID3413.
Эти новые файлы НЕ входят в immutable Windows producer5e456d4. Не объявлять U2.1
или U2 завершёнными до actual fault proof и нужных следующих native gates.

# Возобновление updater — 2026-10-22

Пользователь возобновил цель. Controller запущен заново, installed d052 healthy/
idle0, generation e5b29df5-6e5b-41a4-8f2b-9dc2e664e95f. Desktop не требуется.
Реализован отдельный --final-state-backup-faults / -FinalStateBackupFaults mode:
четыре actual Main backup cases, helper fault marker/PID/attempt, specific exit/
reason, preserved destination evidence, cleanup/duplicate и crash history без
custody. PS whitelist содержит только public diagnostics, private client исключён.
Python проверяет ordered cases, canonical input/binding, exact stop, file evidence,
owned cleanup и after-crash view. Native fault gate ещё НЕ выполнен.

Review исправил race в held-helper assertion: saved Running принимается только
вместе с наблюдением active/CopyingState. Обе final all-target Clippy, fmt/diff, UI build, ZIP tests3 и retained happy
proof проверены. Synthetic fault verifier отклонил9 мутаций; это не native evidence.
Preflight task179b5474-0cd6-47f2-9753-48004f8ec678 подтвердил Windows62,408,929,280
bytes и все retained caches/target ZIP. Следом clean ZIP и actual native fault gate. Target31fa и cache20261017-061527-f4639971 переиспользуются; файлы Windows
не удалять. Для четырёх полных build cases runner использует timeout60min;
это предел наблюдения/Job, не разрешение повторить неизвестную операцию.

План продолжения: native faults → review/закрытие U2.1 → U2.2 consuming replacement
и activation → U3 desktop/migration/rollback → U4 bootstrap. Подробные инварианты и
карта кода в [handoff предыдущей сессии](plans/worker-self-update/resume-2026-10-21.md).
Его статус paused — исторический, текущая цель active. Ниже сохранён snapshot паузы.

# Передача контекста — 2026-10-21, проверки завершены

**Основная цель updater на паузе по запросу пользователя.** Новые Windows runs и
feature work не запускать до команды продолжить. Нет активных Cargo/native runners.

Начинать следующую сессию с [подробной инструкции продолжения updater](plans/worker-self-update/resume-2026-10-21.md).
В ней: карта кода, контракты/инварианты, verified и unfinished части, exact archives,
порядок завершения fault gate, U2.2–U4 и восстановление controller после reboot.
[Итоги Windows gates](plans/worker-self-update/windows-state-backup-gate-2026-10-21.md)
содержат Task IDs, SHA, retained reference и выполненные команды.

| Часть | Состояние |
| --- | --- |
| U0 / internal U1 | Завершены; source/build/health/drain/exact shutdown gates сохранены. |
| U2.1 success | Пройден: 22 native tests / 7 groups, run 20261021-155003-a33e1e12, task b6c13fd7-14ee-4b5f-9158-ee9fba87b347. |
| U2.1 migration12→13 | Пройдена: 1 native test, run 20261021-154240-4bdadc21, task 9efab2b5-0357-4e80-9b42-4ab2fb540a55. |
| U2.1 faults | Feature-only scaffold компилируется/Clippy; PS/Python mode/verifier и actual execution отсутствуют. U2.1 ещё открыт. |
| U2 replacement/activation, U3 desktop/migration/rollback, U4 bootstrap | Не готовы; public install отключён. |

Current contracts: updater13/worker16, supervisor v15/view7. Native producer обоих
последних gates — clean 8bd7629; финальный fault scaffold в них ещё не выполнялся.
12 happy ready files и 6 migration files проверены дважды, verifier отклонил 15+7
искажений actual proofs. Обе final all-target Clippy и fmt/diff прошли; ранее
production v15 прошёл 610 workspace tests. Ошибка borrow в fault parent исправлена.

Installed d052 healthy/active 0, generation ee48ba22-7a9e-4ce3-8ed1-5080a90806a3,
без maintenance/recovery errors. Controller 3902 оставлен работать. Тестовых окон/
процессов нет; desktop можно открывать/закрывать, installed worker оставить.
Ручное обновление сейчас не требуется. Debian ~26GiB, Windows 34,747,117,568 bytes;
измерять заново перед build. Windows dirs/refs/reports не удалять.

**После разрешения продолжить:** закончить отдельный four-case backup fault mode,
проверить owned crash/replay/partial file/report mismatch, экспортировать новый
clean ZIP (старый 8bd не содержит hooks), выполнить isolated native gate через
installed worker с прежним cache 20261017-061527-f4639971. Затем review/реализация
U2.2 replacement и activation. Подробные шаги и ограничения — в resume документе.

## Исторические записи предыдущих шагов

Ниже сохранена история разработки. Формулировки «NEXT», «ещё не выполнено»,
«goal active» отражают момент записи и не отменяют текущий статус выше.

## U2.1 actual Main backup gate prepared, NOT run yet — 2026-10-21

Extends existing update_shutdown.rs gate with optional finalBackup flag: one None
case uses v15 marker-required stop, then owned external parent requests backup
AFTER Main exits. Real seed task succeeds with stdout/stderr and duplicate same
Task ID before Main startup. Parent checks all source/copy SQLite rows, DB/WAL byte
hashes unchanged, full Verified record, custody held and duplicate same attempt.
No runtime replacement, desktop or installed worker mutation. Owned Job cleanup
still proves historical shutdown without reconstructed custody.

Feature-only updater update-validation exposes an in-memory fixture client request;
worker update-validation forwards the feature. Private generated capability goes
ONLY to local supervisor-control.private; it is excluded from shared whitelist,
proof/logs and Git. Default installation/build does not enable this feature. Parent
validates its hash against saved supervisor before use. No installed token copied.

PowerShell -FinalStateBackupGate (sets Main mode) runs state-backup FSM/data/ledger,
SQLite WAL/pins, exact negative CLI and one full Main backup case; expects1 case.
Python --final-state-backup-gate verifies exact source/target, canonical intent,
helper/PID/cleanup/header/authority, readonly flags, full-row preservation/custody/
duplicate. Three existing ZIP runner tests pass. Both full-target Clippy and UI
build pass; Windows fixture cross-check passed. Actual PS/native execution pending.
Logs state-backup-fixture-* and state-backup-native-fixture-check.log. Production
actor already passed610 workspace tests; this chunk is validation-only.

NEXT export clean current source A with built UI and distinct target C31fa634
(schema13/worker16, UI built from detached temporary worktree with reused Node deps),
then approved runner --final-state-backup-gate --cache-run20261017-061527-f4639971.
Installed d052 healthy/idle0, controller3902 alive. Worker only, no desktop/test UI;
announce native run before launch. Recheck ready hashes/proof twice after run and
mutate actual proof for negative verifier checks. Schema12 migration/refusal and
backup fault gate still follow; preserve previous12 reference dirs/DBs. No U2.1/
public update completion yet. No Cargo/native currently active; Debian~26GiB.

## U2.1 supervisor v15/view7 locally verified; native backup gate NEXT — 2026-10-21

Added ShutdownCandidateForBackup / BackupWorkerState / ObserveStateBackup and typed
candidateStateBackup projection. New typed clients validate v15/view7 explicitly;
no fallback. v14 shutdown still has its old meaning/shape. v15 stop preflight checks
exact old helper export marker BEFORE durable shutdown admission. Backup admission
runs blocking while mailbox remains available, ACK follows durable fresh intent,
then one owned driver dispatches. Lost ACK does not revoke committed ownership;
duplicates read saved attempt. One work slot forbids a second physical driver.

Custody keeps the same guard in Arc; a private immutable loan survives blocking
execution, with no mutex held during IO. Supervisor retains the original Arc,
so status remains responsive and physical custody stays visible. If mailbox dies,
driver keeps its loan through owned cleanup; no new guard is synthesized. This
changes representation of the planned return-to-custody, not authority/guard scope.

View7 shows Prepared/StartingHelper/CopyingState/VerifyingBackup/Verified or terminal
uncertainty and saved output digest/size/reason. Historical Verified survives loss
of active driver but grants no physical authority; shutdown custody is separate.
All prior view JSON omits candidateStateBackup. Actual mailbox test verifies no-
custody/unquiesced rejection, no root/worker/backup effects and frozen v14 shape.
Pure view test verifies history/current separation. TS declarations regenerated.

Validation: full workspace610 passed/0 failed/16 ignored; both all-target Clippy
update+worker pass (Linux all-features; Windows update-validation/no bundled C).
Generate/typecheck/fmt/diff pass after registering2 new types in explicit exporter.
Logs state-backup-actor-*. No positive full native export proven, U2.1 NOT complete.

NEXT native success gate: extend existing feature-only actual Main case with v15
stop and after-Main backup request from owned external parent; source/copy rows,
WAL/DB bytes, authority, receipt, durable history and duplicate/no second helper.
Need private test-only handoff of generated fixture supervisor authority to its
parent; keep it ONLY local, never in shared reports/proof/logs, no production API.
Then fault cases and actual12→13/refusal using retained old12 references. Target C
must be schema13 (31fa634 or later), not A294/updater12. Production update still off,
bootstrap U4 only when complete. Installed d052 healthy idle0; controller3902 alive.
No Cargo/native active. Desktop unnecessary; keep installed worker, no manual update.
Debian~26GiB; no user runtime/config/journal or backup changes.

## U2.1 consuming backup library compiled; actor/native pending — 2026-10-21

capture_final_state consumes a fresh non-Serde StateBackupPermit and borrows original
ShutdownExitGuard; caller must retain custody afterwards. It validates exact image/
config/support, pins parent, create-news/syncs one input, records Issued before
create and PID/grant before resume via existing owned ProbeObserver. Cleanup is
saved before parsing report; actual backup file/SHA and guard/config/input are
rechecked before durable Verified. Failure observes saved revision and records
terminal Indeterminate where still authorized; no rerun, overwrite, install or
restore. Parent File ID is checked before first input effect and created input
File ID survives the writer→reader pin handoff. These are implementation checks,
not an executed positive native scenario yet.

Shared StateBackupFilePin replaces helper-specific hashing, keeps streaming memory
bounded and avoids executable256MiB limits for SQLite data. Windows read-only pin
denies write/delete; revalidation checks full stream+identity. One new real file
test passes (full tail hash, empty reject, changed content detection / Windows
sharing branch pending native). Targeted state-backup tests14/0; actual negative
CLI2/0; both update+worker all-target Clippy pass. Logs state-backup-driver-*.
Native driver dispatch and complete12→13/old12 refusal remain pending. CLI guard
from31fa634 exists, schema13/worker16 unchanged, public update disabled.

NEXT supervisor v15/view7: keep all v1–14 JSON/actions frozen, add typed backup
status/start and marker-required shutdown preflight before admission. Retain exact
OS custody throughout blocking library call; mailbox remains responsive and one
physical driver active. Ordinary routine choice can use immutable Arc loan of the
same guard (supervisor keeps original, driver keeps Arc until owned cleanup), no
new guard/authority from JSON. Then combined native full gate with same-schema13
old/candidate builds; retained old12 binaries/DBs are references for migration only.
Do not reuse old candidateA294/updater12 for schema13 health success. Preserve
cache/report/reference directories. Controller3902, installed d052 still healthy,
idle0/no errors; desktop unnecessary/no test windows/manual update yet.
No Cargo/native active; Debian~26GiB. No user config/backups/runtime changes.

## U2.1 old-helper exporter implemented; native success pending — 2026-10-21

Added actual early Main --export-update-state INPUT SHA mode and embedded contract1
marker. It never constructs Worker, starts scheduler/IPC/controller, claims owner,
migrates or changes recovery marker. Input is StateBackupIntent; no destination
argument. prepare_export pins input/config/updater policy/runtime set/source state/
backup parent and exact live owning supervisor. Runtime comes from current exe;
readonly schema13 snapshot must match self OS PID/input/grant/current owner/root.
Source and destination derive from checked policy/one saved attempt only.

Old worker opens Store readonly, checks exact schema16/authority/acceptance, uses
SQLite backup API, hashes/holds the created backup and checks copied header/pages/
application ID/authority/receipt plus source authority and physical pins. Existing
worker application ID0 is preserved, never assigned/migrated. Report binds actual
self/image/input, SHA/bytes/File ID, pages, source/copy authority fingerprint and
original shutdown receipt. This gives no replace or restore authority.

Validation: two actual CLI negative tests pass (bad args/digest/untyped destination
rejected without creating state); three real archive tests pass; both update+worker
all-target Clippy checks pass (Windows update-validation, no bundled C cross-build).
Logs state-backup-exporter-*. Source-only implementation: full positive joined
export/native Windows sharing/owned cleanup not yet proven. Do not mark U2.1 done.

NEXT consuming supervisor backup driver: fresh guard custody and admission, input/
Issued before create, saved PID/grant before resume, exact owned cleanup and report/
actual backup verification before durable Verified. Preserve/return original
ShutdownExitGuard to supervisor, never reconstruct from history. Require helper
marker before any new backup-capable shutdown admission; keep legacy views/actions
explicitly frozen. Then native full gate including actual12→13/old12 refusal from
retained references. U2–U4 public installation disabled/goal active.
Controller3902 and installed d052 remain unchanged; desktop unnecessary/no test UI.
No Cargo/native active. Debian~26GiB. No user config/runtime/journal/shared changes.

## U2.1 readonly export attempt snapshot locally verified — 2026-10-21

StateBackupRecord::require_export_process only accepts original saved helper PID,
input path/bytes/SHA/File ID and unfinished ResumeGranted/Running state. Prepared,
Issued, cleanup/Verified/Indeterminate cannot authorize export. This is data-only;
calling it with JSON does not grant OS custody or a launch permission.
UpdateJournal::inspect_state_backup_export reads backup history + current owner,
epoch, active update/Replacing revision and Active supervisor in one readonly
transaction. It refuses schema!=13 without migration. An actual helper must still
supply its own OS identity and pinned input, validate policy/image/current live
supervisor and preserve source files. CLI/export/consuming driver remain absent.
13 targeted state-backup tests pass, including8 changed PID/input cases and readonly
missing-attempt/schema12 preservation. Both updater all-target Clippy checks pass;
no new native run and no installed update. Logs state-backup-export-*.
NEXT old-helper exporter with physical pins and SQLite evidence, then consuming
supervisor/custody/native gate. U2.1–U4/goal remain active. Controller3902 and installed
d052 unchanged; desktop not required, leave worker running. No Cargo/native active.

## U2.1 durable backup outbox locally verified — 2026-10-21

Updater schema13 now persists immutable StateBackupIntent, CAS current record and
full causal append-only history. Reads join exact shutdown/health/supervisor/policy
scope. Fresh admission and every append require actual original ShutdownExitGuard,
current owner/epoch, unchanged Replacing root and parent directory File ID; readonly
Existing never returns a fresh permit. No helper launch or runtime effect enabled.
Migration12→13 keeps consistent backup/owner lock/epoch fencing; no invented backup
or reconstructed custody. All downgrade fixtures drop the new child tables first.
Six new ledger tests pass; synthetic fixtures do not claim native OS/complete
schema12 shutdown-graph migration coverage.

Review fixed Store::archive_read_only releasing create-new handle too early:
retained handle, Windows no-delete sharing, identity rechecks, no SQLite CREATE,
parent sync. Three real SQLite tests pass: uncheckpointed WAL/source bytes,
existing/partial destination/stale authority, destination rebinding. Native Windows
sharing still needs gate. Test-only fixes: copied rehearsal fixture lifetimes
serialized for inherited writer FD/ETXTBSY; short Unix owned probe waits for parent
PID/resumed callback. No production identity/launch checks weakened.

Validation: workspace no-fail-fast covered602 passed/1 failed/16 ignored; only
failure was short probe PID race. After fixture fix updater lib97/0 passed, so603
workspace tests pass across the final group results. Rehearsal5/0 and archive3/0
included. Clippy update+worker all-targets: Linux all-features and Windows with
update-validation pass; bundled C SQLite Windows cross-build unavailable (lib.exe
missing), not a Rust failure/native claim. Fmt/diff clean. Logs state-backup-ledger-*.

NEXT readonly old-helper export CLI + saved attempt/current-owner/input/self PID
validation in a single snapshot; consuming supervisor/custody integration/native
gate per final-state-backup.md. Check support before stopping worker. U2.1–U4 remain
unfinished, public update disabled. Preserve actual old12/native refs. Installed
d052 still healthy/idle0; no manual update needed yet. Desktop not needed, no test
windows were opened. Controller3902; no Cargo/native active at this checkpoint.
Debian~26GiB; configs/journals/shared reports not modified, no backups required.

## U2.1 pure backup attempt FSM locally verified — 2026-10-21

apps/update/src/state_backup.rs adds immutable canonical digest binding the full
Exited shutdown record, old config/schema and policy directory identity. Input and
output filenames derive from one attempt ID. FSM Prepared→Issued→ResumeGranted→
Running→Cleaned→Verified; cleanup before launch/unresumed paths cannot verify;
Indeterminate terminal/no replay. PID/input/cleanup/report scope and source/output
aliases checked. Data-only methods never mint kernel custody or launch authority.
Five synthetic metadata tests passed (no native/SQLite export claim), both update
all-target Clippy checks passed; logs state-backup-fsm-*. Protocol layer already
passed45 tests, generated TS/typecheck. Current updater schema remains12, CLI and
capability advertising unchanged. U2.1 is not complete.

NEXT schema13 ledger with full immutable history/CAS and explicit migration; root
stays Replacing, raw post-cutover claims remain blocked. Fresh admission must take
actual ShutdownExitGuard, current supervisor/owner/epoch/root joins and locally-
derived existing runtime/update/<id> directory. Readonly duplicate/reopen never
returns a fresh permit. Saved process/grant must precede OS resume; final receipt
requires exact report + owned cleanup + actual file pins while custody held.
Update all synthetic downgrade fixtures to drop new tables before older schema.
Keep old12/native reference directories intact. Then old-helper CLI export and
consuming supervisor driver/native gate per final-state-backup.md. No installed
manual update needed yet; Windows worker d052 only needed for future native tasks.
No Cargo/native active; controller3902. Goal U2–U4 active.

## U2.1 final-backup report contract locally verified — 2026-10-21

Added protocol/update/state_backup.rs: StateBackupAuthority fingerprint/epoch,
StateBackupFileEvidence and WorkerStateBackupReport v1. Envelope checks preservation
of authority, acceptance/lease/state linkage, actual helper host/session shape,
distinct backup File ID, SQLite pages/bytes, read-only/no-worker/no-scheduler/no-
recovery/runtime effects. Data only, cannot authorize backup or replace.
23 negative mutations +unknown processExited rejected. Full protocol tests all
features passed45/0; both protocol all-target all-feature Clippy, generated TS and
UI typecheck passed. Logs .winbridge/update-preparation/state-backup-*. Existing
capability advertising, worker CLI/control/legacy RPC and updater12 unchanged.

NEXT pure StateBackupIntent/Record FSM in apps/update (no effects/permits from JSON),
then reviewed schema13 ledger, old-helper readonly SQLite export and driver/native
gate. Ensure helper supports mode before stopping worker; input/destination are
policy-derived, join one saved attempt+actual PID-before-resume/current owner epoch.
Retain OS guard across preparation/backup/receipt; history cannot restore custody.
U1 complete as below; public update/U2–U4 still unfinished, goal active.
Installed d052 healthy/idle0, controller3902; no native/Cargo active, desktop not
needed and no manual update yet. No user config changes/backups needed.

## Internal U1 COMPLETE; U2.1 final backup NEXT — 2026-10-21

All internal U1 source/build/health/drain/exact-stop checklist items are complete.
Public installation remains off. Main four-case gate2df5f44 and supplemental
source9ca5039 passed. Latest task99937c64-7cb7-42f7-9ef0-81e3fb19bdec,
run20261021-094402-73c4f112:1 integration test/2 cases/3 commands,13 ready files
verified independently twice,9 further proof mutations rejected. ReportSHA
8f5cc63f8fdb600c07216440342c4599ac85579774df218715967f06bb97f03c;
proofSHA575df2a192b42f45046bcc2997ac79561e6cf22071a34e03334b8b050f990d7c.
Details plans/worker-self-update/windows-shutdown-supplemental-gate-2026-10-21.md.
Helper PID2440 held original lock after Main5600 exited; driver stayed Stopping/ACK
until release, then Exited/ Replacing. Crash before Main16424 exit retained exact
Stopping/Acknowledged, same cold receipt and no custody. Native refs remain intact.

Review fixes AFTER producer9ca: f4737e4 bounds helper lock acquisition because
Weak None can precede destructor completion (fixture-only; Windows Clippy passed).
b637fb0 rejects fresh FilesVerified/LocalHealthVerified/ControllerConfirmed through
raw lifecycle until trusted U2 drivers exist; historical duplicate lookup preserved.
166 updater tests,0 failures,6 ignored plus both all-target Clippy/fmt/diff passed.
No need repeat six native cases for these changes. Settings/retention/recovery
update barriers were reviewed; startup and lease guards remain intact.

NEXT implement reviewed plans/worker-self-update/final-state-backup.md (U2.1).
Start typed helper contract/FSM, then updater schema13 durable attempt/receipt,
readonly old-worker SQLite export, supervisor custody/owned probe integration and
native gate. Existing Store::archive_read_only uses SQLite backup API. Require
new old-helper capability BEFORE shutdown admission; after exact exit backup must
include acceptance/final callbacks, before first runtime file effect. Capture/keep
live guard, never recreate from saved Exited/backup. Migration13 must fence old
owner and preserve full schema12 histories without adopting custody; existing
actual gate DBs/old12 binaries can provide consistent-copy/native references.
Public replacement/activation/desktop/rollback/U2–U4 and goal remain unfinished.

ControllerPID3902/session69990 running; installed d052 generationee48ba22 healthy,
idle0, coverage complete/no errors after gate. All native/Cargo sessions terminal.
Leave installed worker running; desktop can open/close, no test UI/no manual update.
Windows47171813376→46212440064 bytes; no cleanup. Debian~26GiB, earlier unused
incremental cleanup227MiB only. No user config edits or backups needed.

## Supplemental lock/crash native gate ACTIVE — 2026-10-21

Task99937c64-7cb7-42f7-9ef0-81e3fb19bdec/run20261021-094402-73c4f112.
Session66672, log shutdown-supplemental-native-run.log. Do not submit another.
Source9ca503964e99c5c2991d8bc2133cc6d3049bbcee; ZIP
winbridge-20261021T094333Z-9ca5039-8e3e52d5.zip SHA
96c1962cdb92ee841ad986b6a34822ea915ec1bb4d5e26bc4905c0f403d67a3b.
Source pin shutdown-supplemental-source.json; same verified A294 target/archive
and original cache. Mode --shutdown-main-supplemental selects only two new cases.
Both feature Clippy checks/Python archive tests/fmt/scaffold passed. Source clean.

Required Windows: installed worker only; desktop unnecessary, no GUI expected.
Owned external lock helper and owned pre-exit crash are deliberate fixture actions.
No manual installed update needed. U1–U4 stays active; native result pending.
Debian available28387192832 bytes before run; Windows script records fresh storage.
Four-case gate source2df5f44 is already passed; report/metrics immediately below.
After supplemental completion verify ready+typed proof independently, fresh installed
health, notify user test processes closed/worker should stay running. Then review
U1 readiness and implement U2 final consistent backup before any replacement.

## Supplemental Main lock/crash fixtures prepared — 2026-10-21

Default four-case gate remains passed at source2df5f44. Added private feature-only
DelayedInstanceLock and SupervisorCrashBeforeExit. Independent owned integration-test
helper locks original worker.lock after normal serve drops all Worker Arcs and
before Main exits; native parent must see Main exit while Stopping/Acknowledged
persists for2s, then release helper and await Exited/custody. Helper metadata is
synced before ready marker. Crash case keeps Main alive after ACK/proof, parent
kills only its owned validation Job, then verifies unchanged Stopping/Acknowledged,
cold receipt, blocked startup and absent custody. Ready marker follows synced proof
so parent cannot read a partial file. Both helpers' records/guards retained.

Mode: validate-update-health-actor-native.py --shutdown-main-supplemental with
same verified A294 target and original cache. Runs only two new cases; do not
repeat the prior four unnecessarily. Both feature all-target Clippy checks and
three Python archive tests passed; prior real native proof still passes extracted
per-case verifier. No new native result yet. Production driver/protocol/schema
unchanged, installed d052 healthy and does not require manual update.

NEXT commit clean source/export with built UI, then automated supplemental gate.
Before run announce worker required/desktop unnecessary/no GUI. Disk >=6GiB
report; Debian26GiB, Windows last47274409984. No local Cargo/native active.

## Actual Main shutdown gate PASSED; supplemental faults NEXT — 2026-10-21

Task1615bebb-015e-4155-b263-082aac5a1631/run20261021-090004-a229a8de succeeded0.
1 native integration test /4 complete cases /3 commands, 17 ready files checked
independently twice;9 tamper mutations rejected. ReportSHA
f6af84ba71dd743a6dc54c18604232d73390ff8363e1ad0d054ea9c2d2fa0730;
proofSHA e7d567289fc3b02b6c6f83347ff23cfeed1c774fcd3c6f3dde9b3ad37d780e28.
Details plans/worker-self-update/windows-shutdown-main-gate-2026-10-21.md.
Source2df5f44/targetA294, worker16/updater12, true Main+feature-only fixture driver,
real four old roles and candidate/helpers. ACK, forced cold lost-ACK recovery,
delayed callback and receipt-write denial passed. One stop per case; actual exit,
live supervisor custody, cold receipt, normal restart refusal and post-exit owned
supervisor crash/history without custody verified. Denial remains Stopping/Uncertain;
fixture stop explicitly separate. No binary replacement/activation/public update.

NEXT add two supplemental cases without repeating these four unnecessarily:
1) independent instance-lock holder after Main retires, prove driver waits after
exact OS exit before acquiring original lock;2) hold Main after acceptance and kill
only its owned validation Job before exit is journaled; retain Stopping/Acknowledged,
never synthesize Exited or restore custody. Explicit feature-only wiring allowed,
normal production Job boundary unchanged. Then finish U1 audit and proceed U2 final
consistent backup before replacement, consuming live guard only. U1–U4 stays active.

Old verified worker16/updater12 binaries remain under this run's
shutdown-old-set/payload and None/runtime. Future references can authenticate the
None/runtime manifest against shutdown.intent.binding.loadedWorker.manifestSha256
in the verified proof before reading expected image hashes; no need to rebuild old
references or overwrite caches. Default worker is in old-set/payload, Main feature
worker in None/runtime. Do not delete either reference directories.

Fresh installed d052 generationee48ba22 healthy/idle0/no errors; no manual update.
ControllerPID3902/session69990 remains. Native sessions44358/46199 terminal; no
native/Cargo active. Windows free48891133952→47274409984. Debian26.44GiB; unused
incremental cache cleanup freed227MiB, log shutdown-main-debian-cache-cleanup.json.
No user configuration changed/backups needed. Desktop unnecessary, no GUI windows.

## Actual Main shutdown native retry ACTIVE — 2026-10-21

Task1615bebb-015e-4155-b263-082aac5a1631/run20261021-090004-a229a8de.
Unified session46199, log shutdown-main-native-retry.log. Do not duplicate.
Clean source2df5f44819e1189bc7748f4661b66075c865df16, ZIP
winbridge-20261021T085927Z-2df5f44-a4856110.zip SHA
9654a6009cf6dd16649e930d1f1f69c97c12f6c0229ae746ae2a19b25783fc0f.
Source pin shutdown-main-retry-source.json; target a294e03 as below. Same
approved cache run20261017-061527-f4639971. Supervisor build passed30.64s;
old real set build in progress. Final native result pending, no success claim.
Installed worker generationee48ba22 healthy/idle before submit, no manual update.
Required Windows: installed worker running, desktop unnecessary, no GUI windows.
Prior failure was fixture language assertion only; first run did confirm actual
exit/custody/cold receipt before failing its restart check. Owned fixtures retired.

## First actual Main gate terminal fixture assertion; retry next — 2026-10-21

Task2da628ca-8936-4808-a8d9-81cf7bcb1753/run20261021-084556-c0c3f50c failed101
only at restart_refused: expected English "unfinished", real Main returned the
correct Russian startup-guard error and update ID. Prior assertions reached actual
Main exit, atomic Exited+Replacing, surviving supervisor/held lock and cold receipt.
No whole-gate pass; lost/delayed/denied cases did not run. Owned test Job retired;
installed worker unchanged/healthy. Shared logs include None stderr/supervisor stderr.
Storage51181858816→48968302592 bytes; Debian26GiB, no cleanup.

Fix assertion to verify exact update ID/non-ready/error exit, plus independent DB
lease check, without relying on display language. Sequential cases now share one
verified target source path for Cargo reuse; runtime/state/installation/guards
stay separate and previous owned Job must be empty before the next case starts.
Source/state/runtime data never copied from installed worker. Rebuild clean source
and repeat automatically, as authorized; do not ask user for a worker update.

## Actual Main shutdown Windows gate ACTIVE — 2026-10-21

Task2da628ca-8936-4808-a8d9-81cf7bcb1753, run20261021-084556-c0c3f50c.
Runner session44358; do not launch a duplicate. Log shutdown-main-native-run.log.
Shared workflow-validation/<run>; local .winbridge/workflow-validation/<run>.
Source e2eff02a36cd19b2e8f654a32d20dac325b97d78, archive
winbridge-20261021T084539Z-e2eff02-79ad8c1a.zip SHA
15840aa9ed97a5185535a3e107245a9cfdb5f65fcab1074f3874c7c919edbcd2.
Target a294e03100712ecf5a3e5ed836ef008bf084b3c4, archive pinned below.
Both same worker16/updater12; target is intentionally older for isolated validation.
Current progress: supervisor build passed39s, old real workspace passed39s, native
desktop compile in progress. Four Main cases have not run yet. No success claim.
Windows report records >=6GiB before builds; Debian before run26.22GiB, no cleanup.
Installed d052 worker only executes the outer task. Desktop not needed/no GUI.
After completion verify all ready files and typed proof through runner, then fresh
installed health and report windows can close, worker should stay running.

## Actual Main shutdown gate prepared — 2026-10-21

Feature update-validation adds explicitly isolated startup wiring to normal Main:
normal runtime guard/open/serve/retirement remain. Default build/install scripts
never enable this feature; public update remains off. Private driver releases its
Worker Arc before shutdown, returns after acceptance and lets Main actually exit.
Native integration update_shutdown covers ACK, lost ACK, delayed receipt callback,
receipt write denial, actual cold CLI after exit, ordinary Main restart refusal,
supervisor survival/custody and owned supervisor crash. It retains all case DBs,
guards and logs. It builds four real old roles and an actual same-schema candidate.

Run existing scripts/validate-update-health-actor-native.py --shutdown-main-gate
with distinct clean source/target archives and original cache run20261017-061527-f4639971.
Target a294e03 exported ZIP already exists, worker16/updater12; using this older
source deliberately tests same-schema shutdown, not migration or upgrade direction.
Both feature-enabled all-target Clippy checks passed. Python archive admission
checks and17 control regressions passed; built UI refreshed. Lost-ACK fixture
closes its listener to force integrated cold reconciliation. Combined native result
is still pending; do not claim U1 finished.

## Shutdown supervisor v14 locally verified; combined native gate NEXT — 2026-10-21

Consuming driver implemented in apps/update/src/shutdown/driver.rs; details
plans/worker-self-update/exact-shutdown.md. Readonly blocking preflight pins exact
live process/state/runtime/config/candidate and checks old cold-reader support
before durable admission. Mailbox stays responsive; original ACK waits for commit.
Issued precedes one RequestShutdown; missing ACK reconciles exact live/cold receipt,
never resends. Wait retained process object and original instance lock <=60 seconds;
atomic Exited+Replacing requires ShutdownExitGuard. Supervisor retains non-Serde
Custody beyond driver retirement. Historical Exited cannot recreate authority.
V14/view6 has separate phase/timestamps/current custody; older views omit field.
No replacement/startup/activation/rollback/public update yet. Final consistent worker
DB backup is still required before first U2 replacement.

588 workspace tests passed,0 failures,16 ignored; targeted projection and v14
negative mailbox tests passed. Both all-target Clippy checks passed (Linux and
Windows target); generated TS/typecheck/fmt/diff passed. Logs
.winbridge/update-preparation/shutdown-driver-*. No native
claim for driver: actual Main + independent supervisor exit/fault gate is NEXT.
Production Main currently enables neither update control nor supervisor launch;
validation must not silently substitute a synthetic Worker for Main. Use explicit
validation-only startup wiring, retain normal serve/close behavior, no production
external-Job override or breakaway. New positive gate must cover ACK/lost ACK,
stale authority, delayed callback/lock, receipt write failure and owned supervisor
crash. Prior primitive/reader gates stay valid, need not repeat them.

Controller restarted normally after session loss, session69990/PID3902. Fresh
installed worker d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2 generation
ee48ba22-7a9e-4ce3-8ed1-5080a90806a3 healthy/idle0/no maintenance or recovery error.
Desktop unnecessary; leave installed worker running. No native task active and
no manual installed update needed yet. Debian available~29GiB; retained Windows
measurement~39GB. No caches cleaned or user configs changed; no backups needed.
Goal U1–U4 stays active, A4 remains paused.

## Cold reader native gate PASSED; shutdown driver NEXT — 2026-10-21

Task9f70a474-5fdd-48e8-8aae-50a313e97172/run20261021-043442-51698f37 succeeded0.
4 tests/3groups,7 ready files independently checked twice. ReportSHA
84bd67e52d0fe5f1c3674fd33af4674a42227409c9af3a91c8b2ea7df1817aad.
SourceA294 and archive pins below; .winbridge/update-preparation/shutdown-reader-native-verified.json.
Details plans/worker-self-update/windows-shutdown-reader-gate-2026-10-21.md.
Actual CLI/client on Windows preserved DB/authority/guard with busy and dropped
fixture owner; report forgery/v6 contracts passed. Synthetic intent metadata, not
actual Main stop. No need to repeat this gate or prior19 primitive tests.
Windows free40189407232→39856279552, no cleanup. Installed d052 generation78e9bbe1
healthy idle0/complete recovery/no errors. Session50830 terminal0. No Cargo/native
active, only docs dirty. Worker should stay running; desktop unnecessary/no windows.

NEXT implement consuming shutdown supervisor driver + versioned mailbox/view.
Preflight may be readonly async work before admission: probe v6 support/state and
cold marker/pins on blocking thread, keep main mailbox responsive, then commit
rootStopping+intent before ACK and dispatch effects. Do not expand legacy v1.
After Issued call exactly once; reconcile through live receipt or the gated cold
reader. ACK != exit; retain exact process handle from BEFORE stop and acquire
original instance lock only after exit. Root Replacing+Exited are already atomic.
IMPORTANT retain ShutdownExitGuard in ephemeral supervisor custody after driver
retirement for U2; serialized Exited cannot restore physical authority after crash.
View should distinguish historical verified exit from current custody. Unknown
outcome never resends stop, restarts worker or removes startup guard.

Then combined actual Main + independent supervisor gate: ACK/lost ACK, stale
scope/epoch, delayed callback/lock, receipt write denial, owned supervisor crash.
Trusted test harness may prepare supervisor bound to real worker outside candidate's
OWN execution domain while parent test Job remains; do not weaken production
external-Job refusal or add breakaway. Owned crash simulation after exit may leave
startup guard/active update intact to prove recovery; never release via a dead
worker or invent a prod escape hatch solely to clean fixtures. No installed update
needed yet; public replacement/activation/rollback/U1–U4 remain open/goal active.

## Cold reader native gate ACTIVE; do not duplicate — 2026-10-21

Task9f70a474-5fdd-48e8-8aae-50a313e97172 / run20261021-043442-51698f37,
runner session50830. Source clean a294e03100712ecf5a3e5ed836ef008bf084b3c4,
WinBridge/sources/winbridge-20261021T043231Z-a294e03-6d95065c.zip,
archiveSHAd0f79b40da71e624087d4ac2531ed56d30e018de24839ef70c4f210ca00d3607.
Pin .winbridge/update-preparation/shutdown-reader-source.json. Run via existing
validate-update-shutdown-primitives-native.py --cold-reader-only, cache original
20261017-061527-f4639971 and saved schema15/11 refs. Three groups/min4 tests.
First Windows actual readonly CLI/client test PASSED (14s), report/protocol groups
still running. No final receipt yet. No GUI; installed worker stays running.

Cold reader/client committed a294e03, after actual Debian CLI/client + report scope
unit,11 entry regressions, TS generation/typecheck, both all-target Clippy/fmt/diff.
No Cargo active. Public auto-update/replacement/activation/rollback remain absent.
Wait for terminal, verify ready files/bytes/SHA/result and fresh installed health.
This reader gate is still synthetic intent metadata, not actual Main self-shutdown.

NEXT consuming shutdown supervisor driver/mailbox/view, then combined actual Main
native gate. Important physical custody: successful exit commit must not drop the
non-Serde ShutdownExitGuard before U2. Retain it in supervisor-owned ephemeral
custody across driver retirement; stored Exited alone cannot restore it after
crash. New U2 needs that custody or explicit manual recovery, never missing PID
inference/guard deletion. A test may simulate OWNED supervisor crash after proof
while preserving startup guard; never force-kill unrelated processes or relax
production refusal of unverified external Jobs. Avoid adding prod escape hatches
just for fixture cleanup. Actual Main can use a trusted test harness to prepare
independent supervisor bound to real worker, outside candidate's OWN execution
domain even if parent test Job remains (see control section below).
Installed d052 was healthy/idle before this task; update/bootstrap not needed yet.
Goal U1–U4 active, desktop unnecessary. No user configuration changed/backups needed.

## Cold shutdown receipt reader/client locally verified — 2026-10-21

Control v6 committed f45b300 after584 workspace tests, TS and both Clippy. New
readonly entry --inspect-update-shutdown INPUT_JSON SHA256 runs BEFORE
normal startup, using no Worker construction/recovery marker. Request DTO in
apps/update/src/shutdown/inspection.rs ties intent, config and old helper image;
report DTO in protocol includes exact helper/config/state/receipt with readonly
flags. ShutdownStateObservation opens only readonly handles and cannot try_lock.
Worker inspector checks bounded input SHA, actual image/build, config SHA/FileID,
state IDs and incarnation, then Store.open_read_only + exact original receipt.
No owner acquisition/migration/IPC/controller/scheduler or physical-exit claim.

Real exe integration test passed before and after dropping fixture owner, including
busy instance lock, missing receipt vs foreign state, pending guard/main DB bytes/
authority preserved, no recovery marker, wrong SHA and different attempt rejected.
This is a synthetic intent metadata fixture with actual CLI, NOT actual Main stop.
11 entry regressions passed:1 new reader +5 coordination +5 rehearsal;2 native
reference cases ignored. Logs shutdown-inspection-{cli-tests,entry-regressions}.

Added inspect_saved_receipt client: pins exact input/config/helper, checks capability
marker BEFORE launch, runs one bounded owned atomic probe/Job and validates helper
PID + full report scope/readonly flags. Input records explicit original journal schema.
Actual CLI/client integration passed; pure report tampering regression passed.
Generation, TS typecheck, both all-target Clippy and fmt/diff passed; final client
Clippy logs shutdown-inspection-client-{linux,windows}-clippy. No Cargo active. .winbridge/update-preparation/shutdown-inspection-storage.txt
has pre-build disk measurement. Installed d052 unchanged/healthy; no native run.

NEXT clean export and --cold-reader-only native gate (3groups/4 tests) in existing
primitive runner, then independent consuming supervisor driver/mailbox/view and
combined actual Main native gate. Client already validates exact request/helper
PID/config/state/intent; missing result means unknown/no resend. Reader is a no-effect query like artifact
queries; can reuse existing bounded atomic probe/Job machinery without inventing a
new worker or restoring old process ownership. Then independent supervisor driver,
versioned mailbox/view and combined actual Main native stop/ACK/lost-ACK/callback
retirement gate. No public auto-update/replacement/activation/rollback yet.
Keep installed worker running, desktop unnecessary. U1–U4 goal active. Previous
native primitive gate remains C704-only and does not cover f45/new cold entry.

## Shutdown control v6 locally verified; cold verifier NEXT — 2026-10-21

Implementation after96c8a2b: protocol adds ObserveShutdownState,
RequestShutdown and ObserveShutdownReceipt under strict worker control6. Central
required_version() drives worker admission and supervisor routing; v1 generic
forwarding rejects every non-v1 action, including new stop. Legacy actions remain
at versions1–5. DTO state/receipt decoders added with context/lease/process checks.

apps/worker/src/update_control/shutdown.rs: worker-derived state IDs bound to its
held instance lock; exact durable updater Issued/Uncertain intent, owner/epoch,
loaded runtime/config/state/Quiesced lease/logical+physical emptiness required.
Existing receipt returns only observation. Updater ACK with missing worker receipt
cannot authorize a fresh acceptance. Fresh Store permit owns StopAfterReply; it
stays in blocking callback return value then reply attempt, stops via existing
serve loop after callback retirement even if delivery is abandoned. No new shutdown
loop, callback never invokes control.close. Direct data handler cannot signal stop.
ObserveShutdownReceipt allowed as readonly history after terminal root too.

Local check workspace passed; current updater executable built first (needed by
control fixtures). One real supervisor/worker v6 observation + negative admission
unit passed: old versions/context/unadmitted stop/v1 forwarding cause no effects.
Protocol tests, generated TS and typecheck passed (shutdown-control-protocol-tests.log,
shutdown-control-types.log, shutdown-control-typecheck.log). Full workspace rerun
PASSED:584 tests,0 failures,16 ignored (shutdown-control-workspace.log). Linux
and Windows all-target Clippy/fmt/diff passed (shutdown-control-*-clippy.log). First run found only a
legacy error-text assertion: restored the supervisor v6/v9/v11/v12 version hint;
old and new control unit cases now passed in the rerun. Earlier package-only
protocol command without jsonschema
hit a preexisting un-gated projection test; corrected command uses --features
jsonschema as workspace does. No Cargo or Windows/native task active. New control source is NOT covered by C704's 19 native
primitive tests below. All local checks completed. Positive stop/ACK/callback retirement remains
unproven until the actual Main combined native gate.

NEXT: bounded cold receipt verifier CLI before normal worker startup (Store readonly,
no Worker construction/recovery marker), independent consuming supervisor driver,
then combined actual Main native stop/ACK/lost ACK/callback retirement gate. New
positive helper archives BOTH worker16/updater12; keep reference15/11. Current public
update capability still off; no supervisor action starts new stop driver yet.
Potential native actual Main strategy: trusted test harness prepares independent
supervisor bound to real running worker Main via library, outside candidate's own
execution domain; enclosing test Job may remain. Do not weaken production refusal
of unverified external Jobs or add breakaway just to pass tests.
Installed d052 healthy unchanged/idle after previous gate; worker stays on, desktop
unnecessary. U1–U4 goal active; no manual bootstrap/update needed at this point.

## Shutdown primitives native gate PASSED; control v6 NEXT — 2026-10-20

Task3970a744-a5ef-4ecc-97a3-3b8a56ce9075 / run20261021-034430-116fd4ff succeeded0.
19 native tests /five groups, nine ready files independently verified twice.
ReportSHA e176d5d90b61dc24cfdfc50e475e04fd8d3a15dce1ff7c8c79a00adfcc19e92f.
Source c704a88; ZIP/pins in launch section below. Details:
plans/worker-self-update/windows-shutdown-primitives-gate-2026-10-20.md.
.winbridge/update-preparation/shutdown-primitives-native-verified.json.

Exact process handle/birth/host/session and owned exit passed; Windows no-delete
file pins/instance lock passed; data/outbox/immutable receipts/ignored-write/CAS/
migration backups passed. Proven worker15 and updater11 refuse actual new16/12
journals without mutation. NOT a complete actual Main/supervisor shutdown gate:
no live stop command, ACK retirement or cold verifier is wired yet. Public update,
replacement/activation/rollback remain absent. U1–U4 goal still active.
Windows free43198169088→42924994560; no cleanup, candidate build or UI. Test cache
reused; installed worker d052 generation78e9bbe1 remains healthy idle0, complete
recovery/no maintenance errors. Runner52754 terminal0. No Cargo/native run active.
Leave installed worker running; desktop unnecessary/no windows to close.

Next implement separately versioned internal worker control v6, worker-computed
state observation, exact saved intent validation, receipt-before-ACK and stop only
after reply attempt/callback retirement. Add readonly cold verifier via pinned old
worker executable (Store readonly, no Worker recovery marker), then consuming
supervisor driver and combined actual Main native gate with BOTH A/C schemas12/16.
Keep saved proven schema15/11 refs below; no installed update/bootstrap needed yet.

## Shutdown primitive native gate ACTIVE; do not duplicate — 2026-10-20

Task3970a744-a5ef-4ecc-97a3-3b8a56ce9075 / run20261021-034430-116fd4ff,
runner session52754. Clean source c704a881ed163c0856bd1a2683ec1156731b8e2d,
WinBridge/sources/winbridge-20261021T034421Z-c704a88-9f51aba2.zip,
archiveSHA8178f7f124ee5e8824216a8f1d206d9d12453de0426c8739111096a544916160.
Pin .winbridge/update-preparation/shutdown-primitives-source.json. Fixed runner:
scripts/validate-update-shutdown-primitives-native.py. Five groups/min19 native
tests, reuse original test-build-cache 20261017-061527-f4639971. Uses saved proven
worker15/updater11 (reference receipt below), no installed runtime changes, no GUI.
Current first group compiling; no final result yet. Report shared/workflow-validation/
20261021-034430-116fd4ff; task/request saved under .winbridge/workflow-validation/.

c704a88 committed physical adapters and atomic journal exit method. 12 updater
shutdown tests +3 platform tests, both all-target Clippy/fmt/diff, UI build and
Python syntax/CLI passed. These local checks do not prove Windows process/lock
semantics. Wait for native task terminal, verify report.ready bytes/SHA and result.
New gate checks primitives/old-reader refusal, NOT complete Main/supervisor stop.
After review proceed control v6/receipt-before-ACK/stop after reply retirement,
cold readonly verifier and combined actual Main native gate. U1–U4 active/uncompleted.
Installed d052 unchanged; desktop unnecessary/no windows, keep worker running.

## Exact shutdown: physical handles/atomic exit adapter; wire NEXT — 2026-10-20

Commit a0e61dd contains the durable data/journals below. Further implementation
adds platform PinnedProcess (Windows live exact birth/host/session handle,
WaitForSingleObject; Unsupported on Debian) and ShutdownStatePins (existing files
only, identities, Windows no-share-delete). Exclusive lock acquisition consumes
pins; no missing lock file is created. ShutdownExitGuard retains exact exited
process, held original instance lock and original runtime pins through commit.
Only this non-Serde guard can call public confirm_shutdown_exit; Exited history
and root Replacing are atomic. Raw lifecycle ShutdownVerified remains forbidden.
No binaries are replaced, new worker is not started, no shutdown RPC exists yet.

Twelve updater shutdown tests and three platform infrastructure tests passed;
exit-write rollback included. Logs shutdown-physical-final-tests.log and
shutdown-process-tests.log. Linux and Windows all-target Clippy passed (shutdown-physical-*-clippy.log);
fmt/diff checked. New fixed native primitive runner prepares five groups/19 tests,
no candidate build or UI: scripts/validate-update-shutdown-primitives-native.py
and test-update-shutdown-primitives.ps1. Syntax/CLI checked; native run NEXT after
clean archive export. Reuse original test-build-cache and proven schema15/11 refs;
no installed runtime change. UI build refreshed for the standard verified exporter.
State/ledger tests use synthetic data and are NOT physical Windows proof. Native
process-pin/rename/stop tests pending. Storage:shutdown-physical-storage.txt ~31GB.

Next consuming layer: control v6 (freeze old v1–v5), worker-created state observation,
immutable receipt before ACK, stop guard released after reply attempt/callback
retirement through existing serve loop. Cold bounded readonly verifier must use
Store readonly directly, never Worker recovery startup. Pin process while still
alive before first stop; lost updater handle cannot be rebuilt from missing PID.
Then combined actual Main/supervisor native gate on matching updater12/worker16 A/C;
keep proven schema15/11 binaries below for old-reader refusal. Installed d052 healthy,
unchanged, desktop unnecessary, worker remains running. No Windows validation active.
U1–U4 goal remains active; public update/activation/rollback still unfinished.

## Exact shutdown: two durable journals; physical driver NEXT — 2026-10-20

Implemented data FSM and shutdown scope digest in apps/update/src/shutdown.rs;
strict protocol DTOs generated to TS, no new wire action/capability yet. Worker
journal16 retains immutable acceptance linked to original ConfirmQuiesced receipt.
Duplicate history grants no fresh stop; new ordinary lease mutation/release is
blocked after acceptance. History remains readable after future explicit handoff.
Worker15 active update is refused before migration writes. Migration backup is
verified (schema/quick_check/authority) and synced before changing schema.

Updater journal12: immutable intent + current delivery + append-only history,
atomic root Stopping/admission, Issued-before-one-use permit; ACK/unknown history
never reissues. Scope joins retained drain/final receipt, baseline/loaded worker,
root transition, active supervisor and owner/epoch. Raw BeginShutdown is guarded;
ShutdownVerified/Exited writes are blocked until actual OS guard is implemented.
No shutdown RPC, stop flag, cold verifier executable, replacement or activation
is enabled. Database/FSM tests are not a native process-stop proof.

Full workspace regression passed:577 tests,0 failures; four worker store tests
and nine updater shutdown tests included. TS typecheck passed. Log
shutdown-ledgers-workspace.log. Linux and Windows all-target Clippy, fmt and diff checks passed. Storage measurement:
.winbridge/update-preparation/shutdown-ledgers-storage.txt (>6GiB).

Next: separately versioned internal control, receipt-before-ACK and stop only after
reply attempt/callback retirement via existing serve loop; bounded readonly cold
receipt verifier (not Worker recovery startup); exact PID/birth/host/session exit
and held instance lock before atomic root approval. See exact-shutdown.md.
New positive native A/C must BOTH use updater12/worker16. Keep schema15/11 references
below: C3b is now a negative/reference artifact, not a new positive helper pair.
Installed d052 remains healthy and unchanged, generation78e9bbe1. Desktop unnecessary;
leave installed worker running. No Windows validation active. U1–U4 goal remains open.

## Drain/quiesce native gate PASSED; next exact shutdown — 2026-10-20

Task e73c941f-42f4-47af-af7f-ba0615b027e0 / run20261020-223955-44db4bea
succeeded0. Do NOT repeat. 26 native tests /12 groups, four actual full candidate/
health/drain operations, 15 ready files independently verified; typed proof checked.
ReportSHA c9bad3d4fbef464cba96fe278c0ef9e5dd4baeadaf1d08f48bdcf266a1848384;
proofSHA 92c53d63f14ce432038bb4225234da022fb3489564990072ec1d3c8574d0a51e.
A28e3a1c/C3b738c9 pins below; .winbridge/update-preparation/drain-native-verified.json.

Verified + lost ACK: all3 matching receipts/root Quiesced atomic; accepted tasks
succeeded0. Exactly ONE BeginDrain request, readonly receipt reconciliation.
Cancel waiting: only first command confirmed; other2 Prepared; accepted tasks stay
Queued/user pause preserved until explicit FIXTURE cleanup. Denied final history:
worker Quiesced but root Draining, last command Uncertain, no false approval.
Every case ends via owned Abort Completed/Release, unchanged old runtime manifest,
retired supervisor. All helpers are actual C executables; old worker is explicitly
test harness with synthetic other-role descriptors. Seeded target source, source
plan0files: this gate is drain/build/health, not a new bulk source-apply gate.
Windows free44661833728→44577452032 bytes. No GUI, no installed replacement.
Later readonly aborted-status fix 8a7d165 is outside A28:72 updater tests + both
Clippy passed; effect paths unchanged. Full joined old health10 migration still
requires separate positive coverage; current migration test only retention/denial.

Retained proven binaries BEFORE upcoming schema/cache changes:
K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\shutdown-base-20261021-024424-6223bb21
worker15 SHA ab3776ab34eb8bb9a2966c2ef15943e1c4ec038b66775bd60f0c0b2dd7ea264a;
updater11 SHA 5ec5bfff6b7fbdc5703a87c54dd1ad0fdcdd71ea55b98f7a7e396cd9f57ec49f.
Both clean C3b/sourceManifest92a75d.../build1792536258192180300, exact query checked.
Copy task46f13b1a-5b9d-4677-a297-7e02344cd87c succeeded0; shared reference.json/ready
verified. .winbridge/update-preparation/shutdown-base-reference-verified.json.
Schemas pinned from reviewed source, not an invented --describe query. Source UTC
reference date21 vs session NY20 is expected. Keep old reference10/worker14 too.

NEXT concrete implementation: plans/worker-self-update/exact-shutdown.md.
Add durable shutdown intent/receipt, separately versioned control, existing worker
stop path, exact process/lock exit proof; raw lifecycle must not bypass proofs.
Do not use recovery startup for readonly inspection: it latches a recovery marker.
No Cargo/native task active. Installed d052 generation78e9bbe1 remains healthy idle,
controller running. Worker required; desktop unnecessary. U1–U4 goal remains open;
shutdown/replacement/activation/desktop/rollback/public update still unfinished.

## Post-archive review: no optimistic aborted status — 2026-10-20

Read-only drain projection now labels aborted ONLY for root FailedBeforeCutover
plus same-snapshot validated Abort Completed/Released evidence. Aborting/Deferred
or failed root without release stays indeterminate; retained Quiesced does not
prove release. Wire v13/view5 shape/enums unchanged. 72 updater lib tests, pure
valid-receipt regression and Linux/Windows all-target Clippy passed. This projection
fix is NOT in running native A28; effects/driver unchanged, final completed-abort
projection is compatible. Logs drain-abort-view-*.

Native task e73c941f/run20261020-223955-44db4bea still active (runner3520).
Structured progress has verified/lost-drain-ack/cancel-drain completed; fourth
case now preparing. Do not resubmit or mark full gate done yet. Installed runtime
unchanged; desktop unnecessary, worker stays running. Next after gate: retain
proven schema15 worker AND schema11 updater before cache overwrite, then implement
exact shutdown as planned. U1–U4 remain open.

## Drain corrected native run ACTIVE; do not resubmit — 2026-10-20

Task e73c941f-42f4-47af-af7f-ba0615b027e0 / run20261020-223955-44db4bea.
Runner session3520; log .winbridge/update-preparation/drain-native-final.log.
Source clean A28e3a1cfcffe665f3ee84dec1af1ed5fe019bf97,
winbridge-20261020T223706Z-28e3a1c-45be39eb.zip,
archiveSHA 2dd107ab2620b3e9426ab6cb7fc8a4c81ed25580a854efc9f653d08fe63f80f7.
Target C3b738c9 remains as below (both11). Exact manifest pins stored at
.winbridge/update-preparation/drain-native-final-pins.json. Original cache and
retained schema10 updater unchanged. Drain fixture uses verified target source
seed and exports incomplete per-case progress, then separate final success proof.

Prior task e1b80adb finished failed exactly at over-broad trigger/expected-Uncertain
assertion. First3 cases passed, bulk first source apply616 files passed, but no
formal full proof. All13 exported failure files verified. Installed d052 healthy
idle0, no maintenance/recovery error; generation unchanged. No installed update.
Fixed test trigger and seeded fixture: targeted SQL regression, Linux/Windows
worker+updater all-target Clippy and fmt passed (drain-fault-scope-* logs).

Wait for this current task and verify ready bytes/SHA + typed proof before marking
drain gated. Desktop unnecessary/no test GUI; leave installed worker running.
Exact shutdown plan/interfaces at plans/worker-self-update/exact-shutdown.md;
implementation is still ahead. U1–U4 and active goal remain unfinished.

## Drain fault fixture corrected; seeded repeat will be needed — 2026-10-20

Current task e1b80adb/run20261020-221336-fd675b77 has completed verified, lost-ACK
and cancel-drain cases (next operation admissions observed), now fourth fault case.
Its frozen A627 has an overly broad deny_final_drain trigger: ordinal2/revision2
also rejects Uncertain history. Production correctly retains Issued if diagnostic
write fails; native fixture expects Uncertain and will fail. Do not mark green.
Local fix restricts fault to delivery.state=confirmed, with SQL regression proving
unknown history still writable while the fault remains installed. Never resend.

Next repeat should use fresh clean A, existing C3b schema11 and original cache.
DrainGate now seeds a verified target ZIP in a new isolated source root BEFORE
opening worker: actual source grant/preparation/build/health still run, source plan
has no changes. This avoids repeating the large already-gated source transaction
in a drain-specific rerun. Proof explicitly seededTargetSource=true. Existing A627
first case also applied616 source files before passing, but has no per-case JSON
export; do not invent a formal complete report from it. New helper emits incomplete
per-case progress (passed=false) alongside final proof, retaining earlier evidence.
Installed d052 healthy; desktop unnecessary, no GUI. No manual worker update needed.
Wait for current task terminal before using shared Windows compiler caches again.

## Drain native repeat ACTIVE; do not resubmit — 2026-10-20

Task e1b80adb-f970-44a7-a476-d44ecdd230dc / run20261020-221336-fd675b77 is active.
Runner session22666; log .winbridge/update-preparation/drain-native-retry.log.
Source A627ccaed38eaec47cbedb0caa7f8e850f0d0bf19:
winbridge-20261020T221320Z-627ccae-a192ab68.zip,
archiveSHA ba1cc666e7c49e7190a886cccd78c4a6cf613d756406bf74830d4c9e86e1d036,
manifestSHA 860f4f5b10fb5cda91e1ffd912e4d98d480360a5fe934d00ff47b40e6f5d24b1.
Target C3b738c9f49c980b922b55b36f687a05bc8bbb3d3:
winbridge-20261020T221051Z-3b738c9-e2020e80.zip,
archiveSHA 78170c4a6f5dfb6c70dd37ee4da026504e8a528282a96d45100f8b68e8e151b9,
manifestSHA 92a75d09439955fe2595c6198337bfe4f162ad68714311924d6007931b82c5a7.
Both updater11; original cache20261017-061527-f4639971. Full source pin file:
.winbridge/update-preparation/drain-native-pins.json.

New build-first order works: ten journal tests, native v13 negative admission and
v12 paused-queue observation passed. More groups/full four-case actor pipeline
remain; do not mark the entire gate passed. Installed worker d052 unchanged; only
it must stay running, desktop unnecessary, no GUI windows. First failure below
is an isolated script-order problem, not unexpected installed-worker behavior.

Review also preserves bounded supervisor startup stderr in fixture panic output,
before TempDir removes the diagnostic file. This test-only diagnostic change is
not in the running A archive; successful effect paths unchanged. Linux worker/test
Clippy passed; no new runtime behavior or retry was added.

## Drain gate bootstrap order fixed; repeat required — 2026-10-20

First native run20261020-221109-4e29c797/task e0864496-78ae-4ae2-9bca-38504cbc2044
failed in v13 admission fixture before Ready: the test cache still contained schema10
updater, while new library fixtures created schema11. Ten drain data/journal tests
passed first. The script had placed supervisor build AFTER worker IPC groups.
Move existing updater build group ahead of all fixtures in both gate modes; no
product timeout/permission change. Repeat automatically with fresh clean A archive;
existing C3b738c9 already supports11 and remains a valid positive target.
Installed d052 is healthy; no runtime applied, no GUI. Report bytes/ready verified
by runner before its expected nonzero result. Source/runtime caches preserved.

## U1: drain actor v13 implemented; combined native gate NEXT — 2026-10-20

Consume the aaeb4e4/schema11 journal through one blocking driver; no public update.
Supervisor v13 DrainCandidate/ObserveCandidateDrain, view5 candidateDrain; old views
omit the field. Fresh intent precedes ACK; Issued precedes RPC; exact v4 receipt
lookup after failed ACK, no resend. Wait via v5 logical observations; accepted roots
continue after explicit user unpause, updater never changes pause or cancels work.
Config/source/current owner, candidate pins, compatibility and physical helper/
compiler cleanup are checked before effects. Root Quiesced still leaves worker live.
Same-owner coordinated abort fences Draining, waits driver retirement, then uses
existing verified source/health/compiler cleanup and owned Release.

563 workspace tests/doctests, final15 worker control tests, both all-target Clippy,
TS typecheck, generated contracts and UI build checked (logs drain-actor-*).
Added native DrainGate mode to existing health scripts: four clean full-pipeline
cases verified/lost-drain-ack/cancel-drain/deny-drain-receipt. Test-only worker drops
one BeginDrain ACK AFTER application and counts requests to prove one mutation;
last-receipt history insertion is faulted to prove no false root approval.
Two accepted other-project tasks wait under user pause; updater cannot admit a new
root. Completion/explicit updater abort must preserve original queue decisions.
Schema10 reference also must refuse actual schema11 journal without changing it.
Native case results are NOT yet available; do not describe this block as gated.

Next: export clean A11/C11 with built UI, run existing Python runner --drain-gate
with the retained schema10 reference documented below and original cache run
20261017-061527-f4639971. Source+target must both be11; old C5a is negative only.
No active Windows test yet. Installed d052 remains unchanged; worker needed,
desktop unnecessary; no GUI windows. After green drain gate review and proceed
with separately durable exact shutdown. U1–U4 remain open.

## U1: drain journal schema11 locally verified; actor next — 2026-10-20

Added immutable drain intent, three frozen commands and append-only per-command
history. Admission commits CandidateVerified→Draining with Prepared rows; delivery
commits Issued before returning a nonserializable permit. Only matching immutable
worker receipts advance commands; final receipt and root Quiesced commit together.
Duplicate admission/delivery observes history, never recreates a permit. Raw lifecycle
BeginDrain/Quiesce is blocked, including agent-supplied zero counters. Cross-DB RPC
is still absent; this library does not authorize shutdown or runtime replacement.

Schema10→11 uses the existing consistent backup, owner lock and epoch fence. Five
new ledger tests cover ordering/CAS, reopen/no resend, ignored history write rollback,
migration retention and raw-event rejection. Migration retains typed health10 bytes
and rejects incomplete provenance/fresh approval; it is NOT a positive full joined
health10 migration test. Full positive drain authority and physical quiescence
remain required in the combined actor/native gate. 560 workspace tests/doctests,
Linux/Windows all-target Clippy and fmt passed. Logs:
.winbridge/update-preparation/drain-journal-{workspace,linux-clippy,windows-clippy}.log.

Retained proven schema10 Windows updater BEFORE candidate cache overwrite:
K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\reference10-20261020-214354-6f259f52\winbridge-update.exe
SHA256 cf25892abe8810a78ed5c9a2b76261b49187126cfb0bac18fa2707f2b2ecdeda,
clean source5a5834a, build1792515211187843900; copy task8ae6ee72-62a1-4dc7-a49c-326780e47e2c
succeeded0. Shared reference.json/ready and artifact query independently verified;
schema10 is pinned by reviewed source, not an invented --describe command.
Local evidence .winbridge/update-preparation/schema10-reference-verified.json.
Use for old-updater refusal, NOT a positive schema11 health/drain gate.

NEXT: consuming driver + separately versioned actor/status, logical wait observations,
exact receipt lookup on lost ACK, cancellation retirement before owned Release;
then combined native A11/C11 test. Installed d052 healthy/idle, generation78e9bbe1;
controller running, desktop unnecessary. No installed update/bootstrap required.
Exact shutdown, apply, activation, rollback and public update remain unfinished.

## U1: drain delivery data contracts ready; SQL schema11 is NEXT — 2026-10-20

Added apps/update/src/drain.rs + public module: PURE data contracts/FSM, no DB,
RPC, process/file effects or execution permits. DrainIntent freezes update/launch/
owner/epoch/pre-BeginDrain root revision, source/health digests, canonical domain-v1
worker-spec fingerprint and 3 command IDs/actions/expected worker revisions.
Reserve one extra worker revision for owned Release. Per-command Prepared→Issued→
Confirmed or Issued→Uncertain→Confirmed; issued/uncertain cannot be issued again;
duplicate exact receipt is a readonly no-op. Revisions define order, not clocks.
Typed receipt rejects another spec/owner/epoch/generation/action-phase/reason/revision.
Read deserialization never carries a delivery permit. Future SQL must enforce
immutable identity/history/CAS and prior-command receipt; pure prepare() is not
authority and can construct another in-memory object with the same key.

Post-gate review fixed future history-read issue: health consistency now checks
captured schema instead of the reader's current constant. Fresh approval remains
exact-current-schema at BOTH driver and journal acceptance; DB history read rejects
newer-than-DB context. No schema bump yet. Test covers consistent other-schema data
vs forbidden fresh authority. Actual migration10→11 with retained health10 remains
required; do not infer migration readiness from the pure regression.

555 workspace tests/doctests, Linux/Windows all-target Clippy, fmt and scaffold217
passed. Logs .winbridge/update-preparation/drain-ledger-{workspace,linux-final,
windows-final}.log. Older551 checks include v12/v5 and UI/contracts. No active
Cargo or Windows validation task. Installed d052 remains healthy idle (last fresh
Info after green gate below); controller is running, desktop unnecessary.

NEXT concrete work: implement apps/update/src/journal/drain.rs and schema11:
immutable intent, per-command durable rows/history, atomic BeginDrain admission,
Issued-before-RPC receipt, matching worker receipts and atomic root Quiesce. Extend
private Driver guards so raw BeginDrain/Quiesce cannot bypass receipts. Then wire
single-owner actor with v12/v5 wait observations, v11/v4 receipt lookup, cancellation
retirement and owned source release; exact shutdown follows separately. In hooks
of advance_with_driver validate the POST-transition edge, not pre-transition phase.
Migration requires consistent backup/epoch fence, no invented drain state for old
updates; positive new native gate needs BOTH matching schema11 archives. Source/
health old-state fixtures and negatives must not masquerade as executable approval.
U1–U4/goal remain open. No installed bootstrap/update needed at this point.

## U1: full candidate-health gate PASSED; next durable drain — 2026-10-20

Task7b42694b-5c0b-4121-a7d4-716c3599b2fa / run20261020-164339-83d2a1d6 terminal
succeeded0. Do NOT resubmit it. [Scope/evidence](plans/worker-self-update/candidate-health.md):
9 groups /13 native Rust tests +4 actual full actor operations. All11 ready bytes/SHA
independently verified, typed proof verified. ReportSHA
8ce81ccc3050e70e2374a97a9e31e3703842f68e888a8a9c72eb4b63823abb69;
proofSHA af7bd666f9a962b497d34dc2ecb35518efca776aa02cbc08974365ae36ff7fee.
Real clean A2feb40f vs actual clean C5a5834a, full four-binary build/config/rehearsal.
PID receipt denial never resumed7628; pre-helper cancellation created no process;
live rehearsal11876 cancellation recorded exact cleanup; verified helpers12612/11840
exit0 with typed reports and atomic CandidateAccepted. Every case then aborts before
cutover with owned worker release and supervisor exit. No install or GUI.
Native v11/v4 receipt read, capacity1 fix, full-image SHA and atomic-hook regression
passed in this same gate. Older synthetic old-other-role descriptors/test-harness
entry point remain explicitly scoped; actual Main/pins have separate base gates.

Windows free47,388,196,864 ->46,235,811,840. Fresh installed Info idle0, unchanged
clean d052/generation78e9bbe1-8cb9-4ad3-93cf-371e981801ad, complete recovery coverage,
no maintenance/recovery errors. Leave installed worker; desktop not needed.
No manual bootstrap or installed replacement yet. No Windows task is running now.

Post-gate review: newly prepared readonly v12/v5 (commit505b006) is NOT in the
frozen native archives. Local551 tests/doctests, Linux/Windows Clippy and UI checks
passed. Include it in upcoming drain gate; do not claim native support yet.
Proceed [drain/quiesce plan](plans/worker-self-update/drain-quiesce.md): durable
intent/commands/receipts and atomic BeginDrain/Quiesce, single-owner cancellation
and source release, then exact shutdown. No schema11 implementation exists yet;
when introduced, export BOTH matching schema11 A/C (these positive archives are10).
Callbacks of advance_with_driver run AFTER root/transition writes: new journal
hooks must validate the POST-transition edge, with ignored-write/rollback tests.
User pause and cleanup exclusion must remain. Entire U1–U4 and goal remain open.

## U1: logical drain observation prepared (historical) — 2026-10-20

New internal supervisor v12 / worker control v5 reads drain blockers/user queue
mode without changing state. Lease/accepted/logical-owner/queue counters share one
SQLite snapshot; conservative registry counts are a separate short observation.
There are NO physical-owner counts or approval flags: even zeros cannot replace
a matching ConfirmQuiesced receipt. Context/generation/owner/epoch/counter bindings
are validated; legacy versions/forwarding and v5 mutation packets are rejected.
No journal-schema change (still10), no public update or drain driver enabled.

Local551 tests/doctests, Linux/Windows all-target Clippy, UI typecheck/build,
generated contracts and scaffold217 passed. Real supervisor/worker IPC fixture
keeps a task queued under user DispatchPaused, observes it before/after source
lease/release, proves root/slots/guard and pause unchanged by reads. Added negative
version/mutation assertions passed in a focused rerun and final Windows Clippy.
Native v12/v5 is still REQUIRED and not covered
by the frozen2feb/5a archive in the live task below. Include it in the upcoming
drain coordination gate, not a new concurrent installed-worker task.

Code review verified automatic_output_cleanup already checks active update lease
under Store before setting its in-flight flag; no extra background cleanup starts
while the source/drain lease is active. Keep that exclusion and user pause.
Next: durable drain intent/command receipts/CAS + single owner driver, then exact
shutdown. Do not confuse these readonly preparations with completed U1 or install.

Live Task7b42694b-5c0b-4121-a7d4-716c3599b2fa passed first2 actor cases; third
operation5316ca3b-153b-4693-9318-1013281ea2e1 was Building at last poll. Read
.winbridge/update-preparation/poll-health-atomic.py, do not resubmit. Installed
d052 stays; desktop not needed. Confirm all4/proof/ready and fresh Info before
closing health gate. New schema11 will require new matching candidate exports.

## U1: combined atomic-health/receipt/capacity Windows gate RUNNING — 2026-10-20

Accepted Task7b42694b-5c0b-4121-a7d4-716c3599b2fa / run20261020-164339-83d2a1d6,
confirmed RUNNING. Runner session50710, log
.winbridge/update-preparation/health-actor-native-atomic.log; readonly paging
.winbridge/update-preparation/poll-health-atomic.py saves current task/output.
No outputs at first poll (bootstrap/extraction). Do not restart on observation
expiry; poll this EXACT task to authoritative terminal.

Clean source A2feb40ff0c124a48b51015144ed64058fe08326a:
winbridge-20261020T164324Z-2feb40f-609809ee.zip,
SHA04fff62d50ef560d40225eebab3e76ef976fe04e112b7112ac543fd13e61d863,
4,947,962 bytes/610 files. Genuine clean target C5a5834a below, both fixed.
Original20261017-061527-f4639971 test/candidate caches reused under their locks.
The script runs nine groups: actual rehearsal entry, FSM edge, full-image SHA,
atomic hook/rollback, v4 protocol, supervisor build, real v11/v4 receipt IPC,
drain capacity1 and all FOUR full actor cases. No GUI or installed replacement.
Local549 tests/doctests and Linux/Windows Clippy passed. Native still unproven.

On failure inspect candidate-health-actor-proof.failure.json FIRST; distinguish
real driver error from fixture timing. Ordinary isolated corrections/repeats are
authorized. On success verify all ready bytes/SHA, typed4-case proof/actual A/C
identities, exact helper cleanup and atomic root acceptance, plus fresh installed
Info. Then review and continue durable drain coordination/exact shutdown. The
new v4 reader and corrected capacity are preparations, NOT a durable drain driver.
Do not mark U1–U4 or goal complete. Future schema11 needs a matching fresh target.
Installed d052 worker should stay running; desktop unnecessary. Stop dependent
Windows work only for installed-worker faults/update requirement. No bootstrap now.
Older receipt-reader-native-plan.json is superseded; do not submit its older run.

## U1: atomic health-hook and drain-capacity fixes ready for native retry (historical) — 2026-10-20

Corrected target C5a5834a6cd0eeab88f75ccd41d09d3b05d864d13 exported clean:
winbridge-20261020T164245Z-5a5834a-b2677d6d.zip,
SHAe5e5122a63142c50acddee2aa6ddba0c1ce834cbfaec085dbf68f24e15363d8e,
4,947,690 bytes/610 files. Both control worker/supervisor A and actual candidate C
will include atomic acceptance, receipt reader and drain-capacity fixes. Distinct
source A also adds a native image-marker/full-SHA regression group; descriptor
hashing must still cover the entire image after finding an early marker.
Commit/export A then run the combined health runner with this target and original
cache. No other Windows task is currently running; fresh Info was healthy idle0.

Task46e7c30a-65a7-4870-9466-5aef6366c727 / run20261020-161358-d2e84c59 is
terminal failed1, NOT running. First three actor cases passed. Final candidate C515
ran config PID14632 and rehearsal PID14252, both exit0, empty trees, complete
capture and typed reports. Strict stderr defect is fixed. Final acceptance failed:
`PreconditionsChanged: Fresh pending health authority required; no replay`.
Structured candidate-health-actor-proof.failure.json proves the actual failure.
Six ready files verified; reportSHA
25833a5e9d2746345a0a0d253f14853ff91077d69cfb6daec06b03ad0002557b.
Windows free56,695,615,488 ->48,173,080,576. Installed d052 unchanged, generation
78e9bbe1-8cb9-4ad3-93cf-371e981801ad, idle0, complete coverage, no recovery or
maintenance errors. No GUI/replacement. Never replay this terminal task.

Real bug: advance_with_driver writes root CandidateVerified and its transition
BEFORE running the atomic child-row hook. accept_health incorrectly repeated
the PRE-transition pending-phase guard inside that hook. It now requires the
exact POST-transition root/revision/current owner/supervisor and retained event/key
in that same transaction; pending probe guards remain unchanged. SQLite hook
regression passed, with rollback on owner/epoch/launch/revision/key mismatch.
This metadata fixture is NOT executable-health approval; full native gate remains.

Review found another concrete issue before durable drain: source/compiler lease
reserved the global work slot even during Draining. At capacity1, accepted other
projects could never drain. New regression reproduced occupied1 vs expected0.
Reservation now ends atomically with BeginDrain; source/admission lease remains,
unknown phases still count conservatively. Test uses two accepted other projects,
checks global capacity1, source fence/new-root rejection and refusal of CloseDispatch
until both synthetic attempts clean up. No named-resource support added.

549 workspace tests/doctests, Linux/Windows all-target Clippy, fmt, PowerShell
syntax, scaffold217 and three Python admission tests passed. Logs
.winbridge/update-preparation/health-atomic-capacity-*.log. Free Debian31,639,314,432.
Commit/export corrected target C and then a distinct clean source A, both with
the two fixes (schema10 unchanged), original cache and fresh isolated run. C515
has clean worker rehearsal but its updater still has the old acceptance-hook bug;
use the new fixed generation rather than a knowingly defective updater candidate.
Health script
now includes atomic-hook, v4 protocol/actual worker receipt and drain-capacity
regressions before all4 actor cases. This combines native coverage; do NOT launch
the older .winbridge/update-preparation/receipt-reader-native-plan.json, now marked
superseded. Reader9be source archive exists but lacks these two fixes. No bootstrap.
Once all4 pass and proof/ready/fresh Info verified, continue durable drain driver
and exact shutdown. U1–U4 remain open; schema11 still needs matching future C.

## U1: readonly command receipts prepared (historical) — 2026-10-20

New internal supervisor v11 / worker control v4 observes immutable command
receipts by exact payload. No journal schema change, mutation, marker cleanup,
source/project filesystem scan, public update or installation. Missing receipt
does NOT prove absence of an in-flight effect and never triggers Advance retry.
Legacy v1–v3 / supervisor v1 cannot smuggle the new query or v4 mutations.
Client validates packet binding, command, phase, revision, reason and owner/epoch.
Historical SourceHeld receipt remains readable after Release/source-directory move.

13 worker-control tests passed (new roundtrip uses real supervisor/worker IPC,
discards original ACK, checks payload conflict/absence/stale epoch/version/slot
and command-row count, then reads historical receipt after release). Two protocol
regressions passed; full workspace547 tests/doctests passed. Final reason-binding
guard was separately re-tested after the full run. Linux/Windows all-target Clippy,
UI typecheck/build and scaffold217 passed; generated TS and final contract
comparison passed. Native validation for v11/v4 is still REQUIRED; current
bed8/515 health task below does not contain this newer code. Added explicit native
protocol group; existing worker-control group includes the new roundtrip.

No new Windows task while46e7c30a-65a7-4870-9466-5aef6366c727 runs. It passed the
first two actor cases; third operation9ddfa41b-db83-4554-91e7-8957ac5dd2d6 was
Preparing at last poll. Poll SAME task to terminal; verify all4 proof cases and
ready hashes plus fresh installed Info. Then export the committed reader and
run the approved full update-preparation gate with original cache and retained
schema9 updater/schema14 worker references. Inspect exact current state before
submitting. Do not reinstall d052 or open UI. Durable drain intents/actor, raw
phase guards and exact shutdown remain to implement; entire U1–U4 stays open.

## U1: corrected rehearsal native gate RUNNING — 2026-10-20

Accepted task46e7c30a-65a7-4870-9466-5aef6366c727, run20261020-161358-d2e84c59,
confirmed running. Runner session37405, log
.winbridge/update-preparation/health-actor-native-stderr.log; readonly paging
helper .winbridge/update-preparation/poll-health-stderr.py saves task/output.
Source A cleanbed8d5e1db6c3c6fcfc5c8ecf487f3d3177bea1b,
winbridge-20261020T161344Z-bed8d5e-22b00df8.zip,
SHAcfd888a9d3ae617394cd3bd2e235c71f034265c5787491cb0a233ccdd3c7d9ea,
4,938,055 bytes/609 files. Target C515b412 archive below. Original cache reused.

Five real Windows rehearsal-entry tests passed, including strict stdout/stderr;
FSM-edge regression passed; real supervisor A compiled. Full actor helper is
running, first operation5c88f822-dd21-430f-9cc6-6ee7a9c3ad5f at Requested at last
poll. All FOUR cases, final proof/ready hashes and fresh installed Info remain
required. Do not re-submit or restart because observation expires. Inspect
candidate-health-actor-proof.failure.json FIRST if terminal failure is reported.
Product source/artifact/helper/control guards remain strict. Windows desktop
not needed, no GUI, installed d052 worker remains running. No manual bootstrap.
After green gate: review, then implement planned durable drain/quiesce and exact
shutdown; schema11 will require a new matching candidate, not this schema10 C.
Goal/U1–U4 remain open. Drain-plan review clarifies admission rejection vs accepted
queue, user pause/app blockers and prohibition of raw phase changes without receipts.

## U1: rehearsal stderr defect found and fixed — 2026-10-20

Corrected target C515b4127fed77acf10b596e3a2e0a7a980b4397c is clean exported as
winbridge-20261020T161311Z-515b412-e1d791a8.zip,
SHA3a203ffc9d7a8e2b20436955de7364cd5520d7d8a2e6417c9480a652859709da,
4,937,787 bytes/609 files. Marker optimization is becbf21. Distinct source A
will also run the five actual rehearsal entry regressions BEFORE the full actor
case, via health-rehearsal-entry group. Commit/export A, then submit one new task
using original20261017-061527-f4639971 cache and this corrected C. No installed
runtime replacement. Poll accepted task to authoritative terminal.

Task1378af21-9a6c-4b98-9b54-cbd72c4e55f8 / run20261020-103249-4030e35e is
terminal failed1, NOT running. The first three cases passed their assertions;
the final case ran both helpers with exit0 and confirmed empty trees/capture,
but health correctly rejected rehearsal stderr. Real rehearsal used normal serve,
which always printed `WinBridge worker ready` to stderr. Earlier standalone
inspection ignored stderr, so its green tests did not cover the strict actor
contract. Four ready files verified; reportSHA
f13a2b0c466fb1c1ad4d852b475669c996f811078b5968f990c9de8d150f711e.
Windows free19,407,204,352 ->19,330,277,376. Do not replay the terminal task.

Fix: normal serve keeps its console announcement; rehearsal readiness comes
from IPC and the typed stdout report, with no console banner. Real errors remain
on stderr and still invalidate health. Standalone inspection now also rejects
stderr. Five local actual-process rehearsal tests passed, including new clean
stdout/stderr regression. Artifact marker scanning stops after first match while
SHA still hashes every byte; read-boundary/late-tail mutation regression passed.
544 workspace tests/doctests, Linux/Windows all-target Clippy, fmt, three Python
admission tests and scaffold217 Markdown checks passed before commit/export/retry.
The fixture now saves structured failure JSON for final outcome mismatches too.

Debian session was rebooted (uptime begins2026-10-20 15:41 UTC); controller has
been restarted. Fresh installed Info: same clean d052 source/build, new generation
78e9bbe1-8cb9-4ad3-93cf-371e981801ad, idle0, complete recovery coverage, no recovery
or maintenance errors. No installed replacement, GUI or bootstrap. Next native
run needs fresh genuine clean target C containing the rehearsal fix and distinct
source A; old2aac target still has the defect. Original caches may be reused.
[Drain/quiesce plan](plans/worker-self-update/drain-quiesce.md) is design only;
implement after full health gate. U1–U4 and public update remain unfinished.

## U1: callback-observer native retry (historical) — 2026-10-20

Source A d5f04a7b6e4102ad5f70329aab7310061634c1f6 clean exported:
winbridge-20261020T103233Z-d5f04a7-a171943e.zip,
SHA02b1c2584aa065752d97106135bed9e72ef3b1db75244b3f98ff4ba7e82589c8,
4,931,534 bytes /608 files. Target C is the existing clean2aac553 archive below.

Task1378af21-9a6c-4b98-9b54-cbd72c4e55f8 confirmed RUNNING/PID2528,
run20261020-103249-4030e35e. Runner session60664;
log .winbridge/update-preparation/health-actor-native-callback.log.
Read-only poll helper .winbridge/update-preparation/poll-health-callback.py
pages ALL output and saves latest task/output. Do NOT restart on observation
expiry; poll this exact task until authoritative terminal. Native FSM edge
regression passed; supervisor built; worker lib native helper compiling at last poll.

This run still needs all FOUR actor cases and proof/ready verification; it is
not green yet. Source full605 effects took about5min in the preceding run,
compiler/artifact checks another few minutes. That preceding run reached saved
health intent and then the TEST's10s callback wait expired. Do not assume that
was only slowness: the new60s observer polls driver state and saves structured
candidate-health-actor-proof.failure.json on early rejection/expiry. Inspect that
artifact FIRST if this run fails; fix real driver errors instead of blindly
increasing deadlines. Probe/control deadlines themselves were not changed.

Native-only observer compiled under Windows Clippy; prior542 local tests remain
applicable to unchanged product. Working installed d052 worker remains healthy,
complete recovery coverage/idle0 before this new task, generation unchanged.
No GUI windows, no installed binary replacement; desktop unnecessary, leave worker.
Ordinary test fixes/repeats authorized; stop only for installed-worker update or
unexpected installed-worker behavior. Do not mark U1/U2/U3/U4 or goal complete.
If actor passes: verify cases/typed process/report/schema/source and ready hashes,
fresh installed Info, review code, then continue U1 drain/quiesce/exact shutdown.
BeginDrain/CloseDispatch/ConfirmQuiesced already exist in worker control v1 but
require parent Draining; durable updater coordination/fresh health authority and
shutdown handoff still need implementation. Public update remains disabled.

## U1: full source/build reached health; callback observer corrected — 2026-10-20

Run20261020-101253-22bdb154 / Task4363a807-dd5b-4759-89a6-e8856aeebb2f terminal
failed1. Real source605/605, actual compiler and artifact verification completed;
health intent ACK/read succeeded. The TEST's10s wait for its v3 callback expired
(line216), before any health success was established. Parent child failed after
617.09s and checked private tree empty. Four ready files verified; reportSHA
a7542605738246a57b79a25d4d009b9e9fb1ada41c41a70a291b7887f0d84fec.
Windows free19,504,570,368 ->19,431,018,496. Fresh installed worker idle0, recovery
coverage complete, generation unchanged and no maintenance/recovery errors.
No windows or installed replacement. Do not replay this terminal task.

We must distinguish expensive pre-callback pin/hash work from an early health
rejection: new fixture polls saved status while waiting up to60s for its callback.
If driver stopped/failed or deadline expires, persist health/status diagnostic
as candidate-health-actor-proof.failure.json in shared and include it in panic.
This is NOT a product probe/control timeout change. Native Windows all-target
Clippy passed; Python admission tests passed. Source-stage600s/helper1500s remain.
Commit/export the correction and automatically retry with same clean C2aac553
and original caches, fresh source A/new task/new isolated runtime. No bootstrap.

## U1: first full actor attempt terminal timeout; fixture retry prepared — 2026-10-20

Source A925e978, target C2aac553. Run20261020-100244-41469852 /
Task2f797372-637c-48e2-b51d-63a0e4156c13 terminal failed1. The native FSM regression
passed and supervisor A built; the copied helper reached SourceApplying, but
its source-stage test deadline120s expired on the real604-file archive. Whole
helper exited failed after176.49s; parent verified owned tree empty. It was NOT
an installed-worker fault or an observation-timeout restart. No compiler/health
success claimed. Four ready files verified; reportSHA
9fb4d7e745987ddf971832a6dc9475b438376662310cf5fbbc5ae1ce26595b9e.
Windows free19,573,338,112 ->19,574,427,648 bytes; installed d052/generation unchanged,
Info idle0, no recovery/maintenance errors. No windows or runtime replacement.

Fixture source preparation/apply deadline now600s; helper total1500s, outer task
1800s. Every5s print prepared-file or applied/total counters from actual filesystem/
SQLite alongside the root phase. No product guard was weakened. Suppress encoded
PowerShell progress CLIXML and require complete installed execution coverage.
Windows all-target Clippy passed after native-only change; three archive-admission
Python tests passed; previous full542 tests still apply to unchanged production.
Commit/export this ordinary fixture correction and repeat automatically with SAME
clean target C2aac553 and original cache. New source A and fresh test instance/key;
never resubmit the terminal old task or adopt its partial source. Prior logs kept.

## U1: full health actor gate prepared, local checks passed — 2026-10-20

Transition fix committed2aac5534290b6d29cb8e401dc44e59a03068318b.
New full native fixture has FOUR operations: denied PID receipt/no resume,
blocked pre-helper callback cancellation, request while actual rehearsal process
is observed alive, and verified real config/rehearsal + atomic acceptance.
Every operation ends with coordinated abort before cutover and confirmed release.
No test backdoors added to product Main; control worker is an explicit copied
test harness, old other-role descriptors synthetic. Candidate is a genuine build
from a separate clean target archive. Detailed scope in candidate-health.md.

542 workspace tests/doctests, Linux and Windows all-target Clippy, fmt, PowerShell
syntax, Python admission tests and scaffold216 md passed. No Cargo process active.
New scripts: validate-update-health-actor-native.py, test-update-health-actor.ps1,
test-update-health-native-runner.py. They reuse the approved installed worker and
original local test/candidate caches. Source ZIP is verified/copied before remote
extraction; report.ready files and detailed proof are checked before success.
Actor test prints current persisted root phase every5 seconds and drains both
outputs after its retained diagnostic quota. No GUI/windows/installed replacement.

Target C exported from a temporary clean Git worktree at2aac553 (worktree removed;
no source/user files removed). ZIP:
/mnt/hgfs/shared/WinBridge/sources/winbridge-20261020T094903Z-2aac553-c6d6a9da.zip
SHA761d42bbb645873f8630a162ab708567ea2775b5a4e6a9f5e1fe4dfd5221d25f,
manifestSHA48f13c3059b1a6115fdbc38e1da81b09c7f6121182259c69d7067c667783cfd3,
604 files /4,913,224 bytes, clean/built UI. It contains the transition fix.
Commit/export the fixture as source A, then run:
python3 scripts/validate-update-health-actor-native.py --archive <new-clean-A.zip>
 --target-archive <above-C.zip> --cache-run 20261017-061527-f4639971
The source A and target C identities must stay distinct; never relabel metadata.

Native actor result is NOT passed yet. After submission retain/poll exact Task ID;
stop on installed-worker problems, ordinary fixture fixes/retries authorized.
Last fresh installed d052 worker connected/idle0/healthy. Desktop unnecessary;
leave worker running. Full U1–U4/public update remain incomplete; no bootstrap now.

## U1: native schema10 base passed; health transition review fix — 2026-10-20

Run20261020-053153-bde36139 / Taske3a474d6-48a2-4460-bd6d-4d43d377a44a
terminal succeeded0. 217 native tests/19 groups, full build/entrypoints passed;
47 ready files verified; reportSHA0975a1ff2ca28a8193048ef88ae45911b27cd3770d7238240b4d404a60d0d3e7.
Fresh installed Info idle0, d052/generation unchanged, no maintenance/recovery error.
No windows; worker remains, desktop not needed. Do not poll/restart terminal task.

[Review](plans/worker-self-update/health-transition-review-2026-10-20.md) found
runtime_baseline::transition hardcoded Requested, so health admission/acceptance
could not pass its reader. Fixed by binding each supported event to its actual
core FSM edge. New real transition-row regression + previous baseline/preimages:
21 tests passed. This was an unexercised internal health route, not a working
installed-worker failure. No manual update needed.

New full health native fixture is WIP in worker update_control tests: optional
full target package and private build adapter forwarding actual build script
with original validation caches. Three cases currently implemented: failed PID
receipt/no resume, blocked callback abort, verified actual helpers + atomic
acceptance. Need finish/check the native runner and fixture, consider live
rehearsal cancellation. Use no product test backdoors, keep actual source hashes.
No claims yet of passing full health actor gate. Remaining U1/U2/U3/U4 unchanged.

## U1: schema-10 native base gate running — 2026-10-20

Internal health block committed 3ca392ad7726faf2ff38a208999f8566222eaeaf.
Clean exported source: winbridge-20261020T093134Z-3ca392a-69c0f0d8.zip,
SHA f41913c6a133630815fe216f10d4bb8bc478aff6bfbbd7cd5edf7ab875e41295,
4,909,328 bytes / 603 files; manifest clean, built UI included.

Existing Windows task e3a474d6-48a2-4460-bd6d-4d43d377a44a confirmed RUNNING,
run20261020-053153-bde36139. Runner session31042;
log .winbridge/update-preparation/health-native.log. Poll this exact task to
terminal; no observation-timeout replay. This is native unit/migration/build/
entrypoint coverage of the new source, NOT full v10 health actor integration.
Derive actual test counts, verify every ready byte/SHA, check installed Info.
Original cache20261017-061527-f4639971; previous worker schema14 reference:
workspaces/snapshot-fixture/workflow-validation/reference-20261020-ebb7c513/winbridge-worker.exe.
Current-schema refusal uses schema9 updater from d052 successful gate:
K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261020-051354-bd565b6c\update-candidate\payload\winbridge-update.exe,
SHA511e020be01ffda16dc09225af45675d198921b25e9bde77e5afd8ba7e5816ff.
No GUI/windows, worker remains d052 and healthy; desktop unnecessary. No bootstrap.

Next vertical proof: actual v10 health actor with clean full candidate bundle,
source/build/artifact/baseline/preimages chain, ACK/status/duplicates, PID-receipt
write rejection, cancellation and terminal acceptance. Reuse worker_control
Fixture/runtime copied-process helper (current Fixture always publishes minimal
synthetic target source, so it needs an explicit full target archive option).
Actual candidate build's source manifest must EXACTLY match the target ZIP;
do not relabel an existing binary with a new synthetic source manifest/hash.
A private native test tool can forward the frozen PowerShell build args and add
-ValidationCacheDirectory to the real build script, reusing original local caches
without copying target or changing production build policy. This permits a real
compiler/report rather than fabricated health approval. Keep flags/test paths
inside isolated test configuration, do not add test backdoors to product Main.
Test A (clean test worker/supervisor build) and target C (clean candidate archive)
may differ: that separation is intentional and must remain visible in proof.
A v3 compatibility gate can hold the health driver to test status and abort;
PID write failure should clean the suspended actual helper without resume.
Native long-probe cancellation already passed separately; do not falsely call
between-helper cancellation a proof of cancelling a running actual helper.

Keep goal active. U1/U2/U3/public update/install remain incomplete. New manual
installed-worker update is not needed for isolated testing and is not requested.

## U1: schema-10 health driver/actor checked locally; d052 native gate passed — 2026-10-20

User restarted/updated installed Windows worker from d052 archive. Fresh Info:
source d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2, build1792486531570951100,
manifest835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241,
generation1a3f242c-7da5-4daf-82fd-eb65566dc7cd; connected, idle0, no recovery/maintenance
errors. Controller PID3756 is running loaded from previous build (707cdec dirty),
WS192.168.44.203:1143 / .winbridge/controller-session/controller-local-development.sock.
Desktop unnecessary; all tests had no windows, no installed replacement.

[d052 native gate](plans/worker-self-update/windows-probe-lifecycle-gate-2026-10-20.md)
succeeded0: run20261020-051354-bd565b6c, taskc53e6a24-0c23-41d8-ae7e-957561a1208c,
210 tests/19 groups, full four-exe build, actual config/fresh-state rehearsal/Main/pins.
47 ready files verified; report SHA cf90231bd186487dcebc48ae32408ed3760f7b9913dc64ea2f1a32ecddb8c441.
The missing previous updater reference was rebuilt, not an installed-worker fault.
See report for exact schema14/8 reference paths, hashes, tasks and prior failed run.
Original cache owner remains20261017-061527-f4639971; do not delete Windows dirs.
For the next schema10 refusal gate use the NEW schema9 updater from d052 gate
(candidate-files-proof.json contains its exact SHA), plus rebuilt schema14 worker.
Do not use installed schema15 worker as an older reference.

[New health code](plans/worker-self-update/candidate-health.md): updater schema10,
supervisor v10/view4, one-use intent, artifact/preimages/source/current v3 binding,
managed config/rehearsal helpers, PID before resume, immutable cleanup, atomic
CandidateAccepted. Duplicate-first returns history, never recreates pending permit.
Actor keeps status responsive, commits Aborting with same Arc authority and waits
for health cleanup before release. Abort re-observes process births/Windows Jobs.
Raw health admission/acceptance forbidden; old view1-3 omit new field. Source/build
phase guard shared without weakening Building requirement for compiler.
Schema9 migration consistent backup/epoch fence; no historical health adoption.

541 workspace tests/doctests, Linux+Windows all-target Clippy, generated TS/check,
fmt, UI typecheck/build and scaffold passed. Logs .winbridge/update-preparation/health-final-*.
Disk check available31,766,577,152 bytes before final workspace; no caches pruned.
Tests cover monotonicity, schema/process/isolation proof, pending/unknown cleanup,
raw bypass, schema9 migration and old views. Typed synthetic tests are NOT an
actual healthy process/journal approval proof. No native v10 integration gate yet.

Next: commit this internal block after final review, then add full isolated v10
actor gate using a clean actual candidate bundle. Persist/ACK/status/duplicates,
PID-write rejection, owner/root fences and cancellation while helper lives need
real end-to-end proof. Reuse already compiled bundle via explicit fixture compiler
receipt, clearly distinguish it from production compiler/install proof. Do not
claim U1/U2/U3 complete or enable public update/bootstrap/activation. Goal remains
active; ordinary script fixes/retries are already authorized. All Cargo steps
must remain sequential, including tests/doctests (no concurrent feature builds).

## U1: probe lifecycle hooks WIP; base native tests passed, helper import fixed — 2026-10-19

7ed3900 / run `20261019-113708-5fe0af67` / Task
`96f5965b-5a8e-49ba-ac90-3f9d3d84c01c` terminal failed1 after all 206 native tests /
19 groups passed. Compatibility v3, kernel removal (NotFound OS2) and original
abort all passed; then early schema assertion lacked a dot-source. Fixed in
707cdec (import at script startup). 36 ready files verified, SHA
`3a26c51feda0d16d8fc38989d24330a92bd5827350955b54c009f1cbff550fa8`.
Actual candidate build/config/rehearsal/Main/pins have not run in this gate.
Installed worker still healthy/idle0, generation unchanged. No windows.

[Probe lifecycle hooks](plans/worker-self-update/probe-lifecycle.md) now support
before-create, PID-before-resume, actual resume request, owner polling and terminal
cleanup callbacks on every outcome. No journal/approval yet. Three copied-process
local tests passed; Windows-only PID-rejection/no-resume/cleanup test added. Runtime
query JSON/wire versions/schemas unchanged. 533 workspace tests/doctests, Linux/Windows Clippy all-targets, contracts/fmt
passed sequentially; runner4/cache10 and scaffold213 md passed. Scripts require all four
native probe tests. Commit/export combined hooks + helper fix after checks, then
repeat same native gate with original cache owner and schema-8 reference.
Next implement health journal+actor as a vertical scenario, not another standalone
report: use hooks for durable intents/PIDs/cleanup, bind v3 and artifact/source
receipts, prevent pending replay, coordinate abort before release and commit
CandidateAccepted only with fresh proof. U1/U2/U3 public update still open.

## U1: portable removal + compatibility combined gate running — 2026-10-19

Clean source `7ed3900028682816ef5d0b653300758ad48a06ed`, archive
`winbridge-20261019T153407Z-7ed3900-b2957da6.zip`, SHA
`9d8259b7c7b477b4261c045fe13f7d81fce730d020c264fd2a1cc8ce379e1a40`
(4,876,028 bytes / 594 files). Run `20261019-113708-5fe0af67`, Task
`96f5965b-5a8e-49ba-ac90-3f9d3d84c01c` confirmed running, PID3076 at last poll.
Runner session 83190; log `.winbridge/update-preparation/worker-compatibility-portable-native.log`.
Actual native deletion boundary test passed: NotFound / OS Some(2), full retry
and final absence checked. Do not describe this as kernel AccessDenied reproduction;
that class has separate controlled fault coverage. Previous 0327204/29b4401 runs
are terminal failed test assertions, not production worker failures.

Poll this existing task to terminal, no observation-timeout restart. Original
cache owner 20261017-061527-f4639971 and schema-8 updater reference4b1eefef... remain.
Expect 206 tests / 19 groups + full build/config/rehearsal/Main/pins, derive actual.
Verify all ready bytes/SHA and fresh installed Info before pass; gate is not green yet.
Production code bf0573a/29b4401, tests7ed3900. Local530 / both Clippy / TS/UI passed.
No windows; worker remains, desktop unnecessary. No manual bootstrap now.

Next after review/pass: complete durable health library + actor as one vertical
scenario. Admission persists one-use intent bound to current artifact/source/
owner/epoch/v3 metadata before ACK; every helper PID/Job receipt precedes resume.
Config check and fresh-journal rehearsal run under owned jobs, never copied live DB.
Refactor low-level probe lifecycle hooks; reuse artifact receipt descriptor to avoid
extra query processes. Save terminal cleanup even on abort; abort/release must
check health quiescence, not just compiler-empty. Pending intents never mint replay
permits. Only fresh success can atomically advance CandidateAccepted. Public
update/exact shutdown/install/U2/U3 remain open.

## U1: portable native removal assertion corrected; repeat next — 2026-10-19

0327204 / run `20261019-112845-985c1652` / Task
`10c55041-01fc-41b1-9426-30449c5d3d41` terminal failed1 in updater lib tests:
the real metadata/open deletion race returned NotFound on this host, not Access
denied. Five ready files verified; SHA
`1b2df0adc2441e04caf293795d2f6ec43055ee622035922641fc8b940476fc62`.
Worker healthy/idle0, generation unchanged. No working runtime behavior failed.

Windows-only test now requires actual open failure of either supported removal
kind (NotFound/PermissionDenied), logs native kind/code and still checks exactly
one full-validation retry / final absence. No arbitrary-error acceptance or skip.
This host does not reproduce original kernel error 5 through deletion; that guard
branch is fault-injected separately with persistent-denial/hardlink rejection tests.
Runtime bf0573a/29b4401 unchanged; Windows Clippy passed, Linux 530 tests unchanged.
Commit/export and automatically repeat with same original cache/schema-8 reference.
No bootstrap/desktop required. Full native gate and durable health receipts remain.

## U1: corrected-boundary combined native retry running — 2026-10-19

Clean source `03272045432ca4e1ad05f2d4ea28c1545abb233b`, archive
`winbridge-20261019T152825Z-0327204-6b6845c8.zip`, SHA
`b9bef2746265312a77204c92a1a6070b8977fbcb0662e7dd78f831a7e3b0ace2`
(4,874,708 bytes / 594 files). Run `20261019-112845-985c1652`, Task
`10c55041-01fc-41b1-9426-30449c5d3d41` confirmed running.
Runner session 25067; log `.winbridge/update-preparation/worker-compatibility-retry-native.log`.
Poll this existing task, never restart because an observation timeout elapsed.
Same original cache owner 20261017-061527-f4639971 and schema-8 reference 4b1eefef...
from baseline gate. Expected 206 tests / 19 groups + full candidate/config/rehearsal/
Main/pins proofs; derive actual counts and verify every ready file before pass.
0327204 only corrects the Windows test boundary; production code remains bf0573a
and 29b4401, local 530 tests / both Clippy targets / generated TS/UI checks passed.
The preceding 29b4401 native run failed only the incorrect new test assertion;
see the next handoff entry and failure report. No candidate approval is enabled.
No windows/installed replacement. Leave installed worker running, desktop not needed.
Next: review final native outcome, then durable one-use health launch/PID/cleanup
receipt with current artifact/source/owner/epoch/v3 bindings. U1/U2/U3 still open.

## U1: native test boundary corrected; combined repeat next — 2026-10-19

29b4401 gate `20261019-112352-a49287c3` / Task
`75f1319b-556b-48c5-9b28-a92814d31c95` terminal failed1 in updater lib tests.
The new DeleteFileW fixture deleted before metadata, so optional-path guard saw
missing entry and returned Ok; its PermissionDenied assertion was incorrect.
Five ready files verified; SHA
`6a1c45b59301dd882acb14f85d713acfafcac9fff9d81eaf57f861fc24ade1fe`.
Production worker remains healthy/idle0, generation unchanged; no manual update.

Corrected Windows-only fixture now reads metadata, performs actual DeleteFileW,
then opens for link check to require real PermissionDenied; releases last handle
and checks full-validation retry. Runtime code bf0573a/29b4401 unchanged. Windows
Clippy passed; 530 local tests from previous full run remain applicable (this test
is Windows-only). Commit/export and repeat the combined native gate using the
same original cache owner and schema-8 reference. No installed changes or UI.

## U1: compatibility + sidecar correction native retry running — 2026-10-19

Previous goal turn made progress: bf0573a guard fix and 29b4401 live/config
compatibility committed. 530 workspace tests/doctests, both Clippy targets,
generated TS/check, UI typecheck/build and runner/cache checks passed sequentially.

Clean source `29b4401dce26546c4e1b4116e6a2b6efce1b0f46`, archive
`winbridge-20261019T152259Z-29b4401-9e875483.zip`, SHA
`cf2e1be8034690fa7f55a62a2eb1182ebdbd1348b78c2c34d5fe150726bd3fa1`
(4,873,004 bytes, 594 files). Run `20261019-112352-a49287c3`, Task
`75f1319b-556b-48c5-9b28-a92814d31c95` confirmed running.
Runner session 53743; log `.winbridge/update-preparation/worker-compatibility-native.log`.
Poll the same task, do not restart on observation timeout. Original cache owner
20261017-061527-f4639971, schema-8 updater reference below unchanged. Expected
206 native tests / 19 groups, full candidate build, actual config-check and
fresh-state rehearsal proofs, actual product Main/pins. Derive actual final counts.
The Windows-only DeleteFileW test must prove full validation handles delete-pending
Access denied, not merely mock this OS behavior. Verify all ready bytes/hash and
fresh installed Info. Full rehearsal/compatibility gate is not yet passed.

Installed worker still source1f3aa9f/schema14, generation unchanged, healthy/idle
at launch. No windows; leave it running, desktop not needed. No bootstrap now.
Next after review/pass: durable one-use health launch intent/PID before resume,
config/rehearsal reports + cleanup bound to owner/epoch/current artifact/source
receipt and fresh v3 compatibility. Do not adopt standalone JSON or run recovery
on a copied live DB. Health approval/exact shutdown/install and U2/U3 still open.
Use sequential Cargo commands: different feature builds during workspace tests
can overwrite metadata needed by later rustdoc. Root free ~32 GB; measurements
and validation logs under .winbridge/update-preparation, no caches pruned.

## U1: compatibility v3/v9 + readonly candidate config implemented; guard fix — 2026-10-19

[Compatibility block](plans/worker-self-update/worker-compatibility.md) is ready for native:
new worker control v3 / supervisor v9 share the existing one-read actor slot;
actual worker/updater schemas/context/config SHA come from authenticated peer.
Candidate readonly config Main validates exact bounded bytes without Worker::open
or copying config values. New marker/typed result; old v1/v2/v6 JSON unchanged.
Two contract and three actual-process config tests passed. An individual worker
--lib test initially used an old updater executable; full-workspace fresh-binary
run passed that control test. Failure assertions now print actual error details.

[Previous Windows gate failed](plans/worker-self-update/windows-worker-rehearsal-gate-2026-10-19.md):
cd85cf9 / run `20261019-074259-5c52f602`, Task
`87eac941-f37c-4fbe-aa9c-b84846cb84e0` terminal failed1, 196 tests passed including
all four new rehearsal tests, but full candidate build not reached. 35 ready files
verified, SHA `117e085550c5888298cf4dd96e9cfa7db37fea146c3d0f5e6fb5330dd729bf66`.
Unchanged cancel callback failed with Access denied on its private updates.sqlite-wal,
not outer process timeout. Installed worker fresh Info healthy/idle0, generation unchanged.

Path fix preserves platform I/O kind; only exact optional WAL/SHM validation may
retry PermissionDenied (3 attempts / 20ms max waits), no DB/file effect replay.
Persistent denial, required DB/lock paths and unsafe replacement still fail closed.
Seven Linux path tests passed; Windows actual DeleteFileW pending-delete test added.
No schema change (9/15). Windows run directories not deleted.

Full checks are being repeated sequentially after an overlapping --lib build
changed shared target metadata during workspace doctests (E0463 transport).
All 526 runtime tests in that workspace run passed, but doc failure means the run
was not green. Avoid concurrent Cargo builds/tests even while build lock is free.
Sequential rerun is green: 530 workspace tests (including doctests), Linux/Windows
Clippy all-targets. Contract generator now declares the new DTOs, regenerated TS
and --check passed; UI typecheck/build, scaffold 212 md, runner 4/cache 10 passed.
Guard fix committed bf0573a; commit compatibility, export clean source and repeat combined native
gate with original cache owner / schema-8 reference.
Do not claim health/approval/public install enabled. Next durable health launch
must persist intent/process/cleanup before ACK/resume, bind v3 metadata and current
artifact/source authority, never adopt standalone diagnostic JSON or copied live DB.

## U1: candidate fresh-state rehearsal native gate running — 2026-10-19

Clean source `cd85cf9b653356196c83879d0d3b141c162f4c49`, archive
`winbridge-20261019T114251Z-cd85cf9-0ef995cf.zip`, SHA
`55c01a95938060c28033451dd8a32774e600d5949d228481741a12b4b7c6cfbc`
(4,856,364 bytes, 586 files). Shared run `20261019-074259-5c52f602`, Task
`87eac941-f37c-4fbe-aa9c-b84846cb84e0` accepted/running.
Debian runner session 14707; log `.winbridge/update-preparation/worker-rehearsal-native.log`.
Poll this existing task; no timeout replay. Reuse only original cache owner
`20261017-061527-f4639971`; schema-8 updater reference/SHA from baseline gate below.
Schema remains 9/15. Expected old gates plus four actual rehearsal tests and
actual full candidate `worker-rehearsal-proof.json`; verify final ready files/hash,
exact schemas/source/image/process and production Info before claiming success.
No GUI/networked new process or installed replacement; worker remains, desktop unnecessary.
Next: review native outcome, then durable owner/epoch/artifact-bound health launch,
receipt/approval and responsive abort; actual launch needs saved intent before any
process creation/resume, not adoption of this standalone diagnostic JSON. Current
worker compatibility must come from authenticated control, not caller schema args.
Do not schedule/recover a copied live journal. U1/U2/U3/bootstrap remain open.

## U1: fresh-state worker rehearsal implemented — 2026-10-19

[Standalone diagnostic](plans/worker-self-update/worker-rehearsal.md) adds a separate
capability marker / typed report; ArtifactQuery v1 and legacy RPC unchanged.
Candidate uses create-only fresh local state/unique IPC, no production config,
controller, delivery, recipes or direct commands. Actual IPC readiness, normal
fresh-instance server stop and outer owned-tree cleanup are separate from future
production activation. Exact schemas are diagnostic expectations, not current-worker
proof. No durable health receipt/approval/FSM transition yet. Schemas remain 9/15.
522 workspace tests including four actual-process rehearsal tests passed;
Linux/Windows Clippy all-targets, fmt/contracts, Node typecheck/scaffold (210 md),
exporter 8 tests, runner 4/cache 10 tests and PowerShell syntax/schema checks passed.
Windows test/build/rehearsal gate pending. Debian free 32.2 GB; disk measurement
`.winbridge/update-preparation/worker-rehearsal-checks-disk.json`, no caches pruned.
Next: finish checks/native gate, then owner/epoch/artifact-bound health intent and
receipt with authenticated production compatibility observation. Never run recovery
on a copied live journal. Installed worker unmodified/healthy; no bootstrap needed.

## U1: preimages actor native gate passed — 2026-10-19

Clean 2cdc850, run `20261019-072235-273b010c`, Task
`567fe768-5bf1-4005-9439-68a2eb1263e1`: succeeded0,
[193 native tests / 19 groups / full four-exe build](plans/worker-self-update/windows-runtime-preimages-actor-gate-2026-10-19.md).
43 ready files verified; report SHA
`8e9f8da9e3f5c1dcc2d87936ec06d800483efe70fe3691da902971094bb1dcbc`.
Actual copied worker process confirms before-ACK intent, responsive status,
create-only independent files, abort accepted while hashing and completed only
after copier quiescence. Other role descriptors synthetic, no health claim.
Actual product Main/pins also passed; forced fixture stop is not production shutdown.
Installed worker healthy/idle0, generation unchanged; no windows, leave worker running.
Next: fresh-state rehearsal (never serve over a copied live worker journal),
durable health/approval, exact shutdown/install. U1/bootstrap remain ahead.

## U1: actor v8 native gate running — 2026-10-19

Clean source 2cdc850, archive `winbridge-20261019T112211Z-2cdc850-0148b636.zip`,
SHA `3d2e433ac2b345c969f0e7f0ec334dfe34347afe09a00d1f5b98095140a5ec7a`.
Run `20261019-072235-273b010c`, Task `567fe768-5bf1-4005-9439-68a2eb1263e1`
confirmed running; local session 83592, log
`.winbridge/update-preparation/preimages-actor-native.log`. Reuse original cache
20261017-061527-f4639971 and schema-8 reference 4b1eefef... from baseline run.
Native fixture now archives five files, aborts, waits exact supervisor exit,
launches a second request on same worker/new epoch, and cancels at held runtime
observation: durable abort before cleanup, no late write. No GUI/production apply.
Poll same task/handle to terminal; verify every ready file before declaring passed.
Leave installed worker running; desktop not needed. If this gate passes, review
and begin candidate health/rehearsal. Avoid starting recovery over a copied live
worker journal: saved process identities must never attach to production jobs.
Use explicit readonly compatibility inspection and an isolated fresh runtime for
actual candidate serve. U1/public install and bootstrap remain ahead.

## U1: preimages actor v8 / cooperative abort ready for native — 2026-10-19

[Actor](plans/worker-self-update/runtime-preimages-actor.md): fast durable admission
before ACK, one blocking copy driver, consistent parent/baseline/preimages schema-3
view. V1–V7 omit the new field. Pending/completed duplicates do not replay writes.
The supervisor shares one physical authority via private Arc; preimages abort uses
a second CAS-checked connection with the same owner/epoch, persists Aborting before
ACK, then runs cleanup/release only after the copier callback actually returns.
Source/compiler busy-abort behavior retained. No-lease abort now reports confirmed
absence instead of a fictitious release. Fixture drop order closes SQLite/owner
before TempDir; actual removal regression added.
518 workspace tests passed, plus actual no-lease IPC abort repeat after message fix;
Linux/Windows Clippy all-targets, fmt/generated contracts/typecheck/scaffold,
exporter/runner/PowerShell syntax passed. Positive copied-process fixture compiled:
successful archive, second request/new epoch, held worker reply with durable abort
and no early cleanup/late write. It is not yet executed on Windows. Other-role
images synthetic, no candidate health/activation claimed. Schema remains 9.
Next: native actor gate using same schema-8 reference below, then health/rehearsal,
exact shutdown and install. U1/public update still open, bootstrap not needed.

## U1: preimages library combined native gate passed — 2026-10-19

[Gate](plans/worker-self-update/windows-runtime-preimages-gate-2026-10-19.md): clean
835bc14, run `20261019-070223-c48d2a35`, Task
`6b6c13ce-f7e4-4863-bd48-78ff8783d60b` succeeded/0. 189 tests / 19 groups,
full build/queries/exact payload, actual Main and serve/pins passed. 43 ready files
verified, report SHA `3d5d84ca7ed6757f5976a8e591559ea44c6c1fb3040b9ef33dd5f08736226498`.
Windows free before/after 21,363,748,864 / 20,961,271,808 bytes; installed worker
same generation, connected/idle0/error null. Desktop closed/no windows, leave worker.
Schema-9 updater image retained for future schema>9 refusal: run above payload
winbridge-update.exe, SHA `b3d1c13773ea1a0e11800a6c856890e0cad7122044ee475527da0781326d755d`.
Actor v8 stays schema 9, so its reference remains the earlier schema-8 updater
SHA `4b1eefef24a3b658a78bf75241aae08ac3d4ed57723f0fe06edd30251e8ccc26`.

## U1: native library run stopped in older abort poll; path guard retry — 2026-10-19

Run `20261019-064446-fb0614ea`, source 823abd0, Task
`e30bf515-e139-45be-8cbb-598d491e68bb` terminal failed/1. All 11 Windows
preimages tests passed, including real read-pin/release semantics. Overall 176
tests passed before artifact-actor success-case abort polling returned Storage
NotFound; the failed message did not identify the path. Working worker is healthy.
Code review reproduced a real optional WAL/SHM metadata→hardlink-open race. Guard
now reruns the entire path validation once after NotFound and adds path context;
unsafe replacements remain rejected, no DB/effect operation is retried. Four
controlled real-filesystem removal/replacement tests passed, including hardlink/
symlink refusal. This is a consistency fix, not proof of which missing file caused
that original failure; native retry is required.
31 ready files verified, report SHA
`8da6466de63632823df1ef0aa7afd1f8e1f3c96aa40a5b84353b231674f926a5`.
Windows free before/after 21,527,560,192 / 21,435,224,064 bytes. No candidate full
build was reached. Installed generation unchanged, connected/idle0/error null.
Automatic retry authorized. Supervisor v8/Arc authority/cancel and fixture-drop
cleanup are WIP, excluded from the isolated library retry; do not mark them ready.

## U1: native preimages library gate running — 2026-10-19

Clean source 823abd0, archive `winbridge-20261019T104419Z-823abd0-df43707a.zip`,
SHA `2469278343f935c9555bd96e2e9fff70459e3ddb0c0f5cb5421371879403c4ff`.
Run `20261019-064446-fb0614ea`, Task `e30bf515-e139-45be-8cbb-598d491e68bb`
confirmed running, local runner session 69250, log
`.winbridge/update-preparation/runtime-preimages-native.log`. Expected native checks
include the new copy/ledger/fault cases and Windows read-pin release, previous
schema-8 updater refusal, full build/Main/pins. No result yet; wait same task/handle,
verify ready files only after terminal status. Original cache owner reused;
reference updater path/SHA below. Installed worker stays, desktop not needed,
no windows or production replacement. While running: implement supervisor v8
fast admission/status + blocking preimages driver and its positive native fixture.

## U1: previous-runtime preimages library — 2026-10-19

[Preimages](plans/worker-self-update/runtime-preimages.md), updater schema 9:
fast metadata admission + one-use non-Clone permit, intent/receipt atomic with
Requested parent transitions. Before source/compiler, copy only known manifest
and four exe to fixed runtime/update/<id>/previous-runtime. Existing/partial sets
are never overwritten/adopted, dropped permit never reauthorizes writes. Blocking
execute pins originals, fresh bound-worker observations/config/epoch/parent checks,
create_new + streaming hash/sync/new File IDs, copied pins held through commit.
Pending blocks source. Final generation/SHA recheck closes a late-tamper gap found
in review. Admission does not hash or call worker IPC before ACK. No runtime replace.
510 workspace tests (10 new local copy/ledger/fault cases), Linux/Windows Clippy
all-targets, fmt/contracts/typecheck/scaffold passed. Windows-specific pinned-copy
release test compiled, not yet executed. Fixtures moved off small /tmp tmpfs onto
local disk without lowering 6-GiB guard; Debian free 32,498,028,544 bytes recorded.
Migration 8→9/retained history tested; schema-8 previous-updater reference below.
Next: native library gate, supervisor v8 copy/status/abort integration and native
positive mailbox/copy case, then health/rehearsal/exact shutdown. U1 and public
install remain open; installed worker unchanged, bootstrap not needed.

## U1: runtime baseline native combined gate passed — 2026-10-19

[Gate](plans/worker-self-update/windows-runtime-baseline-gate-2026-10-19.md): clean
b21aa28, run `20261019-062511-6aceb224`, Task
`84ef11e1-650c-4aa3-89f2-9f2ad2027557` succeeded/0. All 175 tests / 19 groups,
full four-exe build/queries/exact files, actual Main and serve/pins passed.
43 ready files verified; report SHA
`7245c974cc498b5781712744a690a656a81cdd86db304503297c0d2d7d7d9494`.
Windows free before/after 21,924,835,328 / 21,591,035,904 bytes.
Fresh installed Info: same generation, connected/idle0/error null; desktop not
needed, leave worker. For schema-9 refusal use retained schema-8 updater:
K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261019-062511-6aceb224\update-candidate\payload\winbridge-update.exe
SHA `4b1eefef24a3b658a78bf75241aae08ac3d4ed57723f0fe06edd30251e8ccc26`.
New schema-9 code was excluded from that clean schema-8 gate.

## U1: native baseline tests passed, runner schema check fixed — 2026-10-19

Run `20261018-210729-e33e2cbd`, source 34f39ed, Task
`4eb2c866-0759-42c8-87c7-2e888fe85ae0` terminal failed/1. All 175 native tests /
19 groups passed, including positive baseline receipt, held query/status/source
block, duplicate, and previous schema-7 updater refusal. The final PowerShell
proof validator still required schema 7; actual schema is 8. Python final verifier
had the same stale assumption. Both now compare the exact constant in verified
source/archive, never a rolling minimum/local checkout version. Portable PS version
regression and 3 preparation/10 cache runner tests passed; syntax passed.
36 ready files verified, report SHA
`3ea8e8891e939e69d400ff72f49d50f874d5611b32d4617975a41f60e247181e`.
Windows free before/after 21,232,783,360 / 21,121,896,448 bytes. Full candidate build
was not reached; baseline combined gate remains open. Fresh installed worker Info:
same 1f3aa9f/build/generation, connected/idle0/error null; no production changes.
Automatic retry is authorized, no bootstrap. Schema-9 preimages WIP stays excluded
from the clean schema-8 retry. Continue from the task's actual state, do not restart
an already running retry solely because observation times out.

## U1: native baseline gate running — 2026-10-18

Clean source 34f39ed, archive `winbridge-20261019T010715Z-34f39ed-4feb98b4.zip`,
SHA `4ae27f1e9a175bf66ea8017b9eaaf03e33dbbeea8564d5a9089d7e82c96bd746`.
Run `20261018-210729-e33e2cbd`, Task `4eb2c866-0759-42c8-87c7-2e888fe85ae0`
confirmed running through the installed worker. Local runner session 7484,
log `.winbridge/update-preparation/runtime-baseline-actor-native.log`.
Schema-7 updater reference SHA/path описаны ниже; исходный cache owner
20261017-061527-f4639971. Windows gate проверяет новые baseline receipt/actor cases,
previous-version refusal, full build и actual Main/lifetime. Результата пока нет;
дождаться terminal task и проверить ready files перед заявлением о прохождении.
Installed worker оставить работать, desktop не нужен; окон не будет. Production
runtime не устанавливается. После gate — review, затем preimages/health блок.

## U1: baseline supervisor v7 / typed status — 2026-10-18

[Baseline](plans/worker-self-update/runtime-baseline.md): CaptureRuntimeBaseline
сохраняет intent до ACK, один blocking driver с исходной authority выполняет capture,
ObserveRuntimeBaseline читает consistent metadata независимо от hashing. Typed
NotAdmitted/Pending/Verified projection доступен только v7/view schema 2; v1–v6
сохраняют прежний schema-1 JSON без нового поля. Pending/failure блокируют source;
failed driver явно требует inspection/abort, запрос status не повторяет I/O.
Дополнительный review связывает intent owner/epoch с retained supervisor owner.
500 workspace tests и восемь повторных integrity tests passed; fmt/generated
contracts/typecheck/scaffold/exporter/PowerShell syntax passed. Linux/Windows Clippy all-targets passed. Positive copied-process native fixture
расширен: intent before ACK, held query/status/duplicate/source block, retained
receipt и historical duplicate при изменённом config. Проверка ещё не исполнялась;
test harness и synthetic other roles не являются candidate health/activation.
Next native schema-8 gate использует сохранённый schema-7 updater из run
`20261018-160143-62159cbf`, SHA
`70a5ce95c3d705d92b8546a979626d3db03193962979cefee0c625bf91a0922b`;
worker reference остаётся установленным schema14. Установленный runtime не менялся.
U1/public install/health/shutdown всё ещё открыты. Далее native gate, затем runtime
preimages и candidate health/rehearsal; ручной bootstrap пока не нужен.

## U1: live runtime control native gate passed — 2026-10-18

[Gate](plans/worker-self-update/windows-runtime-control-gate-2026-10-18.md): clean
256b182, run `20261018-204941-965a8934`, Task
`1276429a-ff9b-46c3-81ed-673de1a85395` succeeded/0. 165 tests / 19 groups,
full four-exe build/queries/exact files, actual Main и combined pin lifetime passed.
Все 43 ready files проверены; report SHA
`d57e5d937afe745fea91ed1af1eacc0d5b92ab1dd13abdedaaafddd1a3139b55`.
Windows free before/after 21,688,569,856 / 21,313,851,392 bytes. Installed worker
прежний, connected/idle0/error null. Desktop можно закрыть; worker оставить.
Schema-8 baseline и internal v7 разрабатываются отдельно; native для них впереди.
U1 целиком/public install остаются открытыми, bootstrap не требуется.

## U1: durable old-runtime baseline library — 2026-10-18

[Baseline](plans/worker-self-update/runtime-baseline.md), updater schema 8: immutable
intent/receipt атомарны с двумя parent transitions до source effects. Pending
блокирует BeginPreparation/source plan; raw lifecycle events и late adoption
запрещены. Receipt связывает known manifest/четыре images, exact loaded worker,
frozen supervisor root/owner/epoch/config/request. Pins удерживаются до commit.
Повтор готового результата читает историю; epoch restart не перепривязывает pending.
Migration 7 → 8 сохраняет consistent backup/history и не создаёт baseline автоматически.
498 workspace tests passed, включая восемь новых DB fixtures. Старые migration
fixtures обновлены для удаления schema-8 additions перед имитацией старой схемы;
production migration остаётся строгой. fmt/generated contracts/typecheck/scaffold
и exporter tests passed; Linux/Windows Clippy all-targets passed.
Это trusted library без supervisor v7/public UI/update. Native schema-8 gate,
preimages, health/shutdown/install остаются впереди. Independent native schema-7
run `20261018-204941-965a8934`, Task `1276429a-ff9b-46c3-81ed-673de1a85395`
проверяет clean 256b182 с исправлением query retirement; пока running.
Debian free: 32,382,713,856 bytes, измерение сохранено локально. Installed worker
прежний; ручной bootstrap не требуется. Далее supervisor baseline admission/status.

## U1: повтор native control — гонка завершения query — 2026-10-18

Run `20261018-203256-077d1a67`, source db2e565, Task
`8129ff22-538d-4a89-af8c-88101e7be06b` failed/1: 164 tests passed;
следующий runtime query получил ложный Busy после ответа на предыдущий.
Supervisor отправлял ответ до завершения/удаления JoinSet entry. Исправление
публикует ответ только после retirement query; regression повторяет три запроса
подряд без sleep/status roundtrip. Native повтор впереди, gate/U1 открыты.
Все 35 ready files проверены; report SHA
`89f42a1e98a1e0056cb4817af5357f81f313d316cec00f4d610fa1cf50004c8f`.
Windows free before/after: 21,781,127,168 / 21,750,173,696 bytes.
Installed worker idle/healthy, generation прежний; production runtime не менялся.
Schema-8 baseline разрабатывается отдельно и в этот повтор не входит.

## U1: native runtime fixture — исправление проверки mutex — 2026-10-18

Native clean 5b3d591, run `20261018-201934-bbddb98b`, Task
`69b6a2e7-9f7a-4972-a7bd-0be89c48278b` failed/1: 164 tests passed, positive
copied-process test получил корректную identity, но затем его single try_lock
попал на обычный scheduler Store lock. Это не доказывает удержание Store запросом.
Проверка заменена на bounded 2 s acquisition при всё ещё удержанном 10 s checkpoint;
poison/permanent lock по-прежнему fail. Windows Clippy all-targets passed.
35 ready files проверены; report SHA
`ad6b58bb1e7f82102dcaea4668fcad1b45671aeea026d961ee546efd76e85680`.
Windows free before/after 21,942,714,368 / 21,840,375,808 bytes. Full candidate build
не выполнялся; positive control gate и U1 ещё открыты. Installed worker прежний,
idle 0, errors none. Автоматический повтор разрешён; bootstrap не требуется.

## U1: live runtime control v2 / supervisor v6 — 2026-10-18

[Реализация](plans/worker-self-update/runtime-live-control.md): readonly ObserveRuntime
из живого guard, exact process/build/install/scope/owner/epoch и повтор config/context
checks после hashing. V1 lease wire сохраняется, cross-version interpretation запрещён.
Sup v6 обслуживает один async readonly query вне mailbox; observe status и duplicate
Busy проверены. Normal Main/public RPC/MCP update control ещё не включают, schemas15/7.
490 workspace tests, Linux/Windows Clippy all-targets, fmt/contracts/typecheck,
scaffold/Python exporter/runner и PowerShell syntax passed. Recovery fixtures
сериализованы после transient lock conflict; production recovery lock не менялся.
Positive explicit Windows fixture с copied test harness/actual Worker actor +
supervisor IPC/cfg-change-after-hash подготовлен; other-role metadata synthetic,
health/install не заявляет. Combined native gate (план 165 tests / 19 groups) впереди,
включая actual product Main/pin lifetime повтор. Installed worker прежний/healthy,
bootstrap пока не нужен. Далее durable old-runtime baseline до SourceApply.

## U1: shared runtime identity DTO — 2026-10-18

[Контракт](plans/worker-self-update/runtime-identity-contract.md): RuntimeImageEvidence
и LoadedWorkerIdentity перенесены в protocol; старые app-update Rust paths оставлены
re-export, JSON bytes/order и schemas 15/7 прежние. Generated TS добавлен. Два
exact-byte/unknown-authority-field tests, legacy contracts, четыре installed-set
и 11 report/ledger tests passed; workspace check, Linux/Windows Clippy all-targets,
fmt/contracts/typecheck/scaffold passed. Нового runtime control действия пока нет.
Следом readonly control v2 ObserveRuntime с exact active supervisor/worker binding
и fresh config checks, затем durable baseline до SourceApply. Registration image
SHA остаётся updater SHA; worker image identity нельзя подменить этим полем.
Native startup/pin gate ниже остаётся evidence для tested d3c9ed6; DTO refactor не
объявляет новый native execution. Installed worker прежний/healthy, bootstrap не нужен.

## U1: actual worker startup/pins native gate пройден — 2026-10-18

[Gate](plans/worker-self-update/windows-runtime-startup-gate-2026-10-18.md): source
d3c9ed6, run `20261018-160143-62159cbf`, Task
`7d5526c6-d8be-4e09-81f0-7795f5cc8d07` succeeded/0: 159 native tests / 17 groups,
full build/four queries/exact files и actual readonly Main positive/negative passed.
Все 38 ready files проверены. Follow-up Task `1c5bfea2-b598-4782-9bbe-a70184375472`
succeeded/0: actual serve удерживал пять pins, после forced owned idle stop handles
освободились, bytes/updater DB сохранены. Late proof имеет отдельный ready marker.
Новая runner integration не требует повторного build для уже выполненного follow-up;
combined pipeline с её дополнительным вызовом ещё не запускался. Scripts syntax,
Python runner/export и scaffold checks passed; Rust код совпадает с tested d3c9ed6.
Readonly debug verification заняла 9.4 s: activation deadline должен учитывать
hashing до IPC. Тестовый worker остановлен; installed worker прежний/idle 0/errors
none/coverage complete. Desktop закрыт, worker оставить; bootstrap не требуется.
Далее authenticated live identity/control и durable baseline до SourceApply,
затем health/exact shutdown/runtime transaction/activation. U1 открыт, A4 на паузе.

## U1: ранний startup guard — 2026-10-18

[Контракт](plans/worker-self-update/runtime-startup.md): normal serve с known
manifest проверяет runtime/installation/config и loaded worker до рабочего journal;
пять pins удерживаются всю жизнь worker. Readonly verify-runtime не запускает
scheduler/network/update authority. Три actual-main negative tests passed locally;
positive actual Windows main проверяется отдельным approved script на четырёх
настоящих candidate exe. Legacy без manifest остаётся без новой update authority.
Worker/updater schemas и wire API не меняются. 485 workspace tests, Linux/Windows
Clippy, contracts/fmt/scaffold, export/runner tests и PowerShell syntax passed.
В общем local run один быстрый Unix report fixture дал unknown outcome; отдельный
повтор и 30 диагностических repeats прошли. Ревью выявило возможный exit до PID
binding; report helper теперь ждёт durable Running вместо timing sleep. Production
unknown/no-replay поведение не ослаблено, полный повтор passed. Positive native
gate впереди. Bootstrap/public install ещё не готовы; installed worker здоров.

## U1: artifact actor native gate пройден — 2026-10-18

[Gate](plans/worker-self-update/windows-artifact-actor-gate-2026-10-18.md): clean
48b8d21, run `20261018-153922-df4b0630`, Task
`eb05e4e8-7459-4079-a5ef-6341fbe53170` succeeded/0. 156 native tests / 16 groups,
полный Rust/native desktop build, четыре queries и actual exact-file inspection
passed. Все 36 ready files проверены. Artifact actor positive/negative, schema 7,
known-set pins, abort read snapshot и исправления staging/finalization подтверждены.
Installed source/generation прежние, idle 0/errors none/coverage complete.
Desktop не нужен, worker оставить. U1 не завершён; normal startup/live identity,
baseline до SourceApply и exact shutdown дальше, bootstrap пока не требуется.

## U1: runtime manifest, abort snapshot и Windows fixture staging — 2026-10-18

[Installed-set reader/pins](plans/worker-self-update/installed-set.md) добавлены
как trusted readonly API: known manifest вместо auto-adoption, scope/roles/source/
hash/File ID guards. Publication, normal worker startup/control, durable baseline
до SourceApply и replacement ещё НЕ подключены. [Abort read snapshot](plans/worker-self-update/abort-read-snapshot.md)
исправляет реальную race header/history, найденную full local run; deterministic
two-connection WAL regression и affected source actor passed.

Native clean 9b0fb0b run `20261018-151002-c0223f35`, Task
`ad02318d-8f75-4a28-9745-8efb348e351d` failed/1 в positive artifact actor:
payload оказался с extra compiler files. 149 предыдущих tests и negative actor
passed; новый full candidate build не выполнялся. Portable finalization wiring
passed. 31 ready files проверены; Windows free before/after 23,111,782,400 /
23,058,837,504 bytes. Fixture теперь компилирует в target и create-only копирует
четыре exe в payload; extra-file запрет сохранён, error показывает bounded names.
Оба updated explicit local actor tests passed. Installed worker прежний/idle 0,
errors none, coverage complete; actual runtime/config/journal не менялись.

Full local повтор после snapshot fix: 482 workspace tests, Linux/Windows Clippy,
contracts/fmt/typecheck/scaffold, portable PowerShell, export/runner tests passed;
combined native repeat впереди. U1 целиком
не завершён, bootstrap не нужен, A4 остаётся на паузе.

## U1: artifact actor и finalization fix — 2026-10-18

[Supervisor v5](plans/worker-self-update/artifact-actor.md): authenticated readonly
artifact driver в responsive mailbox, duplicate observation, explicit active-kind,
atomic receipt/FSM и candidate-digest safe abort. Установки/health approval нет.
Два explicit local tests passed: rustc/четыре actual fixture metadata queries и
incomplete-report refusal/abort. 477 workspace tests, Linux/Windows Clippy,
fmt/contracts, typecheck/build:ui/scaffold, export/runner tests passed.

[Native 94d9a35 run](plans/worker-self-update/windows-artifact-set-diagnostic-2026-10-18.md)
`20261018-145111-7277b3db`, Task `cabd8b2c-1b85-4be7-a24f-bd70dd0a7075` failed/1:
149 tests и full build/four queries passed, final script variable ошибочна.
Readonly Task `b95883dd-ce36-4875-9da8-44e8f2bc31e9` succeeded: actual same candidate
files inspected. Fixed `$queryHelper`; portable regression выполняет реальный
final try/finally statement с metadata mock и включён до full native build.
Combined native rollup ещё впереди. Installed source 1f3aa9f/generation 994671b4
не менялись, idle 0/errors none/coverage complete. Desktop закрыт, worker оставить.
Следом old runtime/loaded image guards, full product layout и exact shutdown;
bootstrap пока не требуется, A4 на паузе.

## U1: exact artifact-set receipt — 2026-10-18

[Контракт](plans/worker-self-update/candidate-artifacts.md): typed report parser,
четыре fixed payload files / роли / source / OS / SHA / File IDs, совместные
Windows pins до commit. Immutable receipt и Building → ValidatingCandidate
атомарны; raw CandidateAccepted не разрешает обойти будущий health driver.
Updater schema 7, consistent migration 6 → 7 + epoch fence; safe abort включает
candidate digest. Worker schema 15 и legacy/supervisor wire shapes неизменны.
Mailbox artifact action, old runtime baseline, health/shutdown/install впереди.

477 workspace tests passed; Linux/Windows Clippy all-targets, explicit local
compiler actor, contracts/fmt, typecheck/scaffold, 8 export tests, Python runner
и PowerShell syntax passed. Ledger regressions используют явно synthetic query
receipts и НЕ доказывают executable health. Новый standalone readonly CLI
`inspect-candidate-files` проверит actual full candidate в native gate; его
`journalBound` и `candidateVerified` всегда false. Build-only script сохраняет
отдельные readable command/proof files. Native schema-7 gate ещё впереди.
Installed worker прежний/исправен; bootstrap пока не требуется.

## U1: compiler report native gate пройден — 2026-10-18

[Gate](plans/worker-self-update/windows-report-binding-gate-2026-10-18.md): clean
914df7c, run `20261018-110211-6ecc2e54`, Task
`65d0be5f-474f-43e0-8341-a6ae4025a164` succeeded/0. 139 native tests / 15 groups,
schema-5 → 6 refusal/backup, compiler report + actor receipt и полный candidate
build прошли. Все 29 ready files проверены. Installed worker прежний/idle 0,
coverage complete, errors нет. Desktop не нужен; worker оставить работать.
Далее exact artifact set / durable receipt и old runtime guards; U1/install
не завершены. Новый readonly artifact parser уже разрабатывается, без установки.

## U1: durable compiler report binding — 2026-10-18

[Контракт](plans/worker-self-update/compiler-report-binding.md): updater schema 6,
report intent до dispatch, runtime/update directory IDs, bounded report bytes /
File ID / SHA, atomic report receipt + successful compiler completion. Trusted
readonly API возвращает exact verified bytes; legacy/generic build не присваивает
поздний report. BuildRecord/Spec и supervisor v1–v4 wire shape неизменны; worker
schema остаётся 15. Потеря обязательного report оставляет Indeterminate, replay нет.

467 workspace tests passed, включая шесть новых report regressions: file/parent
replacement, same-size edit, hard links, missing/empty/oversize, ignored SQLite
receipt/completion writes, existing staging, schema-5 backup/epoch/no-adoption.
Linux/Windows Clippy all-targets, TypeScript/build:ui/scaffold, Python runner tests,
8 export tests, PowerShell syntax, explicit local compiler actor и
generated-contract check passed; native schema-6 gate впереди. Native
runner требует отдельный report-binding proof и подтверждение actor receipt.
Installed worker прежний/исправен; bootstrap/public install не включены.

## U1: compiler/artifact native gate пройден — 2026-10-18

[Gate](plans/worker-self-update/windows-artifact-identity-gate-2026-10-18.md): clean
a91b904, run `20261018-103821-c88c4d20`, Task
`5c1e0a9b-c20c-4642-b596-653eebc1468d` succeeded/0. 133 native tests / 15 groups,
полный Rust/native desktop build и четыре bounded role queries passed. 28 ready
files проверены по bytes/SHA. Исправление staging до probes подтвердилось;
hard-link guard сохранён. Installed worker прежний/idle 0, errors нет, coverage
complete. Desktop не нужен, worker оставить. Следом durable compiler report /
artifact-set binding и exact shutdown; U1/public install ещё не готовы.

## U1: review и native artifact staging — 2026-10-18

Native clean 06211fe run `20261018-101634-46ac2235`, Task
`816bfa89-5c28-41a6-9132-4e185e2f16f9`: все 133 tests / 15 groups passed,
включая compiler actor, safe abort, output, artifact probes и absolute paths.
Полный Rust/desktop build тоже прошёл, но gate НЕ завершён: identity query
Cargo worker.exe отклонён как hard link. Диагностический readonly Task
`760b0aa6-f79f-48f7-9584-88f78e10626a` сохранил failed-candidate-report.json и
identity-worker.log в shared. Gate исправлен без ослабления проверки: create-only
payload copies теперь создаются ДО probes, helper/queries работают с ними.
Portable hard-link isolation regression добавлен; повтор native впереди.
Windows free before/after: 25,842,319,360 / 25,685,712,896 bytes.

Code review также исправил повторный File ID check пути после query (Unix
same-bytes replacement regression + 11 preflight, Linux/Windows Clippy passed).
PowerShell wrappers временно используют UTF-8 и восстанавливают encoding;
ранняя диагностика уже показала OEM mojibake. Installed worker не менялся,
bootstrap пока не требуется. Далее durable artifact-set receipt и exact shutdown.

## U1: исправление Debian validation runner — 2026-10-18

В a8e6ce8 artifact-result checks ошибочно выполнялись до submit; Windows task
не создавался. Checks перенесены после чтения итогового candidate proof.
Два mock-IPC regression tests подтверждают admission до result inspection и
запрет submit при active user work. Rust/runtime код не менялся; 460 tests и
предыдущие Clippy/UI checks остаются актуальны. Следующий native run проверит
все изменения после двух реальных failed runs, установка ещё отключена.

## U1: artifact identity/probe и абсолютные tool paths — 2026-10-18

[Контракт](plans/worker-self-update/artifact-identity.md). Четыре exe получают
--artifact-info; static marker проверяется до запуска legacy desktop. Bounded
Job/stdout/stderr query не читает config, не пишет state, не разрешает installation.
Build-only script проверяет четыре role/source identities внутри compiler tree
и сверяет SHA с payload. Durable artifact-set receipt/health/shutdown ещё впереди.
460 workspace tests, 11 preflight tests и worker/fixture metadata checks passed;
Linux/Windows Clippy all-targets, fmt, generated contracts, typecheck/build:ui/
scaffold и 8 export tests passed. Native проверка desktop/query и полный повтор
ещё впереди.

Второй native run 6ae7382 `20261018-095232-7c0952a3` завершился failed/1
штатно с report.ready.json: 20 passed tests, один новый unit test выявил допустимый
для общего prefix helper drive-relative `K:relative`. Tool resolver теперь
отдельно требует is_absolute; schema-3 PowerShell executable тоже.
Windows free before/after 25,076,011,008 / 25,038,508,032 bytes; installed worker
idle 0, прежний generation, recovery/maintenance errors нет. Первый run и
PowerShell AuthorizationManager diagnosis сохранены ниже. Gate ещё НЕ пройден.

## U1: compiler output и исправление PowerShell paths — 2026-10-18

[Bounded output](plans/worker-self-update/compiler-output.md): typed readonly byte
pages, File ID cursor, live/terminal distinction и retained SHA proof. 18 compiler
regressions, explicit PowerShell actor и 8 worker-control tests passed locally.
Linux/Windows Clippy all-targets, fmt, contracts/typecheck/build:ui/scaffold
passed также после path fix; native повтор ещё впереди.

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

## Самообновление U1: compiler mailbox и coordinated abort — 2026-10-18

[Реализация и границы](plans/worker-self-update/compiler-actor-abort.md): внутренние
v3 build / v4 abort, config 3, updater journal 5. Compiler Intent до ACK, один
execution permit; source/Job/output proof до worker Release. Runtime apply,
public update и bootstrap не включены. 453 workspace tests, отдельный PowerShell
actor fixture, Linux/Windows Clippy all-targets, fmt, generated contracts,
typecheck/build:ui/scaffold, 8 export tests и syntax checks passed. Native gate
этого блока ещё не выполнен.

## Самообновление U1: compiler admission до dispatch — 2026-10-18

[Контракт](plans/worker-self-update/compiler-admission.md). admit_build сохраняет
Intent и возвращает fresh one-use BuildPermit без worker RPC/полного source scan.
Это acceptance metadata; execute заново требует live lease/source/config и actual
owner, затем перед OS create повторяет source/≥6 GiB checks. Saved row/duplicate/
dropped permit/new owner не создают execution grant. Старый execute_build сохраняет
pre-admission live validation. Schemas 4/15 прежние, новые DTO не нужны.

31 source/compiler tests passed, включая 5 новых admission/duplicate/restart/source/
offline/race checks с реальным счётчиком user-code starts в generated target.
Linux/Windows Clippy/tests cross-check и fmt passed; native rollup впереди.
Следующий блок — fixed local PowerShell build policy и compiler actor dispatch,
bounded output read, artifacts/coordinated abort, exact shutdown, U2–U3.
Installed worker прежний; bootstrap пока не нужен. A4 отложен.

## Самообновление U1: readonly identity остальных exe — 2026-10-18

Updater/fixture поддерживают --build-info до config/fixture effects. Query возвращает
BuildIdentity, не читает даже неверный default config и не создаёт journal/runs.log.
8 preflight и 4 coordination tests passed; Windows Clippy/tests cross-check, fmt и
Python syntax passed. Два новых CLI checks войдут в следующий native rollup; его
script проверяет реальные test names. Полный artifact verifier ещё не реализован.
Installed worker прежний. Следующий блок — fresh compiler admission до actor ACK,
управляемый compiler driver в supervisor и fixed local build policy; затем artifacts,
coordinated abort/release, exact shutdown, U2–U3. A4 отложен.

## Самообновление U1: source preparation actor — 2026-10-18

[Контракт](plans/worker-self-update/source-actor.md). Internal supervisor v2 принимает
source preparation после fresh Preparing commit. Owner/journal временно принадлежат
одному blocking driver; Observe читает consistent saved view и не ждёт worker reply.
Pinned ZIP → SourceHeld → preimages → fresh SourceApplyPermit → Verified/Building.
Source lease остаётся; compiler/artifacts/abort/shutdown/install не подключены.
Config schema 2 задаёт sourcePackagesDirectory; schema 1 сохранена для прежних
сценариев, source start без новой policy отклоняется до intent. Journal schemas
4/15 и legacy RPC hashes прежние; новые actions/DTO не сохраняются в БД. V1 replies
сохраняют прежнюю JSON форму. Normal worker main/MCP публично функцию не включают.

438 workspace tests и Linux/Windows Clippy/tests cross-check passed; 6 control tests
включают responsive mailbox, duplicate/Stop busy, missing policy и corrupt ZIP.
В fixture исправлены Unix socket path и delay без удержания Store mutex. Test gate
только cfg(test), production env bypass нет. Contracts generation/check, typecheck/build:ui/scaffold, 8 export tests, fmt и
Python/PowerShell syntax passed. [Native rollup clean fc1965c](plans/worker-self-update/windows-source-actor-gate-2026-10-18.md)
passed: 109 tests/14 groups, 25 artifacts и полный candidate build, включая source
admission и exact atomic root-exit. Run `20261018-043430-4fccfd01`, task
`49cc5f0d-2757-4e4e-aa7b-078d32cfab2d` succeeded/0. Installed healthy/idle.
Windows free before/after 25,554,857,984 / 26,192,838,656 bytes; Debian 33,531,867,136.
UI-окон не было; desktop можно закрыть, worker оставить. Installed worker прежний; bootstrap пока не нужен.
Следом compiler/artifact/abort driver, exact shutdown и U2–U3. A4 отложен.

## Самообновление U1: source admission перед dispatch — 2026-10-18

[Контракт](plans/worker-self-update/source-admission.md). Fresh commit parent/source
Applying выдаёт одноразовый непередаваемый SourceApplyPermit; повтор/receipt лишь
наблюдает. Исполнение можно передать blocking thread после ACK. Dropped permit и
owner restart не создают новый grant; barrier сохраняется. Синхронный API использует
тот же путь. 26 source/compiler integration tests и Windows Clippy/tests cross-check
passed; 3 новые проверки admission/restart/race. Native rollup впереди; он также
усиливает atomic owner-crash proof проверкой exact root exit. Schema 4/15 прежние.
Actor mailbox/orchestration, artifacts/abort и exact shutdown ещё не подключены.
Installed worker прежний, bootstrap пока не требуется; A4 отложен.

## Самообновление U1: atomic compiler launch — 2026-10-18

[Контракт и ограничения](plans/worker-self-update/atomic-executable.md).
Trusted compiler использует отдельный stable Win32 adapter: suspended CreateProcess
с ordered JOB_LIST, explicit stdio HANDLE_LIST, owned process/thread handles и CRT
argv. Внешние Jobs сохраняются; breakaway и post-create fallback отсутствуют.
Обычные worker PATH/.bat/.cmd команды сохраняют прежний adapter. Windows-only tests
проверяют crash до PID receipt и отсутствие user effects/orphan; schema 4/15 прежние.
431 workspace tests, Windows Clippy/tests cross-check, fmt и script syntax passed.
После итогового path/cleanup ревью 10 compiler regressions passed.
[Native gate clean 11566cb](plans/worker-self-update/windows-atomic-launch-gate-2026-10-18.md)
passed: 102 tests/14 groups, 24 artifacts и полный candidate build; installed healthy/idle.
Run `20261018-040345-92ee23bc`, task `81f6284b-339e-45ef-a4eb-2fe478ffd334`.
Windows free before/after 26,389,467,136 / 26,163,970,048 bytes; Debian 36,498,391,040.
UI-окон не было; desktop можно закрыть, worker оставить.
Installed runtime прежний;
bootstrap не требуется. Следом source/build actor orchestration, artifacts/abort,
exact shutdown и U2–U3; A4 отложен.

## Самообновление U1: managed compiler — 2026-10-18

[Контракт и ограничения](plans/worker-self-update/compiler-driver.md).
Trusted library сохраняет compiler intent/PID/Job/resume receipts до Windows effects,
отдельно собирает stdout/stderr и подтверждает root/tree/readers cleanup.
Timeout/nonzero, ignored writes, source edits, history corruption и duplicate не
создают второй процесс. Attempt ID отделён от binary build identity. Readonly
`journal build-status` не продолжает попытку даже при текущем Empty Job.
Updater schema 4: consistent v1/v2/v3 migration backup и old-binary refusal;
worker schema остаётся 15. Новая source/build orchestration ещё не подключена;
source lease/parent Building сохраняются до координированного artifact/abort driver.

429 workspace tests passed; после итоговых уточнений 10 compiler tests passed.
Итоговые core/platform checks, Clippy, Windows/tests cross-check, fmt/contracts,
typecheck/build:ui/scaffold и Python/PowerShell syntax прошли.
[Native gate clean 8990bd7](plans/worker-self-update/windows-compiler-gate-2026-10-18.md)
passed: 95 tests/13 groups, 22 checked artifacts и полный candidate build.
Run `20261018-034803-ef13a3c8`, task `5d1982b9-6537-44d9-824f-d0ec70e92523`
succeeded/0; installed worker healthy/idle, build/generation прежние.
Windows free before/after 29,884,207,104 / 27,965,808,640 bytes; Debian 36,538,798,080.
UI-окон не было; desktop можно закрыть, worker оставить. Следующий platform block
закрывает CreateProcess → Job assignment gap. Installed worker не изменяется; bootstrap пока
не требуется. После этого блока — source/build actor driver, artifact verification,
abort/release, exact shutdown и U2–U3. A4 остаётся отложенным.

## Самообновление U1: fresh SourceControl — 2026-10-17

[Контракт](plans/worker-self-update/fresh-source-control.md). Добавлен readonly
process-authenticated adapter к обычному worker Info: exact PID/birth/build/context,
config SHA до/после, bounded fresh response без reconnect/rebind. Source driver
по-прежнему отдельно проверяет свою live lease/owner/revision перед effects.
Долгое baseline/release чтение выполняется без Store/process registry mutex;
короткая transaction проверяет admission barrier/accepted owners до scan, затем
fresh authority/config/revision/blockers и atomic work slot проверяются до grant.
Scan сам не является source/build grant. Named-resource policy пока откладывает его.

10 worker update/control tests, 13 source/crash tests и 3 coordination regressions
passed; Clippy passed. Windows/tests cross-check и fmt/script syntax passed.
[Native gate clean 3e01e56](plans/worker-self-update/windows-fresh-source-gate-2026-10-17.md)
passed: 82 tests/13 groups, 21 checked artifacts и полный candidate build.
Run `20261017-175317-171ce504`, task `f9018010-5548-4892-a3eb-5ec2516b9f82`
succeeded/0; report SHA `e48db4bb337af5aa0ea42b3d189d34d3161d110e223d14d059fda03853c7239a`.
Installed build/generation прежние, final healthy/idle. UI-окон не было; desktop можно
закрыть, worker оставить. Windows free before/after 28,010,450,944 / 27,720,368,128;
Debian after verification 34,723,389,440 bytes. Worker/updater
schema остаются 15/3. Автоматический supervisor source driver, compiler ownership,
shutdown/install пока отключены; U1 целиком открыт. Installed worker прежний.
Следом compiler intent/receipt/OS tree cleanup, затем source/build orchestration,
exact shutdown и U2–U3. A4 отложен.

## Самообновление U1: worker launcher/control — 2026-10-17

[Контракт и ограничения](plans/worker-self-update/worker-control.md).
Trusted worker API запускает supervisor вне своего execution domain, связывает его
с exact worker/build/config/incarnation и открывает отдельный supervisor-only IPC.
Обычный CLI/MCP его не включает. Production fixed runtime/external Job guards
сохраняются; test override private и не является разрешением обходить Job.
Duplicate-first работает при занятом updater owner lock. SourceHeld атомарно
занимает общий work slot; named-resource policy пока явно откладывает grant.
Drain требует updater Draining intent; release при незавершённых file/build/cutover
phases запрещён. Callback завершения сервера дожидается blocking authority calls.
Worker schema 15/updater schema 3 не менялись. Source apply, shutdown и install
через новый канал пока отключены; U1 целиком открыт, A4 отложен.

417 workspace tests и 9 итоговых update/control tests passed. Проверены race task
claim/source grant, receipt rollback, forged peer/epoch/build/config, повтор при
живом owner, premature drain и verified release. Clippy, Windows/tests cross-check, fmt/contracts, typecheck/build:ui/scaffold и
Python/PowerShell syntax passed. [Native gate](plans/worker-self-update/windows-worker-control-gate-2026-10-17.md)
clean c705299 passed: 80 tests/13 groups, 21 checked artifacts и 4 candidate exe.
Run `20261017-174249-58eff6c0`, task `6dbfdff2-a789-43f6-bb90-06f1da3c75bd`
succeeded/0; report SHA `58484ae7fc24a2d78f743aac90e48341759cd69df4ef0a59207566c74defdfda`.
Installed build/generation прежние, healthy/idle; source/runtime/config не менялись.
Worker оставить работать, desktop можно закрыть. Debian free 34,772,668,416 bytes;
348 old ELF test executables (12,165,789,064 bytes) очищены, основные/running binaries
и данные сохранены. Windows free before/after 28,252,860,416 / 28,264,001,536 bytes.
Следом — fresh authenticated SourceControl, длительная verification без удержания
Store lock, compiler ownership/cleanup и exact shutdown, затем U2–U3.

## Самообновление U1: supervisor handoff — 2026-10-17

[Контракт и ограничения](plans/worker-self-update/supervisor.md),
[native Windows gate](plans/worker-self-update/windows-supervisor-gate-2026-10-17.md).
Внутренний launcher сохраняет intent/PID receipt, child получает следующий updater
owner epoch и отдельный authenticated IPC. Active фиксируется до ACK; parent exit
его не отменяет. До handoff child ограничен no-effects deadline, после — TTL takeover
отсутствует. Unknown launch не повторяется. Stop + parent failure атомарны.
Updater schema 3, consistent v1/v2 migration backups; schema-2 source status читается
readonly. Worker schema 15 не менялась. Production worker launcher/source-control
IPC/exact shutdown/install пока не готовы; U1 целиком открыт, A4 отложен.

413 workspace tests, final updater/targeted lost-ACK, Clippy, Windows/tests cross-check,
contracts/typecheck/build:ui/scaffold и Python/PowerShell checks passed.
Clean `64bb3b5` native gate: **76 tests / 12 groups**, actual ZIP и полный build
четырёх exe; 19 artifacts проверены. Run `20261017-161030-d0a06df8`, parent task
`6480fd5d-8e09-4740-8c3d-d1d9bc8874ce` succeeded/0; report SHA
`52f4000b3b4242dded4a7fbf2ab809f0a6b08bdcd8a6da8467df8620ffb8a335`.
Ревью/native test исправили отдельный desktop lockfile и bounded busy-pipe connection
до отправки данных; повторной отправки команды нет. Actual source smoke достиг
Ready за 8.113 s, manifest до/после тот же.

Установленный worker clean `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние; итоговый Info healthy/idle.
Рабочие source/runtime/config не обновлялись; UI-окон не было. Desktop можно закрыть,
worker оставить; обновление пока не требуется. Windows K free before/after:
31,665,422,336 / 30,528,774,144 bytes; Debian final 28,935,110,656 bytes.
Удалены 164 устаревших Debian test executables, 10,669,626,464 bytes reproducible cache;
running controller/main binaries и runtime/config/journals/reports сохранены.

Наследуемый внешний Job не обходится: externalJobObserved true в validation task.
Gate доказывает исключение из указанного test domain и parent exit, но не production
worker shutdown. До install нужны реальный worker adapter и проверка окружения.
Следом — source-control IPC/drain/quiesce/exact shutdown; перед source/build
необходима атомарная work/resource reservation, readonly Info её не заменяет.

## Самообновление U1: source transaction — 2026-10-17

[Контракт и восстановление](plans/worker-self-update/source-transaction.md).
Trusted library driver реализует pinned preparation/preimages, SourceApplying
barrier, no-clobber file moves, immutable intent/receipt и manifest-last/full verification.
Updater schema 2 с consistent schema-1 backup/owner-lock migration; worker schema 15
не меняется. Readonly source-status/source-inspect; crash/blocked не повторяют effects,
worker SourceHeld/startup guard остаётся. 405 local workspace tests passed;
11 source tests повторно прошли после итоговых style fixes. Clippy/workspace Windows
cross-check, contracts/typecheck/build:ui/scaffold и Python/PowerShell syntax passed.
[Native Windows source gate](plans/worker-self-update/windows-source-transaction-gate-2026-10-17.md)
clean `fb3752c` passed: 65 tests / 9 groups, actual ZIP, staging guards и полный
build четырёх exe. Run `20261017-145136-0c69deed`, task
`8cf6da5f-007b-4775-8646-1d86e9e75747` succeeded/0; 14 artifacts проверены,
report SHA `386a16889ce65af867cb304106c4219e1fe4053bc248c151a0b891644282be24`.
Installed `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` не менялись; final healthy/idle.
Windows free before/after 34,595,745,792 / 33,953,144,832 bytes; Debian final
22,479,953,920 bytes. Original compiler cache owner `20261017-061527-f4639971`
переиспользован под leases; test state/proof новые, Windows runs не удалялись.
Desktop можно закрыть, worker оставить. Source block завершён; следующий блок —
guarded infrastructure supervisor вне managed task Jobs, затем local IPC/shutdown.
Supervisor/local IPC/shutdown/install/bootstrap не готовы; U1 целиком открыт.
Установленные Windows source/runtime/config и worker не обновляются. A4 отложен.

## Самообновление U1: worker coordination — 2026-10-17

[Контракт, границы и следующие зависимости](plans/worker-self-update/worker-coordination.md).
Worker schema 15: durable source/dispatch lease + immutable command receipts,
single active update, CAS/authority fencing, baseline/target verification worker.
SourcePending/Held блокируют связанные workspace; Draining запрещает все новые roots,
children принятого workflow продолжаются. Quiescing закрывает обычные mutations,
затем Windows physical scan подтверждает Quiesced. User pause сохраняется отдельно.
Startup marker/DB guard до миграции и под instance lock; authority activation,
config saves, output cleanup и recovery/new-pair не обходят lease.
Source verifier общий для updater/worker. Info metadata opaque/read-only, UI объясняет
состояния и stale/unknown; remote mutation capability/IPC ещё не объявлены.

393 local workspace tests passed. Ревью исправило app/project count, гонку periodic
reconciliation при freeze, повтор admission внутри transaction и cleanup gate;
исторические migration fixtures сохраняют свои прежние schema. Actual RPC-2 response
schema SHA осознанно изменился из-за optional Info metadata:
`ca892a30ddabf3a3b24942f8d9d04c1c7edef45fa60f28df5566b32f465a25fa`.
Legacy requests/enums прежние; старая pair evidence не подтверждает новый digest/build.
[Native gate clean b0d5730](plans/worker-self-update/windows-worker-coordination-gate-2026-10-17.md)
passed: 52 tests / 7 groups, 11 checked artifacts, actual ZIP, полный build/staging
четырёх exe и production renderer по checksummed Windows IPC observations.
Run `20261017-102946-eea3af7d`, parent `6804eb6f-dff5-4c24-91b7-72f0d40b4710`
succeeded/0; report SHA `0c358b3047b02e53d2028b2344e62866b3534467919c6a30a5287a120c7cc91a`.
Candidate build `1792247503961887500`; runtimeApplied/candidateVerified/windowsOpened
false. Только compiler caches из original owner `20261017-061527-f4639971`
переиспользованы под lease, source/state/proofs новые. Windows free before/after
36,320,370,688 / 34,816,147,456 bytes; Debian final 22,994,698,240 bytes.
Установленный worker healthy/idle: `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc`; source/runtime/config не изменялись.
Final readonly health/idle passed; build/generation прежние. UI-окон не было.
Desktop можно закрыть, worker оставить; ручное обновление сейчас не требуется.
Следом source transaction/preimages/reconciliation, standalone supervisor,
local IPC и exact shutdown. U1–U4 открыты; bootstrap пока не запрашивать.

## Самообновление U1: journal и ZIP preparation — 2026-10-17

Продолжение U0: [реализованная граница и следующие зависимости](plans/worker-self-update/durable-preparation.md).
Отдельная SQLite schema 1/application ID WBUP, installation/scope binding,
transactional acceptance до ACK, duplicate-first, immutable lifecycle receipts,
CAS/revision/installation sequence, single active update и readonly snapshot.
OS owner lock + durable epoch/UUID; crash не повторяет update и не освобождает
ManualIntervention barrier. SourceApplying failure тоже блокирует installation.
Bounded source ZIP verification и create-only local staging/readback; live src
не меняется. CLI verify-archive, journal init/status; RPC/MCP update capability
и trusted driver transition/accept commands извне не открыты.
Native harness проверяет actual ZIP и может переиспользовать также test compiler cache
под lease. Source/test state/staging/reports всегда новые.

Локально: 382 final workspace tests, затем дополнительный silent-write/readback guard
и 28 updater tests passed; Clippy, Windows updater cross-check, contracts/typecheck/
build:ui/scaffold, Python/PowerShell checks passed.
[Native gate](plans/worker-self-update/windows-durable-preparation-gate-2026-10-17.md)
clean `93978bc` passed: 42 tests, actual ZIP verification, script guards и полный
build/staging четырёх exe. Run `20261017-095021-2614d81f`, parent
`975d53b8-3862-49c3-bcfb-3639155c8afa` succeeded/0; report SHA
`62c90e148a049f565aaa84a8b1103f2fdde0d386a3c0c1f5829ae569e8790071`.
Installed `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние, final health/idle passed.
Windows free before/after 38,396,719,104 / 37,591,474,176 bytes; Debian final
23,421,390,848 bytes. Native test/workspace/desktop compiler caches переиспользованы
из проверенного run `20261017-061527-f4639971`; test state/proof всегда новые.
UI-окон не было. Desktop можно закрыть, worker оставить; update пока не требуется.
U1 целиком открыт: source lease/apply transaction,
standalone launch, maintenance/drain/quiesce/startup guard/IPC впереди. Installed
worker exe/source/runtime/config не заменялись; updater не открывает worker journal.
Штатные записи validation tasks сохраняет сам worker. A4 остаётся отложенным.

## Самообновление: U0 завершён — 2026-10-17

Пользователь одобрил анализ и поручил реализацию. [План U0–U4](plans/worker-self-update/README.md)
уточнён и повторно проверен: source ownership, standalone supervisor, два журнала,
maintenance/startup guard, same-schema apply, desktop/migration и bootstrap gates.
Реализован U0: отдельные protocol DTO/core FSM, readonly updater CLI/source plan,
create-only build staging и automatic native preparation scenario.
[Текущее использование и ограничения](guides/worker-update-preparation.md).
Self-update capability не объявляется, worker stop/source apply/runtime replace
ещё не реализованы. Windows runtime не обновлять до isolated gates и bootstrap.
Локально passed: 360 workspace tests (13 новых), Clippy с schema/TS features,
contracts/typecheck/build:ui/scaffold, PowerShell parsing/create-only stage tests.
Actual clean ZIP `1f3aa9f` прошёл verify-source: 464 файла / 8,214,661 bytes.
[Final native gate](plans/worker-self-update/windows-preparation-gate-2026-10-17.md)
на clean `7a7d15f` passed: 19 native tests, PowerShell junction/create-only guards,
полный build/staging четырёх exe, source readback и actual worker artifact identity.
Run `20261017-062536-7f2b8b26`, parent `f4009480-b995-4b27-834e-f86f00178a3b`
succeeded/0, report SHA `48448556ffb2574b4421a69255ab9a172697700d46761af01912f46ffd29a925`.
Первый cold run выявил Tauri gen false conflict; исправлен, 13 relevant tests
повторены. Следующее ревью добавило pre-build output-link guard; final Windows
repeat проверил его. Только compiler caches переиспользовались; reports/state новые.
Подключённый worker уже clean `1f3aa9f`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc`, build `1792228466051801600`;
health/idle подтверждены read-only. Это обновляет последнее наблюдение ниже,
но не закрывает installed resource gate A4.3.
Working build/generation до/после прежние, final health/idle подтверждены.
Windows free final 45,821,747,200 bytes, Debian 27,201,228,800 bytes.
Ручное обновление сейчас не требуется; worker оставить, desktop можно закрыть.
Следующий U1 — durable source transaction, standalone supervisor и maintenance.
U1–U4 остаются открытыми; self-update capability не объявлять и не применять runtime
автоматически до соответствующих gates. A4 временно отложен по решению пользователя.

## Внеплановый анализ самообновления — 2026-10-17

Пользователь временно отложил продолжение архитектурного плана и запросил анализ
автоматического обновления Windows worker/desktop. [Отчёт и последовательность U0–U4](analysis/worker-self-update/README.md):
отдельный updater до изменения src, build-only staging, durable maintenance/handoff,
проверка новой пары и ручная помощь при неоднозначности/migration failure.
Нужен один bootstrap новой update capability; текущий код её не реализует.
Выполнен только анализ. Windows runtime/configs/source не менялись; новый Windows
прогон не запускался. A4.3/полный A4 остаются открытыми; к installed gate вернуться
после этой внеплановой работы. Последняя подтверждённая runtime готовность — ниже.

## A4.3 — native candidate готов, нужен update worker, 2026-10-17

[Отчёт и ревью](plans/architecture-transition/windows-resource-scheduling-gate-2026-10-17.md),
[план](plans/architecture-transition/resource-sets.md),
[поведение](guides/resource-scheduling.md),
[обновление](validation/architecture-worker-update.md).
Code `fc5c90e`: frozen root/workflow requirements до ACK, persisted owner policy,
atomic complete claims с прежним intent/attempt/project lease, physical lifetime/
uncertainty и fair projects. Build/cache отпускаются при Ready, process resources
остаются до cleanup. Scope fingerprint включает только собственные binding/defs.
Schema 14 с consistent backup schema 13; working journal пока 13. RPC 1/2,
operation-page v1 и прежние schema hashes сохраняются; production policy не включалась.

Final clean **347 workspace tests**, 38 shared Rust/TS cases, Clippy/checks,
contracts/typecheck/build и renderer passed. Полный UI regression прошёл на `46c9445`;
после scope-only fix frontend не менялся. Final native `20261017-045801-304be26a`,
parent `dbca6dff-12e4-4f9b-a13b-b8cacd2386e9` succeeded/0: **80 tests / 20 groups,
desktop compile, 25 verified artifacts passed**. Report SHA
`3a0a20de827ca8f3578f12253f702797ebf3f13312e6b5d507886a001c5327f4`.
Readonly recheck и renderer по checksummed final Windows observations passed.
Ревью исправило unsafe legacy enable/active resource disable и чужой binding,
инвалидировавший project evidence; regressions прошли и на Windows.

Controller exact clean `fc5c90e`, build `1792227308688010918`, SHA
`6f0ec2888ccf4b009bdbd59483b8d0ad452b61509675f7cac9a99c78fd451f51` совпал с consumer
proof и /proc/PID/exe. Serve `35492`, PID 24440; прежний `29dba9a` artifact сохранён,
shutdown graceful, endpoints/configs прежние. Session
`c0cc059c-1401-491d-be16-ab74960ef689`. Actual MCP RPC2 pair с `99351ea` passed:
receipt `418729e49f17d13a6f79879df4a95c0c894ff6ebfa376f4474236be1665fc34f`.
Installed generation `377f2ea2-a0be-4a9d-b2e9-eae49d5ed764`, incarnation
`5a8eb173-4193-4332-b00f-67e3850b2c97` прежние; health/idle подтверждены.
Resource contract у installed provider отсутствует — не объявлен включённым заранее.

Достигнута согласованная граница: один обычный update worker/desktop, затем automatic
actual installed claims/metadata/UI gate. ZIP/SHA — в
`K:\VMs\shared\WinBridge\sources\architecture-update.latest.json`.
Существующие local configs/journal сохранять; production limits не менять.
Desktop можно закрыть, worker оставить до update; UI окон нет. A4.3 не закрыт,
полный A4 открыт: далее A4.4 независимые consumers, потом A5 actor pilot.

Windows free final before/after 51,842,183,168 / 51,738,898,432 bytes;
Debian before final build 27,654,430,720 bytes. Incremental выключен, caches
переиспользованы, Windows directories не удалялись. Runtime source после final
native gate не менялся. Для будущего native повтора после update нужны retained
schema-12/13 exe через --queue-reference-worker / --resource-reference-worker;
пути и SHA сохранены в native report. Local source transcript не менялся/не экспортируется.

## A4.2 завершён; следующий A4.3 resource sets, 2026-10-17

[Installed gate и ревью](plans/architecture-transition/windows-queue-control-installed-2026-10-17.md)
passed. Worker clean `99351ea`, build `1792224150478905600`, generation
`377f2ea2-a0be-4a9d-b2e9-eae49d5ed764`, journal incarnation
`5a8eb173-4193-4332-b00f-67e3850b2c97` прежняя; actual queue-control v1/schemas есть.
Mode running/revision 0/policy absent; working config/queue не менялись.
Installed exe SHA `62727ba0f7181b2cd9737903afcf326945123323a7f814cc2f21180d04710628`
без rebuild прошёл isolated limits/pause/duplicate/cancel/restart/receipt/revision
и cleanup 0. Test workers остановлены; parent `afc7a052-08a6-43fd-9d52-36b31af9cec7`
succeeded/0. Report SHA `cfe26e87a92ffb4eb267ed99eeaba1b9c24c427102d9413291dfe0e4f4469c3d`.
Readonly --finish и production renderer по actual Windows observations passed.
Actual MCP pair receipt `cbb3ac49d20187e3e197739fcfa3d50339766876648661cdad6b6f34a1eb7743`.
Controller exact clean `29dba9a` повторно запущен после restart Debian; serve `97067`,
PID 4195, session `e75e3bb3-45ba-4af4-b96a-aa4e01ea4e0f`; SHA consumer proof прежний.
Worker healthy/idle; desktop не нужен, UI окон нет. Сейчас обновление не требуется.

Повторное ревью atomic admission/CAS/receipts/authority/UI recovery важных ошибок
не выявило. Добавлены reusable installed-exe/immutable-request/readonly-finish и
renderer checks. Full A4 открыт: A4.3 full resource sets, A4.4 independent consumers.
Debian около 26 GiB free; Windows installed check before/after
62,572,552,192 / 62,571,175,936 bytes. Сборки/Windows cleanup не было.

## A4.2 — native candidate готов, нужно обновление worker, 2026-10-16

[Отчёт и ревью](plans/architecture-transition/windows-queue-control-gate-2026-10-16.md),
[план](plans/architecture-transition/queue-control.md),
[поведение](guides/queue-control.md),
[инструкция обновления](validation/architecture-worker-update.md).
Code `29dba9a`: transactional bounded root admission, durable QueueMode, CAS revision
и immutable receipts; local-only typed IPC и readonly MCP metadata. Workflow children
не занимают root quota; dispatch pause не останавливает начатого owner. UI сохраняет
исходный command/scope/revision до send, lost ACK/reopen не исполняют новую команду.
Schema 13 создаёт consistent backup schema 12; working journal пока не мигрировался.

**337 clean workspace tests**, 46 shared Rust/TS cases, Linux/Windows Clippy,
contracts/typecheck/build, actual Unix queue UI и полный прежний UI regression passed.
Ревью исправило fairness для queued tasks удалённого project; rollback, concurrent
admission/CAS, child exemption, restart и malformed receipt проверены.
Native run `20261016-194546-f0e20b76`, task `0d86baed-b6c4-4349-a3bc-e8b77d74363c`
succeeded/0: **69 tests / 16 groups, desktop compile, 20 verified artifacts passed**.
Report SHA `b0bd8dac7210b9556d62e8e7bfac957a464eeed3f184c1d79ddcbc32791ed5ce`.
Readonly recheck сохранённого run тоже прошёл; новой задачи/сборки не создавал.
Previous schema-12 exe сохранён в local Windows run для будущего
`--queue-reference-worker`; path/SHA находятся в report.queueReferenceWorker.

Controller clean `29dba9a`, build `1792194184549633653`, consumer artifact SHA
`c36d56200ec0d67f3b9d0c3c6c3f557205f76a91cc61089a5ff66beccba9e3e5` совпал с proof
и `/proc/PID/exe`; serve `7327`, PID 31211, session
`90f93baa-71b1-4b3c-9287-8c83c41a3690`. Прежний bff20cd artifact сохранён,
shutdown graceful; endpoints/configs прежние. Actual MCP RPC2 pair с `7e49cfc`
прошла; receipt `c0ef052edcdcd86cc417fa51e03eca634b872963407cf323573adc8c56055e45`.
Installed worker generation `c2320f8e-e903-4668-8699-48e83e13f413`, journal
incarnation `5a8eb173-4193-4332-b00f-67e3850b2c97` прежние, healthy/idle.
Queue-control contract у installed старого provider отсутствует — не включён заранее.

Достигнута согласованная граница: нужен один обычный update worker/desktop, затем
actual installed metadata/control/receipt gate автоматически. ZIP и SHA брать из
`K:\VMs\shared\WinBridge\sources\architecture-update.latest.json`.
Production quotas не менять. Desktop можно закрыть, worker оставить до обновления;
тестовых UI окон нет. Full A4 открыт: A4.3 resource sets, A4.4 independent consumers.
Debian before build 28,365,783,040 bytes free, Windows before/after
49,530,875,904 / 47,879,094,272 bytes; incremental выключен, caches переиспользованы,
Windows directories не удалялись. Локальный codex-session transcript не менялся,
исключён из Git/export. После native gate изменены только docs.

## A4.1 завершён; следующий A4.2 backlog/pause, 2026-10-15

[Installed gate и ревью](plans/architecture-transition/windows-work-scheduling-installed-2026-10-15.md)
passed: actual worker `7e49cfc`, build `1792075647649211100`, generation
`5cfc53f0-5cfc-435a-a50e-b8f2ee7fb4eb`, прежняя journal incarnation.
Установленный exe без rebuild прошёл 3 isolated policies / 135 admissions:
Info 7/5/5 ms, cancel 95/51/52 ms, cleanup slots 0, все test workers остановлены.
Parent task `5ac141e6-de96-4011-9ac5-e6f2c545c4ab` succeeded/0; report SHA
`8ca689487eee038e0e3d5b26483cc54be585e58633fc981dc3fb4c4dfba94ba3`.
Production renderer прошёл actual Windows metadata/scopes/default/stale checks.
Actual MCP pair receipt
`b564d23d4d573535f65a9d072b496ab33bd93b4b1556716667170dd9b063f5ba`.
Controller `bff20cd`, build `1792075089522830086`, current session
`6dce72d4-229f-4cdd-aaa9-366e794a900e`; worker healthy/idle, configs прежние.
Ревью atomic claims/physical release/fairness/workflow handoff важных ошибок не
выявило. Исправлена harness FileShare ошибка, полный повтор прошёл.
Desktop не нужен, UI окон нет; worker оставить. Новое обновление пока не требуется.
Full A4 открыт: следующая A4.2 bounded root admission + раздельная pause.

## A4.1 — native gate готов, нужен installed update, 2026-10-15

[Отчёт](plans/architecture-transition/windows-work-scheduling-gate-2026-10-15.md),
[обновление](validation/architecture-worker-update.md), [поведение](guides/work-scheduling.md).
Code `bff20cd`: opt-in global work slots + fair projects + typed metadata/UI;
326 workspace tests, focused final pool/workflow/store tests, Linux/Windows Clippy
passed. Native run `20261015-103809-d68fa193`, task
`63ba3507-02b9-48dc-b18f-7b2e9ea22d32` succeeded/0: **57 tests / 12 groups,
desktop compile, 15 verified artifacts passed**. Report SHA
`9fbfdc39cda96abf7a8149d7d9cdbf7009f228d27f908c39fb0ea6d398cb89d7`.
Найден и исправлен expensive Windows path scan при полном pool: cold startup с
45 queued tasks даёт Info 12/4/8 ms вместо 9.5 s, cancel 46/52/66 ms.
Atomic claim, physical cleanup, прежние RPC/schema и production config сохранены.

Controller clean `bff20cd`, build `1792075089522830086`, consumer proof SHA
совпал с executable; serve `26577`, PID 44875. Actual MCP RPC2 pair с installed
`a8cf019` прошла; receipt
`64877717460d085593828f07a6f07c0a0e6fc20c5b694e04437d8b22e7709d9e`.
Worker generation/scope прежние, healthy/idle. New work-scheduling metadata
на старом provider отсутствует — actual установленный новый контракт не заявлен.

Граница продолжения: одно обычное обновление worker/desktop, затем автоматический
installed pool/metadata/UI gate. Рабочий budget не включать автоматически.
Desktop можно закрыть, worker оставить до обновления; тестовых UI окон нет.
Debian free 29,065,887,744 bytes (~27.1 GiB), Windows ~42.4 GiB,
CARGO_INCREMENTAL=0; caches переиспользованы, Windows directories не удалялись.
Full A4 открыт: A4.2 backlog/pause, A4.3 full resource sets, A4.4 multi-consumer.
Ни системных изменений, ни правок рабочих configs не было.

## A4.1 — work scheduling candidate, 2026-10-15

[План и ревью](plans/architecture-transition/work-scheduling.md),
[пользовательская семантика](guides/work-scheduling.md).
Opt-in global work budget проверяется в той же BEGIN IMMEDIATE, что task/import
claim; counts из durable leases, включая uncertainty. Fair projects выбираются
по retained dispatch ordinals. Workflow child не дублирует slot root, Ready app
освобождает work capacity, сохраняя application lease. Default/config/journal
сохраняют legacy mode; production limits не менялись. Новых schema migrations нет.
Readonly catalog metadata + typed Rust/TS reader + UI summary; reasons идут в
A3 operation pages. Queue count включает queued roots до создания child.
326 workspace tests, focused pool/workflow tests, Linux/Windows Clippy,
23 new shared Rust/TS cases, typecheck/build и actual Unix UI passed.
Native Windows candidate gate ещё впереди. Full A4 открыт; backlog/pause и
binary/IPC/full resource sets остаются следующими пакетами.
Installed worker `a8cf019` здоров/idle, controller daemon `545166d`, serve `75826`.
Desktop не нужен, новых Windows UI окон не будет. Debian ~27 GiB free,
CARGO_INCREMENTAL=0. Candidate после готового native gate потребует нового
installed worker для actual work-scheduling contract/pool проверки.

## A3 завершён; A4.1 начат, 2026-10-15

[Actual installed operation gate](plans/architecture-transition/windows-operation-pages-installed-2026-10-15.md)
passed: worker `a8cf019`, build `1792055908607920500`, generation
`69b65ecc-fe9d-42f2-a4cc-210b75127a66`, прежняя journal scope.
173 history rows / четыре вида; admission fence, duplicate key, actual local pipe,
resources, UTF-8/output/exit, controller restart и retained cursor прошли.
Production UI renderer проверен на actual MCP/Windows data, без Windows UI окон.
Task `21b7e911-c197-4c7a-af3a-50eea99ad9bc`, PID 14640, succeeded/0;
report SHA `d21dbc9d6bb8643a7b1f19f6d2569424def45cabbf214b0bfb73785e8ba3485e`.
Actual RPC2 MCP pair также passed; исходный receipt сохранён после reconnect.

Controller binary `545166d`, serve session `75826`, WS session
`01823f6a-0e4c-480e-a45b-a38ec3f1da03`. Worker здоров/idle, desktop закрыт.
Новое обновление пока не требуется; Windows/config/system settings не менялись.
Следующий вертикальный пакет — [A4.1 work slots/fair selection](plans/architecture-transition/work-scheduling.md).
Production limits по умолчанию прежние; новые значения испытываются только на
изолированном test config. CARGO_INCREMENTAL=0; Debian free ~28 GiB.

## A3.3 — проверенный кандидат; требуется installed update, 2026-10-15

[Windows gate/report](plans/architecture-transition/windows-operation-pages-gate-2026-10-15.md):
code `02970db`, run `20261015-050417-8b310ea9`, task
`ce8a33b6-0380-487b-9955-ecdc158dfa26` succeeded/0; **48 native tests, 8 groups,
desktop compile и 10 verified artifacts passed**. Первый run выявил пропущенный
schemars в desktop lockfile; исправление и полный автоматический повтор прошли.
316 workspace + negotiation regression, Linux/Windows Clippy, typecheck/build,
actual Unix IPC/WS/CLI/MCP/UI прошли. RPC 1/2/schema 12 прежние.

[Operation pages](plans/architecture-transition/operation-pages.md): четыре вида,
one SQLite snapshot, scoped refs/revisions/reasons/resources, admission-fenced
pagination; exact contract/schema negotiation и readonly extension handlers.
MCP `winbridge_operations`, CLI и UI подключены. Actual installed worker `35b716a`
здоров/idle, но новый контракт ещё не объявляет; запрос отвергнут до dispatch.
Нужен [один update](validation/architecture-worker-update.md). Full A3 открыт;
после actual new/new read-model gate — A4 queue/resources/fairness.

Controller clean `545166d`, build `1792054890343016109`
прошёл exact consumer proof и запущен, serve session `16347`,
generation `ff109089-6ab5-4abc-a470-c9988b62576a`. Actual MCP pair
`paired-20261015-050741-83112b53` и current Windows projection/browser passed,
receipt `9e912c90128634cfe3a035e985ffb86a3e6ddd89b49aaaed00f2b27e2a536bfb`.
Worker build/generation/incarnation прежние; session `c9ea293f-14a8-4205-a3a7-daf740120c23`.
Desktop можно держать закрытым, worker оставить работать; тестовых окон нет.
Debian free ~28 GiB, Windows ~52 GiB; incremental disabled, cache reused,
пользовательские данные/Windows validation directories не очищались.

## A3.2 завершён; A3.3 начат, 2026-10-15

[Actual installed pair/projection gate](plans/architecture-transition/windows-projection-pair-2026-10-15.md)
passed: actual worker `35b716a`, build `1792051201472964300`, generation
`c4b847e1-b167-42d9-b3f5-d1faf5f94933`, прежний journal scope. Controller
`d817fe0` запущен после перезапуска Debian; current serve session `6371`.
MCP pair receipt, actual Windows local pipe disconnect/reconnect и actual-data
browser renderer/task navigation passed. Desktop можно оставить закрытым,
worker оставить работать. Native windows не создавались.
Review fixed recent-history sampling >128; four focused tests/Clippy passed.
Новый code ещё не заменяет runtime evidence `d817fe0`; следующая проверка
закрепит новый exact consumer. [Следующий A3.3](plans/architecture-transition/operation-pages.md).
Full A3 не завершён. Debian free ~31 GiB, incremental disabled.

## A3.2c native projection — кандидат, 2026-10-14

[Контракт и ревью](plans/architecture-transition/readiness-projection.md).
Negotiated readonly frame/cache/UI implemented; 307 workspace tests,
Linux/Windows Clippy, 29 shared Rust/TS cases, typecheck/UI build и actual
WS→local IPC→browser/disconnect passed.
[Native candidate gate](plans/architecture-transition/windows-projection-gate-2026-10-14.md):
41 tests/desktop compile, seven verified artifacts passed;
full A3.2/A3 остаётся открытым. Installed worker `b2aeb9f` здоров/idle, но
projection contract 1 ещё не объявляет. Перед actual new/new UI gate потребуется
обновление постоянного runtime. Candidate gate выполнялся через actual tested
controller `69fb4b1` (build `1792019331881246150`).
Desktop можно оставить закрытым; worker здоров/idle. Новое обновление требуется
для negotiated projection 1; это граница текущего цикла пользователя.
Debian free ~31 GiB, incremental disabled; Windows cache переиспользован, ~21.8 GiB free.

Operational deployment: controller clean `d817fe0`, build `1792021098069838891`,
прошёл actual CLI/MCP/WS/IPC consumer test и artifact SHA verification. Запущен
на прежних WS/IPC endpoints, generation `28499055-2403-48e8-87f0-8a31355de60c`,
serve session `32599`. Installed worker автоматически переподключился, session
`c730b7ca-f260-4e53-b981-2a8c7568a2e9`, build/generation/scope прежние, idle.
Old receipt/verification сохранены, но changed controller tuple не получает
enabled. Старому worker не отправлялись новые projection frames.
Evidence `.winbridge/architecture-transition/a32c-deployment.json`.

## A3.2b runtime producer — проверен, 2026-10-14

[Контракт и ход](plans/architecture-transition/paired-runtime.md).
User updated worker до clean `b2aeb9f`, build ID `1792012729498364300`, generation
`3b5139d6-7d98-4abf-8bca-481512a9abbb`, scope прежний
`5a8eb173-4193-4332-b00f-67e3850b2c97`. Required policy/schema fields проходят
через installed controller `f834f64`; connected/idle, ошибок нет.
Actual installed-provider executable gate (seven effects/IPC/cleanup checks) passed,
run `20261014-180817-61117dcf`, task `8621c915-0dfe-4bfd-b69a-49ce96af958c`.

Local controller API/CLI/MCP + immutable producer plan implemented; 304 workspace,
Linux/Windows-target Clippy, TypeScript and actual CLI/MCP test passed.
Deployment consumer proof и actual paired producer/restart gate passed:
consumer `69fb4b1`, run `20261014-190934-94c2dc4f`, receipt
`c386138b57f506ae80637e9b7873d78f5829a7d2fd8af20558289d1d8a477dfd`.
Controller serve session `82480`; Windows worker прежний/idle. Review исправил
interruption reconciliation, supervisor key release и durable private proof.
[UI projection A3.2c](plans/architecture-transition/readiness-projection.md) — следующий пакет.
Full A3.2/A3 открыт. Worker оставить работать,
desktop не нужен, тестовых UI-окон не будет. CARGO_INCREMENTAL=0; Debian free ~31 GiB.

## A3.2a readiness foundation — завершён, 2026-10-14

[Контракт и ревью](plans/architecture-transition/readiness-foundation.md).
Installed worker обновлён пользователем до clean `f128c9a`, generation
`a1412884-4287-4bb2-a24c-3c2018d39e25`, incarnation
`5a8eb173-4193-4332-b00f-67e3850b2c97`. Controller из `f128c9a` работает
на ws://192.168.44.203:1143, generation `514b13b5-6c6a-4b84-92d3-ffed91de811e`.
Actual MCP fixture task `44f71380-dede-45eb-835a-c80aaf018648` succeeded/exit 0;
same key/same Task ID и stale scope refusal прошли. Worker idle/без recovery errors.

Добавлены typed pair/evidence/readiness, sealed dispatch decision, create-only
local receipt store, optional loaded project-policy hashes и actual DTO schema
digests в Info; UI объясняет наличие/отсутствие policy metadata.
300 workspace tests, Linux/Windows Clippy, 36 Rust/TS cases, UI build passed.
[Native gate](plans/architecture-transition/windows-readiness-gate-2026-10-14.md):
39 tests + desktop compile, шесть verified artifacts passed; code `b2aeb9f`,
run `20261014-095509-b06820e8`. Post-gate review canonical hashing fixed and
checked with both JSON map features. Full A3.2/A3 открыт. Producer и runtime
UI/CLI/MCP readiness API пока не подключены. Следующий A3.2b требует installed
worker с policy metadata; не подменять её чтением disk config или candidate proof.
Debian cleanup освободил 13.7 GiB obsolete test binaries; free ~31 GiB.
Desktop можно оставить закрытым; worker оставить работающим.

Operational boundary после gate: controller обновлён до clean `f834f64`,
build ID `1791986520723239267`, generation `f91ebe2f-9cf4-4937-81a7-968987db3275`,
IPC/WS прежние, serve session `51744`. Worker автоматически подключился с session
`67ebc158-35f8-492e-a6a4-739c8535592d`; build/generation/incarnation прежние,
idle/без live applications и ошибок. Новый decoder подтвердил отсутствие
`projectPolicies`/`contractSchemas` у actual installed `f128c9a`, исключив
потерю metadata старым controller как причину. Proof
`.winbridge/architecture-transition/a32-update-boundary.json`.
Это условие пользователя «пока не потребуется новый worker». Source update
`winbridge-20261014T135448Z-b2aeb9f-7bf64364.zip` уже прошёл 39 native tests;
[точная команда обновления](validation/architecture-worker-update.md).
До actual new provider не утверждать readiness/pair enable. Full A3 открыт.

## A3.1 discovery catalog — завершён, 2026-10-14

[Контракт](plans/architecture-transition/discovery.md), [native evidence](plans/architecture-transition/windows-discovery-gate-2026-10-14.md):
286 workspace tests, Linux/Windows Clippy, 24 Rust/TS cases и **203 native tests**
с desktop compile passed. Ревью исправило nullable build metadata, confirmed-error
scope и cache-chain/removed-cache handling. Run `20261014-063318-85934278`,
succeeded/exit 0, 10 verified artifacts. Cache reused; Windows free ~30.5 GiB,
Debian ~23 GiB. Installed schema-6 worker connected/idle, без active apps.

Следующий пакет [A3.2](plans/architecture-transition/paired-readiness.md) требует
обновить installed Windows worker, чтобы проверить полный new/new WS/IPC/MCP route
и exact pair identities на постоянном firewall path. [Инструкция](validation/architecture-worker-update.md).
Source/code `ec75bcb` прошёл native gate; пакет с финальными docs готовится.
Controller обновляется агентом, config/state сохраняются. Full A3 не завершён.
Пользователь разрешил рекомендованные architecture/scenario fixes и automatic
retries выбирать самостоятельно; правило сохранено в AGENTS.md.

Operational handoff: controller пересобран и запущен из `f128c9a`, build ID
`1791974645864017237`, generation `514b13b5-6c6a-4b84-92d3-ffed91de811e`,
ws://192.168.44.203:1143. Старый Windows worker автоматически переподключился,
session `ff1e57d4-b73c-40fc-9fcf-4bd95229be26`; idle, ошибок нет. После нового
controller старый worker по-прежнему не передаёт catalog/fenced context:
это actual old-provider limitation, а не потеря metadata старым hop.
Готовый update ZIP `winbridge-20261014T104405Z-f128c9a-5070ec9f.zip`, SHA-256
`f63b7fcf198b5aae82b87b61bf613f9d1266489596d029f56f519616d0a3ceff`, в
`/mnt/hgfs/shared/WinBridge/sources`. ZIP/clean manifest проверены; runtime code
идентичен native-tested `ec75bcb` (между commits только docs). Выполнение цикла
дошло до условия пользователя «нужен новый worker»; дальше manual update по
[инструкции](validation/architecture-worker-update.md), без ручного UI checklist.


[Оглавление документации](README.md) · [Планы, ревью и отчёты завершённых этапов](done/stage0-7/README.md).
Материалы прошлых сессий ниже сохраняют хронологию; инструкции запуска находятся в `validation/`, руководства функций — в `guides/`.

## A2.4.2b.1 request fence — завершён, 2026-10-13

Обновление 2026-10-14: **полный A2 завершён**. [Archive/new pair b.2/b.3](plans/architecture-transition/recovery-pair.md)
и [инструкция](guides/recovery-pair.md): whole-domain witness вне SQLite,
outer global Job каждого owner, create-only preparation/activation, постоянный
source retirement. 280 workspace tests, Linux/Windows-target Clippy, UI/browser
checks прошли. [Windows gate](plans/architecture-transition/windows-recovery-pair-gate-2026-10-14.md):
197 native tests, desktop compile и 9 verified artifacts passed; canonical drive
prefix bug и misleading retired-source activation исправлены. Shared run
`20261014-044152-738951fb`, succeeded/exit 0. Следующий этап — A3 discovery/read model.
[Первый пакет и ревью плана](plans/architecture-transition/discovery.md): optional
support catalog реализован локально: 285 workspace tests и 22 общих Rust/TS
fixtures. Real Unix worker/WS/controller IPC/MCP route и future metadata passed.
[Native gate A3.1](plans/architecture-transition/windows-discovery-gate-2026-10-14.md)
первый fresh gate прошёл (202 tests/desktop compile, 10 verified artifacts).
Ревью исправило nullable build metadata в UI и foreign/error reply scope;
286 workspace tests и 24 Rust/TS fixtures passed. Финальный native повтор
protocol guard готовится с cache `20261014-054619-d5d99c71`. Пользователь
разрешил recommended fixes/retries автоматически; worker здоров/idle.
Paired enable/read model ещё не реализованы.
Installed schema-6 worker/generation/session прежние, connected/idle.
Debian: [cleanup](guides/build-storage.md) освободил ~19.4 GiB incremental caches;
новые проверки с CARGO_INCREMENTAL=0. Runtime/config/history/reports сохранены.

[Контракт и ревью](plans/architecture-transition/fenced-requests.md),
[CLI/MCP инструкция](guides/fenced-requests.md). Contract 2 закрепляет instance ID
и journal incarnation у caller до отправки; transport/controller/worker не
перепривязывают повтор к новой области. Legacy DTO/frame bytes сохраняются.
Schema 12 хранит immutable admission: требование v2 разрешено только до owner
activation в пустом журнале. Desktop хранит request вместе со scope; uncertainty
не освобождает pending action для неявного нового key. MCP добавляет отдельный
`winbridge_invoke_fenced`; local maintenance не становится remote.
Ревью исправило stop/join embedded worker и различение unconfirmed/error в RPC logs.
Local: **271 workspace tests**, Linux и Windows-target Clippy, UI
typecheck/build/contracts, bound-request test и real Unix browser gate прошли.
[Windows gate](plans/architecture-transition/windows-fenced-request-gate-2026-10-13.md):
186 tests и native desktop compile passed, 9 artifacts verified, task
`03258208-f61a-4a55-8855-9dc20dd28ead` succeeded/exit 0/PID 3120.
Без UI-окон; установленный schema-6 worker/generation/session остаются прежними.
Свободно ~30.2 GiB на K:; старые runs не удалялись.
Это только b.1: archive preparation, evidence всей execution domain и activation
не реализованы. История не переносится как active; marker не снимается.
Следующий пакет — b.2, затем b.3 и общий gate; full A2 остаётся открытым.


## A2.4.2a journal inspection — завершён, 2026-10-13

База `d1f3090`; [контракт, ревью и ограничения](plans/architecture-transition/journal-inspection.md),
[инструкция](guides/journal-recovery.md). Исправлены fatal scheduler error и stale Ready
application до первого tick. `serve-recovery` открывает существующий schema-11 journal
READ_ONLY, без epoch/migration/recovery/probe/retention/dispatch; отдельный persistent
marker запрещает normal restart до Store::open. History/output/attention и inventory
legacy overlap доступны; новые RPC/мутации запрещены по умолчанию. UI объясняет
historical statuses и отключает actions. Schema не меняется, optional DTO сохраняет
legacy JSON. Local: 260 workspace tests, 10 новых recovery tests; Windows-target tests,
Linux/Windows Clippy, UI typecheck/build/contracts passed. Clean real Unix worker
browser gate passed. [Native receipts и история диагностики](plans/architecture-transition/windows-inspection-diagnostic-2026-10-13.md):
узкий run `20261013-164022-975ce2c4` — 26 tests; полный
`20261013-164733-080393fb` — **176 Windows tests**, 5 groups exit 0, 7 artifacts
verified, task `c1977ce7-939d-44fd-acb6-f276d00644d9` succeeded/exit 0/PID 13452.
Actual old-worker schema-11 refusal сохранил test journal. Windows code clean
`b90a708`, driver/cache reuse `e60f396`; отдельный resume guard и process traces.
Два прежних runtime deadlines не повторились; их исходная причина не доказана,
timeouts не увеличены. Reused cache сохранил новые test states/report и прежние
run directories; осталось ~25.9 GiB на K:.
Installed schema-6 worker остаётся прежним, desktop закрыт. Полный A2.4.2/A2 открыт:
A2.4.2b — controlled new incarnation/admission после restore; rollback detection
и безопасный выход из latched inspection ещё не реализованы. Пользователь выбрал
[restore/admission policy](plans/architecture-transition/restore-admission-options.md).
Продолжение ревью: [request boundary](plans/architecture-transition/restore-boundary-review.md).
Один новый instance ID не защищает старый payload после config reload; для новой
пары request fence обязателен **до** admission. Windows pipe case aliases тоже
не являются новой областью. Выбор 2026-10-13: новая пара и новый журнал, прежняя
история как архив, явное переподключение агентов и новый request contract.
Реализация A2.4.2b начинается с request fence; далее archive preparation и
проверяемый cutover. Independent admission ledger не нужен; установленный
worker пока не менять. Full A2 остаётся открытым.

## A2.4.1 physical process reconciliation — завершён, 2026-10-13

База `899a7ab`; [контракт, ревью и ограничения](plans/architecture-transition/physical-reconciliation.md).
Исправлен resume до PID commit. Schema 11 сохраняет immutable named Job Object
binding вместе с PID до resume; startup/повторная проверка освобождают прежнюю
lease только после подтверждённого отсутствия дерева на том же host/session.
Unknown command остаётся indeterminate; старый key не создаёт новый процесс.
Optional bounded recovery inventory виден в Info/UI; legacy без binding ждёт.
Local gate passed: 250 workspace tests, финальные 30 import/lease/reconciliation
и 40 FSM/ownership/lease/guard tests; real Unix worker browser gate (recovery inventory,
verified import/timestamp, workflow, settings, history, retention) passed. Windows-target
workspace/tests, Linux/Windows Clippy, UI typecheck/build/contracts, AST/export/scaffold
passed. Implementation `5004f3d`, fixture correction `4da9af1`;
[native gate](plans/architecture-transition/windows-reconciliation-gate-2026-10-13.md)
passed: 164 tests (7+14+142+1), четыре cargo groups exit 0; task
`d2d3d2ca-3d98-4a84-a504-cb476e57942f` succeeded/exit 0/PID 19616,
run `20261013-152019-2ff8a535`, шесть artifacts verified. Actual schema-6 reference
отказал schema 11 без изменения test journal. Installed worker remained connected/idle,
build/generation/session прежние. Desktop можно оставить закрытым, worker оставить работать.
Installed schema-6 worker не обновлять. Полный A2/A2.4 открыт:
следующий пакет A2.4.2 — explicit restore/recovery mode и unresolved legacy owners.

## A2.3.2 durable project/resource leases — завершён, 2026-10-13

База `dd04276`; [контракт, ревью и проверки](plans/architecture-transition/project-resource-leases.md).
Schema 10: project mutation/application lifetime scopes; claim + attempt + lease
атомарны. Terminal outcome освобождает ресурс только с physical cleanup receipt;
без receipt проект ждёт, другие проекты продолжают работу. Readiness передаёт
mutation scope приложению без ложного cleanup, поздний callback не освобождает
новую lease. Root/child reservations проверяются в той же SQLite transaction.
15 новых lease tests и fault test handoff; PID fault/crash profile regressions
расширены. Legacy unknown ownership сохраняется без вымышленных attempts/proofs.
Local gate: 245 workspace tests и финальные 44 leases/task_fsm/workflows tests passed;
Windows-target workspace/tests, Clippy/fmt, AST/Python guards/scaffold passed.
Implementation `9d1fa9d`; [native gate](plans/architecture-transition/windows-resource-leases-gate-2026-10-13.md)
passed: 157 tests (7+14+135+1), четыре cargo groups exit 0; task
789e716b-dc64-419b-8d1d-8656b032d855 succeeded/exit 0/PID 16504,
run 20261013-130210-1b9361d0; шесть artifacts verified. Actual schema-6 reference
отказал schema 10; отдельный test journal byte-identical. Installed worker b5d39f9
остался connected/idle без maintenance/recovery ошибок, build/generation/session
прежние. Executables/configs/schema рабочего journal не обновлялись;
добавлена одна задача gate. Desktop можно закрыть, worker оставить.
Полный A2 открыт; следующий пакет A2.4 — physical reconciliation/restore recovery.

## A2.3.1 authority/attempt tokens — завершён, 2026-10-13

База `6b1959e`; [контракт и оставшаяся работа](plans/architecture-transition/ownership-context.md).
Schema 9 добавляет journal incarnation, owner epoch/UUID и retained execution attempts.
Worker активирует owner после exclusive OS lock; common write transaction защищает
все domain writers. Task/import claim + attempt атомарны; PID/output/progress/result
и fallback проверяют token. Terminal task/import и attempt completion атомарны;
late callback не стирает cancel/output/result. Snapshot authority читается с record
одним SELECT. Legacy bytes/keys неизменны; migration consistent backup/rollback.
16 новых regressions, включая competing claims и real tree cleanup после fault.
Local gate: 229 workspace tests passed, reference test ignored; Windows-target
workspace/worker tests, Clippy/fmt/AST/Python syntax/argument guards/scaffold passed.
Native `--task-cas-only` расширен новым target ownership и реальным отказом schema 9
reference binary на отдельной DB. Рабочий installed worker b5d39f9 connected/idle,
generation 02939593-8adf-4878-a346-473092697a4a, session 631c2e63-edc8-44f8-8cce-89abec7caf8b.
На Windows требуется только этот worker, desktop/test UI windows не нужны.
Implementation `ad9b1f9`; [native gate](plans/architecture-transition/windows-ownership-gate-2026-10-13.md)
passed: 141 tests, четыре cargo groups exit 0; task 087435ea-188d-4636-ae56-04b362acab6b
succeeded/exit 0/PID 9524, run 20261013-115405-a1c5c6ef; шесть verified artifacts.
Все 16 ownership tests прошли, включая реальное завершение managed tree при fault.
Actual schema-6 reference отказал schema 9, test journal byte-identical. Post-gate
working worker connected/idle, activeTaskCount=0, maintenanceError/recoveryRequired
нет, build/generation/session прежние. Runtime/config/DB не обновлялись; рабочий
journal получил одну task gate. Desktop можно закрыть, worker оставить работать.
Durable project/resource leases (A2.3.2), physical reconciliation/restore mode
(A2.4) ещё не реализованы; A2 остаётся открытым.

## A2.2 domain revisions/atomic links — завершён, 2026-10-13

База `5f3c007`; [контракт и ревью](plans/architecture-transition/domain-transitions.md).
Schema 8: revisions imports/workflows/applications/attention, все writers CAS;
immutable identity/terminal records. Atomic child import/task+step, start registry,
readiness+step, application stop+task intent; outcome attention вместе с result.
Explicit accept/dedup и ACK проверяют rows, ACK монотонен. Legacy orphan registry
relinked без сброса outcome/replay. Migration из schema 7 делает consistent backup,
raw records/payloads/keys/cursors/attention sequence сохраняются. Wire неизменен.
Local gate: 213 workspace tests passed, reference test ignored; Windows-target
workspace/worker tests, Clippy/fmt/AST/Python syntax/argument guard/scaffold passed.
16 new SQLite regressions; `--task-cas-only` расширен до domain CAS/resume/profiles
и actual schema-8 отказа reference worker с отдельной DB. Implementation `df978dc`; [native gate](plans/architecture-transition/windows-domain-cas-gate-2026-10-13.md)
passed: 125 tests, четыре cargo groups exit 0, task 9c994702-e113-4932-b21b-21fc2f3fc762
succeeded/exit 0/PID 5352, run 20261013-105937-b33d858a, шесть verified artifacts.
Actual schema-6 binary отказал schema 8, test journal byte-identical.
Установленный worker b5d39f9 connected/idle на старой schema 6, прежние generation/
session; desktop закрыт/не нужен. Working runtime/config/DB не обновлялись. Post-gate info: connected/idle,
activeTaskCount=0, maintenanceError/journalRecoveryRequired отсутствуют; прежние
generation 02939593-8adf-4878-a346-473092697a4a, session 631c2e63-edc8-44f8-8cce-89abec7caf8b
и build identity. Рабочий journal получил одну обычную gate task. Desktop можно
закрыть, worker оставить запущенным. Полный A2 открыт; A2.3 owner epoch/incarnation, A2.4 physical reconciliation впереди.

## A2.1 task revisions/CAS — завершён, 2026-10-13

База A1 `60c841f`. [Writer inventory и оставшийся A2](plans/architecture-transition/durable-transitions.md).
Schema 7: task revision, SQLite update guard, все task writers через общий CAS;
TaskSnapshot вместо blind Store::save. FSM/current completion сохраняют cancel/
output; stale observed transition отвергается. Output/attention/cleanup faults
откатываются атомарно; revision входит в cleanup preview hash. Additive migration
с consistent backup, legacy JSON/keys неизменны. Общее ownership/epochs/recovery
и остальные домены пока не реализованы; A2 не закрыт.
15 SQLite/CAS regression checks + один explicit ignored reference test добавлены;
native `--task-cas-only` обязан запустить старый exe в отдельном новом state и
подтвердить отказ schema 7. 197 workspace tests passed, reference check ignored;
Windows-target workspace/tests, Clippy/fmt/AST/Python syntax/arg guards/scaffold passed.
Synthetic snapshot restart fixture теперь bounded waits actual instance lock release,
production lock неизменна. Implementation `8f631dd`; [native gate](plans/architecture-transition/windows-task-cas-gate-2026-10-13.md)
passed: 107 tests, task eecd118f-c11c-411d-9ac3-f7ebd8850526 succeeded/exit 0,
shared run 20261013-095922-543e9452, шесть verified artifacts. Reference worker
b5d39f9 с отдельным новым state действительно отказал schema 7, journal byte-identical.
Post-gate installed info: connected/idle, прежние generation/session/build из A1,
maintenanceError/journalRecoveryRequired отсутствуют. Установленные exe/config/DB
не обновлялись; рабочий journal получил одну task gate. Desktop можно закрыть,
worker оставить. A2.1 закрыт, A2 нет; следующий A2.2 — versioned import/workflow/
application/explicit attention writers, затем ownership/epoch/incarnation/reconciliation.

## A1 Task FSM — завершён, 2026-10-13

Продолжение после A0 `f8c22d5`: [pure Task FSM](plans/architecture-transition/task-fsm.md)
в core, worker Store::transition для launch/PID/cancel/finish/recovery.
DB/wire/fingerprints/IDs/dependencies прежние; Starting → v1 running без PID.
Найден/исправлен успех UPDATE без изменённых rows: dispatch теперь только после
сохранённого decision; identity сверяется. Core 7 и worker 4 tests добавлены.
Automatic `--task-fsm-only` / `-TaskFsmOnly` включает baseline и lifecycle/workflow
regressions без desktop/UI windows. 182 workspace tests, Windows-target workspace/
protocol/core/worker tests check, Clippy/fmt/scaffold/AST/syntax пройдены.
Implementation commit `34f21f5`; [native gate](plans/architecture-transition/windows-task-fsm-gate-2026-10-13.md)
passed: 66 Windows tests, task e0f271c9-6a21-445f-9eba-a4e85382a762 succeeded/exit 0,
shared run 20261013-092340-93391277, четыре verified artifacts. A1 закрыт.
Controller поднят с .winbridge/controller-snapshot.local.json,
192.168.44.203:1143, exec 25088, generation 6ec4284a-6d9f-47b8-866a-f9cda6f7c702;
Worker подключён и idle: generation 02939593-8adf-4878-a346-473092697a4a, session
631c2e63-edc8-44f8-8cce-89abec7caf8b, installed build b5d39f9, dirty false.
Post-gate info подтверждает прежние generation/session/build и activeTaskCount=0,
maintenanceError/journalRecoveryRequired отсутствуют. Установленные Windows exe/
configs не заменялись; рабочий journal получил одну задачу gate, test state отдельный.
Desktop можно закрыть, worker оставить. Следующий пакет A2: инвентаризация всех
writers, additive revisions/ownership/recovery guards до миграции actor owners.
Нельзя считать A1 phase guards уже реализованной защитой CAS/epoch для всех доменов.

## Начало архитектурного перехода, 2026-10-13

Пользователь поручил объединить три аналитических отчёта, сделать повторное ревью,
commit плана и приступить к реализации. [Единый план A0–A9](plans/architecture-transition/README.md)
и [ревью](plans/architecture-transition/review.md) разрешают расхождение порядка:
durable FSM/ownership сначала, queue нескольких проектов перед полной actor миграцией;
outbox/inbox только при durable receiver. Текущая база кода `a42cd7e`; этапы 0–7
сохраняют завершённый статус. План и supporting reports зафиксированы `d524e68`
**до** начала реализации. [A0 завершён](plans/architecture-transition/baseline.md):
ADR 0005, frozen v1/session/task/schema-1 SQL fixtures, 6 protocol + 8 Store tests.
Независимое acceptance чтение, lost ACK/reopen/frozen fingerprint, conflict,
project scope, intent recovery/cancel, terminal guards и output sequence/cursors
passed. 171 workspace tests, Windows protocol/worker `legacy_baseline` cross-check,
fmt/scaffold (114 docs)/diff checks passed. Нет process/network/UI запуска новых
fixtures: fake paths/PID не используются для OS effects. Все execution regressions
в общем прогоне — Unix adapter. Runtime/DB schema/dependencies не менялись.
Следующий пакет A1: чистая typed Task FSM с прежней v1 проекцией; Windows lifecycle
интеграция требует automatic native gate. В scenario с явными `--test` добавить
оба baseline targets, они пока автоматически входят только в workspace test.
Windows worker/controller в этой сессии не проверялись; прошлые generation/session
ниже не считать подтверждением нынешней связи. A0 не требует Windows обновления.

## Исправления закрытия desktop, 2026-10-10

По сообщению пользователя остальные controls работали, close/hide/quit перестали
отвечать после добавления и удаления проекта; отдельно не работала внутренняя
кнопка «Скрыть» в popup. Воспроизведена ложная dirty configuration из-за JSON
форматирования: friendly undo теперь восстанавливает saved bytes. Отказ закрытия
перенесён к tray, save/native ACK ограничены 10 секундами, поздний save не скрывает
окно. State I/O вынесен из native GUI loop; popup hide/open стали async commands.
Добавлен [локальный JSONL desktop log с ротацией](guides/desktop-diagnostics.md),
неблокирующей очередью и независимой проверкой native dispatch. Во втором native
прогоне `20261010-211212-f58ced85` popup bug воспроизведён: hide ACK успешен, но
окно видно. Win32 SW_SHOWNOACTIVATE не обновляет Tao visibility cache, поэтому
Tauri hide — no-op. Popup show/hide теперь используют парные Win32 операции;
hide проверяет actual visibility, popup крестик и навигация используют тот же helper.
157 workspace Rust tests, UI integration (включая зависший save/native ACK),
TypeScript/build/scaffold/Clippy, desktop cargo check/Clippy и Windows platform
all-targets cross-check проходят. [Native Windows gate](done/stage0-7/stage5/windows-desktop-diagnostics-gate-2026-10-10.md)
passed: 42 Rust tests, два native UI запуска, 17 verified artifacts. Run
`20261010-212047-c983fced`, task `9755a19a-8a79-490b-869d-d9ba739c7641`
succeeded/exit 0; report SHA-256
`defa5e348fb31f071289e7ae985035720d774751e3af2e71e1b3f47a1f6cd4d9`.
Native-tested source clean 3d759b3; два неудачных прогона объяснены в receipt.
ACK barrier включается только отдельным validation scenario после реальной записи;
normal launch его не использует. Working worker clean fa7e4d7, healthy/idle прежней
generation/session; runtime/configs не заменялись. Test windows закрыты, рабочий
desktop можно открыть/закрыть, working worker оставить. Новый ручной gate не требуется;
для применения нужен update runtime.

## Ревью source identity после roadmap, 2026-10-10

[Целевое ревью](done/stage0-7/stage7/review-source-identity-2026-10-10.md): воспроизведён и исправлен
false-clean export при скрытых index changes/изменении байтов во время capture.
Exporter сравнивает actual bytes/path set с закреплённым commit, Git build identity
консервативна при assume-unchanged/skip-worktree. Parent checkout/source path
symlinks/duplicate export manifest отклоняются. 8 Python regressions, 153 workspace
Rust tests, Clippy/Windows protocol all-targets/fmt/parser/scaffold проходят.
Узкий automatic Windows `--identity-only` passed: 5 protocol tests, 2 verified
artifacts. Run `20261010-185249-d913cb3f`, task
`ca1d5c9c-6ea0-4a80-a41e-c96aeee23f70` succeeded/exit 0; report SHA-256
`7ada477b60c10f70ddee31997f2d71700c4ef16e2e1998eeefdc22cb4a05d4e6`.
Native-tested clean ef4dd26 archive:
`K:\VMs\shared\WinBridge\sources\winbridge-20261010T225241Z-ef4dd26-9cc9a527.zip`,
265 files/3 654 320 bytes, SHA-256
`591b18c61016cd850ac635c1605c3699fcbb576cbbb85f6412381ba13bd0551c`.
ZIP/source files/report/log проверены readback; последующий commit меняет только docs.
Этапы 0–7 завершены; новое продуктовое расширение пока не выбрано пользователем.
Working worker clean fa7e4d7, прежней generation/healthy/idle; configs/runtime не
заменялись. Gate завершён без test windows: desktop можно открыть/закрыть,
working worker оставить работать. Controller жив, прежний session/peer; новый
manual gate не требуется. Для обычного обновления использовать archive выше
и prepare-windows-snapshot.ps1; новые runtime configs не нужны.

## Этап 7 завершён — два прикладных проекта, 2026-10-10

Пользователь обновил Windows из winbridge-20261010T100754Z-fa7e4d7-827eab23.zip
и оставил worker, desktop закрыт. После reboot controller запущен заново
с .winbridge/controller-snapshot.local.json (exec 16587), generation
f5e6273b-1e7f-490a-bba0-95b648c278a2, session 3cf5eb0b-99a6-440e-a143-27ca028d8340.
Worker clean fa7e4d7, build ID 1791669115612660300, manifest SHA-256
26b0ebb7371588c40579dadd163172ecc128fb5628c7c5b088fa57919a52a11b,
generation 372266b6-3dee-4d87-9de2-ccab896dbcf8; healthy/idle.

[Ревью и план](done/stage0-7/stage7/review-projects-2026-10-10.md): исправлен чужой parent Git commit
в build identity, неизвестные исходники не объявляются clean. Targeted regression
прошла. [Прикладной сценарий](guides/project-integration.md) completed: isolated Netwatch + WinBridge worker через
общий build profile, без новой transport/queue. [Native gate](done/stage0-7/stage7/windows-projects-gate-2026-10-10.md)
passed: 59 Rust tests, два real application cycles, nested tree stop/private IPC/
indeterminate recovery/coordinator restart, 15 verified artifacts. Run
20261010-182417-737310b6, outer task 3f8a8caa-14cc-464d-b8c3-00c7ab2ade24,
report SHA-256 cd11f064976133f16a8143c53e96c2a3ac372fad548c6e7cf8449970f35f2b16.
Две неудачные подготовки/проверки сохранены и объяснены в gate, мутации с неизвестным
исходом не повторялись. Working worker healthy/idle прежней generation; тестовые
процессы остановлены, desktop можно открывать/закрывать, рабочий worker оставить.

153 workspace Rust tests, Clippy, Windows all-targets/final protocol check, fmt,
DTO/scaffold/TypeScript/build проходят. Native-tested Rust/scripts clean 6e38c55;
после gate добавлены docs и небольшая UI подпись unknown provenance (typecheck/build).
Static interfaces закреплены следующим consumer; отдельный SDK пока не нужен.
**Этапы 0–7 завершены.** Новую функцию выбирать по следующей пользовательской задаче,
а не считать deferred dirty snapshots/source cleanup незавершённым текущим этапом.
Пользовательские configs/user-dirty sources/runtime не меняли; backups не требовались.

Готовый archive с built UI:
`K:\VMs\shared\WinBridge\sources\winbridge-20261010T223803Z-e6e2b2e-5ee765aa.zip`.
Clean e6e2b2e, 263 files/3 647 444 bytes, SHA-256
`7f989e1dec88cd5a42e2a1a66b43fd73a718190785d9446040464cd0e93c39e0`.
ZIP и hashes всех manifest files проверены readback. Rust/runtime/scripts совпадают
с native-tested 6e38c55; добавлены docs и UI unknown provenance text. Для следующего
обычного обновления Windows использовать этот archive и prepare-windows-snapshot.ps1;
повторный ручной gate не требуется. Installed Windows worker остаётся fa7e4d7.

Controller после gate собран clean e6e2b2e, build ID 1791671885109194347,
serve exec 6415, generation 149ba1b3-0a9a-4f1c-882e-bb6e7f6041d1;
session 6011e826-d858-4f9a-97ee-2d7b694586b6, peer 192.168.44.1:14256.
Reconnect прошёл, working worker прежней generation/healthy/idle, activity log
droppedRecords=0/lastError=null. Config/listen/IPC/log rotation прежние.
Последний handoff commit записывает receipt, runtime code не меняет. После reboot
эти IDs считать историей и заново проверять live status.

## Этап 7 — общие профили рецептов, 2026-10-10

[Ревью](done/stage0-7/stage7/review-stage7-2026-10-10.md) выполнено; важная ошибка queued workflow
исправлена: recipe definitions/provenance закрепляются до ACK, изменение recipes
после restart не может незаметно изменить принятый build/start. Legacy queued
recipe workflow без definitions требует нового явного запроса. История/keys
сохраняются, source/cache cleanup не добавлена. README больше не противоречит
готовности этапа 6.

[Первый сценарий](guides/project-profiles.md) реализован: статический core CommandHandler,
modules/runner для обычных tasks и workflow steps, config recipeProfiles, один
профиль на проект, fixed argv с workspaceRoot/projectId bindings. Проекты могут
добавлять локальные recipes; collisions отклоняются. Bindings используют registered
root даже при snapshot cwd; shell/environment evaluation отсутствует. Expanded
Info ограничен 512 KiB. Task origin сохранён атомарно с acceptance. UI показывает
profile/metadata/origin, Settings позволяет выбрать профиль, JournalTimeline общий.

152 full workspace Rust tests, Clippy/Windows all-targets, DTO/TypeScript/build/
scaffold/fmt и full browser integration проходят. [Automatic Windows gate](done/stage0-7/stage7/windows-profiles-gate-2026-10-10.md)
passed: 68 native tests, five checksum-verified artifacts. Run
`20261010-060810-0996d143`, outer task `6ef13e91-8cc2-4766-ac25-b6a6097e01b2`,
report SHA-256 `6f8d1abcaa472a0e20a51b2b364d29a675b3de9dd148651f220e93bd399a6581`.
Первый сценарий этапа 7 завершён; весь этап пока не закрыт. Working Windows worker
clean af90ec0 healthy/idle прежней generation; desktop закрыт. Installed runtime
не заменялся, пользовательских runtime configs/user-dirty files не меняли;
backups не требовались.

Source archive: `K:\VMs\shared\WinBridge\sources\winbridge-20261010T100754Z-fa7e4d7-827eab23.zip`,
clean fa7e4d7, built UI, 253 files/3 619 262 bytes, SHA-256
`422d9f07948f5d4833ef54a9c71a05161c46794d3647daa4cc0ce612fb109f77`.
После native gate меняется только документация; runtime code совпадает с archive.
Для применения новых profiles/output/resume нужен общий update worker/desktop
обычным prepare-windows-snapshot.ps1; повторный ручной gate не требуется.

Controller после gate обновлён до clean fa7e4d7: exec session 18450, generation
`eb0d1da7-2c20-4f21-b8da-e61eae5d9bbc`, session
`91e23903-e429-4637-8125-111b76493bb1`, Windows `192.168.44.1:7798`. Installed
worker generation `c182793f-5391-4bf8-9e70-9c94b8b24a5c` прежняя, connected=true,
maintenanceError/journalRecoveryRequired=null, activeTaskCount=0. Listen/IPC/
activity log/config прежние; source IP filtering не добавлен. После новой сессии
проверить live status заново.

## Закрытие этапа 6 — 2026-10-10

Готовый общий release worker/desktop с built UI:
`K:\VMs\shared\WinBridge\sources\winbridge-20261010T092801Z-ff64ebb-5223a958.zip`.
SHA-256 `d06bfe08d593e829ef196d545475b4272a21d6e2f45a1f243e3e0ed92b736fb1`,
243 files, 3 594 138 bytes, clean ff64ebb. Runtime Rust/UI/scripts code совпадает
с native-tested 38f4e5d; после gate менялась только документация. Последний
handoff commit ниже в истории только записывает этот release, archive не переписывается.
Для установки остановить desktop/worker и выполнить prepare-windows-snapshot.ps1
из распакованных исходников; тесты вручную повторять не требуется.

[Automatic Windows resume gate](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md) прошёл:
39 native tests, exact Debian ready/manifest/blob pin, actual cancel/crash/restart,
explicit resumed import и offline pinned build. ZIP read из shared уменьшился
33 555 078 → 8 389 254 bytes; 24 MiB rehashed из local cache. Observed one-pair
import 17 158,97 → 13 388,86 ms; это descriptive sample, не общий speedup.
Report `workflow-validation/20261010-051707-f5ebc3d7`, task
`9ab94af0-988f-4086-961d-4451cc85e162`, report SHA-256
`1b84f21af9050d83cdd6102b5697d8f39ba17a5fb73f9f34b680fe7ffcfc2cb6`.
Native code clean `38f4e5d`, source archive
`winbridge-20261010T091640Z-38f4e5d-4690e508.zip`, SHA-256
`1646f6fc2f88a6f1fe764084b4303d4c1156eaf63a0a89a9f5ed04341ad4f923`.
Первый failed harness run сохранён и объяснён в gate; не является worker failure.

143 workspace tests, final targeted resume tests, Clippy/fmt, Windows all-targets,
DTO/TypeScript/build и full browser integration проходят. Review устранил
разрастание фаз/events, смешение block count с source files и ошибку cmd junction
в test harness. Native pipe instances теперь unique. Source Git tree был чистым;
незащищённых пользовательских конфигов не меняли, backups не требовались.

**Этапы 1–6 завершены. Следующий этап — 7**: стабилизировать handler/module
interfaces и подключение следующего проекта без копирования transport/journal.
SocketDelivery и further list virtualization не нужны по текущим измерениям;
source-cache cleanup и dirty snapshots — отдельные будущие расширения.

Installed worker остаётся clean af90ec0, generation
`c182793f-5391-4bf8-9e70-9c94b8b24a5c`, healthy/idle; resumeAvailable false.
Для новых output/resume функций нужен общий update worker/desktop из нового
архива, но повторный ручной gate не требуется. Desktop закрыт, только worker
оставлен работать. Tests закрыли свои процессы, working runtime не заменялся.
Debian controller обновлён до clean 38f4e5d (code совпадает с native gate),
exec session 57191, generation `ab58dece-3d55-4a0c-860f-bbb5f104268e`,
connected session `2277c073-9c1e-4678-a124-66f2dd6c1c05`, Windows
`192.168.44.1:6282`. Listen/IPC/config/rotating activity log прежние.
После новой сессии всё проверять заново; эти IDs не использовать как live proof.

## Продолжение 2026-10-10 — snapshot chunk/resume в работе

Связь повторно проверена: installed worker clean af90ec0, generation
`c182793f-5391-4bf8-9e70-9c94b8b24a5c`, healthy/idle; controller прежней
running generation `1a3c0c7c-998a-4d35-8ec2-4572c218eb4e` подключён.
Desktop закрыт, для backend проверки нужен только рабочий worker.

[План resume](done/stage0-7/stage6/snapshot-resume-plan.md) и [контракт](guides/snapshot-resume.md): opt-in
schema 3, manifest-pinned 1 MiB blocks, отдельный local cache и новый явный
import/key после cancel/crash. Прежние key/record/staging сохраняются; automatic
retry/build/cleanup не добавлены. Core/sync/worker/CLI/MCP/UI реализация добавлена;
Linux owned crash/restart, cancelled same-key и offline pinned build проходят.
Automatic native Windows gate ещё не выполнен; этап 6 не закрыт.

# Следующей сессии: продолжение WinBridge

## Текущая сессия — 2026-10-10

Debian перезапущена. Пользователь обновил Windows из
`winbridge-20261009T110529Z-af90ec0-59c18ec0.zip`, закрыл desktop и оставил worker.
Controller запущен: exec session 15995, generation
`1a3c0c7c-998a-4d35-8ec2-4572c218eb4e`, session
`80e475d8-26b3-4c44-866a-f2a6ccaf6ea6`, connected=true.
Read-only Windows Info подтвердил clean `af90ec036e641e630583f8753fa92e8f8fba6f04`,
manifest `a36e9f8329cff23c0760063fc66e1c07670f77ddfcb6a4e7c1b831520bf5b528`,
generation `c182793f-5391-4bf8-9e70-9c94b8b24a5c`, delta available,
activeTaskCount=0, maintenanceError/journalRecoveryRequired=null.

[Ревью](done/stage0-7/stage6/review-stage6-2026-10-10.md) не обнаружило новых важных ошибок delta;
исправлены неограниченный desktop output buffer и склейка UTF-8 через gaps.
Следующий сценарий выбран по baseline 2 MiB output: [batching/virtualization](done/stage0-7/stage6/output-batching-plan.md).
Код реализован; 135 workspace tests, Clippy и Windows all-targets compile прошли.
Browser integration с actual output/lost read ACK/virtualization прошла;
[native gate завершён](done/stage0-7/stage6/windows-output-gate-2026-10-10.md): run
`20261010-042006-6c67052d`, task `ddf23ba0-c6b3-4eb8-ae3d-14511f320a79`, 11 tests,
четыре verified artifacts, report SHA-256
`a73633f445734e78bceb9ee28799c74fec97a5db9db478b30d31472f09c9629c`.
Clean b980ac0 backend сравнён с af90ec0 в отдельных state directories: страницы
17→11, median Read RPC 1960.72→1900.94 ms, hashes совпали. Три пары/одиночные
control samples не доказывают общий pipeline speedup; raw timings сохранены.
Installed generation/source прежние, healthy/activeTaskCount=0. Тестовые runtime
завершены, новых окон не было; рабочий worker оставить запущенным, desktop закрыт.
Этап 6 не закрыт: следующий сценарий — snapshot chunk/resume.
Заключительный UI fix сбрасывает frozen view после output cleanup; дополнительная
browser regression и отказ полного отчёта с gaps прошли. Это UI изменение не
меняет native backend b980ac0; повтор backend gate не требуется.

Архив рабочего обновления с финальным UI:
`/mnt/hgfs/shared/WinBridge/sources/winbridge-20261010T083346Z-1055485-519596ba.zip`,
clean `10554850942dac0c6fb2d888bb3914ce328327ad`, 237 files, built UI,
SHA-256 `50ef318f71056fb1b5755cf7a730e6b196cd7a58e3e39e46c88b124d0fe60110`.
Installed worker остаётся af90ec0; обновление runtime автоматическим gate не
выполнялось. Для нового UI используется обычный `prepare-windows-snapshot.ps1`.
Новых ручных проверок не назначено. Позднейший docs-only commit фиксирует архив;
код и native proof остаются указанными выше.

## Предыдущая сессия — 2026-10-09

Пользователь обновил Windows из `winbridge-20261009T084236Z-58f3c14-eb872418.zip`.
Read-only Info: source `58f3c143d41bf5a2137aad45dc9d7650b4bdde94`, clean,
manifest SHA-256 `5612374373c36a48c6b2cbbae83b4aa89b527494e38ada7a7deabdd02aa88b15`,
generation `41623e38-adb1-4a34-a2c4-3a43d9c1cf2c`, maintenance available/healthy,
activeTaskCount=0. После delta gates controller обновлён до clean `af90ec0`: exec
session 57048, generation `95fd938e-feb3-4854-8299-91adf7c7638f`,
worker session `0731567c-81fe-4813-9c52-7d8443671613`, connected=true.
Прежний controller exec session 55195 завершён штатно; worker generation прежняя.
Пользователь поручил ревью и продолжение. Исправлена обработка неверных nested
JSON types в settings UI; backend maintenance regression проходит. Работа по [плану этапа 6](done/stage0-7/stage6/stage6-plan.md): baseline/Git batch/delta завершены;
следующий сценарий resume/output batching/virtualization выбирается по measurements.
Desktop не требуется; рабочий worker оставить запущенным.

Этап 6 реализует [автоматический baseline](validation/performance-baseline.md): monotonic
publisher timings без изменения ready/RPC, commit fixtures/tree I/O/Windows
import/build/output/cancel через installed worker. [Baseline](done/stage0-7/stage6/performance-baseline-2026-10-09.md)
прошёл: run `20261009T093357Z-0dd53063`, 12 samples/59 operations, report SHA-256
`a6d886b28d8c4ecf50ba0ca80b8c14ecb78722d663d11570f1fd006be56b16cd`.
Installed worker healthy/activeTaskCount=0, generation прежняя. Первая выбранная
оптимизация — Git batch capture (518 files: capture 5.6 s, shared write 0.22 s).
Это baseline до delta. Завершённая delta/local source cache описана ниже; resume/output batching/virtualization ещё впереди.

Git batch capture [завершён](done/stage0-7/stage6/performance-capture-gate-2026-10-09.md): один процесс,
bounded header/body, streaming SHA, exact separator и kill/wait при раннем отказе.
123 workspace tests/Clippy/Windows compile проходят. Девять matched pairs на
исходных commits: capture median 7004.6→515.6 ms, 13.58×, все ready/hashes совпали.
Comparison `20261009T095122Z-095becc1`, report SHA-256
`59043c75788028b202f4dc3fe722bac3201f24e7c1b35e86fe098d2832ffe7c2`.
Новый clean b4d355a publisher прошёл 21 Windows operation, run
`20261009T095310Z-d5a9d432`, report SHA-256
`1b8d7e9d0ca0a7f95430a9cb31988b8cfe16f4113e5d69da4333228317d5ea2c`.
Worker unchanged/healthy/activeTaskCount=0; desktop не нужен, TEST окон не было.
Этап 6 не закрыт: delta/local source cache теперь завершена; дальше resume/output batching/virtualization с сохранением immutable generations/
source verification/cancel/recovery. Общий pipeline speedup не заявляется на
основании одного нового Windows repeat; raw timing variation сохранён в отчётах.

Delta завершена по [согласованному плану](done/stage0-7/stage6/snapshot-delta-plan.md): schema 2,
ZIP changed files, pinned local base, новый immutable target и full tree verification.
Добавлены capability/RPC, CLI/MCP публикация/import, понятный preview/history и
автоматический `--delta-only` gate без desktop. 131 workspace tests, browser delta/lost-ACK integration, Clippy, contracts, UI build и Windows all-targets cross-check проходят. При ревью также исправлена recovered import phase/summary после crash между publication и journal result. [Windows gates завершены](done/stage0-7/stage6/windows-delta-gate-2026-10-09.md): native run `20261009-064716-ffce5995`, 40 tests, report SHA-256 `b34285db8c87586ff7655afb513ceea3c03fbc46bcd441d0b8043e25e1a660a8`; cross-host run `20261009T105909Z-c59b2060`, три fresh one-file commit, report SHA-256 `9cdab83a6c5420f99184fda0039393b684f419a536e3b381d5a0520ad837008b`.
ZIP 2 367 994→193 bytes, Debian publication median 1150.73→414.14 ms; Windows observed import 1834.00→1675.17 ms. Full target hashes/build pins/Unicode output совпали. Cancel parent/child 102.88 ms, attention 52.75 ms; worker Info healthy. Installed worker остаётся `58f3c14`, его runtime не меняется.

Архив для рабочего Windows update:
`/mnt/hgfs/shared/WinBridge/sources/winbridge-20261009T110529Z-af90ec0-59c18ec0.zip`,
SHA-256 `6147aa3c8e568e1cfaeffb60dbb86c4ce9aa0080e21941ff853c55657f16965d`,
clean `af90ec0`, 231 files, built UI. Installed worker пока `58f3c14`; delta
поддерживает новый архив. Для update пользователь штатно завершает desktop/worker,
распаковывает архив и запускает `prepare-windows-snapshot.ps1`. Новый controller
уже запущен; Windows runtime в gates не заменялся. Новых ручных проверок нет.
После gates явно сообщено: TEST runtime завершён, desktop можно оставить закрытым,
рабочий worker оставить запущенным. Позднейший handoff commit меняет только
документирование runtime/archive, код release `af90ec0` остаётся тем же.

Ниже — история реализации и прежних версий до этого обновления.

Debian перезапущена; пользователь обновил Windows из
`winbridge-20261008T112855Z-1982de8-82bb6412.zip`, выполнил
`prepare-windows-snapshot.ps1`, закрыл desktop и оставил worker.
Controller запущен с прежним `.winbridge/controller-snapshot.local.json`,
теперь clean controller source `263ec63`, exec session 55195, generation
`62ebf6ca-f139-4c6b-8480-b94fa208c56b`. Exec session 57959 завершён штатно.
Worker source `1982de8b6a7e9a34e6790d5009c30406413ebf08`, sourceDirty=false,
manifest SHA-256 `491b40038ddcdca97199a30cd21c91c7f574a5b4e554341d48e479633d8614d7`,
generation `750f3499-f4a9-42c6-aa43-13d248abb245`;
session `ef6c0389-e5c0-4968-ac58-55b28820fdd3`, connected=true.
HistoryRead/typed attention доступны; journalRecoveryRequired=null, activeTaskCount=0.
[Ревью](done/stage0-7/stage5/review-stage5-2026-10-09.md) исправило неограниченный readback существующего
diagnostic файла. Проверены сохранение/collision/Unicode и MCP contracts.
Проекты/соединение и output retention остаются работой этапа 5. Пользователь выбрал
ручной restart после config save и включаемую automatic output cleanup;
[план](done/stage0-7/stage5/settings-retention-plan.md) фиксируется до реализации. Рабочий worker оставить запущенным,
desktop для текущего backend ревью не требуется. Installed runtime/config/state
не менять в автоматических тестах. Предыдущие gates не повторять без причины.

[Настройки и output retention](guides/settings-retention.md) реализованы по согласованному
плану: actual config path, active/saved JSON, SHA/generation conflict guard,
create-only previous version и atomic replace/readback, ручной restart; identity/state
закреплены. Четыре maintenance RPC local-only, controller и worker WebSocket
отклоняют их, MCP не объявляет. Schema 6 добавляет durable cleanup keys/results
с consistent v5 backup. Preview/apply до 50 tasks защищает pending attention и
active/indeterminate workflow/app; удаления snapshots/reports/history нет.
Opt-in automatic pass раз в 60 секунд фоновый; failures видны в Info/UI.
117 workspace tests и два добавленных теста batches/controller refusal прошли.
Workspace/desktop Clippy, contracts/typecheck/build также прошли. Actual browser UI проверил config/cleanup
lost ACK, same-key reconciliation, dirty close guard и сохранение task metadata.
Windows all-targets compile и native desktop check прошли.
[Settings/retention native gate](done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md)
успешен: clean source adbda09, run `20261009-042415-34160b98`, task
`56ee9fa9-b87d-4392-84c9-efcfd70a70c6` succeeded/exit 0; report SHA-256
`1bfb9e0e9b59ff58d4a050e4a3ec29b24819b1c50ba723b2209d21ba783de5df`.
36 backend tests, два actual UI запуска, 19 verified artifacts; save оставил
PID 19068, restart только тестового worker применил quota/automatic policy.
Cleanup без ошибок. Installed worker по-прежнему 1982de8, generation прежняя,
healthy/activeTaskCount=0. Пользователю сообщено: TEST окна закрылись, desktop можно
завершить, рабочий worker оставить работать. `--settings-retention-only` — единый
изолированный автоматический сценарий; ручной gate не требуется.
Этап 5 завершён в согласованном scope output retention + сохранение metadata/dedup.
Следующая работа — этап 6, измерения существующих capture/publish/import/build и
размеров output/journal до выбора delta/resume/batching/virtualization.
Финальная browser проверка поймала старый history cursor во время refresh;
page теперь сбрасывается до ответа, pagination disabled. Повтор прошёл с новой
keyboard navigation/config/cleanup checks; новый native gate не требовался.

## Предыдущая сессия — 2026-10-08

Debian перезапущена. Пользователь распаковал clean archive
`winbridge-20261007T103234Z-f7c4878-0ac18efc.zip` и выполнил
`prepare-windows-snapshot.ps1`. Read-only worker info подтвердил Windows source
`f7c4878fa5800bec5d8f4ae2781ee9b5108f0b36`, sourceDirty=false,
manifest SHA-256 `c318705bfd7916f98421dda6c467e4edbcc4bc111834dc83ad7219d4f5fc0663`,
generation `8c5d8c94-697c-4e9f-9cdb-765930ee36f3`, attentionAvailable=true,
workflowAvailable=true, features artifacts/netwatch_health и activeTaskCount=0.
Рабочий runtime уже обновлён; записи о `523fc5b` ниже относятся к прежней сессии.

Controller обновлён до `cc18156` для MCP gate и запущен с прежним
`.winbridge/controller-snapshot.local.json`, текущий exec session 27844,
generation `6928c40a-2c66-4bde-8226-a6b59d2fb789`, `ws://192.168.44.203:1143`.
Status connected=true, session `90a2ff78-83e3-435b-975b-f0d78b0e0947`, lastError=null.
Старый exec session 6831 завершён штатно; это был controller предыдущего прогона;
read-only Info по WebSocket успешен. Старые exec session IDs после reboot не действуют.
Доступность всегда проверять заново. Пользователю сообщено: desktop можно завершить
через «Завершить интерфейс»; крестик скрывает его в tray. Worker оставить запущенным.
Восстановление соединения само по себе не требует повторения пройденных gates.
План workflow/app attention зафиксирован коммитом `aeae684` до реализации:
[workflow-attention-plan](done/stage0-7/stage5/workflow-attention-plan.md). Реализованы typed targets и
additive feed/ACK/capability, schema 5 с consistent v4 backup и сохранением sequence,
единый parent workflow event, отдельные runtime outcomes приложения, quiet stop/cancel.
Terminal record/event атомарны; journal fault блокирует stale reads и admission.
UI открывает точный workflow/application ID вне recent 10. [Ревью и исправления](done/stage0-7/stage5/review-workflow-attention-2026-10-08.md).
89 workspace Rust tests, Clippy и browser integration прошли.
[Windows gate](done/stage0-7/stage5/windows-workflow-attention-gate-2026-10-08.md) завершён:
clean source `3c59351`, run `20261008-060121-324c2ef1`, task
`7e2cc850-e9ae-4205-a12f-ea27d399efb2` succeeded/exit 0, report SHA-256
`f5512e89434690ec036636e6f7f03226d78eee1b4e3f2dadbdc3c13c10a39988`.
10 attention + 9 workflow tests, actual native popup → точный workflow → UI ACK,
desktop exit 0 и все 12 artifacts подтверждены. Cleanup без ошибок; установленный
worker прежний, healthy, activeTaskCount=0. Тестовые окна закрылись. Пользователю
сообщено, что рабочий desktop можно завершить, worker оставить работающим. Для него нужен установленный worker, desktop
необязателен; --workflow-attention-only собирает отдельный заметный тестовый desktop.
Installed runtime/config/state не заменять. Следующие сценарии — история/диагностика, MCP.

Продолжение этапа 5: [ревью](done/stage0-7/stage5/review-stage5-history-2026-10-08.md) исправило stale
application read после child result fault; commit `8f9ee71` фиксирует исправление и
[план истории/диагностики](done/stage0-7/stage5/history-diagnostics-plan.md) до реализации.
[История и task diagnostic](guides/history-diagnostics.md) реализованы: additive HistoryRead,
50-record keyset pages/filter/search, точные старые IDs, keyboard actions, read-only
storage usage и editable redacted preview/save. Дополнительно исправлен незаписанный
import result: fault/lease сохраняются до recovery. 103 workspace Rust tests, Clippy, Windows all-targets compile, standalone desktop
check/Clippy, contracts/typecheck/build и actual browser integration прошли;
[Windows history gate](done/stage0-7/stage5/windows-history-gate-2026-10-08.md) завершён: source a64caf7,
run `20261008-065054-f727f7bb`, task `04cf615b-0510-451e-bfd3-e3f8721cc87a`,
succeeded/exit 0, report SHA-256
`d7f39ff0217399261f629992d1814c7a071dd53ff1330dab74d8684820c868b9`.
13 backend tests, actual history/preview/edit/save/retry UI, 17 verified artifacts.
Cleanup без ошибок; installed worker healthy/activeTaskCount=0, generation прежняя.
Пользователю сообщено, что тестовое окно закрылось, desktop можно завершить, worker оставить. Рабочий runtime остаётся f7c4878; новый сценарий
`--history-diagnostics-only` выполняется в отдельном runtime через установленный worker.
Новые ручные Windows наблюдения не требуются. MCP начинается по [плану](done/stage0-7/stage5/mcp-plan.md),
зафиксированному до реализации коммитом `ab0eae0`; retention/settings остаются.
[MCP adapter](guides/mcp.md) реализован: 30 typed tools с schemas из DTO, bounded stdio
lifecycle и forwarding в existing controller IPC. Четыре contract tests и actual
Unix pipeline/lost ACK/dedup/output/history/diagnostic/client exit/cancel прошли.
107 workspace tests, Clippy, Windows all-targets compile и contracts/typecheck прошли.
[Windows MCP gate](done/stage0-7/stage5/windows-mcp-gate-2026-10-08.md) завершён:
run `20261008-072201-391f141c`, report SHA-256
`e401fa6a6b063090c53eb61bde1c78f47f80c87f662346a2ee51ff795e0af2c1`.
Setup/success tasks succeeded, repeat key сохранил task/PID, client exit сохранил
running/PID и explicit cancel завершился cancelled. Старый worker получил только
поддерживаемые запросы: HistoryRead отказал controller с not transmitted.
Installed worker не заменён, healthy, activeTaskCount=0. Следующая работа — GUI
projects/connection settings и полная retention policy; stage 5 пока не закрыт.
Ручные UI gates для мелких правок не назначать; новые worker/desktop обновлять вместе.

## Завершённый сценарий — 2026-10-07

**Native tray/popup/settings gate пройден автоматически на Windows.**
[Доказательства](done/stage0-7/stage5/windows-companion-gate-2026-10-07.md): clean source `f7c4878`,
run `20261007-063240-8e63081e`, task `f4d7db75-db65-4e15-8a75-7d05bd2e21d1`
succeeded/exit 0, report SHA-256
`5b2fa9985438efcab6006ce73c13a8f2bb96a6e2b2cccf988060e54b2af93c86`.
21 core + 9 attention Windows tests, offline worker/desktop builds, два actual native
UI запуска с exit 0 и все 16 checksum artifacts подтверждены. Реальные tray/hide,
защита случайного close, single instance/reopen, popup без focus, sound API,
точная task page без implicit ACK, rate aggregation/deferred ACK и restart cursor/settings
прошли. Тестовые процессы завершены, рабочий worker `523fc5b`/generation прежние.

[Первый прерванный прогон](done/stage0-7/stage5/windows-companion-attempt-2026-10-07.md) сохранён как история.
По разрешению пользователя исправлены UX тестового окна, args/PID/exit/checkpoints,
CreateNoWindow, local concurrent output drain и publication bytes/SHA из одного массива.
Перед каждым Windows прогоном объяснять необходимые worker/desktop и появление тестовых
окон; после явно сообщать, что можно закрыть. Для автоматического gate нужен рабочий
worker, рабочий desktop необязателен. Пользователю уже сообщено: desktop можно закрыть,
worker оставить работающим. Это правило закреплено также в AGENTS.md.

[Tray companion](guides/companion.md) реализован и проверен; рабочий runtime ещё не обновлён,
новые worker/desktop собрать вместе при следующем согласованном обновлении.
Локально прошли standalone desktop Clippy, 20 core tests, contracts/typecheck/UI build,
PowerShell parser/capture smoke и browser integration с настоящим Unix worker.
Workflow/app attention, history/export/MCP остаются незавершёнными, stage 5 не закрыт.

Новое ревью исправило незаписанный process result: journalRecoveryRequired,
stale GetTask/cancel error, остановка admission/queue и сохранение lease до recovery.
72 workspace tests, Clippy, Windows compile и TypeScript прошли.
[Ревью](done/stage0-7/stage5/review-attention-2026-10-07.md), [следующий сценарий](done/stage0-7/stage5/tray-plan.md).
Installed worker `523fc5b` остаётся здоровым; новые native проверки выполнять в
отдельном runtime. План tray/quiet/sound/close фиксируется перед реализацией.

Пользователь обновил worker/desktop. Read-only `info` подтвердил Windows build
`523fc5b`, sourceDirty=false, generation `bff7f8da-af71-432b-83af-739a020d9e37`,
features artifacts/netwatch_health и connected controller. Controller generation
`381e409f-8949-413c-a439-7d0622159614`, session `7fa9c4d3-eae9-42d1-8d83-966e2d79bd13`.
Предыдущие записи runtime ниже — история; доступность всегда проверять заново.

Начинается [этап 5](done/stage0-7/stage5/stage5-plan.md). Замечание пользователя: Base64 `-EncodedCommand`
не объясняет действие. Первым исправлением будет decoded PowerShell script с пояснением
и отдельными точными аргументами. Затем durable attention/acknowledgement, tray/settings,
history/export и MCP. План фиксируется до реализации по инструкции пользователя.
При вопросах или непредвиденном поведении остановиться и уточнить; новые мелкие ручные
Windows gates не назначать.

Первый сценарий этапа 5 реализован: decoded PowerShell с исходными argv отдельно,
worker attention journal (schema 4, consistent v3 migration backup), task outcome
attention и explicit request/dedup/acknowledgement, paged list, capability guards,
CLI и UI task links/explicit confirmation. [Контракт и границы](guides/attention.md).
71 workspace Rust tests, Clippy -D warnings, Windows all-targets compile,
TypeScript/UI build, standalone desktop check и browser integration прошли, включая
generation cursor reset. [Windows attention-only прогон](done/stage0-7/stage5/windows-attention-gate-2026-10-07.md)
успешен: 7 native tests, task `719111f4-cdfd-4ac4-8926-f6bc548eeed2`, report
`workflow-validation/20261007-044046-eebbd202`, SHA-256
`006784bf39af25640bed803eee08d27668ee2855427e0d6df758605e7c000f3f`.
Source `6b855f6` clean, ZIP SHA-256
`27e7510dbba8247d076fdc5bab4b2c7eda2eb3f7e3ecb197eeddcd8265bd800b`.
Final UI decoder принимает то же Base64 padding/trailing-bit поведение, что .NET;
Rust код после native proof не менялся. Основной worker здоров, generation/session прежние.
Stage 5 не закрыт: workflow/app attention, history/export/MCP ещё предстоят;
tray/popup/sound/settings прошли gate, описанный в текущем шаге выше. Installed runtime остаётся `523fc5b`; первый срез не требует нового
ручного gate. Следующий рабочий сценарий — workflow/app outcome signals, затем диагностика/MCP. Не просить
обновлять runtime по каждой небольшой правке; готовые изменения объединить.

## Сначала прочитать

Корневой AGENTS.md → README → architecture → roadmap → ux → shared-folders → ADR. Затем `git status --short` и текущую таблицу готовности. Первоначальный proposal в context сохранён для истории; его TLS и общий workspace с Netwatch отменены.

## Цель и явные решения пользователя

Пользователь хочет гибкий локальный каркас Debian VM ↔ Windows host. ИИ агент в Debian должен запускать команды в Windows папке проекта, доставлять последний commit/снимок, собирать, запускать, читать логи и запрашивать popup/звук. Все действия должны быть понятны пользователю в Windows UI.

Принято: имя WinBridge; отдельный `~/winbridge` с Git; нет encrypted connections; shared folder для доставки готовых снимков, отдельный Windows workspace для сборки; recipes + произвольные команды внутри зарегистрированного project. Не возвращаться к этим вопросам без новых требований. Разрешённые рутинные действия в проекте не требуют отдельного подтверждения каждого command.

## Фактическая готовность

| Часть | Состояние |
| --- | --- |
| Git repository и структура | Созданы; история начинается с commit подготовки. Точный hash смотреть в git log. |
| Rust protocol/core/controller/worker | Worker runtime/CLI и platform/transport crates прошли Windows gate; worker capabilities: task_submit/task_cancel, а workspace_import объявляется при delivery config; native snapshot runtime gate пройден 2026-10-06. Controller CLI submit/get/list/output/cancel/watch реально проверен с Windows worker; runtime и native UI gate этапа 2 пройдены, этап завершён. |
| Stage 7 runner/profiles | Static CommandHandler и общий resolver, reusable recipes, bounded metadata, durable task origin/workflow pins, общий UI timeline. Первый gate: [68 native tests](done/stage0-7/stage7/windows-profiles-gate-2026-10-10.md) + browser. Второй: [реальные Netwatch/WinBridge cycles](done/stage0-7/stage7/windows-projects-gate-2026-10-10.md), 59 native tests/15 artifacts, private IPC/nested stop/recovery/history; 153 workspace tests. Этап завершён, SDK не нужен текущим consumer. |
| DTO и config | Strict schemaVersion 1 loader, registry/recipes/direct/cwd/timeout policy. LocalRequest/Response, TaskRecord/Summary, output byte pages. TS генерируется из Rust; check:contracts проверяет drift. |
| UI | Пять native Windows шагов подтверждены пользователем. Новый fix формы сохраняет executable/argv/cwd/timeout/recipe по worker/project/root. Browser остаётся demo/report viewer; UI integration проверен на настоящем Unix worker. |
| Tauri desktop | Native host запуск подтверждён. workerConfigPath в winbridge-desktop.local.json, форма в winbridge-desktop-state.local.json рядом с exe. worker_rpc, четыре local draft commands и desktop_build; close ждёт save ACK каждой изменённой command/delivery/workflow scope. Shared package select и delivery form restore проверены UI integration; пользователь принял их без нового подробного ручного gate. Сохранение формы и dev:ui дополнительно подтверждены пользователем на Windows. |
| Local IPC/runtime tasks | User SID + instance named pipe, user-only protected DACL, exclusive journal и Windows Job Objects подтверждены Windows отчётом. Unix socket/private state также проверены; bounded frames/deadline/connections. |
| HGFS delivery/import | Git commit capture, deterministic Stored ZIP, actual HGFS .part/readback/ready, durable worker import/lease/pinned build и UI реализованы. 16 actual Unix/HGFS/WebSocket gate checks и actual owned import crash/recovery проходят. 19 actual Windows import/build/run/cancel/NTFS junction checks пройдены 2026-10-06; delivery UX принята пользователем, подробная ручная проверка отложена. Dirty source mode ещё не реализован; stage-6 baseline/Git batch/delta/local source cache/output batching/virtualization и [pinned chunk resume](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md) завершены с automatic native gates. |
| SQLite/process tree | Local WAL/FULL, durable acceptance, dedup, queue/cancel, launch intent/recovery, independent pipe drain/quota. Реальные Windows tree timeout/cancel/crash cleanup и сохранённый indeterminate без retry подтверждены. |
| Task output | Bounded byte/JSON/row pages с прежним cursor; incremental Unicode tails 65 536 characters/stream, gap markers и не более 28 DOM rows. [11 native Windows tests и matched IPC samples](done/stage0-7/stage6/windows-output-gate-2026-10-10.md), browser lost read ACK/filter/pause/loaded bytes проходят. [Delivery resume](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md) прошёл automatic gate. |
| WebSocket | Versioned handshake/RPC/heartbeat/reconnect/watch реализованы; 14 session groups и UI integration проходят в Debian. Шесть групп Windows runtime gate прошли, включая прежний task/PID/generation после offline. Подключение и все три новых native UI наблюдения подтверждены пользователем; этап 2 завершён. |
| Журнал controller | Входящие IP не фильтруются (решение пользователя 2026-10-05). activity.jsonl в stateDirectory, 1 MiB + пять архивов, bounded writer; droppedRecords/lastError видны в CLI status. Legacy allowedWorkerAddresses игнорируется. |
| Workflow/process/log | Реализованы: durable import/build/start/stdout readiness/log, SQLite workflows/applications (schema 3), текущая schema 6 с typed attention/cleanup; consistent upgrade backup, app registry/explicit stop, operation-owned leases, byte cursor rotate/truncate/prefix rewrite. UI steps/timeline/task links/profile restore/log. Actual Windows automatic regression; подробная native UI проверка отложена. См. [workflows](guides/workflows.md). |
| Netwatch profile | Named profiles/drafts, output filters, artifact catalog и build identity реализованы. Этап 4 завершён по общему Windows Netwatch прогону; native визуальная проверка отложена. |
| History/diagnostic/MCP | Mixed 50-record keyset pages, filters/search, exact old IDs, keyboard palette, editable redacted create-only reports и read-only storage usage. History native Windows gate: 13 tests + actual UI + 17 artifacts. MCP: 30 typed tools над controller IPC; actual Unix lost ACK и Windows submit/dedup/client exit/cancel подтверждены. [History](guides/history-diagnostics.md), [MCP](guides/mcp.md). |
| Attention | Durable task outcome/request attention, dedup/acknowledgement, schema 5 + consistent v3/v4 backups, capability guards, CLI и paged UI/typed targets. Tray/popup/sound/settings прошли автоматический native gate с двумя UI запусками. Workflow/app attention прошли автоматический Windows gate: 19 tests и native popup/target/ACK. [Journal](guides/attention.md), [companion](guides/companion.md). |
| Settings/output retention | Local-only active/saved config, checked atomic save/manual restart; schema 6 cleanup preview/apply/durable results, protected targets, opt-in automatic policy. 36 backend Windows tests, два actual UI запуска и 19 verified artifacts; [gate](done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md). Этап 5 завершён. |
| Родительский Netwatch | Исправления закоммичены отдельно: 39409e5. Его working tree после commit чистый. |

**Этап 1 завершён 2026-10-05.** [Доказательства](done/stage0-7/stage1/windows-gate-2026-10-05.md): прогон `20261005-161827-c8deef84`, все 10 checks passed, ipcAclVerified true; пять native UI шагов подтверждены пользователем, включая отдельно уточнённый stop/restart worker. JSON uiVerified остаётся false, потому что ручные шаги не оцениваются скриптом; shared report не менялся. Worker --describe возвращает scaffold false и task_submit/task_cancel, в коммите этапа 1 controller оставался scaffold. Теперь controller реализует task_submit/task_cancel через worker; readiness этапа 2 требует отдельного Windows gate.

## Среда и полезные факты

- Debian в VMware (`systemd-detect-virt` → vmware).
- `/mnt/hgfs` — fuse.vmhgfs-fuse; `/mnt/hgfs/shared` существует. Пользовательский mapping на Windows: `K:\VMs\shared`. Native Windows validation reports реально записаны и прочитаны в Debian. Windows workspace пользователя: `K:\_proj-26\winbridge\src`.
- Выбран существующий VMnet8: Debian 192.168.44.203, Windows 192.168.44.1 (подтверждено пользователем). Порт WinBridge 1143; настройки сети не изменялись.
- Имеются Rust stable и Windows MSVC target для cargo check. Windows worker теперь доступен через controller по ws://; registry — только fixture. Он позволяет настоящие Windows команды внутри зарегистрированного project. Глобальные настройки Windows/VMware/firewall не менялись агентом.
- Rust wrappers: `/home/user1/.local/bin`; Node/npm: `/home/user1/.nvm/versions/node/v24.19.0/bin`. В новом workdir они могут отсутствовать в PATH.
- Рабочие repos: `/home/user1/winbridge` и `/home/user1/netwatch`. Не изменять Netwatch для реализации WinBridge и не добавлять path dependency на него.
- Git identity не была настроена глобально. Commits использовали временные `-c user.name=funame -c user.email=user1@debf3.local`, взятые из прежней истории Netwatch; глобальный config не менялся.
- Снапшоты перед переносом и exact restore commands: context/migration-recovery.md. UTC дата снимков 2026-10-05 соответствует локальному вечеру 2026-10-04.
- Отчёты: validation.md и windows-validation.md. Реальные Unix processes/IPC/journal и UI adapter проверены; screenshot local-worker-linux.png относится к Linux adapter.
- Runtime CLI: `--config PATH validate|serve|info|submit|get|list|output|cancel|import|imports|get-import|cancel-import|preview`; controller также publish. `output TASK_ID [AFTER_SEQUENCE]` возвращает bounded page: до 1024 chunks / target 192 KiB raw / conservative 900 KiB encoded, list — последние 10 summary. Desktop читает тот же journal через IPC, не открывает SQLite.
- На этапе 1 recipes имеют фиксированный argv: дополнительные recipe.arguments отклоняются; direct mode принимает массив строк. Один мутирующий процесс на project. maxOutputBytesPerTask по умолчанию 16 MiB; gap отмечается, pipes продолжают drain.
- Rust optional feature `typescript` только для generation. `ui/src/runtime/contracts.ts` — generated file, не править вручную.

## Workflow сессия 2026-10-06

Review fixes закоммичены `700ddd5`: bounded config/desktop state и journal link/reparse guards. Основной цикл этапа 4 реализован и проверен: [контракт](guides/workflows.md), [37 native Windows tests](done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md), [автоматический сценарий](validation/windows-workflow-validation.md). Report run `20261006-132034-a57beb0d`, task `fc247059-8df0-4005-9253-377d328a0fac`, SHA-256 `914c7e54e90ddac912079d6a543c4cc0cc5c0a257f7aaf3c19eea79cba8ba46a`. 59 Unix tests, 15 session groups, actual UI integration, Windows compile/Clippy/contracts/build/standalone desktop проходят. Подробная native UI проверка отложена пользователем; не назначать новый ручной gate для мелких изменений.

Пользователь обновил рабочий Windows runtime выпуском `652635b` через `prepare-windows-snapshot.ps1`; новая generation `210bd4b6-e35f-48e1-85a6-81c973c79605`, workflowAvailable=true, соединение подтверждено. Controller generation `ca6e5e43-3739-4f2a-819c-11e8e8ce63e0`, exec session 9142; доступность проверять status. Не повторять прежний gate; для новых stage-4 features требуется совместимый выпуск. По новой инструкции пользователя при неожиданной ошибке worker остановиться и запросить update. 37 native tests выполнены ранее в isolated source/cache/state через старый worker; дополнительно установленный runtime прошёл actual WebSocket import/build/start/readiness/log/dedup/stop без ручной проверки пользователя. Shared report `workflow-validation/installed-20261006-155439-827cbbac`, SHA-256 `466334303cfa8a2e28483d79a3b6e87c85341f293ad661647d246b6fd4babb8b`; [IDs и доказательства](done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md). Тестовое приложение остановлено, worker оставлен работающим.

Committed fixture `s-e0ee0e50ee6c6de17a487199ca09689dec8f6abd`, manifest `474f1188ec53f7b0492e6a20ec8e5c17295c7c281cdc148706e904547cfb0cb1`; frozen request `.winbridge/workflow-fixture/20261006-131804-d59a5488/workflow.request.json` уже выполнен как workflow `be3004e6-8056-4e50-a5e5-58a41fde2c75`. Повтор key возвращает историю, не запускает приложение снова. `prepare-workflow.py` создаёт новый fixture/config/profile отдельно при необходимости нового цикла. Новые ручные UI checks по мелким исправлениям не назначать.

Этап 4 завершён: 41 native Windows test, native desktop check и отдельный Netwatch validation clone прошли автоматически. Production Netwatch не запускать. Profiles/output filters/artifacts/build identity реализованы; подробнее в последнем разделе. Повторять прежние gates и назначать ручную проверку пользователю не нужно.

## Проверки текущей подготовки

```bash
export PATH=/home/user1/.nvm/versions/node/v24.19.0/bin:/home/user1/.local/bin:$PATH
cargo test --workspace --locked
cargo check --workspace --target x86_64-pc-windows-msvc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo run -p winbridge-controller --locked -- --describe
cargo run -p winbridge-worker --locked -- --help
npm ci
npm run check:scaffold
npm run check:contracts
npm run typecheck
npm run build:ui
python3 scripts/validate-local.py
python3 scripts/validate-session.py
npm run test:ui
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked
```

`validate-local.py` и `test:ui` требуют предварительный `cargo build --workspace --locked`; browser test требует Chromium/Playwright browser с sandbox. Обычный Rust workspace не требует GUI SDK; standalone desktop check в Debian требует GTK/WebKit (они доступны здесь). Windows desktop cross-check остановился на отсутствии llvm-rc; глобальные toolchains не менялись. На Windows `validate-windows.ps1` использует bundled-sqlite feature и проверяет настоящие Win32 primitives. Unix crash cleanup в script явно выполняется самим harness: это не доказательство Windows kill-on-job-close. Browser demo запускается `npm run dev:ui`; можно открыть настоящий сохранённый TaskReport отдельно от fixtures.

## План прежней сессии — 2026-10-06

Этап 7 завершён; актуальная готовность описана в начале. Ниже — план прежней сессии.
Ниже сохранён исторический план этапа 4.

Текущий рабочий этап — этап 4. Этап 3 закрыт по решению пользователя 2026-10-06; новые UI исправления приняты, подробная ручная проверка отложена. Этап 2 завершён: [Windows runtime и native UI gate](done/stage0-7/stage2/windows-session-gate-2026-10-05.md) пройдены. Пользователь подтвердил задачи из Debian, потерю связи при продолжающейся задаче, новый session с прежним Task ID после reconnect и отмену. 14 групп настоящих session checks, 29 Rust tests, UI integration, Windows all-targets compile и standalone Linux desktop check проходят. [Ревью](done/stage0-7/stage2/review-stage2-2026-10-05.md) исправило cleanup прерванной сессии, oversized requests, отражение входящего JSON, Welcome capabilities и handshake budget. Адреса пары подтверждены; не задавать вопрос повторно. IP Windows не блокируется.

[Snapshot runtime](reference/snapshot-format.md) реализован. Protocol: manifest/ready/import/preview/ref DTO и optional TaskSubmission.snapshotId; generated TS. Module workspace-sync: точный Git root/commit (без replacement objects), Stored ZIP, .part files/readback/ready-last на HGFS, bounded ZIP parser, actual hashes, local immutable generation, filesystem link/reparse/hardlink guards. Worker SQLite schema 2, consistent v1 migration backup, durable import dedup/cancel/recovery и одна lease для imports/commands. TaskRecord сохраняет accepted snapshot/commit/manifest hash; под lease перед запуском сверяются этот hash и всё дерево, включая unexpected files. Caches/output должны находиться вне snapshots. Native UI: preview, mapping, phases/progress/history/cancel и сохранённый snapshot field.

44 Rust tests, Clippy/fmt, Windows all-targets compile, contracts/TypeScript/UI build, 14 существующих session groups, новые UI checks и standalone Linux desktop check проходят. Реальный HGFS gate `debian-20261005-183623-f32e6b76`: 16 checks, platform linux, passed true, пять ImportRecord и два TaskRecord. Report SHA-256 `bca344044caa6fcad6d99bab6701ed035e12d134090ce1117cb82df4d20491dc`; в shared `WinBridge/snapshot-validation`, report.ready.json содержит checksums. Actual owned crash/recovery: `.winbridge/sg/c07bd640/crash-report.json`, killed during copy_payload, тот же Import ID `d34f270a-40dd-4bd3-8140-511edc6a5926` recovered interrupted, no final source и no auto build. Прогон выполнен после bound-manifest fix.

Найдено actual HGFS ограничение: rename непустого package directory возвращает ENOENT. Исправлено на per-file .part/readback/rename и ready last. Два первых .part directory оставлены как evidence, не удалять автоматически. Shared ready package исходного fixture: `deliveries/snapshot-fixture/s-9c3799cf4a9a4c658b222158dc319b2fb0718279`, manifest hash `51305c76dd251b4edf27b217634eaef66d72d14ed8a9bb61222ae9a812461131`, payload 885 bytes. Дополнительные 64 MiB gate packages и owned staging сохранены; retention ещё не реализован.

[Windows snapshot runtime gate пройден](done/stage0-7/stage3/windows-snapshot-gate-2026-10-06.md): `debian-20261006-115911-c3629b0a`, actual platform windows, passed true, 19 checks. SHA-256 report `fa10b55472d8df70512637c4918da405d26b2fd09efb9e489800a5783636f7d6`; все пять экспортированных файлов сверены с report.ready.json. Подготовка пользователя `20261006-181804-b74b6c4b`, worker generation `69c79c25-93d2-475b-93ce-53e3af67a92e`. Task build `0f9646e9-86b2-4786-8993-b58caaddabb2`; task run `102d9bb3-434a-4df7-9bd5-3952d38e3772`, оба exit 0, запуск вывел Unicode строку. Пользователь подтвердил import/preview/build/failure/history и command form restore. Найдены UX неудобства: shared package ID был текстом, delivery project/snapshot не сохранялись. Исправлены read-only paged discovery/select, независимый delivery draft в том же локальном файле, общий close flush и понятный отказ непроверенной сборки. 48 Rust tests, UI integration, Windows compile/standalone desktop проходят. **Этап 3 закрыт по решению пользователя 2026-10-06: UI исправления приняты, подробная ручная проверка отложена.** Повторять 19 runtime checks не требуется. Подготовлены create-only `.winbridge/controller-snapshot.local.json`, committed fixture Git root и `prepare-windows-snapshot.ps1`. Windows script сохраняет исходный worker/desktop config, создаёт дополнительный `winbridge-worker.snapshot.local.json` в том же fixed runtime/instance/state, добавляет snapshot-fixture и cargo build-cache recipe. Agent gate `validate-snapshot.py` требует actual platform windows и дополнительно проверяет NTFS junction/canary. Native Linux proof не подменяет эти наблюдения. Следующее действие — review fixes и workflow этапа 4; меньше ручных Windows проверок, автоматические сценарии через уже подключённый worker. Не повторять предыдущие Windows gates.

В текущей сессии listener уже запущен на выбранном IP; актуальную доступность проверить командой `controller --config .winbridge/controller-snapshot.local.json status`. Runtime config исключён из Git.

Основной workflow этапа 4 реализован; осталось безопасно интегрировать отдельный Netwatch profile/probe и закончить editable profiles/output filters/artifacts. Повторять этап 2 и пройденный snapshot runtime gate не требуется. Подготовка пользователя `20261005-235244-97785d52`, native Debian run `debian-20261005-170829-53b92c6d`: passed/platform windows/reconnectVerified, семь TaskReport проверены без изменения shared. Task `a5f391a8-dee8-4eeb-8151-efd2e383ccec` сохранила PID 17620 и generation worker после 15 секунд offline controller; затем отменена. На момент завершения gate этапа 2 controller работал с подключённым worker; текущую связь проверять status. Повторять этот runtime gate или gate этапа 1 без новой причины не требуется.

Runtime постоянный: `K:\_proj-26\winbridge\runtime`, runtimeDirectory в локальном settings JSON; старый settings без поля поддерживается. Configs создаются только при отсутствии, затем определяют actual worker settings. Форма/project/state сохраняются; неизменившиеся exe не переписываются, обновление обнаруженного запущенного runtime отклоняется. Дата/ID остаются у отчётов и staging. Старые per-run каталоги не удаляются и автоматически не мигрируют.

Попытка Windows `20261005-224617-fe7ed534` дала TCP/handshake timeout во время запроса файервола, затем socket permission denied 10013. Пользователь сообщил о файерволе с разрешением для каждого нового пути exe. Его Test-NetConnection из pwsh подтвердил доступ к Debian: True, source 192.168.44.1; activity.jsonl содержит tcp_accepted с этого IP. Это подтверждает TCP для PowerShell, но не выполнение Windows worker. Worker теперь различает TCP, WebSocket upgrade и Hello/Welcome в ошибках, а 10013 включает actual exe path. После исправления постоянного пути подключение и gate прошли; firewall/VMware не менялись агентом.

После обновления desktop пользователь подтвердил сохранение формы и dev:ui на Windows: «все проверил включая dev:ui, все работает». [Windows инструкция](validation/windows-validation.md) остаётся для повторных прогонов. Этап 1 закрыт полностью; повторять его gate с нуля не требуется.

Не повторять подготовку или пройденный gate с нуля. Не запускать production Netwatch для проверки этого service; fixture executable достаточен. Новые capabilities включать по фактическим доказательствам.

На этапе 2 plain WebSocket подключает Debian; на этапе 3 HGFS доставляет snapshots. Операции писать только в выбранный namespace после настройки/разрешения реального сценария. Mount/service/firewall настройку не проводить самостоятельно.

## Какие правила Netwatch полезно сохранить

Изучить agent/src/ipc.rs, launcher supervisor/runtime и scripts/unpack_source_archive.py в Netwatch, вместе с tests. Уже исправлены busy named pipe, один heartbeat deadline, bounded pong write, Windows junction обход и restore только собственного backup. Не копировать старые дефекты. PID ownership и instance identity нужно тестировать до lifecycle изменений.

## Готовый запрос для продолжения

> Продолжи WinBridge в /home/user1/winbridge по AGENTS.md и docs/handoff.md. Этапы 1–6 завершены automatic Windows gates; следующий этап 7 — handler/module interfaces и reusable project profiles. Baseline/Git batch/delta/cache/output batching/virtualization/pinned chunk resume прошли реальные gates; old source generations/history сохраняются, прерванные filesystem mutations и неизвестные внешние команды автоматически не повторяются. Native resume code clean 38f4e5d; installed worker clean af90ec0 healthy/idle, для новых функций нужен общий update worker/desktop, ручной gate не требуется. Актуальную связь/source identity проверять заново; controller был обновлён и connected. Automatic Windows checks проводить в отдельных runtime, заранее обозначая нужные процессы/TEST окна, после сообщать, что можно закрыть. Не повторять gates без причины; при неожиданной ошибке рабочего worker остановиться и уточнить.

## Завершение этапа 4 — 2026-10-06

Добавлены editable named profiles и unfinished drafts в desktop state с close/save ACK,
фильтрация и pause просмотра stdout/stderr/log, исторический artifact catalog с bytes/SHA/source/build task,
фоновые artifact/health checks и build identity worker/desktop. DB schema остаётся 3;
старые stdout workflow fingerprints сохраняются. Controller/UI проверяют workflowFeatures
перед отправкой новых полей. Оригинальный Netwatch repository не меняется.

Пользователь согласовал отдельный clone Netwatch с instance ID. [Сценарий](validation/netwatch-validation.md):
исходный commit 39409e56a26ef438adab3488afea504ef40f1363, validation commit
e295806456a5715510918dc178ff1e7975e8626b, manifest
b9a2cb5d1740057566452ad5acfe5f6c19be45390182a58ceaa470a1b06700ed.
Prepared profile `.winbridge/netwatch-validation/20261006-163106-3ae4e9ff/workflow.request.json`
зарегистрирован для отдельного native test root; не отправлять его установленному worker.
Unix actual Netwatch import/build/artifact/start/IPC/log/stop прошёл:
`.winbridge/netwatch-unix/97d94ee6/report.json`, workflow c11c5856-0568-4887-b383-55376449f49c,
PID 177361 остановлен. 63 Rust tests, Clippy -D warnings, contracts/typecheck/UI build,
Windows all-targets compile и browser integration прошли; автоматический Windows Netwatch прогон также завершён (ниже).

Полный native gate: `WinBridge/workflow-validation/20261006-163840-a97da6e3`,
outer task 98c4a0eb-00f8-4f20-8747-77ecddc8af12, report SHA
f1ccabe0039fadeb3daa7820f1e347896cddb4d70691632579fa9c5ed0ff0195.
Source commit 612b3b2 (clean ZIP), SHA f85b755a24503ba64901e99361de0b2951aa2dfaa58f5471348e028c79855185.
41 Windows tests, native Tauri check, Netwatch workflow
6f647cac-c662-4761-a836-a383757809d1 и application
7cd82e14-5337-49ef-8062-67272ad51128, PID 21416: ready подтверждён IPC,
затем explicit stop; startup history succeeded, runtime и test worker остановлены.
Оригинальные Netwatch files и установленный fixed worker не менялись.

Дополнительная проверка UTF-8 отчёта: `workflow-validation/encoding-20261006-612b3b2.json`,
SHA f530848f81367eb6740faf15ee6a0e14fa7a21166271413182f1f95fac75555a,
task ea16150d-452a-4996-b385-c41628428f10 exit 0. Journal/CLI worker сохраняют
правильный UTF-8; PowerShell native capture портил кириллицу в report по OEM code page.
Общий scripts/worker-cli.ps1 теперь читает stdout/stderr как UTF-8 одновременно,
без изменения console/global settings. Проверен реальным Windows CLI и PowerShell
в Debian. Rust/UI код финального выпуска совпадает с native-tested 612b3b2;
после gate изменены только reader тестового отчёта и документация.

Установленный worker generation 210bd4b6-e35f-48e1-85a6-81c973c79605 остаётся
здоровым; stage-4 Netwatch probe/artifacts требуют одного совместного update worker/desktop.
На этой дате следующий roadmap этап был 5; актуальный следующий этап указан в начале.
