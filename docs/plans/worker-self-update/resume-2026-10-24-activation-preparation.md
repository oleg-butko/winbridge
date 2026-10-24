# Продолжение updater после закрытия проверок 2026-10-24

**Исторический checkpoint daa88da, до preparation gate/read binding. Актуальная
инструкция — [resume activation files](resume-2026-10-24-activation-files.md).**
Ниже сохранено состояние того commit; его NEXT/«не запускался» не являются
текущими заданиями. Последний whole-set Main21 завершён failed по deadline.


Это основная инструкция для следующей сессии без контекста чата. Она заменяет
старые NEXT/ACTIVE в handoff и прежних resume-документах. Пользователь попросил
завершить начатые проверки, сохранить подробную точку продолжения и сделать
коммит. Полный updater **не завершён**, публичная установка выключена.

## Короткая точка продолжения

Реализованы activation source/intent/startup FSM, updater ledger18 и внутренний
supervisor20/view12 для **подготовки** activation. Следующая работа — объединённый
изолированный Windows gate: actual original-custody preparation плюс миграция
настоящей сохранённой истории17→18 и отказ старого executable. Сценарий этого
нового gate ещё требуется реализовать. Запуск нового worker из заменённого runtime,
передача его state/lock, подтверждение controller и завершение update — впереди.

| Компонент | Текущее состояние |
| --- | --- |
| Updater journal | Schema18; activation intents/current/history и graph reader. |
| Worker journal | Schema16; activation receiver/worker-side handoff receipt отсутствуют. |
| Внутренний supervisor | v20/view12 для PrepareActivation/ObserveActivationPreparation. Предыдущие ответы1–11 сохраняют форму. |
| Production support | Activation/controller_confirmation/desktop_update не объявлены реализованными. Production drain возвращает долговечный Blocked при отсутствии обязательных contracts/drivers. |
| Activation OS driver | Не реализован. Prepared не даёт права create/resume/state-open. |
| Установленный Windows runtime | Старый рабочий d052c6c, не изменялся этими проверками. |
| Native18 gate | Не запускался. Linux и Windows cross-check не заменяют его. |

## Что сделано и почему

1. Production drain17/actor19 проверен на настоящем isolated Main. Решение
   Blocked сохраняется до ACK, не создаёт BeginDrain/shutdown и запрещает обход
   через legacy v13. Actual16→17/refusal также пройден. Две предыдущие попытки
   выявили stack overflow supervisor Main; исправление — отдельный owner thread
   с 8MiB stack и heap boundary. Успешный третий прогон доказывает это исправление.
2. `ActivationSource` связывает только полный FilesMoved22 с installed worker и
   manifest: installation/set, owner/epoch/launch, root revision, config/shutdown/
   backup digests и canonical digest всего replacement. История не даёт custody.
3. `ActivationContext/Intent/Record` задают типизированную причинную цепочку
   Prepared → Creating → Created → ResumeGranted → ReadyForState → HandoffIssued
   → StateOwned → LocalVerified. Подтверждённое отсутствие процесса отделено от
   неизвестного исхода. Поздние факты можно сохранить; поздние новые grants
   запрещены. LocalVerified не означает успешный update целиком.
4. Ledger18 проверяет intent/current/history, полный retained replacement,
   настоящий FilesVerified edge и context из authoritative shutdown, backup,
   health/config/rehearsal и supervisor rows в одной SQLite transaction.
   Windows-only `prepare_activation` требует actual self PID живого original
   supervisor и тот же resident guard. Сохраняется Prepared, root не меняется.
5. Actor20 переносит **тот же Box original guard** в единственный blocking slot и
   возвращает его вместе с результатом, в том числе при обычной ошибке. Durable
   Prepared записывается до ACK. Свежий nonce хранится только в resident non-Serde
   объекте; DTO, journal и отчёты не получают plaintext nonce. Duplicate/cold
   observation возвращает данные и не создаёт новый nonce или право исполнения.
6. Readonly view12 отдельно показывает сохранённый phase и live custody.
   Поздние execution states отображаются Progressed, а не Prepared. Старые
   view1–11 не получают нового поля. Проверки запрещают caller-supplied launch
   material, неверные версии/scope/revision/token и worker-command smuggling.

## Проверки при закрытии

**Все начатые проверки завершены; активных Cargo/native runs нет.**
Новых важных ошибок в закрывающем наборе не обнаружено. Добавлены четыре
regression tests: strict request20, strict readonly view, безопасная projection
startup states и реальный local IPC observe/reject20. Последний при feature
update-validation проверяет обе actions с семью неверными версиями/параметрами;
это Linux IPC, не positive native original-custody admission. Существующий frozen
views test расширен до view12.

Все Cargo команды выполнялись с `PATH=/home/user1/.local/bin:$PATH` и
`CARGO_INCREMENTAL=0`, с основным target. Закрывающие логи находятся в
`.winbridge/update-preparation/`, имеют префикс `activation-close-` и не коммитятся.

| Проверка | Команда / результат | Лог после префикса activation-close- |
| --- | --- | --- |
| Начатый precheck | `cargo check -p winbridge-update -p winbridge-worker -p winbridge-protocol --all-targets --features update-validation --locked` passed до добавления четырёх tests; final all-target Clippy ниже проверяет окончательный Rust. | Отдельный `activation-actor-first-check.log`. |
| Focused activation | `cargo test -p winbridge-worker -p winbridge-update -p winbridge-protocol --features update-validation --locked activation`: **28 passed/0 failed**. Из них23 updater (Source5/FSM10/SQL6/projection2),1 worker IPC и4 прежних journal tests, совпавших с фильтром. | `focused.log` |
| Полный workspace | `cargo test --workspace --features update-validation --locked`: **713 passed/0 failed/16 ignored**. Ignored tests требуют explicit native fixture/reference/helper; не выдавать их за executed. | `workspace.log` |
| Linux Clippy | `cargo clippy -p winbridge-update -p winbridge-worker --all-targets --features update-validation --locked -- -D warnings` passed. | `linux-clippy.log` |
| Windows cross Clippy | Та же команда с `--target x86_64-pc-windows-msvc` passed. Это compile/lint, не выполнение Windows API. | `windows-clippy.log` |
| Protocol / JSON Schema | `cargo test -p winbridge-protocol --features jsonschema --locked`: **53 passed/0 failed**. | `protocol-jsonschema.log` |
| TypeScript contracts | `npm run generate:contracts`, `npm run check:contracts` passed; обновлён `ui/src/runtime/contracts.ts`. | `contracts-generate.log`, `contracts-check.log` |
| UI | `npm run typecheck`, `npm run build:ui` passed; стандартное предупреждение Vite о chunk более500kB не блокирует build. Native desktop/tray не проверялись и не менялись. | `typecheck.log`, `ui-build.log` |
| Формат / документация | `cargo fmt --all -- --check`, `git diff --check`, `npm run check:scaffold` passed; **262 Markdown files** и local links. | `format.log`, `diff.log`, `scaffold.log` |

Машинные сводки без config/tokens:
`.winbridge/update-preparation/activation-close-checks.json` и
`.winbridge/update-preparation/session-close-20261024-activation-preparation.json`.
Первая сохраняет exit codes, команды и измерения диска; вторая — границы проверки,
worker identity и итоговые counts. Это generated local receipts, не часть Git.

Сохранённый native17 gate повторно проверен **readonly**, без новой Windows task:
33 tests/10 groups/12 commands, 27 ready files дважды, 38 Main +19 migration
negative mutations. Сохранённая task снова прочитана как succeeded/exit0.
Лог `activation-close-native17-readonly.log`. Это доказательство для producer17,
**оно не проверяет последующие source/intent/FSM/ledger18/actor20 изменения**.

На закрытии проверены Info рабочего worker: connected, activeTaskCount0,
maintenanceError=null, journalRecoveryRequired=null. Новых Windows окон и
native-прогонов в этой закрывающей сессии не запускалось. Desktop можно закрыть;
установленный worker оставить работать для следующего автоматического gate.

Перед полным workspace набором Debian free25,507,385,344 bytes, после закрывающих
проверок16,544,423,936 bytes (около15.4GiB); shared free28,283,740,160 →
26,875,727,872 bytes. Последнее Windows измерение из завершённого native17 gate:
29,266,731,008→28,588,085,248 bytes; это **историческое** измерение. Перед следующей
Windows сборкой измерить локальный диск заново. Минимум6GiB на обеих сторонах;
cleanup не выполнялся. Windows validation directories не удалять.

## Сохранённые доказательства: ничего не повторять автоматически

Последний успешный native17:

- Task `a4f17a0d-4eea-4af2-8f2c-78bc6235c0af`, run `20261024-085627-3a106020`.
  Terminal succeeded/exit0. Update `101651ca-55b4-4600-b331-9993a07c29fb`.
- Producer `628c0f65ef47b216f3f52006001cba3ecc0480a1`:
  `winbridge-20261024T085529Z-628c0f6-471d2793.zip`, SHA256
  `1ec59abbc9db7e807c0ab45b59f2381ba06052df38bf745ab3df98deaa983eb3`, manifest
  `29d3f513a0966d92c34f723ea94140a73d40667ef80950e2dd5dc8327aac99e0`.
- Target `efccfbe73930763f6925cbd3d26efc15d6713a37`:
  `winbridge-20261024T003539Z-efccfbe-b7a614ff.zip`, SHA256
  `06056b8bee56c7b49c72abc48d24a619324a9d75f4981d767ed2bc1cc62c4a67`, manifest
  `94832d6dfe58dc4268a3dd4b3e77865378a538dbb7762374673822d1d3e7ef84`.
- Report SHA256 `c00c643d1496ff9a9f9db97e0f08de8f06f5ebdb2bbff24ca06759c946c8be66`;
  Main proof `d3e279a686f3c66933dd30ed27409b9fa2fdf33c019a3ef51b8a8ccdf888fc6f`;
  migration proof `03d016e0fbeea4f51ec3987d47cfc56ebf5028826266c4ebebaef42fc854f5c1`.
- Verified pin:
  `.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
  Shared reports: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-085627-3a106020`.
- [Подробный gate](windows-compatible-drain-gate-2026-10-24.md), включая readonly
  команду с `--verify-run`; она не создаёт task. Старые failed namespaces
  `20261024-003723-8bd37767` и `20261024-083044-f6ed4809` не replay/resubmit.
  [Их диагностика](windows-compatible-drain-first-attempt-2026-10-24.md).

Готова **ограниченная candidate18 fixture**, не пакет для установки пользователем:

- Commit `7f6a029c1fd626077adc034a96b54d63da551df5`.
- `/mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T103055Z-7f6a029-3e94038a.zip`.
- SHA256 `4f17e0e0c5f4be899a3e4456b53240c0a9bd9b02b5a5aad2dee458ba29e7a788`;
  manifest `7deaebcf66a9e447169bdd65d8991572bcfb6625719ba5e7945b9552a7fbe86b`.
- 750 files, clean, includes built UI; export pin
  `.winbridge/update-preparation/activation-fixture18-export.json`.
- При закрытии повторно проверены ZIP/manifest SHA256 и bytes/SHA256 всех750 файлов.
  Audit: `.winbridge/update-preparation/activation-close-fixture18-audit.json`.
- Включает ledger18, **не включает actor20**. Для нового gate нужен distinct clean
  producer18 после реализации harness и его commit. Не заменять fixture18 старым
  target17: fresh-health/preparation проверяет текущую schema18.

## Окружение и безопасное возобновление

Debian controller в момент закрытия: PID3922, plain `ws://192.168.44.203:1143`.
Это не service. После reboot PID не актуален. Перед стартом проверить process/socket
и не запускать второй controller. Команда запуска при отсутствии процесса:

~~~bash
./target/debug/winbridge-controller \
  --config .winbridge/controller-snapshot.local.json serve
~~~

Local IPC: `.winbridge/controller-session/controller-local-development.sock`.
Controller log текущего процесса:
`.winbridge/update-preparation/controller-resume-20261023.log`.
Не печатать содержимое runtime config, private test input, control token или
encoded submit body в чат/отчёты. Для readonly Info/get_task можно импортировать
`scripts/validate-workflow-native.py` и вызвать `call(endpoint, request)`.

Installed worker: commit `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`, buildId
`1792486531570951100`, manifest
`835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241`, generation
`8f8d1fb9-e5b1-4984-a17e-d067db46268e`. После restart generation меняется;
identity и состояние всегда перепроверять через Info, не доверять этой записи.

- Installed runtime `K:\_proj-26\winbridge\runtime`, source `K:\_proj-26\winbridge\src`.
- Shared `/mnt/hgfs/shared` ↔ `K:\VMs\shared`; WinBridge source namespace
  `/mnt/hgfs/shared/WinBridge/sources`.
- Изолированный project `snapshot-fixture`:
  `K:\_proj-26\winbridge\workspaces\snapshot-fixture`.
- Retained validation cache run `20261017-061527-f4639971`, `test-build-cache`.
  Только один владелец cache lease/один native build одновременно. Рабочий runtime
  и Windows reports не очищать. CARGO_INCREMENTAL=0, reuse основного Debian target.
- До каждого Windows run сообщить: нужен только installed worker, desktop не нужен,
  UI-окон не будет. Если тест требует окна, заранее описать их вид и период,
  когда их нельзя трогать. После terminal повторить, что можно закрыть.
- Обычные исправления изолированного build/test сценария и fresh retries разрешены.
  Если installed worker требует update или проявляет неожиданное поведение,
  остановить зависимые Windows действия и сообщить конкретную причину.

## Карта кода и следующий конкретный шаг

| Файл | Назначение / где продолжать |
| --- | --- |
| `apps/update/src/activation.rs`, `activation/lifecycle.rs` | Flat binding, transient preparation, data FSM и readonly projection. |
| `apps/update/src/journal/activation.rs`, `journal/activation/tests.rs` | Graph reader, metadata admission, SQL invariants и synthetic17→18 migration. |
| `apps/update/src/supervisor.rs` | Actor20 request/response, blocking slot, original guard и resident nonce; frozen views test. |
| `apps/update/src/journal/supervisor.rs` | View12 в согласованном SQLite snapshot; прежние views1–11. |
| `crates/protocol/src/update/activation_preparation.rs` | Strict readonly DTO; `update.rs` содержит actions. TS генерируется из Rust. |
| `apps/worker/src/update_control.rs` | Реальный local IPC test observe/reject20 без original guard. |
| `apps/worker/src/update_control/validation.rs` | Feature-only actual Main fixture и private control handoff; сюда добавить явный preparation-only input. |
| `apps/worker/tests/update_shutdown.rs`, `update_shutdown/replacement.rs` | Actual replacement Main fixture; extended capture20/duplicate/cold assertions после FilesMoved. |
| `scripts/test-update-health-actor.ps1`, `validate-update-health-actor-native.py` | Существующий runner/observer replacement gate; добавить explicit schema18 mode, сохранив старые readonly modes. |
| `scripts/update-replacement-proof.py`, `update-replacement-faults-proof.py` | Независимые причинные проверки и отрицательные мутации старого replacement proof; расширять отдельным versioned proof. |
| `apps/update/tests/compatible_drain_migration.rs` | Шаблон actual copy-only migration/refusal; новый17→18 test ещё требуется. |
| `scripts/test-update-support-actor.ps1`, `validate-update-support-actor-native.py` | Последний успешный17 gate и readonly observer/reference; не менять ожидаемую schema старого режима на18. |

Проверить имена модулей через `rg --files` перед изменением: здесь нет файла
`validate-update-replacement-native.py`, actual replacement использует health runner.

1. Прочитать README, актуальные tops handoff/architecture/roadmap/ux и этот файл;
   `git status --short`, `git log -6 --oneline`, проверить disk/processes/Info.
   Не перезапускать принятые tasks по observation timeout; сначала get_task.
2. Сделать review actor20/ledger18 вместе. Positive guarded branch пока проверен
   только компиляцией. Особое внимание: возврат guard при ошибке, duplicate до
   admission, lost ACK, отсутствие свежих прав у cold reader.
3. Реализовать explicit preparation-only Main fixture: прежний внутренний
   legacy test route доводит изолированный runtime до FilesMoved22/VerifyingLocal,
   **без production19 decision**. Не отключать production blockers и не добавлять
   activation capability ради теста. В original supervisor вызвать capture20,
   проверить durable Prepared до ACK, duplicate и тот же attempt/nonce digest,
   неизменный root/files/worker state и launch count0. После завершения original
   supervisor cold view не получает nonce/guard; ordinary serve всё ещё blocked.
4. Включить в тот же native gate actual17→18/refusal. Источник — successful17 run
   выше. Создать новую consistent SQLite Backup API копию, сохранить/проверить
   schema17 backup, старые rows/history/keys и новый owner epoch; activation
   rows0, fresh admission из исторического контекста запрещён. Actual старый17 exe
   должен отказаться init/status18 до записи в DB/nonempty WAL. Original reference
   DB/WAL/images/manifest не изменять и не мигрировать на месте.
5. Добавить explicit18 runner/observer mode и independent proof checks; boolean
   passed недостаточно. Проверять identity/paths/source pins, causal revisions,
   history/current и root, saved nonce **digest**, отсутствие launch и cold custody.
   Ready files читать/хэшировать дважды, искажения обязательных полей должны
   отвергаться. Старый17 `--verify-run` должен оставаться работоспособным.
6. Локальные проверки → commit harness → clean producer18 export с built UI:
   `python3 scripts/export-source.py --output-directory /mnt/hgfs/shared/WinBridge/sources --include-built-ui`.
   Новый producer должен отличаться от готовой fixture18. **Готовой команды нового
   native18 run пока нет**: не выдумывать флаг и не запускать старый hard-coded17
   runner с archive18. Сначала реализовать и проверить этот режим.
7. Один изолированный Windows task через существующий установленный worker;
   сохранить run/task/source/target/cache pins до ожидания. Observation timeout
   означает повтор readonly наблюдения того же Task ID, не повтор submit. После
   terminal — independent audit/negative mutations, review, handoff/roadmap/commit.

## После preparation gate: оставшаяся реализация

Следовать [подробному activation плану](runtime-activation.md), затем
[U2–U4](README.md). Последовательность принципиальна:

1. Consuming physical bridge: move handles запрашивают DELETE и несовместимы с
   обычными read pins. Под original guard проверить пять файлов/anchors и получить
   final installed-set pins, не допуская принятия чужого файла по старому пути.
   Exclusive `worker.lock` пока удерживается; нельзя просто освободить всё.
2. Один suspended launch из постоянного runtime: durable Creating до OS call,
   actual PID/birth/session/host и owned Job до resume grant. Unknown outcome не
   перезапускается автоматически. Nonce не передавать в argv/stdout.
3. Private worker activation receiver до journal writer: exact loaded set/config
   и authenticated supervisor, ReadyForState. Обычный serve сохраняет maintenance
   refusal до Store::open. Нужен in-process permit, не CLI bypass.
4. Durable state/instance-lock handoff с подтверждением нового владельца того же
   state/journal/lock, новой generation и reconciliation. При новой worker table —
   worker schema17 и actual16→17/refusal gate; текущая worker schema всё ещё16.
5. Local health/normal IPC, затем настоящая controller session/build confirmation.
   Новый процесс остаётся в maintenance до commit, user pause сохраняется отдельно.
6. Отдельный durable lifetime handoff owned activation Job: успешное завершение
   supervisor не должно убить новый worker. Не менять task Jobs, breakaway policy,
   DACL или системные настройки. Commit/release только после всех receipts.
7. Оставшиеся U2 fault cases и rollback: допустим только до нового journal owner/
   effects и при доказанных preconditions. Затем U3 desktop save/exit/tray/quiet
   hours и migration rehearsal, U4 manual bootstrap и настоящий installed update.
   Только после U4 объявлять автоматическое самообновление готовым и возвращаться
   к отложенному архитектурному A4.3 gate.

Нельзя считать сохранённый Prepared, FilesMoved или LocalVerified разрешением
cold replay. Не ослаблять production capability check, maintenance startup,
schema refusal, owner/epoch/fencing или policy ради продвижения сценария.
