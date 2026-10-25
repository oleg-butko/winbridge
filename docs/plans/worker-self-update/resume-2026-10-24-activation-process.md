# Передача контекста: private receiver и подготовленный Main22 gate

**Позднее продолжение:** [actual Main22 gate112/0 пройден](windows-activation-process-gate-2026-10-24.md),
taskf5d9bf8d-983e-4c7b-8bbb-c6147da5e33b/run20261025-021054-05388d1d,
producer a9860f4/target1c23d48. Не submit/replay заново. Следующий шаг — state
handoff из раздела8; exact proof pins/measurements в gate report. Ниже сохранён
closing checkpoint **до** запуска как история, а не актуальный pending.

2026-10-24, America/New_York (закрывающие логи могут иметь UTC дату2026-10-25).
Пользователь поручил закончить текущие проверки, сохранить подробный checkpoint
и сделать коммит. **Новый Windows process gate в этой сессии не отправлен.**
Следующая сессия начинает с его запуска, а не с повторной реализации launcher.
Полное самообновление не готово; public install остаётся выключенным.

Этот документ и верх [handoff](../../handoff.md) приоритетнее прежних NEXT/ACTIVE.
[План U0–U4](README.md), [activation plan](runtime-activation.md),
[предыдущая передача контекста](resume-2026-10-24-activation-files.md) сохраняют
историю. Незавершённые функции ниже не записывать как пройденные проверки.

## 1. Точная граница готовности

| Область | Состояние при закрытии |
| --- | --- |
| Updater journal | Schema18. Source/compiler/artifacts/health/drain/shutdown/backup/preparation/replacement и activation ledger существуют. Actual17→18/refusal проверены прежним native gate. |
| Worker journal | Schema16. Установленная версия не менялась; новый private entry существует только в candidate source. |
| Внутренний supervisor | Contract22/view14; views1–13 сохраняют прежнюю форму. Один resident owner и blocking slot. |
| Последний Windows execution | Actor21 whole-set read binding, **97 passed / 0 failed**. Он не запускал новый activation receiver. |
| Новый launcher/receiver | Реализованы, подключены к actor22, проверены локально и MSVC cross-check. Настоящее Windows исполнение **не подтверждено**. |
| Новый Main22 harness/verifier | Добавлены в этот checkpoint; подготовлены25 test groups/28 commands, minimum104 passed executions. Actual task/run отсутствуют. |
| Передача состояния | HandoffIssued → StateOwned/local ownership **не реализованы**. Receiver не открывает Store и scheduler. |
| Завершение update | Controller confirmation, process lifetime handoff, production management, U3 desktop и U4 actual installed update впереди. |

`crates/protocol/src/update/updater_compatibility.rs::UpdaterDriverContracts::implemented_windows()`
по-прежнему возвращает `activation=None`, `controller_confirmation=None`,
`desktop_update=None`. Production compatibility/drain path обязан сохранять
Blocked при нехватке driver; нельзя включать public install ради теста.
Isolated legacy execution route — только явный validation scenario.

Основные предыдущие коммиты:

- `547a487` — закрытие read-binding97 и прежняя передача контекста.
- `1c23d48520316af0bf493cbf1be5bf441dffcea3` — suspended launcher/private receiver;
  это готовая **target source fixture** для следующего gate.
- `32c0db3` — pins этой clean fixture.
- `c5ac6da` — actor22/view14 и исправление lifetime I/O.
- Текущий closing commit — Main22 harness/verifier и этот документ. Его полный
  hash получить через `git log -1 --format=fuller -- docs/plans/worker-self-update/resume-2026-10-24-activation-process.md`.

## 2. Что реализовано и почему именно так

`ActivationFilesGuard::launch_receiver` потребляет единственный live Bound guard.
Non-Serde process owner удерживает original file/state/instance custody, один
child/Job, private channel и bounded stdout/stderr. Durable sequence:
Prepared0 → Creating1 → Created2 → ResumeGranted3 → ReadyForState4.
Creating сохраняется до OS effect; настоящий suspended PID/birth/host/session/Job
сохраняется до resume и до fallible output-reader allocation. Неизвестный исход
не становится разрешением повторить CreateProcess. Duplicates только observe.

Новый `winbridge-worker --activate-update ABSOLUTE_CONFIG UPDATE_ID ATTEMPT_ID`
проверяет exact loaded generation/config, actual parent и текущий graph. Nonce
передаётся только по authenticated same-user pipe; его нет в argv/логах/proofs.
Receiver отвечает Ready, удерживает pins и ожидает следующую команду **без
instance lock/Store/scheduler**. EOF, parent death, неизвестная команда или
ограничение ожидания завершают receiver без writer. State-transfer command пока нет.
Обычный `serve` по-прежнему отказывает при unfinished update до открытия writer.

Review исправил lifetime I/O: owner thread8MiB поллит launcher через исходный
Tokio Handle supervisor. Временный Runtime закрыл бы retained pipe после возврата
guard. Regression проверяет настоящий IPC exchange после retirement thread
(Unix socket здесь, named pipe на Windows), без нового TCP/firewall endpoint.

Этот checkpoint добавляет validation-only barriers сразу после durable Creating
и Created. Они проверяются на original deadline/authority и выпускаются только
scope/hash-bound marker, опубликованным staging→rename. Release не даёт permission
и не продлевает120000ms window. Production build не содержит эти hooks.

Main22 проверяет lost launch ACK без повторного запуска, duplicates на обоих
barriers и Ready, actual before-create empty Job, suspended Created identity/Job,
loaded five-file set, live custody, original lock и неизменный DB/WAL. После
завершения owned test Job внешний retained kernel process pin обязан подтвердить
actual exit; cold snapshot сохраняет receipts, но не восстанавливает resources.
`workerLaunchCount=1` относится к activation path, а не ко всем health/build/probe
процессам сценария. State handoff count остаётся0.

Новый independent verifier связывает source/context, пять causal revisions,
before-effect markers и hashes, actual process/Job/loaded set и live/cold views.
Добавлены28 process mutations; общие replacement/migration audits сохраняются.
Expected warm activation явно задаётся verifier: в новом режиме она произошла,
в старых должна отсутствовать. Cold automatic launch всегда0.

## 3. Карта кода для продолжения

| Файлы | Назначение |
| --- | --- |
| `apps/update/src/runtime_replacement/activation_process.rs` | OS driver, consuming process/failure owner и два validation hook. |
| `apps/update/src/runtime_replacement/activation_process_validation.rs` | Feature `update-validation`: scoped barriers, canonical record digest, original deadline/authority. |
| `apps/update/src/activation/owner_stack.rs` | Большой стек, исходный IO driver, сохранение input при startup error. |
| `apps/update/src/supervisor/activation_files.rs`, `supervisor.rs` | Bound → Launching → Ready/LaunchFailed, typed22 dispatch, единственный blocking slot. |
| `apps/update/src/activation/process_view.rs`, `journal/activation.rs` | Cold/live projections, guarded current/history graph. |
| `apps/worker/src/activation.rs`, `main.rs` | Private receiver до обычного startup/Store. |
| `crates/protocol/src/update/activation_receiver.rs`, `activation_process.rs` | Private contract1, marker, typed view14. |
| `apps/worker/tests/update_shutdown.rs`, `update_shutdown/activation_process.rs` | Explicit ignored actual Main22, внешняя process pin и cold proof. |
| `scripts/test-update-health-actor.ps1` | `-ActivationProcessGate`, isolated build/cache lease, selected tests, ready reports и diagnostics. |
| `scripts/validate-update-activation-preparation-native.py` | Submit/observe/recovery; `--activation-process` mutually exclusive с `--activation-files`. |
| `scripts/update-activation-process-proof.py`, `test-update-activation-process-native.py` | Independent process audit/mutations и typing/privacy tests. |
| `scripts/update-activation-preparation-proof.py` | Общий frozen scope checker выделен без выдуманных intermediate receipts; старый режим сохранён. |
| `apps/worker/src/store/ownership.rs` | Будущий state handoff: сначала изучить `begin_authority` и update fencing. |

## 4. Завершённые проверки и их границы

Закрывающие проверки завершены: workspace727/0 (16 ignored), Linux validation
Clippy exit0, MSVC validation/production Clippy exit0, Python3+5+9, fmt/diff и
scaffold269 passed. Команды и результаты также записаны в
[Main22 gate document](windows-activation-process-gate-2026-10-24.md#local-checks-перед-publish).
Raw receipts/logs остаются ignored в `.winbridge/update-preparation/`:

- `activation-main-final-rust-checks.json` и `activation-main-final-*.log` —
  финальные workspace tests/Linux validation Clippy; free space before/after.
- `activation-main-windows-clippy.log` — MSVC validation all-target Clippy, exit0.
- `activation-main-windows-production-clippy.log` — MSVC production, exit0.
- `activation-main-checks.json` — fmt и Python process3/preparation5/runner9, exit0.
- `activation-main-legacy97-readonly.log` — текущие shared helpers заново приняли
  прежний Windows97 proof:30 files twice,72 Main+19 files+12 migration mutations;
  **новая execution task не создавалась**.
- `activation-main-close-scaffold.log` и `activation-main-close-checks.json` — Markdown/local links,
  final fmt/diff/Python checks. `pwsh` в Debian нет: исполнение PowerShell и
  end-to-end Main22 остаются задачей настоящего Windows gate.
- `activation-main-close-installed.json` — безопасная Info сводка installed
  worker и disk measurement при закрытии. Никаких private requests/tokens.
- `activation-main-close-summary.json` — итог: counts/checks, повторный healthy
  installed Info, проверенный target ZIP pin, free space и явные false для
  nativeProcessRunSubmitted/producerExported/stateHandoffImplemented/publicInstallEnabled.

Уже сохранённые проверки actor22 (`activation-process-verified.json`) до текущего
harness: workspace727/0,16 ignored; protocol-jsonschema57/0; owner IPC2/0;
generated contracts/check, TypeScript/UI, local/cross Clippy и fmt passed.
Нынешний harness не меняет публичные DTO/UI, поэтому эти проверки не выдаются
за новый native execution proof. MSVC cross-check выполняет компиляцию, не Windows.

## 5. Последний положительный native proof — только read binding

- Task `60d8285c-3fbe-45dd-8feb-ef63fe75486b`, run `20261024-205117-2da937b6`.
- Producer941a2e6 / target7f6a029; succeeded/exit0,97 tests/20 groups/22 commands.
- Actual17→18/refusal пройден; Prepared0/history1, rootVerifyingLocal15,
  five read pins/original lock/DB/WAL подтверждены.
- Binding43.346s, исходный budget109.753→66.407s;120s не увеличен.
- Local pin `.winbridge/workflow-validation/20261024-205117-2da937b6/activation-preparation-verified.json`.
- Shared report `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-205117-2da937b6`.
- Report SHA `a47044213dd3eca85148fe5650470d5e3a9137510300aa33f3979d6470b61ae0`.
- Main proof SHA `85436df722b67d645796f878307062d3ede75cdc485a00d7f635687bf438e8c9`.
- Migration SHA `23fd663dd12affa2451869473045463566368ba2cae11b32369058efd78dadf4`.

Все остальные pins и readonly verification command — в
[предыдущем checkpoint](resume-2026-10-24-activation-files.md).
История трёх terminal failed attempts и исправление дорогого marker scan —
[в gate report](windows-activation-files-gate-2026-10-24.md). Их не replay.
**Старую fixture7f6a029 нельзя использовать для нового receiver: в ней нет entry.**

## 6. Окружение и данные следующего запуска

Installed Windows worker при closing Info connected/healthy, activeTaskCount0,
maintenanceError=null, journalRecoveryRequired=null, recoveryCoverage.complete=true.
Build commit `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`, buildId1792486531570951100,
manifest `835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241`,
generation `8f8d1fb9-e5b1-4984-a17e-d067db46268e`. Он нужен только для dispatch
изолированного сценария; сейчас ручное обновление не требуется.

Controller оставлен работать PID3922 (parent3921):

~~~bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
~~~

Адрес `ws://192.168.44.203:1143`, IPC
`.winbridge/controller-session/controller-local-development.sock`, log
`.winbridge/update-preparation/controller-resume-20261023.log`.
После reboot PID/адрес/соединение перепроверить; второй listener не запускать.
Параметры из runtime config в Git не переносить.

Readonly Info без печати config/private requests:

~~~bash
python3 - <<'PY'
import importlib.util, json
from pathlib import Path
spec = importlib.util.spec_from_file_location('native', 'scripts/validate-workflow-native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
endpoint = Path('.winbridge/controller-session/controller-local-development.sock')
info = native.call(endpoint, {'method': 'info'})['info']
print(json.dumps({key: info[key] for key in (
    'build', 'generation', 'activeTaskCount', 'maintenanceError',
    'journalRecoveryRequired', 'recoveryCoverage', 'controllerConnection'
)}, ensure_ascii=False, indent=2))
# После submit статус той же task читать так; submission/private data не печатать:
# task = native.call(endpoint, {'method': 'get_task', 'taskId': 'TASK_ID'})['task']
# print({key: task.get(key) for key in ('taskId', 'status', 'exitCode')})
PY
~~~

Windows runtime `K:\_proj-26\winbridge\runtime`, source/build
`K:\_proj-26\winbridge\src`; project snapshot-fixture root
`K:\_proj-26\winbridge\workspaces\snapshot-fixture`.
Shared `/mnt/hgfs/shared` ↔ `K:\VMs\shared`; namespace `WinBridge`.
Cache run `20261017-061527-f4639971`, `test-build-cache`; один exclusive lease.

Для Main22 готова clean **target** source fixture:

- Commit `1c23d48520316af0bf493cbf1be5bf441dffcea3`.
- Shared source ZIP `winbridge-20261024T215731Z-1c23d48-0490a451.zip`.
- ZIP SHA `08003571809f0733dfa9dedd75d9f86f13de68c96c9affec0dec615a7a811575`.
- Manifest SHA `249205a0d2d9d54ad6254dd260987d639996649a92acd2b7adac3d4e844df2d6`.
- 774 files,5,682,595 bytes, built UI; receipt/pin
  `.winbridge/update-preparation/activation-receiver-fixture18-{export,pin}.json`.

Она содержит receiver contract1/schema18, но не actor22: новый **producer** должен
содержать actor/harness из текущего checkpoint и иметь другой clean source commit.
Новый producer ZIP ещё не экспортирован. Никакого Main22 task/run для observe пока нет.

Положительный schema17 migration reference:
`.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
Это terminal production17 task `a4f17a0d-4eea-4af2-8f2c-78bc6235c0af`, producer628c0f6,
33/0. Runner проверяет reference и строит consistent17→18 copy; original DB/WAL/
image сохраняются. Не заменять reference audit-файлом failed run.

## 7. Первые действия новой сессии

1. Прочитать AGENTS, верх handoff/roadmap, этот файл,
   [actor22](activation-process-actor.md) и [описание gate](windows-activation-process-gate-2026-10-24.md).
   Проверить `git status --short` и commit. Native/Cargo/observer runs при закрытии
   завершены; pending execution не равно неизвестному исходу уже отправленной task.
2. Проверить существующий controller и fresh worker Info. Если installed worker
   offline — запустить уже установленный; desktop не нужен. Если installed worker
   требует update либо ведёт себя неожиданно, остановить зависимые Windows действия
   и сообщить конкретную причину пользователю.
3. Перед heavy build измерить Debian/Windows free space; минимум6GiB на каждой
   стороне. Последняя Windows measurement из native97:16,670,916,608 bytes,
   это историческое значение. Debian closing workspace/Clippy:31,788,261,376→26,837,753,856 bytes, выше6GiB;
   точные measurements есть в receipts выше.
   `PATH=/home/user1/.local/bin:$PATH`, `CARGO_INCREMENTAL=0`, общий `target`;
   Cargo последовательно. Разрешена только очистка unused reproducible Debian
   caches без active build, не config/journal/reports/controller. **Windows cleanup
   не разрешён.** В этом closing блоке ничего не очищалось.
4. Из clean checkpoint экспортировать producer. Если менялся UI, сначала собрать
   `npm run build:ui`; текущий built UI есть. Не добавлять generated ZIP в Git:

~~~bash
python3 scripts/export-source.py \
  --output-directory /mnt/hgfs/shared/WinBridge/sources --include-built-ui \
  > .winbridge/update-preparation/activation-process-producer-export.json
~~~

5. До submit сообщить: нужен только installed worker, desktop можно закрыть,
   UI-окон не будет; worker не закрывать до завершения. Один новый namespace,
   без изменений installed runtime/state. Подставить archivePath из export receipt:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/PRODUCER.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T215731Z-1c23d48-0490a451.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-process
~~~

6. Сохранить task/run из малого stdout; большой output писать в ignored log.
   Metadata до ACK — `.winbridge/workflow-validation/RUN/activation-preparation-run.json`,
   idempotency prefix `native-activation-process-`, flags activationFiles=true и
   activationProcess=true. Не печатать request.json/token/nonce/EncodedCommand.
   При обрыве observer сначала get_task; повторить **ту же** команду с
   `--verify-run RUN`, сохранив оба archive и mode. Это observe/recover прежней task,
   а не новый submit. Running/unknown outcome никогда не replay.
7. Если task succeeded, а observer отверг proof — разбирать факты и исправлять audit,
   затем readonly verify того же run. После terminal isolated script/test failure
   разрешено самостоятельно исправить обычную причину, сделать clean producer
   и новый run; повторное согласование рекомендованного решения не требуется.
   Source/runtime permission и installed-worker stop rule остаются обязательны.
8. При успехе сохранить actual test counts, hashes/report/ready files twice,
   migration и negative mutations, timing исходного deadline, один actual PID/Job,
   retained exit/cold proof, unchanged installed Info. Обновить handoff/roadmap
   и commit. После terminal явно сообщить: desktop можно закрыть, installed worker
   оставить. Нельзя назвать целый updater завершённым по одному Ready gate.

## 8. Последовательность реализации после положительного Main22

1. **Передача того же state/instance lock.** Добавить durable HandoffIssued,
   контролируемое освобождение original journal/instance handles и acquisition
   того же FileID новым child. Требуется authoritative StateOwned receipt,
   сохранение прежних tasks/history/idempotency/settings/user pause. Нельзя
   десериализовать guard/nonce или расширять Prepared-only checker на поздние states.
2. **Worker authority и recovery.** Изучить `Store::begin_authority`: сейчас
   unfinished active_worker_update блокирует normal authority, есть отдельные
   require_update_activation/write fences. Достаточность нынешней schema16 для
   handoff ещё не решена. Если нужна worker17 — явная compatibility policy,
   actual16→17 migration/refusal, а не обход startup guard. Не предполагать, что
   RequestContext уже содержит все нужные epoch fences: проверить контракт.
3. **Local/controller confirmation.** Новая local IPC generation/build и реальная
   новая controller session должны подтвердить ту же operation/installation.
   Нынешний isolated Main не доказывает это: controller URL у test runtime=None.
4. **Lifetime и commit.** Новый worker должен пережить штатный уход updater,
   оставаясь управляемым. Сейчас owned Job kill-on-close; Drop close_unconfirmed
   не является empty proof. Не ослаблять DACL/kill-on-close глобально, не добавлять
   unrestricted breakaway. Commit/release только своей maintenance после receipts.
5. **Остальной U2.** Fault/crash matrix каждого нового boundary, rollback по
   authoritative evidence, management CLI/MCP/status и opt-in policy. Frozen
   contracts расширять новой версией, не менять форму старых views.
6. **U3 desktop.** Понятные progress/result, save/exit/update, popup при открытом UI,
   tray/background при закрытом; notification успеха/ошибки и конкретное manual
   recovery при ошибке. Нужны отдельные native tests без вмешательства в рабочий UI.
7. **U4 installed self-update.** Один согласованный bootstrap новой capability,
   затем настоящий update stable `runtime` с recovery и notifications без ручной
   переустановки на каждый commit. Только после этого полная цель выполнена и
   можно возвращаться к отложенному архитектурному A4.3.

## 9. Что намеренно не входит в Git

Runtime configs, tokens/private requests, SQLite/WAL, stdout/stderr, generated
proofs/reports, caches, ZIP и binaries. В коммит входят код, тестовый сценарий,
документация и non-secret pins. Важных пользовательских dirty/unversioned файлов
не меняли, отдельные backups не требовались. `/home/user1/backup`, firewall,
services/mounts/global config и installed runtime не изменялись.
