# Actual Main22: один private receiver до state writer

2026-10-24 America/New_York, запуск02:10UTC2026-10-25.
**Gate пройден:112 passed /0 failed,25 groups/28 commands; observer exit0.**
[Actor22/view14](activation-process-actor.md), [library/target fixture](activation-receiver.md).
Updater18/worker16 прежние; activation/public install off. Это изолированная
проверка, не замена установленного runtime и не полное самообновление.

## Положительный actual run

- Task `f5d9bf8d-983e-4c7b-8bbb-c6147da5e33b`, run `20261025-021054-05388d1d`,
  terminal succeeded/exit0.112 tests/25 groups/28 commands, actual Main701.20s.
- Clean producer `a9860f4f4033f5fc400f4c6c9de648ff5e83e9a0`, ZIP
  `winbridge-20261025T021042Z-a9860f4-677c4a65.zip`, SHA
  `c3702ccc454dcb1effca858664f4f2794c7fbe18f05b379b65e15ca9e8e49d02`, manifest
  `828dec1647c3558885007b3ced54639ca797495fa4e0ca32b1902746bfe4edad`.
- Target fixture1c23d48 и reference/cache ниже не менялись.
- Observer проверил38 ready files дважды, отверг44 replacement +28 process +12
  migration mutations. Actual17→18/refusal passed. Отдельный old files audit
  не запускался: новый process verifier проверяет actual bound set в этом scope.
- Report SHA `8f155ff1b368cedc27daa0b68681094d0814754cec52f9db9fbb651ce407a61b`.
- Main proof SHA `be5c15afbf4031d43bcc7892f85ffb6ff3998508cf839712d837971ca4662401`.
- Migration SHA `48ebf3539d8400a536b00950efc8bf72c6f25e86d7938eb92d58370fea68026e`.
- Observer log `.winbridge/update-preparation/activation-main-native.log`;
  local pin `.winbridge/workflow-validation/20261025-021054-05388d1d/activation-preparation-verified.json`.
- Shared report `/mnt/hgfs/shared/WinBridge/workflow-validation/20261025-021054-05388d1d`.

Actual activation attempt `0830fd67-c6ef-4495-9a6e-aa8ae3e58d27`, receiver PID4744 /
FILETIME134373690472836061, original supervisor PID25192. Один live child/Job,
Prepared0→Creating1→Created2→ResumeGranted3→Ready4, original instance lock/DB/WAL
сохранены. RootVerifyingLocal/revision15 не изменён. stdout/stderr пусты, state
handoff0. Lost ACK и duplicates не создали второй процесс. После owned Job exit
retained actual process handle signalled, Job empty; cold view NotResident,
custody/state=false, saved Ready retained, automatic cold launch0.

Measured after activation admission: Creating63,777ms, Created74,155ms,
ResumeGranted85,101ms, Ready91,397ms, persisted Ready93,571ms. Original120,000ms
window не продлевалось. Main wall timings от начала helper (до admission):
Bound59,638ms, Ready102,459ms. Это разные точки отсчёта.

Windows free14,689,849,344→13,645,647,872 bytes, Debian до dispatch26,836,901,888 /
после26,838,966,272 bytes. Cleanup не выполнялся. Fresh installed Info после
terminal: прежний d052/generation8f8d1fb9-e5b1-4984-a17e-d067db46268e,
connected/active0, maintenanceError=null/journalRecoveryRequired=null,
recoveryCoverage.complete=true. Desktop можно закрыть, installed worker оставить.
Installed runtime/state не заменялись, UI-окон не было.

Review actual proof не выявил необходимости менять launcher. Следом physical
state-lock release с retained identities/failure custody, затем authenticated
writer handoff/local/controller/lifetime и оставшиеся U2/U3/U4. Public install off.
Не replay terminal task. Readonly recheck — та же команда ниже с exact producer
ZIP и `--verify-run 20261025-021054-05388d1d`.

## Что исполняется

Новый explicit `-ActivationProcessGate` / `--activation-process` проходит весь
прежний actual Main shutdown/backup/preparation/replacement, затем в original
supervisor: Prepare → five-file Bind → Launch без тяжёлых промежуточных audits.
Исходный120000ms deadline не продлевается. Native validation updater заморожен
в run-local `activation-validation-updater.exe` с SHA readback до других builds.

После durable Creating validation-only barrier останавливает owner до OS call.
Проверяется actual SQLite revision1, отсутствие процесса в уникальном scoped Job,
занятость original instance lock и unchanged DB/WAL. Release публикуется через
staging/rename и связывается с input/record SHA; это не permission и не renewal.
После durable Created второй barrier подтверждает actual PID/birth/host/session
и occupied Job до resume. Duplicate starts на обоих barriers только наблюдают.

После resume target worker из clean fixture1c23d48 отвечает authenticated Ready.
Проверяется один PID/attempt, exact loaded set/config/manifest, live resident
process/custody, original lock, write/delete denial и неизменный worker journal.
История — строго Prepared0/Creating1/Created2/ResumeGranted3/Ready4. Ни HandoffIssued,
ни Store/scheduler не запускаются, update root остаётся VerifyingLocal15.
Утерянный первый launch ACK не повторяется. Legacy view13 не получает поле14,
но продолжает показывать пять удерживаемых файлов.

Harness удерживает отдельный kernel process pin через завершение owned test Job.
Cold audit требует signalled actual handle и пустые Jobs, ту же историю без
process/file/state custody и automatic cold launch0. Normal serve также обязан
отказать до writer (existing replacement cold check).
`workerLaunchCount1` относится только к activation path: artifact/health/rehearsal
и normal-startup-refusal probes являются другими явно выбранными scope.

## Группы и independent audit

25 native test groups/28 commands; точный passed count фиксируется по реальным
логам, minimum104. Включены former file/preparation/backup/replacement regressions,
новые process contract/mailbox/owner tests и atomic create-before-resume test.
Fixture binary перед atomic test собирается из текущего producer отдельно.
Owner I/O test использует named pipe на Windows, без TCP/firewall endpoint.

`update-activation-process-proof.py` независимо связывает before/replacement,
frozen source/context, всю causal history, оба before-effect markers и their
hashes, actual process/Job/loaded identity, live/duplicate/cold projections.
Числовой FILETIME проверяется как exact u64 (может превышать JSON-safe counter);
bool вместо0/1 не допускается. Context verifier выделен из старого preparation
checker без подстановки несуществующих histories/receipts.

Existing replacement verifier теперь явно принимает expected activation prefix:
старые modes требуют afterOwnedExit.activationStarted=false, новый —true, так как
warm activation действительно состоялась. Cold automatic launch при этом0.
Отрицательные mutations проверяют process/Job/scope/history/output/state flags;
base replacement и migration audits остаются отдельными. Ready files дважды
проверяются с SHA. Эти данные нигде не становятся OS/state permission.

## Pins и возобновление

- Clean target `1c23d48520316af0bf493cbf1be5bf441dffcea3`:
  `winbridge-20261024T215731Z-1c23d48-0490a451.zip`, ZIP SHA
  `08003571809f0733dfa9dedd75d9f86f13de68c96c9affec0dec615a7a811575`, manifest
  `249205a0d2d9d54ad6254dd260987d639996649a92acd2b7adac3d4e844df2d6`.
- Producer — отдельный clean commit a9860f4; export receipt
  `.winbridge/update-preparation/activation-process-producer-export.json`.
- Actual17 reference:
  `.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
- Cache `20261017-061527-f4639971`, один exclusive lease.
- Run metadata остаётся `activation-preparation-run.json` в local run directory;
  mode flags activationFiles=true/activationProcess=true обязательны для recovery.

Observer command с выбранным producer ZIP:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/PRODUCER.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T215731Z-1c23d48-0490a451.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-process
~~~

После submit сохранить task/run и ждать именно его. Observation timeout не
terminal: get_task, затем та же команда с `--verify-run RUN`; не resubmit.
Если proof отклонён observer, а native task succeeded, исправить audit по фактам
и повторить только readonly observe. Новая execution task — лишь после terminal
диагностики в новом namespace. Installed worker unexpected/update requirement
останавливает зависимые Windows действия; обычные isolated script fixes разрешены.

Для прогона оставить installed worker, desktop не нужен, UI-окон не будет.
Перед heavy build измерить обе стороны (минимум6GiB). Windows directories не
очищать. После terminal явно сообщить, какие процессы оставить работать.

## Local checks перед publish

Закрывающий checkpoint2026-10-24: **все перечисленные проверки завершены**.
Это результаты до submit; активный run и producer теперь указаны сверху.
[Подробная передача контекста](resume-2026-10-24-activation-process.md) — точка старта
новой сессии; она содержит карту кода, окружение, pins, recovery и следующие U2–U4.

| Проверка | Результат |
| --- | --- |
| `cargo test --workspace --locked` |727 passed /0 failed /16 ignored;223.02s. |
| Linux validation all-target Clippy | Exit0,38.35s; `-D warnings`. |
| MSVC validation all-target Clippy | Exit0,40.29s; hooks/Main22 cross-compiled. |
| MSVC production all-target Clippy | Exit0,36.95s; validation hooks исключены. |
| `cargo fmt --all -- --check` | Passed. |
| Python process/preparation/runner tests |3+5+9 passed. |
| Legacy Windows97 readonly recheck |30 files twice;72 Main+19 files+12 migration mutations rejected; execution не повторялась. |
| Markdown/local links и `git diff --check` | Passed;269 Markdown files. |

Финальный Linux Clippy:

~~~bash
PATH=/home/user1/.local/bin:$PATH CARGO_INCREMENTAL=0 \
  cargo clippy --workspace --all-targets \
  --features winbridge-worker/update-validation,winbridge-update/update-validation \
  --locked -- -D warnings
~~~

Native proof не подменяется этими checks. `pwsh` в Debian отсутствует; выполнение
PowerShell и actual process/pipe/Job на Windows проверит только следующий gate.
Публичные DTO/UI в нынешнем harness не менялись; предыдущие actor22 checks
protocol57/0/generated contracts/TS/UI сохраняют отдельный source scope.

Logs/receipts `activation-main-*` в ignored `.winbridge/update-preparation/`.
Debian до closing workspace31,788,261,376 bytes, после workspace/Clippy
26,837,753,856 bytes (>6GiB); очистка не выполнялась. Last measured Windows
16,670,916,608 bytes относится к предыдущему native97, перед heavy build
измерить заново. Installed Info connected/healthy/active0, buildd052 unchanged.

После positive Main/review — state/instance-lock handoff и ownership receipt,
local/controller/lifetime и remaining U2 faults/management/U3/U4. Пока не писать,
что activation или updater целиком проверены.
