# Updater: продолжение после runtime replacement, 2026-10-23

Это актуальная точка входа для новой сессии без истории чата. Она заменяет
очерёдность из [предыдущего handoff этого дня](resume-2026-10-23.md), где ещё
предстояли preparation faults и replacement driver. Старый документ сохраняет
историю и доказательства, но начинать реализацию по его списку уже не нужно.

Пользователь попросил завершить текущие проверки, подробно сохранить результаты
и дальнейший порядок работы, затем сделать коммит. При закрытии новые блоки
реализации и новые Windows-задачи не запускались. Последний production commit
перед этим документом: `d1b2beaae8da84775f6d20362a6a0a5b087290e9`.

## Быстрый старт следующей сессии

1. Прочитать `AGENTS.md`, корневой README, handoff, architecture, roadmap и ux,
   затем этот документ и [общий план U0–U4](README.md).
2. Проверить `git status --short`, `git log -8 --oneline`. Не перезаписывать
   пользовательские изменения; для важных невосстановимых файлов нужен backup.
3. Проверить, работает ли controller и доступен ли его readonly IPC. После
   перезагрузки Debian запустить controller командой из раздела окружения ниже,
   если другого экземпляра ещё нет. Исторические PID не являются целью для kill.
4. Через IPC прочитать worker Info. Установленный worker должен быть connected,
   без maintenance/recovery errors; учитывать его activeTaskCount и coverage.
   Desktop не нужен для headless gate. Если worker offline — попросить запустить
   установленный worker, обновление исходников для этого не требуется.
5. Не пересылать terminal задачи из таблицы проверок. Saved history/FilesMoved
   не дают права продолжить их физические эффекты в новой сессии.
6. Первый следующий блок: native проверка нового updater support query и
   интеграция совместимости candidate updater в durable approval **до drain**.
   Затем activation с передачей живых guards, local health и controller receipt.
   Public install включать только после U3 и согласованного U4 bootstrap.

## Что реально готово

| Блок | Подтверждённое состояние | Что этим не подтверждено |
| --- | --- | --- |
| U0 | Source/ZIP/manifest preflight и build-only staging. | Автоматическая установка. |
| Internal U1 | Source transaction, supervisor, compiler/artifact/health actors, drain/quiesce, exact worker shutdown. | Публичный update endpoint/MCP. |
| U2.1 | Final consistent SQLite backup после exact exit, migration/refusal и четыре actual backup faults. | Восстановление после activation нового journal owner. |
| U2.2 preparation | Пять независимых prepared files, ledger/driver, actual Main и три actual faults; migration13→14/refusal. | Замена только по сохранённой JSON-записи. |
| U2.2 replacement | Schema15 ledger, consuming custody, same-volume owned rename, actor v17/view9, actual Main positive gate. | Запуск нового worker, controller confirmation, весь U2.2. |
| Replacement faults | Четыре actual Main cases прошли native gate 60/0; independent audit68. | Полная crash matrix всех файлов, disk-full и activation faults. |
| Updater support query | Readonly image/process-bound query и inspector, local/cross checks. | Native Windows query, durable approval/drain enforcement. |
| U3/U4 | Ещё не реализованы целиком. | Desktop coordination, production cutover, автоматическое installed self-update. |

Текущий source: **updater journal15 / worker journal16 / supervisor17 / view9**.
Старые views1–8 не получают `runtimeReplacement`. Readonly CLI journal/supervisor
diagnostics имеют JSON schema2 и nullable `runtimeApplied`: false до cutover,
null для смешанного/неопределённого набора, true для VerifyingLocal и позднее.
Это поле описывает файлы, а не успешное включение новой версии.

**Полный updater не готов. Public installation/CLI/MCP capability выключены.**
Рабочий Windows runtime не заменялся этими тестами. Architecture-transition A4
по-прежнему отложен до завершения updater.

## Последние Windows проверки

### Positive replacement: завершён, повторять не нужно

Подробный [отчёт actual Main15](windows-runtime-replacement-main-gate-2026-10-23.md).

- Task `665e7e2f-e734-49cf-ae87-d83498a29cbd`, run
  `20261023-163606-15675844`, producer
  `8457004c315fc5718e216fd0f364d5800a083eee`.
- Terminal **succeeded / exit0**; 60 native tests /12 test groups, 20 ready files
  проверены дважды; 44 искажения actual proof отвергнуты независимым verifier.
- Полный Main: source → build → artifacts/health → drain/quiesce → exact exit →
  final backup → prepared files → original custody → replacement.
- Replacement revision22, history23 records; пять old files перенесены в
  displaced-runtime, пять new files — на постоянные имена, manifest последним.
  File IDs сохраняются при rename. Root revision14→15, phase VerifyingLocal.
- До закрытия собственного test Job original physical custody=true; после него
  история FilesMoved сохраняется, custody=false. Обычный запуск нового worker
  запрещён maintenance до миграции/изменения worker DB. Activation не запускалась.
- Source, preimages, final backup и worker DB/nonempty WAL сохранены.
- `report.json` SHA256:
  `54e65d7f2cab94cd35be3e9bc7ccf248d69052bd9e462e173f4c8fd407b33098`.
- `candidate-health-actor-proof.json` SHA256:
  `e3511ebfee87f81fcd9a60f7dbd7affdca3d83c3297fd6a5c46d1f02b929dec5`.

Первый отдельный run `20261023-160520-ee8053b8`, task
`cf49a541-0abf-4aa3-bb64-295faca99381`, terminal failed/exit1. Actual replacement
уже достигла FilesMoved, но test reader запросил несовместимый share mode и получил
Windows error32. Исправлен именно observer, production pins не ослаблялись;
добавлен native SDK regression на чтение/rename и сохранение write/delete exclusion.
Второй run выше является новым изолированным прогоном, не replay первого.
Оба namespace, failed reports и retained files сохранить.

### Replacement fault matrix: результат закрывающей проверки

Завершена task `63232320-8bf6-4826-b13c-315ead0b4433`, run
`20261023-173625-e4eea0a6`, producer
`e5678273f0a3f913c9c1aa88702e96ef81b8835b`.
Она завершена: **succeeded /exit0,60 native tests /12 groups**. Все 32 ready files
проверены дважды,68 намеренных искажений actual proof отвергнуты. Подробный
[отчёт gate](windows-runtime-replacement-faults-gate-2026-10-23.md) содержит
отдельные Update/Attempt IDs, фактические outcomes и exact hashes.
Не resubmit; никакой активной native задачи больше нет.

- `report.json` SHA256:
  `adbcbfd771f13c3aa9d270b07b884008c3d7ae4395bfdbbd0c42d9cb5dbf7773`.
- `candidate-health-actor-proof.json` SHA256:
  `8a5b1963a0c7d9eeca69a29aeb3cef6776ed90e9f5997a8e03559008523c7ed0`.

Матрица состоит из четырёх последовательных настоящих Main cycles:

| Case | Что должно быть доказано |
| --- | --- |
| ExistingDirectory | Чужая displaced directory с sentinel сохраняется; Indeterminate revision1, все old/prepared bytes прежние. |
| LockedOldFile | Сторонний no-delete handle мешает retire; original locks/custody сохранены; снятие блокировки не даёт повторить эффект. |
| ReceiptWriteDenied | Первый old worker физически retired, но receipt не сохранена; в SQL остаётся исходный issued prefix, затем Indeterminate4; duplicate ничего не переносит. |
| HoldBeforeReceipt | Собственный test Job прекращается после OS retire до receipt; SQL остаётся RetireIssued3, marker afterEffect4 не является permission; cold custody=false, replay запрещён. |

Подробности [fault harness и границ proof](runtime-replacement-faults.md).
Успех этих тестов означает корректное сохранение отказа/неопределённости, а не
успешную установку. В fault report `runtimeApplied:null`,
`installedRuntimeApplied:false`, root остаётся Replacing, activation=false.
Эти четыре cases не закрывают всю матрицу U2: disk-full, crash у каждого
последующего file intent/receipt, mixed-set startup и сбои activation/confirmation
требуют отдельных проверок. Не переносить покрытие первого retire на все шаги.

## Exact source/target pins

Архивы находятся в `/mnt/hgfs/shared/WinBridge/sources` в Debian,
`K:\VMs\shared\WinBridge\sources` на Windows.

| Назначение | Архив / commit | Archive SHA256 | Manifest SHA256 |
| --- | --- | --- | --- |
| Positive producer | `winbridge-20261023T163545Z-8457004-b8548a57.zip` / `8457004c315fc5718e216fd0f364d5800a083eee` | `fc284b81054618e13e65fcc73a1463d10f126a41c10a8ff012e79a22ffdd9f1f` | `e91035d320acc54e0dd0bd5afd5ae90c02af52e69279fe5d3b80b49723d4f4ed` |
| Fault producer | `winbridge-20261023T173516Z-e567827-79b806bb.zip` / `e5678273f0a3f913c9c1aa88702e96ef81b8835b` | `19698c773f60ffff2901ec6535357a88e41e75669deee1c4b6db85cbf1a8b66e` | `205e39d548244a7a02d42ba1ca29493867a9b59b66b9fdc33febd95872253972` |
| Target fixture обоих прогонов | `winbridge-20261023T091226Z-5fbc1cf-b4ac44a5.zip` / `5fbc1cf2e37086dad8e65667ed3af31b1a16d9f7` | `3891ad4689779c2eb2507b70be4ad5d2f599cf4816fb4b882d64f2de0006c6fb` | `98e52e10e99e25fbc8cf9864b4af3e4d83419e8a1297cbcd330dc61e5c4546f3` |

Target5fbc — намеренно ограниченная fixture с schema15/16. Она не умеет полный
activation/self-update, не имеет нового runtime-support marker и **не является
production target**, даже при совпадении номеров SQLite schema.

Shared reports: `/mnt/hgfs/shared/WinBridge/workflow-validation/<run>` ↔
`K:\VMs\shared\WinBridge\workflow-validation\<run>`.
Локальные full proof/pins/logs находятся в ignored
`.winbridge/update-preparation/`; они не коммитятся:

- `replacement-main-native-verified.json`, `replacement-main-native-first-failed.json`;
- `replacement-faults-native-active.json`, `replacement-faults-native.log`;
- `replacement-faults-native-verified.json`, `replacement-faults-proof-audit.json`;
- `session-close-20261023-replacement-verification.json` — финальная сверка обеих задач;
- `replacement-migration-native-verified.json` — actual14→15/refusal;
- `updater-support-local-tests.log`, `updater-support-*-clippy.log`.

Если local pin потерян, восстановить только data JSON с `source` и `target` из
таблицы выше и выполнить independent audit уже сохранённого proof:

```bash
python3 scripts/test-update-replacement-proof.py \
  --proof /mnt/hgfs/shared/WinBridge/workflow-validation/20261023-173625-e4eea0a6/candidate-health-actor-proof.json \
  --pin .winbridge/update-preparation/replacement-faults-native-active.json
```

Это чтение и mutation audit в памяти, оно не отправляет Windows commands.
Verifier сам выбирает positive/fault формат по kind. Кроме proof обязательно
сверить каждый файл из `report.ready.json` по bytes/SHA и terminal task/exit.
Один успешный флаг JSON без этих связей доказательством не является.

## Карта реализации и существенные инварианты

| Файл / модуль | Назначение |
| --- | --- |
| `apps/update/src/runtime_replacement.rs` | Immutable intent/record, exact preparation binding и типизированная FSM. |
| `apps/update/src/journal/runtime_replacement.rs` | Schema15 current/history, fresh admission/permit и trusted `finish_runtime_replacement`. |
| `apps/update/src/runtime_replacement/physical.rs` | Consuming `RuntimeReplacementGuard`, original state/instance custody, five old/new owned handles. |
| `apps/update/src/runtime_replacement/driver.rs` | Один blocking driver; durable intent → one OS attempt → verified receipt. |
| `apps/update/src/shutdown/physical.rs`, `shutdown/state.rs` | Original `ShutdownExitGuard`, `ExitedStateCustody`, те же process/file locks. |
| `apps/update/src/supervisor.rs` | Internal v17 action/view9, admission до ACK, responsive mailbox, resident result guard. |
| `crates/platform/src/source_files.rs` | Native owned-file/directory operations, pinned ancestors и same-volume rename. |
| `apps/worker/src/runtime_guard.rs`, `store/update.rs`, `update.rs` | Manifest/loaded-image pins и pending-update startup guard до DB effects. |
| `apps/worker/tests/update_shutdown.rs`, `update_shutdown/replacement.rs`, `replacement/faults.rs` | Настоящие normal Main процессы, isolated state/runtime, positive/fault proof. |
| `apps/update/src/runtime_replacement/validation.rs` | Только clean Windows `update-validation`: bounded barrier после move до receipt. В release build отсутствует. |
| `scripts/test-update-health-actor.ps1`, `validate-update-health-actor-native.py` | Approved orchestration, один native build-cache lease, source/target/ready binding. |
| `scripts/update-replacement-proof.py`, `update-replacement-faults-proof.py`, `test-update-replacement-proof.py` | Независимые causal/physical verifiers и negative mutation audit. |

1. Serializable evidence — история, а не OS authority. Fresh permit не Clone/Serde,
   выдаётся лишь после commit. Existing/duplicate/reopen не создают новый процесс
   или move. Lost ACK не отменяет committed operation.
2. `Custody::take_for_replacement` передаёт **тот же original guard**, а не заново
   захватывает похожие locks. Busy Arc loan восстанавливает прежний Arc.
3. Prepared handles приобретаются, пока old read pins ещё удерживаются; затем
   guard сохраняет исходные process/state/instance handles и заменяет только
   необходимые file pins на owned READ+DELETE handles.
4. Fixed ancestor pins, exact File ID/size/SHA, same volume, no-overwrite и одна
   попытка OS move. Чужие directory/entries не adopt/clean. Manifest — последний.
5. Guard удерживается и после обычного failure. Если OS effect был, а receipt
   нет, сохраняется исходный causal prefix и Indeterminate; автоматического
   retry/rollback/adoption из сохранённых данных не делать.
6. `finish_runtime_replacement` единственный trusted FilesVerified путь: physical
   full set + current owner/epoch/record + atomic root CAS. Raw lifecycle не может
   прислать FilesVerified, LocalHealthVerified или ControllerConfirmed вместо proof.
7. После смерти owned supervisor Job locks исчезают, но история не исчезает.
   Обычный worker startup должен оставаться закрыт maintenance до отдельной
   авторизованной activation. File move completion не означает Committed.

## Updater support: последнее локальное изменение

[Детали query](updater-runtime-support.md), commit `d1b2bea`. Отдельный marker
`WinBridge.UpdaterRuntimeSupport.contract-1`, strict DTO schema1/queryContract1,
early `--runtime-support` до config/journal. Inspector принимает pinned image и
artifact descriptor, проверяет marker **до запуска**, actual process stamp,
source/build/role, immutable bytes, bounded probe cleanup и strict поля.
Нет marker → Unsupported без пробного запуска неизвестного флага.

Windows объявляет final backup15, preparation16, replacement17;
activation/controllerConfirmation/desktopUpdate пока null. Linux native contracts
null. `Standard`/`Validation` показывают реальный build flavor. Inspection имеет
`readOnly:true`, `cleanupVerified:true`, `journalBound:false`,
`candidateVerified:false`; не является install permission или health receipt.

Код: `crates/protocol/src/update/updater_support.rs`,
`apps/update/src/updater_support.rs`, early branch `main.rs`,
`apps/update/tests/updater_support.rs`, generated TypeScript.

Проверки этого commit: updater package **225 passed /0 failed /6 ignored**,
Linux и Windows all-target Clippy с update-validation, generated contracts,
TS typecheck/UI build, Python runner9, fmt/scaffold247. Шесть ignored — отдельные
native/crash fixtures, не пройденные этим local command. Последний workspace
driver check до query:653 passed /0 failed /16 ignored. Нового полного workspace
после query не заявлять. Native fault producer e567 не включает query.

## Следующие вертикальные блоки: порядок и критерии

### 1. Candidate updater compatibility до остановки worker

- Сделать actual Windows тест `--runtime-support` / `inspect`, включая настоящий
  old5fbc без marker, strict process/image binding и отсутствие побочных файлов.
  Добавить в следующий approved native runner; общий cache не занимать параллельно.
- Сохранить support evidence в immutable durable health/approval binding к
  candidate ImageEvidence/descriptor/source/health/config/owner scope.
- В production проверять полный требуемый набор контрактов до drain, а не только
  matching schema; Validation build не допускать как production target.
- Missing/unsupported contract должен давать понятный blocker при ещё работающем
  old worker. Изменение image/build/owner/revision обесценивает старую receipt.
- Если нужны новые persistent fields/actions — versioned schema/contract, frozen
  old readers и migration/refusal test. Не объявлять activation support заранее.
  Новую обязательную проверку не добавлять молча в старый frozen health/approval
  contract: согласовать новую версию, сохранить прежний negotiated путь и явно
  отделить limited native fixtures от разрешённой production установки.

### 2. Activation из resident replacement guard

- Ввести отдельный typed intent/record/ledger и one-use permit. Admission требует
  того же живого завершённого replacement guard, FilesMoved и актуального owner.
- Новый worker запускать управляемо, suspended PID-before-resume; продумать и
  доказать точный handoff original state/instance locks и runtime file handles:
  текущие READ+DELETE owners нельзя просто заменить несовместимыми read pins.
  Не делать unlock-all → normal serve из JSON и не ослаблять обычный startup guard.
- Отдельный authorized verification mode до миграции/scheduler/effects должен
  сверить installation/update/attempt/epoch/image/config/journal incarnation.
- Durable local health receipt → AwaitingController; exact new build/session/
  contracts confirmation → Committed. Снимать только собственный maintenance,
  сохраняя пользовательские pause/settings и task idempotency history.
- Bounded timeout, crash до/после PID/lock handoff, stale process/epoch, lost ACK,
  local health failure и controller loss не превращать в автоматический relaunch.
  Нужны exact outcome/manual recovery и independent actual-process proofs.
- Перед production cutover дополнить оставшуюся U2 fault matrix, включая нехватку
  места и crash boundaries остальных файлов; использовать feature-only faults
  в своих isolated namespaces, не заполнять пользовательский диск.

### 3. U3: desktop, восстановление и наблюдаемость

- Если desktop работает: понятное предупреждение/progress, save-before-exit,
  authenticated ACK и exact exit; обновление desktop той же binary transaction.
- Если desktop закрыт: никакого всплывающего main/popup, допустим запуск в tray;
  один tray и видимый результат успеха/ошибки, quiet hours/drafts сохранены.
- Для migration — rehearsal на consistent backup актуального final state,
  old-version refusal. После новых DB effects не обещать безопасный binary-only
  rollback. Отдельно проектировать rollback eligibility и manual recovery.
- Автоматический UI gate помечает тестовое окно и показывает ход; до/после run
  сообщать пользователю, какие окна/worker оставить, что можно закрыть.

### 4. U4: public orchestration и один bootstrap

- Installation-level management/status/CLI/MCP с opt-in local policy; источник
  только WinBridge package, paths из local config, без произвольных runtime roots.
- Когда isolated pipeline целиком доказан, объяснить необходимость **одного**
  ручного bootstrap установленного d052. До этого installed runtime не трогать.
- Следующий installed update должен агент выполнить без Ctrl+C/распаковки/rebuild
  пользователя. Доказать stable exe paths, actual versions, сохранность history/
  config, восстановление controller и нормальный бизнес-сценарий.
- Итог updater/install, UI outcome и доставка отчёта — разные статусы. Ошибка
  должна читаться без живого worker и содержать точные ручные действия.
- Обновить guides/readiness и лишь затем вернуться к installed A4.3 gate.

Для каждого блока: review → важные исправления → адресные local/cross checks →
clean source export → isolated native run → ready/proof audit → commit/docs.
Не дробить работу на ручные Windows проверки, если её покрывает approved сценарий.
При обычной ошибке isolated harness выбрать рекомендуемое исправление и повторить
автоматически **новым run ID**. При неожиданном поведении установленного worker
или необходимости его обновления остановить зависимые Windows действия и объяснить.

## Окружение, запуск и диск

```bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
```

Endpoint `192.168.44.203:1143`, IPC
`.winbridge/controller-session/controller-local-development.sock`.
В этой сессии controller PID3922; его config/log не коммитятся.
Installed worker source `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`, build
`1792486531570951100`, generation `8f8d1fb9-e5b1-4984-a17e-d067db46268e`.
Generation/session/PID перепроверять, это наблюдения, не постоянные идентификаторы.

Пример readonly проверки из корня репозитория, без запуска новых задач:

```bash
python3 - <<'PY'
import importlib.util
from pathlib import Path
spec = importlib.util.spec_from_file_location('native', 'scripts/validate-workflow-native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
endpoint = Path('.winbridge/controller-session/controller-local-development.sock')
info = native.call(endpoint, {'method': 'info'})['info']
for key in ('generation', 'build', 'controllerConnection', 'activeTaskCount',
            'maintenanceError', 'journalRecoveryRequired', 'recoveryCoverage'):
    print(key, info.get(key))
for task_id in ('665e7e2f-e734-49cf-ae87-d83498a29cbd',
                '63232320-8bf6-4826-b13c-315ead0b4433'):
    task = native.call(endpoint, {'method': 'get_task', 'taskId': task_id})['task']
    print(task_id, task['status'], task.get('exitCode'), task.get('reason'))
PY
```

Не выводить целиком raw task/submission/CommandLine и private supervisor-control:
для статуса достаточно выбранных полей, а control может содержать authentication
данные. Не открывать production SQLite writer ради диагностики.

- Installed path `K:\_proj-26\winbridge\runtime`, main source/build
  `K:\_proj-26\winbridge\src`; постоянные exe paths нужны firewall пользователя.
- Тестовый project root `K:\_proj-26\winbridge\workspaces\snapshot-fixture`;
  native cache run `20261017-061527-f4639971`, один build lease одновременно.
- Измерять свободное место Debian и Windows перед тяжёлой сборкой, минимум6 GiB;
  `CARGO_INCREMENTAL=0`, основной local target переиспользовать.
- Если места мало, остановить новые builds и чистить лишь unused reproducible
  Debian caches, не binary работающего controller. Windows validation directories,
  journals/config/runtime, reports и shared пользовательские данные не удалять.
- Не менять Rust source во время local Cargo. Никаких services/firewall/system
  settings, TLS, mounts или plaintext secret backups для этой работы не требуется.

## Закрытие сессии и что оставлено работающим

Финальная независимая сверка: **2026-10-23 18:20 UTC**. Оба последних Windows
task terminal succeeded/0, native observer завершён exit0. Все **52 ready files
проверены дважды**, оба full proof прошли verifier и 112 negative mutations
отклонены; три уникальных source/target ZIP полностью сверены с manifests.
Новых Windows tasks, сборок или UI-окон при закрытии не создавалось.

Worker source/build/generation прежние, connected, activeTaskCount0,
maintenanceError и journalRecoveryRequired=null, recoveryCoverage.complete=true.
Controller PID3922 оставлен работать; installed worker оставить запущенным,
desktop можно закрыть. Local Cargo/native runs больше нет.

Debian free при сверке **26,487,005,184 bytes**; Windows fault gate измерил
37,855,776,768→35,630,841,856 bytes. Очистка не требовалась; runtime/config/journal,
retained validation directories, отчёты и backups не удалялись. Локальные
конфигурации пользователя не менялись; отдельные backups не создавались.

В этой завершающей задаче изменялась только документация. Python runner tests
повторены9+4/0, Markdown links/examples/scaffold и `git diff --check` прошли.
Rust/UI заново не пересобирались: их предыдущие результаты приведены выше,
production source после d1b2bea не менялся. Не объявлять pending Windows support
query выполненной лишь потому, что эти local/cross проверки прошли.

По просьбе пользователя реализация остановлена на этой контрольной точке.
Следующая сессия начинает с пункта 1 следующего вертикального блока — support
query/compatibility; не с preparation/replacement заново. Полный updater не
объявлен завершённым, новый installed package/ручное обновление сейчас не нужны.
