# Updater: передача контекста после candidate support, 2026-10-23

Это точка входа для следующей сессии без истории чата. Пользователь попросил
закончить текущие проверки, подробно описать сделанное и дальнейшую реализацию
и сделать коммит. Следующий блок actor/pre-drain на этой контрольной точке
не начинался. Долгосрочная цель остаётся active; **полный updater не готов**.

Implementation commit: **7186bb652a6148b2866dc966a055f041de2d67a9**.
Он добавляет schema16 foundation поверх 3a4daea (typed compatibility policy),
f1e2f10 (native support gate), d1b2bea (readonly query). Финальный
документационный commit находится в истории Git после него.

## Быстрый старт следующей сессии

1. Прочитать AGENTS.md, README, docs/handoff.md, architecture, roadmap, ux,
   затем этот документ и [общий план U0–U4](README.md).
2. Проверить git status и последние commits. Исторические ACTIVE/NEXT/paused
   в длинных документах не являются текущими заданиями. Пользовательские
   изменения не перезаписывать; правила backup — в AGENTS.md.
3. Проверить реальные процессы controller/Cargo/Windows observers. PID этой
   сессии ниже не является разрешением завершать будущий одноимённый процесс.
4. Прочитать readonly worker Info через IPC: build, generation, connection,
   activeTaskCount, recoveryCoverage, maintenanceError, journalRecoveryRequired.
   Если offline, попросить запустить **установленный** worker, обновление
   исходников для этого пока не требуется. Desktop для headless gate не нужен.
   При неожиданном поведении installed worker остановить зависимые Windows
   действия и сообщить конкретную причину.
5. Не пересылать terminal задачи из таблицы. Для новой реализации нужны новый
   clean source export и отдельный isolated run, не replay старого namespace.
6. Первый implementation block: подключить candidate-support driver к **новому
   versioned actor/action/view**, затем production drain к его проверенному
   результату. Это ещё не сделано. Далее activation, U2 faults, U3/U4.

## Где мы относительно конечной функции

Пользователь хочет автоматическое обновление Windows worker из shared source
ZIP: подготовить source в постоянном src, собрать staging, согласованно закрыть
worker/desktop, заменить binaries в постоянном runtime, запустить и проверить
новую версию, сообщить успех/ошибку. Ручное вмешательство — при неудаче,
а не на каждом обычном update. Постоянный exe path нужен для firewall.

| Блок | Фактическая готовность | Чего пока нет |
| --- | --- | --- |
| U0 | ZIP/manifest/source preflight, build-only staging, отчёт. | Installed cutover. |
| Internal U1 | Source transaction, supervisor, compiler/artifact/health actors, drain/quiesce, exact exit. | Public production update capability. |
| U2.1 | Final consistent SQLite backup, migration/refusal, четыре actual backup faults. | Activation и восстановление нового владельца рабочего журнала. |
| U2.2 preparation | Пять independent prepared files, driver/actor, actual Main/fault gates. | Saved JSON не заменяет physical custody. |
| U2.2 replacement | Schema15 ledger, consuming original guards, same-volume one-attempt moves, actor17/view9; actual positive Main и четыре faults. | Authorized activation, local health/controller confirmation; полная fault matrix. |
| Support discovery | Actual standard/validation query, marker-negative old image, image/PID/cleanup binding. | Само по себе не даёт права устанавливать. |
| Compatibility schema16 | FSM, immutable ledger, fresh permit, trusted managed query driver, frozen executor policy context. | Actor route, обязательный support→drain binding, native Main execution нового driver. |
| U3 | Требования desktop/tray/notifications определены. | Полная согласованная замена/восстановление desktop и UX результата. |
| U4 | План bootstrap/public interface определён. | Installed automatic update, production CLI/MCP, end-to-end gate. |

**Текущий source: updater journal16 / worker journal16 / supervisor17 / view9.**
Новые таблицы не означают новый actor contract. Старые actions/views frozen.
Поле runtimeApplied в diagnostics относится к файлам, не означает успешный
запуск или controller confirmation. Public install выключен.
Architecture-transition A4 отложен до завершения updater.

## Что изменено в текущем блоке

[Техническое описание](candidate-support.md) и [policy](updater-compatibility.md).

| Файл | Ответственность |
| --- | --- |
| apps/update/src/candidate_support.rs | Intent/record/result, causal FSM, process/cleanup binding, non-Clone/non-Serde permit, physical abort barrier. |
| apps/update/src/candidate_support/driver.rs | Один journal-bound runtime-support query через ProbeObserver; source/health/lease/config/artifact/executor revalidation и terminal receipt. |
| apps/update/src/journal/candidate_support.rs | Schema16 migration, immutable current/history, transactional CAS/readback, trusted admission, readonly inspect. |
| apps/update/src/candidate_support/tests.rs | Пять model regressions: causal steps, missing PID, unknown cleanup, report/policy подмена и frozen context. |
| apps/update/src/journal/candidate_support/tests.rs | Пять SQL regressions: CAS, ignored writes, tampered history, cold no replay, synthetic15→16 backup/fencing. |
| apps/update/src/abort.rs и journal/abort.rs | Не освобождать source при pending/unknown support или неподтверждённом physical cleanup. |
| crates/protocol/src/update/updater_compatibility.rs и updater_support.rs | Strict driver snapshot DTO/новые blockers. Runtime-support query1 shape не расширен. |
| crates/core/src/update/updater_compatibility.rs | Policy по frozen driver snapshot; шестой regression, запрет validation executor. |
| apps/update/src/updater_support.rs | Общая current driver map и переиспользуемый report binding. |
| apps/update/tests/candidate_support_migration.rs | Windows-only actual retained15→16/refusal на новой SQLite Backup API копии. |
| scripts/test-update-support.ps1 | Migration mode, две flavor builds, семь test groups под одной cache lease. |
| scripts/validate-update-support-native.py | Reference/source/task/ready/proof audit, 22 migration mutations, readonly recovery того же accepted task. |
| ui/src/runtime/contracts.ts | Regenerated DTO; нового пользовательского updater UI пока нет. |

Обновлены synthetic downgrade fixtures, удаляющие новые таблицы только при
построении старых тестовых schema. Старый assert с жёстким current schema15
заменён на UPDATE_SCHEMA_VERSION. Production миграция не удаляет историю.

### Существенные результаты review

- Driver capabilities фиксируются в intent. История не переоценивается по
  изменившейся поддержке будущей версии executable.
- Cleanup с creation=true без actual PID никогда не становится подтверждённым
  завершением. Неизвестный исход удерживает source barrier.
- Existing admission читается до fresh-only lifecycle checks. Повтор возвращает
  прежнюю запись, без нового permit/процесса. Текущая authority проверяется;
  старый epoch не получает права менять данные.
- Terminal write/history/readback идут в одной транзакции. Ignored SQL write
  не превращает receipt в успех; ошибка сохранения оставляет barrier.
- Abort проверяет data quiescence и actual process/Job. Будущая actor integration
  обязана также координировать support-active по аналогии с health-active.
- Native migration helper читает counters как i64 с checked conversion:
  исправлена cross-Clippy ошибка неподдерживаемого FromSql для u64.

## Инварианты при продолжении

1. Intent до процесса; actual PID/birth/session/host и Job до resume; cleanup
   после actual exit. No-process-created proof отличается от неизвестного PID.
2. Saved report/assessment/empty blockers не являются authority. Только fresh
   admission создаёт одноразовый permit. Cold open не восстанавливает permit.
3. Root остаётся CandidateVerified при support query. Worker health и
   совместимость updater — разные факты и отдельные контракты.
4. Marker-negative updater не получает неизвестную команду, fallback отсутствует.
   Migration отдельно исполняет старые **известные** artifact/journal команды
   для доказательства refusal; это другой сценарий.
5. Validation candidate и executor запрещены production policy. Объявленные
   future contracts не включают отсутствующий local driver.
6. Source/config/all artifact/executor pins, live lease, health/root/owner
   проверяются до и после query. Cached inspector JSON не заменяет проверку.
7. Backup15/preparation16/replacement17 drivers уже есть;
   activation/controllerConfirmation/desktopUpdate отсутствуют. Не рекламировать
   их версии до настоящих drivers и gates.
8. FilesMoved history не восстанавливает original resident custody после crash.
   Не «усыновлять» старую replacement по журналу.
9. Один OS attempt на external intent; unknown outcome → Indeterminate.
   Timeout/lost ACK не разрешают автоматический replay эффекта.
10. Не ослаблять startup maintenance refusal, file pins или frozen contracts
    ради прохождения теста; сначала исправлять harness.

## Проверки Debian и Windows cross-check

| Проверка | Результат / локальный лог |
| --- | --- |
| Workspace test, locked, CARGO_INCREMENTAL=0 | **674 passed, 0 failed, 16 ignored**; candidate-support-workspace-tests.log. |
| Final model/ledger filter | **10/0**; candidate-support-final-model-tests.log. |
| Linux all-target Clippy update/worker/protocol + update-validation | Passed; candidate-support-final-linux-clippy.log. |
| MSVC all-target Clippy тех же packages/features | Passed; candidate-support-final-windows-clippy.log. |
| Generated contracts / TS typecheck | Passed; Rust DTO и committed TS совпадают. |
| cargo fmt check / git diff check | Passed. |
| UI build | Passed; candidate-support-final-ui-build.log, прежнее предупреждение chunk >500kB. |
| Python observer tests / py_compile | **7/0**, passed. |
| Scaffold/local links | Passed; финальное число документов — в закрывающем разделе ниже. |

Логи в ignored .winbridge/update-preparation/. Cross-check — компиляция,
не исполнение Windows программы. Synthetic SQL/model tests не доказывают
живой запуск нового journal-bound driver через supervisor Main.

## Завершённый Windows gate

Run **20261023-224925-0eae733c**, task
**f88c0185-6665-424c-8e07-92d0867612ad**, producer7186bb6.
Terminal **succeeded/exit0**,25 native tests/7 groups/9 commands.
13 ready files проверены дважды,36 query +22 migration mutations отвергнуты.
Readonly verify-run той же task также passed. **Не resubmit.**
[Подробный отчёт gate](windows-candidate-support-gate-2026-10-23.md).

Проверено: standard/validation actual queries; model/SQL10, policy6; actual15→16
на consistent копии настоящей replacement history. Epoch2→3, старые rows и
migration backup сохранены, три новые таблицы пусты. Old15 init/status refuse16
без DB/nonempty WAL изменений. Original reference image/manifest/DB сохранены.
Факт миграции не создаёт support permit и не восстанавливает physical custody.

Report SHA256 f4218decd2ee2bcdeaea7664001fdae8560080aa99e99f6e550bc64f6930eae3;
query proof SHA256 48b499a9ebf467e04ffc84526034ea0efd5d65f8a15d60d13ec1a56ed97d5ee5;
migration proof SHA256 92739281417b9238feea8ef789be490e60bf27237ca83c813b062a7a3feeb6d1.
Source manifest SHA256 e4740f078caf2743768e78c90246df3b5bdb308fd69195821ae03ae000ff3484.
Full verified pin: .winbridge/workflow-validation/20261023-224925-0eae733c/support-verified.json.
Это query/metadata/migration gate, **не actual Main нового support driver**.

Source ZIP: winbridge-20261023T224917Z-7186bb6-59eeb744.zip.
SHA256: **94ed85726985c32ee6678e9bc1d1c013c4f9813df2be6fd7206a7642cf695a49**.
Source manifest и exact run metadata:
.winbridge/workflow-validation/20261023-224925-0eae733c/support-run.json.

Shared: /mnt/hgfs/shared/WinBridge/workflow-validation/20261023-224925-0eae733c
↔ K:\VMs\shared\WinBridge\workflow-validation\20261023-224925-0eae733c.
Observer log: .winbridge/update-preparation/candidate-support-native.log.

Readonly recheck **той же** task, без submit:

~~~bash
python3 scripts/validate-update-support-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261023T224917Z-7186bb6-59eeb744.zip \
  --reference-pin .winbridge/update-preparation/replacement-main-native-verified.json \
  --cache-run 20261017-061527-f4639971 \
  --candidate-support-migration \
  --verify-run 20261023-224925-0eae733c
~~~

Migration flag обязателен и при verify-run. При потерянном ACK observer читает
saved request и ищет exact payload через history/get, не отправляет заново.
Observation timeout не означает terminal; повторно читать тот же Task ID.

## Предыдущие terminal gates — не пересылать

| Gate | Task / run | Доказательства |
| --- | --- | --- |
| Actual Main replacement15 | 665e7e2f-e734-49cf-ae87-d83498a29cbd / 20261023-163606-15675844 | 60 native/12 groups, 20 ready files twice, 44 mutations; FilesMoved22, VerifyingLocal; activation нет. |
| Четыре replacement faults15 | 63232320-8bf6-4826-b13c-315ead0b4433 / 20261023-173625-e4eea0a6 | 60 native/12 groups, 32 ready files twice, 68 mutations; unknown prefixes/no replay. |
| Readonly updater support15 | d97ea580-8adb-43ff-bf71-224d0d9b222c / 20261023-190500-afa1564e | 8 native/4 groups, 9 ready files twice, 36 mutations. |

Source/report/proof hashes и replacement invariants:
[предыдущий handoff](resume-2026-10-23-replacement.md),
[positive report](windows-runtime-replacement-main-gate-2026-10-23.md),
[fault report](windows-runtime-replacement-faults-gate-2026-10-23.md),
[support report](windows-updater-support-gate-2026-10-23.md).
Результаты относятся к своим immutable commits, не автоматически к schema16.

Actual reference новой миграции: schema15 из positive run, update
f0c6e634-1376-44cc-88ea-0e516143399e, installed target5fbc;
updater SHA e964676e8e80d2a9747a2d5f1b1453205c29c17966695ee0ff6c9a4b4efa2b81;
manifest SHA f14984418fe3876c061509a6aa3f4285ebd1c2ad867732be59903dee0846caf1.
Reference5fbc не production target и не same-schema16 кандидат для следующего
Main gate. Export7186bb6 — ограниченная schema16 fixture, умеет query,
но ещё не activation/install.

## Конкретный дальнейший план

### 1. Actor и наблюдаемая support operation

Работать в apps/update/src/supervisor.rs, protocol update DTO и новых support
view/types. По образцу health integration добавить Capture/ObserveCandidateSupport
и client API под **новой согласуемой версией** (следующие свободные номера сейчас
18/view10; сверить перед реализацией).

Admission до ACK, один blocking workslot. Fresh передаёт permit в spawn_blocking,
Existing возвращает observation. Lost ACK не отзывает accepted admission
и не создаёт повтор. Status/cancel остаются responsive. Abort учитывает
support-active, ждёт cleanup, сохраняет unknown barrier. Старые contracts
не получают новые semantics/fields.

Regressions: duplicate/lost ACK, busy handoff, stale epoch/root/config/image/health,
crash до PID/после grant/до terminal receipt и cold observation без replay.
Сначала library/actor checks, затем actual Main на Windows.

### 2. Обязательный production support→drain binding

Legacy DrainCandidate нельзя молча объявить новым контрактом. Добавить новый
production entry и атомарную ссылку на exact support attempt/revision/digest
и health/root/source/owner. Для ledger link может понадобиться новая таблица
/schema; тогда нужны новые backup/migration/refusal gates. Frozen v1 DrainIntent
не расширять без отдельной версии.

Перед BeginDrain заново проверить поддержку и данные. Checked с blockers,
Unsupported/Rejected/Cancelled/Indeterminate запрещают drain. Пока
activation/controller/desktop drivers отсутствуют, production кандидаты обязаны
оставаться blocked, даже при matching schema.

Native Main gate должен доказать actual journal-bound query + durable blocker
**до** BeginDrain, доступность old worker и SourceHeld. Проверить actual actor
record/history/PID/Job/cleanup. Отдельно доказать отказ validation executor
и candidate; не подменять flavor. Metadata/migration gate это не заменяет.

### 3. Activation, подтверждение и восстановление

Передать **тот же живой** resident RuntimeReplacementGuard в activation driver:
suspended candidate, PID-before-resume, точный instance/state/file-handle handoff,
authorized maintenance startup. Ordinary startup сейчас закономерно отказывает
во время maintenance; снимать guard ради запуска нельзя.

Нужны local IPC health на новом владельце journal, migration identity,
controller receipt новой generation/build и сохранность незавершённых задач.
Без свежего подтверждения не показывать «обновлено». Unknown outcome → truthful
reconciliation/manual intervention, а не replay или rollback наугад.

### 4. Remaining U2 faults, U3 и U4

- Расширить actual crash/receipt faults на последующие file boundaries,
  mixed runtime set, disk-full, activation/controller confirmation. Прежние
  четыре cases не являются полной fault matrix.
- U3: desktop coordination, сохранение forms, уведомление о ходе/результате;
  если desktop был закрыт — без popup, с tray indication. Тестовые окна заметно
  отличаются; предупреждение пользователю перед GUI gate обязательно.
- U4: один согласованный bootstrap/public CLI/MCP route, config/journal
  migrations, полный end-to-end gate. Только затем installed auto-update.
- После каждого вертикального блока review существенных рисков, исправления,
  соответствующий автоматический gate и обновление handoff/roadmap.

## Окружение и безопасные команды

Controller этой сессии PID3922 (перепроверять), ws://192.168.44.203:1143.
IPC .winbridge/controller-session/controller-local-development.sock.
После reboot, если controller отсутствует:

~~~bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
~~~

Installed worker: commit d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2,
build1792486531570951100,
manifest835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241,
generation 8f8d1fb9-e5b1-4984-a17e-d067db46268e.
Его runtime не менялся isolated gates.

- Source/build K:\_proj-26\winbridge\src; installed K:\_proj-26\winbridge\runtime.
- Shared /mnt/hgfs/shared ↔ K:\VMs\shared, только разрешённый namespace WinBridge.
- Isolated project snapshot-fixture:
  K:\_proj-26\winbridge\workspaces\snapshot-fixture.
- Cache run20261017-061527-f4639971 / test-build-cache, одна lease.
- .winbridge/** ignored: не коммитить DB/config/output/logs, credentials
  или полные authenticated supervisor payloads.
- Перед heavy builds ≥6 GiB на обоих хостах, измерения сохранить.
  CARGO_INCREMENTAL=0, основной Debian target. Не удалять binary работающего
  controller. Windows validation dirs/reports/reference не очищать.
- Не менять firewall/services/mounts/startup/security boundaries.
  Backup важных пользовательских файлов по AGENTS.md, secrets отдельно.
- Перед Windows run сообщать worker/desktop/window requirements; после terminal
  — что можно закрыть и какой worker оставить.

Readonly Info/poll без печати request/секретов:

~~~python
import importlib.util
from pathlib import Path
spec = importlib.util.spec_from_file_location('native', 'scripts/validate-workflow-native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
endpoint = Path('.winbridge/controller-session/controller-local-development.sock')
info = native.call(endpoint, {'method': 'info'})['info']
print({k: info.get(k) for k in ('build', 'generation', 'activeTaskCount',
    'controllerConnection', 'maintenanceError', 'journalRecoveryRequired', 'recoveryCoverage')})
task = native.call(endpoint, {'method': 'get_task',
    'taskId': 'f88c0185-6665-424c-8e07-92d0867612ad'})['task']
print({k: task.get(k) for k in ('taskId', 'status', 'exitCode', 'error')})
# read_output chunks содержат bytes: список целых, а не поле text.
~~~

## Финальная точка закрытия

2026-10-23T22:54:13Z: terminal task succeeded/exit0, observer завершён exit0.
Readonly verify-run завершён exit0,13 files дважды и оба mutation audits passed.
Cargo/native tasks отсутствуют; controller3922 оставлен работать.

Installed d052 connected/active0, та же generation8f8d1fb9, ошибок maintenance/
recovery нет, coverage complete. Runtime не обновлялся. Desktop можно закрыть;
worker оставить работать. Ручного обновления сейчас не требуется.

Debian free при финальной сверке25,609,560,064 bytes; Windows после gate
32,755,130,368 bytes. Новых тяжёлых сборок и очисток после этого gate не было.
Финальная machine-readable сводка (ignored, не в Git):
.winbridge/update-preparation/session-close-20261023-candidate-support.json.

Scaffold после добавления этого handoff и gate report:254 Markdown files,
local links/config/task/version checks passed. Документы не меняют immutable
native producer7186bb6; будущие gates требуют нового clean source export.

Работа в этой сессии завершена на контрольной точке по просьбе пользователя.
Цель не помечена complete/paused: actor/pre-drain, activation и U3/U4 впереди.
Первый следующий шаг — раздел1 дальнейшего плана, а не новый discovery/migration
прогон. По завершённым taskIDs ничего не запускать повторно.
