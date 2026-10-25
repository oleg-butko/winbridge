# Продолжение апдейтера: checkpoint StateOwned23, 2026-10-25

**Исторический checkpoint до actual Main23 и worker17.** Для продолжения читать
[новую инструкцию journal activation](resume-2026-10-25-journal-activation.md).
Physical132/0 уже пройден, atomic SQLite layer добавлен; утверждения ниже об
отсутствующих state harness/flags и schema16 относятся к прежнему source.

Дата сессии: 2026-10-25, America/New_York. Пользователь попросил завершить
текущие проверки и сохранить подробную инструкцию для следующей сессии.
**Начинать с этого файла, а не с исторических NEXT/ACTIVE в прежних отчётах.**
На момент закрытия все проверки завершены; нового Windows state-transfer
task/run нет, ожидающего выполнения Cargo/observer тоже нет.

## 1. Что готово и где проходит граница

Рабочий WinBridge умеет доставлять исходники и запускать автоматические
изолированные проверки на Windows. Его установленный worker нужен для запуска
тестового сценария; его runtime при этих проверках не заменяется.

Самообновление находится в **U2.2** плана [U0–U4](README.md). U0 и внутренний U1,
backup/fault gates U2.1, preparation/replacement и запуск private receiver уже
проверены на настоящей Windows. В source добавлена передача прежней блокировки
состояния тому же receiver. Именно эту новую межпроцессную передачу теперь
предстоит проверить сквозным native gate.

| Элемент | Точное состояние |
| --- | --- |
| Updater journal / worker journal | Schema **18 / 16**, этот блок их не меняет. |
| Supervisor / consistent view | Internal contract **23**, view **15**. Views1–14 сохраняют старую форму. |
| Private receiver | Challenge/Ready1 сохранены; отдельный state handoff **2** и marker `WINBRIDGE_PRIVATE_STATE_HANDOFF_V2`. |
| Actual Main22 | **112 passed / 0 failed**: один запущенный receiver до Ready, прежняя блокировка остаётся у supervisor. |
| Selective lock-release adapter | **7 native tests / 0 failed**: consuming release, сохранение handles и failure custody. |
| Actor23 / физический StateOwned | Source реализован, локальные/cross проверки пройдены; **сквозного Windows proof ещё нет**. |
| Writable Store / новая JournalAuthority | **Не реализованы** для activated receiver. Он только берёт lock и читает прежний journal readonly. |
| Local IPC / controller confirmation / lifetime transfer | Для activated receiver **ещё не реализованы**. |
| Public install / автоматическое installed self-update | **Выключены**. `activation`, `controller_confirmation`, `desktop_update` в implemented drivers остаются `None`. |

`StateOwned` здесь означает физическое владение прежним instance lock с теми же
FileID и новым generation. Эта receipt не разрешает запись в worker SQLite,
не подтверждает новую epoch/работающий сервис и не означает успех обновления.
После теста owned Job завершается; эксплуатационная передача lifetime ещё впереди.

## 2. Что реализовано и исправлено после Main22

Source commit **`1370d6b030e18a22266be1b02d24e4371eee01ac`**
(`feat(update): hand physical state to the authenticated receiver`) содержит:

1. Отдельные typed V2 command/receipt. Команда связывает update, attempt,
   revision5 и domain-separated canonical SHA всего HandoffIssued record.
   Существующий OS-authenticated channel, actual PID/birth/host/session,
   current owner/root/graph обязательны; JSON digest не заменяет authority.
2. Parent потребляет тот же Ready guard в исходном IO driver и единственном
   blocking slot. Сначала CAS/readback **HandoffIssued5**, затем повторные
   authority/physical/deadline checks, затем release собственного lock handle.
   Residual handles и exact exited-process pin остаются у supervisor как
   `ReleasedExitCustody`, включая ошибочные исходы. Обратного relock/retry нет.
3. Receiver сверяет свой прежний Ready, current HandoffIssued, loaded runtime,
   config и identities существующих directory/journal/lock. Он приобретает
   существующий lock, проверяет readonly точную schema и полный прежний
   quiesced lease/accepted shutdown receipt. Новые state files не создаёт.
4. Child возвращает новый generation с теми же logical instance/incarnation и
   физическими IDs. Parent проверяет receipt и сохраняет **StateOwned6**.
   Полный нормальный ledger: Prepared0 → Creating1 → Created2 → ResumeGranted3
   → Ready4 → HandoffIssued5 → StateOwned6.
5. Повтор actor23 start наблюдает ту же попытку. Потеря ACK/частичная запись/
   неизвестный исход не создают второй child, release или private command.
   Cold reader не восстанавливает runtime handles из ledger.
6. View15 показывает saved receipt отдельно от resident process/liveness и
   nullable original lock observation. Legacy14 после handoff даёт
   Indeterminate с указанием на23; legacy13 сохраняет five-file диагностику.
   Форма старых views не расширяется.

Review исправил важный недостаток admission: header/incarnation без полного
accepted shutdown scope были недостаточны. Теперь обязательны **exact schema**
и равенство полного accepted receipt/lease. Два настоящих SQLite теста проверяют
отсутствие receipt, чужие scope/process/owner/command, old/future schema и
отсутствие миграции/смены epoch при чтении. Safe local path/reparse checks идут
до canonical identity comparison.

Ошибки выполнения локальных проверок уже разрешены:

- Mailbox fixture первоначально копировала устаревший standalone
  `target/debug/winbridge-update` и получала EOF. Explicit build перед mailbox
  test исправил это; это не проблема установленного Windows worker.
- Windows Clippy потребовал переименовать локальный enum variant в `Owned`.
  Durable/protocol `ActivationState::StateOwned` и wire state не переименованы;
  финальные validation и production Clippy прошли.

Исходное startup window **120000 ms** не продлевается. Receiver один раз
фиксирует remaining monotonic budget; новый message или wall-clock rollback
не дают новое окно. Поздняя receipt допустима как факт, но не новый grant.

Подробности реализации: [activation-state-handoff.md](activation-state-handoff.md).

## 3. Завершённые проверки и их достоверность

### Source1370d6b до закрывающего docs commit

| Проверка | Результат |
| --- | --- |
| Workspace | **741 passed / 0 failed / 16 ignored**. Ignored native tests не выдаются за выполненные. |
| Focused activation | **33 / 0**. |
| Readonly state journal | **2 / 0**, реальный SQLite. |
| Actual local supervisor mailbox23 | **1 / 0**, после standalone build. |
| Protocol с jsonschema | **59 / 0**. |
| Linux validation и Windows MSVC validation/production all-target Clippy | Passed. |
| Generated contracts + stale check, TypeScript, UI build, fmt | Passed. |

Machine summary: `.winbridge/update-preparation/activation-state-v23-verified.json`.
Raw logs: `activation-state-v23-*` в том же каталоге. Финальные логи с суффиксами
`-checked`, `-final` и `-production-clippy` приоритетнее промежуточных failed logs.
Cross-compilation не считается запуском Windows.

После будущих изменений source подбирать проверки по риску; существующие
команды для воспроизведения основных локальных проверок:

~~~bash
PATH=/home/user1/.local/bin:$PATH CARGO_INCREMENTAL=0 cargo test --workspace --locked
PATH=/home/user1/.local/bin:$PATH CARGO_INCREMENTAL=0 cargo test -p winbridge-update --lib activation --locked
PATH=/home/user1/.local/bin:$PATH CARGO_INCREMENTAL=0 cargo test -p winbridge-worker --lib physical_journal_readonly --locked
PATH=/home/user1/.local/bin:$PATH CARGO_INCREMENTAL=0 cargo test -p winbridge-protocol --features jsonschema --locked
~~~

Для нового native harness нужны также validation-feature compile/Clippy на
MSVC и production-проверка без validation feature. После DTO изменений:
`npm run generate:contracts`, `npm run check:contracts`, `npm run typecheck`,
`npm run build:ui`. `cargo fmt --all -- --check` и `npm run check:scaffold`
проверяют формат и локальные ссылки. Это инструкция на будущее, а не новые
исполнения, добавленные к counts выше.

### Проверки при закрытии этой сессии

- Fresh readonly Info: installed worker подключён, activeTaskCount0,
  maintenanceError=null, journalRecoveryRequired=null, recoveryCoverage.complete=true.
  Оба ранее завершённых native task по-прежнему succeeded/exit0.
- **Readonly same-run recheck Main22 прошёл**, observer exit0: 38 ready files
  проверены дважды, 44 replacement + 28 process + 12 migration mutations
  отвергнуты; actual17→18/refusal evidence принято. Новой task не создавалось.
- У lock-release run дважды перечитаны 6 checksum-verified ready files и
  совпал pinned report SHA. Это проверка сохранённого отчёта, не новое Windows
  исполнение и не отдельный новый semantic audit всего primitive runner.
- Receiver2 ZIP повторно прошёл archive/manifest/source pin verification.
- Python process evidence **3/0**, preparation evidence **5/0**, `cargo fmt`
  и scaffold/local links checks прошли. После изменения документации scaffold
  и diff проверены повторно. Полный workspace и UI build повторять из-за
  одних docs не требовалось; их результаты выше относятся к тому же source.

Closing logs/receipts: `.winbridge/update-preparation/close-state23-20261025-*`.
В этой закрывающей части production код, runtime/config/journal не менялись,
новые Windows команды не отправлялись, cleanup не выполнялся.

## 4. Retained native runs: наблюдать, не отправлять заново

### Main22: receiver до Ready, original lock held

- Task **`f5d9bf8d-983e-4c7b-8bbb-c6147da5e33b`**,
  run **`20261025-021054-05388d1d`**, succeeded/exit0, 112 tests/25 groups/28 commands.
- Producer **a9860f4**, target **1c23d48**. Actual receiver PID4744,
  FILETIME134373690472836061, supervisor PID25192; one process/Job, Ready4.
- Report SHA `8f155ff1b368cedc27daa0b68681094d0814754cec52f9db9fbb651ce407a61b`.
- Proof SHA `be5c15afbf4031d43bcc7892f85ffb6ff3998508cf839712d837971ca4662401`.
- Migration SHA `48ebf3539d8400a536b00950efc8bf72c6f25e86d7938eb92d58370fea68026e`.
- Local pin: `.winbridge/workflow-validation/20261025-021054-05388d1d/activation-preparation-verified.json`.
- Shared: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261025-021054-05388d1d`.
- [Полный отчёт и timings](windows-activation-process-gate-2026-10-24.md).

Безопасная команда **только повторного чтения того же run**, не нового запуска:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T021042Z-a9860f4-677c4a65.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T215731Z-1c23d48-0490a451.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-process --verify-run 20261025-021054-05388d1d
~~~

В этой команде старый target нужен для проверки старого proof. **Для нового
state gate он не годится**: в1c23d48 есть только Ready1, нет handoff2.

### Selective release primitive

- Task **`7799e394-45bc-4f20-82ce-2dc065a27345`**,
  run **`20261025-024054-42fcdb0c`**, succeeded/exit0, producer **da1c7b6**.
- 5 release tests + 2 прежних state-pin tests, 6 ready files twice.
- Report SHA `668720de2be7f1da3c61b45c63d9e220e5e5121eaa1c47d54b5e826bf9e6f4b5`.
- Local pin: `.winbridge/workflow-validation/20261025-024054-42fcdb0c/state-lock-handoff-verified.json`.
- [Описание, pins и ограничения observer](state-lock-handoff.md).

Оба run terminal. Их история не доказывает actor23/interprocess StateOwned.
Повторный submit этих сценариев сейчас не требуется.

## 5. Готовые inputs следующего native gate

**Target receiver2 уже экспортирован clean**, повторно экспортировать его не нужно:

| Поле | Значение |
| --- | --- |
| Source commit | `1370d6b030e18a22266be1b02d24e4371eee01ac` |
| ZIP в shared sources | `winbridge-20261025T092432Z-1370d6b-257867fc.zip` |
| ZIP SHA256 | `579728e29124e3bc914e22f070e3ab5634f68b17d0a0072d74a4387685230656` |
| Manifest SHA256 | `2dc515ce88cb981a633c9b4acd4a1504d4fc2368d088fb73f3373068d3772f16` |
| Размер / состав | 5,750,619 bytes, 790 files, built UI, dirty=false. |

Receipts `.winbridge/update-preparation/activation-state-v2-fixture-{export,pin}.json`.
Это **source fixture**, не Windows execution proof. Её код уже содержит receiver2;
новые validation hooks нужны в producer updater, а не в target fixture.

Следующий **producer** должен быть отдельным clean commit после добавления
harness/verifier/state-boundary hooks. В текущем HEAD до закрывающего docs commit
`6f93f62` такого harness **нет**. Actor source находится в1370d6b;6f93f62 лишь
фиксирует fixture. Текущий closing commit также документационный.

Cache: `20261017-061527-f4639971`, подпапка `test-build-cache`; один exclusive
lease, не параллелить native builds с этим cache.

Migration reference: `.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
Это положительный production17 task `a4f17a0d-4eea-4af2-8f2c-78bc6235c0af`,
producer628c0f6,33/0. Runner делает consistent копию для **17→18/refusal**.
Не заменять reference failed-run audit, не трогать original DB/WAL/images.

## 6. Первый незавершённый шаг: actual Main23 StateOwned gate

Ниже **план следующей реализации**, а не список существующих возможностей.
Сейчас нет ни `--activation-state`, ни `-ActivationStateGate`, ни нового
state-proof verifier, ни соответствующего Rust Main case. Не пытаться просто
запустить такие flags или использовать Main22 как подтверждение handoff.

### Сценарий и timing

1. Добавить отдельный `apps/worker/tests/update_shutdown/activation_state.rs`
   и explicit ignored Main case. Пройти настоящий shutdown → backup →
   preparation → replacement → Prepare20 → Bind21 → Launch22 → Ready →
   **немедленный Handoff23**; одну потерю ACK имитировать без повтора эффекта.
2. Не вставлять тяжёлые file/history/duplicate audits между Ready и handoff.
   В Main22 Ready получен через91.397s после admission, сохранён через93.571s;
   от исходных120s остаётся около26s. Положительный gate должен доказывать
   соблюдение этого окна, а не увеличивать его ради прохождения.
3. Для нового positive mode прежние Creating/Created blocking barriers можно
   пропустить: они уже доказаны отдельным Main22. Не менять прежний gate и
   не утверждать, что его barriers повторно пройдены в новом режиме.

### Наблюдение настоящей OS блокировки

Добавить validation-only **неблокирующие** boundary probes в producer updater;
в production они не участвуют и authority не предоставляют:

- После сохранения HandoffIssued5, до original unlock: свежая попытка взять
  exact lock обязана получить именно lock-busy (WouldBlock/Windows lock
  violation), не произвольную IO ошибку. IDs совпадают.
- После подтверждённого unlock, до отправки V2: свежая попытка взять тот же
  lock обязана пройти. Пробный handle сразу освободить **до** команды receiver.
- Scope-bound markers связывают build/input/current record5/hash/actual parent
  и observed state IDs. Не экспортировать token/nonce или право на startup.
  Не добавлять ожидание управляющего release/дополнительный IPC roundtrip
  в оставшийся бюджет. Fresh authority/deadline checks сохранить.

В `activation_process_validation.rs` можно добавить trace-only input flag с
default=false и пропуском **значения false** при сериализации,
чтобы прежняя сериализованная input shape оставалась неизменной. При refactor
сохранить lifetime прежнего input `artifact::Pinned`; не отпускать pin раньше
завершения old hold. Точное имя и код flag выбрать при реализации.

### После StateOwned и после завершения Job

После receipt6 проверять уже без новых startup grants:

- Те же actual PID/birth/host/session, attempt и один occupied Job; receipt
  связана с Ready/current graph, новый UUID generation отличается от старого.
- View15: original lock=false после подтверждённого release, saved receipt и
  live child показаны раздельно. Внешняя попытка взять lock получает busy,
  пока child жив. Five-file sharing/IDs, DB/WAL bytes/schema/epoch неизменны.
- Ledger содержит ровно реальные семь prefixes0–6; root остаётся
  VerifyingLocal15. Lost ACK/duplicate start не меняют PID/generation/revision
  и не создают повторные release/command/process.
- Views14/13 сохраняют frozen shape и правильную диагностику после release.
- Удерживать настоящий child kernel pin до owned Job stop. После stop handle
  signalled, Job empty, тот же lock/IDs вновь доступны; пробу сразу отпустить.
- Cold view15 сохраняет StateOwned/history, но NotResident и без ресурсов/
  нынешней liveness/автозапуска. Ordinary serve остаётся startup-blocked.

Новый proof может иметь `activationState`/`coldActivationState`. **Не подделывать
old `coldActivationProcess` с Ready4**, если реальная durable current state6.
Положительный тест не заменяет будущую handoff fault/crash matrix.

### Harness, независимый verifier и отправка

1. В `scripts/test-update-health-actor.ps1` добавить explicit state mode, input
   и выбор нового Main test; проверить несовместимые flags до их импликаций.
   Validation updater собрать и заморозить раньше других build invocations.
   Сохранить прежние regression groups, actual17→18/refusal и добавить
   state protocol/mailbox/actor/journal/release tests. Counts читать из logs.
2. Добавить независимый `scripts/update-activation-state-proof.py`: strict
   types/privacy, обе boundary observations, полная причинная история0–6,
   same child/IDs/Job, SHA/context/generation/deadline и warm/cold evidence.
   Можно переиспользовать preparation `verify_scope`, runtime replacement
   audit и canonical helpers. Старый process verifier не ослаблять.
   Mutation audit должен отвергать искажения process/state/record hash/markers/
   generation/history/cold/custody и сериализованные permissions.
3. В `scripts/validate-update-activation-preparation-native.py` добавить
   mutually-exclusive `--activation-state`, собственные prefix/kind и
   same-run `--verify-run`. Request/meta с SHA и fsync сохранять **до ACK**;
   при восстановлении проверять тот же mode/scope/source/target.
4. Проверить локально, сделать review/commit, затем из clean producer:

   ~~~bash
   python3 scripts/export-source.py \
     --output-directory /mnt/hgfs/shared/WinBridge/sources --include-built-ui
   ~~~

5. Перед новым Windows run объявить: **нужен installed worker; desktop не
   нужен; тестовых UI-окон не будет**. Проверить fresh Info и оба диска≥6GiB.
   Запустить один isolated state run с новым producer и target1370d6b.
   Сначала сохранить task/run, затем наблюдать тот же запуск до terminal.
6. При потере observer читать saved metadata/тот же task; не replay unknown
   outcome. Если ACK потерян, recover по исходному verified request/key.
   Если native succeeded, но audit отверг proof, сначала исправить checker и
   readonly проверить тот же run. Новый namespace для обычного test fix —
   только после terminal прежнего task. Рабочий installed worker при
   неожиданном поведении означает остановку зависимых действий и сообщение
   пользователю, а не автоматическое вмешательство в его runtime.

## 7. Карта кода следующему агенту

| Файл | Для чего читать/менять |
| --- | --- |
| `apps/update/src/runtime_replacement/activation_process.rs` | Parent `handoff_state_blocking`/`handoff_state`, HandoffIssued→release→send→receipt6; места validation boundary probes. |
| `apps/update/src/runtime_replacement/activation_process_validation.rs` | Существующие Creating/Created hold hooks; нового trace-only state mode пока нет. |
| `apps/update/src/shutdown/state_handoff.rs`, `shutdown/physical.rs` | Consuming release и failures; `Some(true)` до release, `None` при unconfirmed, `Some(false)` после подтверждённого unlock. |
| `apps/worker/src/activation.rs` | Same-child lock/readonly receipt admission и bounded owner scope; writable activation отсутствует. |
| `apps/update/src/activation/channel.rs`, `journal/activation.rs` | PrivateV2 record binding, snapshot current+shutdown receipt в одной read transaction. |
| `crates/protocol/src/update/activation_receiver.rs`, `activation_state.rs` | Frozen Ready1/новыйV2/state receipt и view15. |
| `apps/update/src/supervisor.rs`, `supervisor/activation_files.rs` | Contract23 dispatch, single blocking owner, duplicates, legacy/warm/cold projections. |
| `apps/worker/tests/update_shutdown.rs` | `GateInput`, `case`, actual child pin, after-owned-exit и ignored Main22; расширить новым state path. |
| `apps/worker/tests/update_shutdown/activation_process.rs` | Старый actual Ready pipeline/barriers/cold helper; сохранить его семантику, не выдавать state6 за Ready4. |
| `apps/worker/tests/update_shutdown/activation_preparation.rs` | Readonly history helper и preparation scope. |
| `scripts/test-update-health-actor.ps1` | Build cache lease, frozen validation updater, groups/Main/proof/export. |
| `scripts/validate-update-activation-preparation-native.py` | Submit/observe/recover, source/target/reference pins и mutation audits. |
| `scripts/update-activation-{preparation,files,process}-proof.py` | Существующие независимые checks для переиспользования; state verifier создать отдельно. |

## 8. Окружение и восстановление сессии

Closing readonly Info, **2026-10-25T09:38:08Z**:

- Installed source `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`,
  buildId `1792486531570951100`, manifest
  `835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241`.
- Generation `8f8d1fb9-e5b1-4984-a17e-d067db46268e`, connected/active0,
  maintenanceError=null, journalRecoveryRequired=null, coverage.complete=true.
- Controller PID3922, parent3921, `ws://192.168.44.203:1143`.
  Session `0d55d6c6-ad2e-4423-b2d8-53326ab7d14a`. Это наблюдение, не обещание
  доступности после reboot. Installed runtime не менялся.

Controller запущен командой:

~~~bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
~~~

IPC `.winbridge/controller-session/controller-local-development.sock`,
log `.winbridge/update-preparation/controller-resume-20261023.log`.
После reboot сначала проверить listener/Info; второй controller не запускать.
Если прежнего процесса нет, запустить с существующим локальным config.
Установленный worker сейчас не требует ручного обновления для isolated gate.
При offline попросить запустить уже установленный worker; desktop не нужен.

Readonly Info без печати config/private request:

~~~bash
python3 - <<'PY'
import importlib.util, json
from pathlib import Path
s = importlib.util.spec_from_file_location('native', 'scripts/validate-workflow-native.py')
n = importlib.util.module_from_spec(s)
s.loader.exec_module(n)
e = Path('.winbridge/controller-session/controller-local-development.sock')
i = n.call(e, {'method': 'info'})['info']
print(json.dumps({k: i.get(k) for k in (
    'build', 'generation', 'activeTaskCount', 'maintenanceError',
    'journalRecoveryRequired', 'recoveryCoverage', 'controllerConnection'
)}, ensure_ascii=False, indent=2))
# Только статус уже известной задачи; submission/private payload не печатать:
# t = n.call(e, {'method': 'get_task', 'taskId': 'TASK_ID'})['task']
# print({k: t.get(k) for k in ('taskId', 'status', 'exitCode')})
PY
~~~

Пути:

- Repo Debian `/home/user1/winbridge`.
- Installed Windows runtime `K:\_proj-26\winbridge\runtime`, source/build
  `K:\_proj-26\winbridge\src`; текущие тесты выполняются отдельно.
- Project root `K:\_proj-26\winbridge\workspaces\snapshot-fixture`.
- Shared `/mnt/hgfs/shared` ↔ `K:\VMs\shared`, namespace `WinBridge`.

Debian free при closing Info: **25,672,343,552 bytes** (>6GiB).
Последняя Windows measurement из native7:
**13,133,565,952 → 13,134,938,112 bytes**, это историческое измерение.
Перед новой heavy build **обе стороны измерить заново** и сохранить в report.
При <6GiB прекратить новые builds; на Debian разрешена очистка только
неиспользуемых воспроизводимых caches при отсутствии активной сборки.
Windows validation directories без отдельного разрешения не удалять.

Для Rust: `PATH=/home/user1/.local/bin:$PATH`, `CARGO_INCREMENTAL=0`, основной
`target`, последовательные Cargo commands. Не удалять binary работающего
controller, не редактировать source во время active Cargo. Перед selected
mailbox test обязательно собрать standalone из того же source:

~~~bash
PATH=/home/user1/.local/bin:$PATH CARGO_INCREMENTAL=0 \
  cargo build -p winbridge-update --features update-validation --locked
~~~

Configs/journals/private requests/ZIPs/binaries/raw logs остаются ignored.
Не коммитить и не печатать токены/nonces/configs; SQLite копировать consistent
backup API. Системные настройки, firewall/services/VMware не менять.
В closing docs task user changes отсутствовали, backup важных файлов не
требовался. Никогда не трогать `/home/user1/backup`.

После завершения проверок **desktop можно закрыть; installed worker оставить
работать**. Новых тестовых UI-окон не создавалось.

## 9. После положительного Main23: порядок оставшейся реализации

1. Review StateOwned native proof и fault boundaries. Только затем спроектировать
   отдельную writable activation transaction/receipt: новая journal authority,
   CAS/epoch, lease/shutdown history и explicit schema policy. Изменение worker
   schema пока **не выбрано/не реализовано**. Нормальный `Store::begin_authority`
   запрещает active update; обходить этот запрет без нового контракта нельзя.
2. Local IPC/service health и новый controller session/generation/build для той
   же операции. Test config с controllerURL=None не подтверждает реальное
   подключение. Old idempotency/history/logical context сохраняются.
3. Передача owned Job/process lifetime до выхода updater, commit/release только
   собственного maintenance после receipts. Не ослаблять глобально
   kill-on-close/DACL/breakaway ради прохождения теста.
4. Remaining U2 fault/crash/rollback/reconciliation matrix, management CLI/MCP/
   status и opt-in policy; последствия неизвестного исхода должны быть видны.
5. **U3**: native desktop save/exit/update, popup для открытого UI и tray-only
   уведомление при закрытом UI; настоящая автоматическая native проверка.
6. **U4**: при необходимости один manual bootstrap capability, затем реальное
   автоматическое обновление installed stable runtime и recovery. Только это
   закрывает пользовательскую цель самообновления. После него вернуться к A4.3.

Сейчас не объявлять updater готовым и не включать public install. Ближайшая
реальная работа — **написать отсутствующий state harness/verifier, затем один
автоматический isolated Windows gate с готовой receiver2 fixture**.
