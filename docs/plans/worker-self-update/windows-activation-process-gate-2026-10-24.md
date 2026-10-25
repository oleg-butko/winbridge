# Actual Main22: один private receiver до state writer

2026-10-24. **Harness/verifier подготовлены; actual run ещё не отправлен.**
[Actor22/view14](activation-process-actor.md), [library/target fixture](activation-receiver.md).
Updater18/worker16 прежние; activation/public install off. Это изолированная
проверка, не замена установленного runtime и не полное самообновление.

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
- Producer будет отдельным clean commit после local checks; export receipt
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
Новый actual Windows process gate не отправлен, producer ZIP ещё не экспортирован.
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
