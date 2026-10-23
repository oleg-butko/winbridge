# Постепенная реализация и расширение

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
