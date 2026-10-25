# Передача контекста updater: native read binding завершён — 2026-10-24

Это инструкция для новой сессии без истории чата. Checkpoint547a487 закрыл
тогдашние проверки по просьбе пользователя. После него активная цель продолжена:
следующий source-блок описан отдельно ниже. Native результаты этого файла
относятся к read binding, а не к новому launcher. **Полное самообновление не готово.**

Этот документ и верх [handoff](../../handoff.md) приоритетнее исторических
NEXT/ACTIVE в длинных документах. Предыдущие неудачные попытки сохранены в
[отчёте Windows gate](windows-activation-files-gate-2026-10-24.md) и Git.
Не повторять их и не начинать заново уже завершённую диагностику deadline.

## Новейшее продолжение: actor22/view14

[Actor/source checks и следующий gate](activation-process-actor.md) приоритетнее
старых NEXT ниже. Launcher подключён к единственному blocking slot, duplicate
не создаёт process, original IO driver сохраняется после retirement owner thread.
Source updater18/worker16, supervisor22/view14. Workspace727/0/protocol57/0/
local+cross+TS/UI passed. **Native Main/harness/verifier process mode ещё впереди**,
прежний97/0 доказывает только read binding. Clean receiver fixture1c23d48 готова;
нужен distinct clean producer с actor и новым harness. Полный updater не готов.

## Продолжение после checkpoint547a487

[Добавлены suspended launcher/private receiver library](activation-receiver.md).
Source теперь содержит OS driver Creating→Created→ResumeGranted→ReadyForState
и worker entry до Store writer. **Actor dispatch добавлен позднее (ссылка выше); actual native execution gate
ещё впереди**; native97 ниже относится только к producer941a2e6/read binding.
Не реализовывать эти modules заново: следующий блок — versioned actor action/view,
новая receiver fixture, distinct producer и настоящий Main до ReadyForState.
Затем original state/local/controller/lifetime. Schemas18/16, supervisor21/view13
и public install off прежние. Проверки новых source изменений — в linked document.

## Где остановились

Проверен переход от заменённых runtime-файлов к удержанию **всего нового набора
пятью строгими read handles в том же original supervisor**. При этом остаются
удержаны прежний instance/state lock, exited-process custody, anchors и старые
displaced files. Новый activation worker ещё не создаётся, writer ему не передан.
Установленный рабочий Windows runtime не менялся.

| Компонент | Реализовано и подтверждено | Что остаётся |
| --- | --- | --- |
| Updater journal | Schema18, source/intent/FSM, graph-bound current/history, CAS/readback; actual17→18 migration и отказ старого executable. | Guarded execution transitions, основанные на настоящих процессах и receipts. |
| Worker journal | Schema16, нынешние maintenance/shutdown/ownership fencing; новый private pre-writer receiver добавлен в source. | Native receiver proof и подтверждение нового владельца прежнего state. |
| Supervisor | Internal21/view13, preparation20/read binding21; suspended OS library добавлена отдельно. | Actor dispatch/native launch proof, state/local/controller/lifetime handoff. |
| Файлы | Manifest-last replacement и native-проверенные one-file и whole-set move→read transitions. | Consuming factory, разрешающая один запуск из этого live guard. |
| Production | Missing-contract/driver решение Blocked до drain; public install выключен. | Положительный production update, management CLI/MCP/status и opt-in policy. |
| Desktop | Существующий desktop не менялся в этом блоке. | U3 update notifications, save/exit/background/tray и обновление desktop. |

`UpdaterDriverContracts::implemented_windows()` по-прежнему не объявляет
`activation`, `controller_confirmation`, `desktop_update` реализованными.
Наличие FSM/истории/пяти read pins не заменяет этих контрактов.

## Закрывающий Windows gate и доказательства

- Producer: **941a2e6125e27d753588988639fd7bb6c08bb90e**.
- Task: **60d8285c-3fbe-45dd-8feb-ef63fe75486b**.
- Run: **20261024-205117-2da937b6**.
- Результат: **succeeded, exit0; 97 tests / 20 groups / 22 commands**.
- Независимый observer завершён exit0. Все30 ready files проверены дважды;
  отвергнуты72 Main +19 file-binding +12 migration отрицательных мутаций.
- Actual17→18 и отказ старого executable открывать18 для записи пройдены на
  consistent копии настоящей прежней истории. Original reference DB/WAL/image
  не изменялись.

На настоящем Main durable Prepared0/history1 сохранён до потребления ACK.
Consuming Bind получил exact five-file PinnedInstalledSet; строгий reader не мог
открыть набор до handoff и смог после. Запись/удаление worker запрещены, original
instance lock удержан, DB/WAL bytes сохранены. Duplicate возвращает тот же attempt;
старые views не получают новое поле. После завершения owned test Job холодный
читатель видит NotBound/heldFiles0/originalStateHeld=false при прежнем Prepared.
Обычный serve отказан до writer. Root VerifyingLocal/revision15 неизменён.

Точный успешный scope: update `2200886e-0e1f-4fe9-80ae-156f1f04b6b4`, activation
attempt `d6c6dc30-8d0a-4fc7-9204-8b4f298a6936`, replacement
`6097c48a-3283-46f2-ab15-0b9a4df95675`, set
`30092f6c-0200-4126-b90a-ad1bf4ba08c6`.
Счётчики launch/handoff0 описывают scope сценария, а не универсальное наблюдение
за всеми Windows процессами: health/rehearsal/startup-refusal probes исполняются.
Будущий execution gate должен доказать actual PID/creation time/Job/receipts.

| Доказательство | SHA256 |
| --- | --- |
| Report | `a47044213dd3eca85148fe5650470d5e3a9137510300aa33f3979d6470b61ae0` |
| Main proof | `85436df722b67d645796f878307062d3ede75cdc485a00d7f635687bf438e8c9` |
| Migration proof | `23fd663dd12affa2451869473045463566368ba2cae11b32369058efd78dadf4` |
| Supervisor timing log | `eedf9e455479e910a487bc628112f462d0689688d3f7a0aa7fbfd53f004ae3b8` |

Local run directory:
`.winbridge/workflow-validation/20261024-205117-2da937b6/`.
Там находятся `activation-preparation-run.json` (metadata до ACK),
`activation-preparation-verified.json` (passed pin, общее имя также для Files mode),
`activation-internal-timings.json` и `activation-files-timing-verified.json`.
Shared: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-205117-2da937b6`;
на Windows тот же путь начинается с `K:\VMs\shared`.
Observer log: `.winbridge/update-preparation/activation-marker-native-main.log`.
Все эти runtime данные ignored, в Git входят описание и pins, а не базы/логи.

### Exact source/target/reference

| Назначение | Archive в `/mnt/hgfs/shared/WinBridge/sources` | ZIP SHA256 / manifest SHA256 |
| --- | --- | --- |
| Producer941a2e6 | `winbridge-20261024T205029Z-941a2e6-57b71ec0.zip` | `cf91e79d0f1bbbf44d83c49b6a33a5adf8773253a5f9bdea59542738e9aa6907` / `77659ef4d7063e40b273da32f9daa9691a9f2fcc3e2a3245e49d481115bbc9a4` |
| Target7f6a029 | `winbridge-20261024T103055Z-7f6a029-3e94038a.zip` | `4f17e0e0c5f4be899a3e4456b53240c0a9bd9b02b5a5aad2dee458ba29e7a788` / `7deaebcf66a9e447169bdd65d8991572bcfb6625719ba5e7945b9552a7fbe86b` |

Cache run `20261017-061527-f4639971`, один exclusive build lease.
Successful schema17 reference:
`.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
Не подменять его отдельным audit failed run: нужен сохранённый positive reference.

При необходимости повторить **только readonly проверку тех же доказательств**:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T205029Z-941a2e6-57b71ec0.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T103055Z-7f6a029-3e94038a.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-files --verify-run 20261024-205117-2da937b6
~~~

Это не новый Windows submit. Не терять `--activation-files`: mode — часть scope.
Для неизвестного/running исхода сначала get_task, затем observe того же run;
observer timeout не означает завершение Windows задачи. Новый execution gate
получает новый namespace, а не replay завершённой задачи.

## Что исправлено после трёх failed attempts

Три прежние задачи terminal failed/exit1; каждая дала94 preliminary passed и
один failed Main по preparation deadline. Их task/run/pins сохранены в gate report:

| Producer | Run | Что выяснили |
| --- | --- | --- |
| 74e8839 | `20261024-153856-fe2e4f76` | Whole-set binding не уложился в исходные120000ms; positive proof нет. |
| 612df2e | `20261024-162445-229e2220` | Повтор после graph/harness изменений; internal trace отсутствовал в actual copied updater. |
| 17c980c | `20261024-200207-c87a1025` | Actual diagnostic updater подключён;140 timings выявили доминирующее время custody marker scans. |

1. Убрано повторное full replacement graph read **в одной SQLite transaction**;
   возвращается тот же validated record, graph/fencing checks сохранены.
2. Тяжёлые duplicate/history audits harness перенесены после Bind; доказательство
   durable-before-ACK и проверки identity сохранены.
3. Добавлены feature-only scalar timings: static phase, elapsed/remaining,
   file/checkpoint index. В них нет token/nonce/private input/path. Production
   остаётся без trace. Timing — диагностика, не authority receipt.
4. Исправлена сборка actual supervisor: `bundled-sqlite,update-validation`, frozen
   create-only run-local `activation-validation-updater.exe`, SHA readback до
   других feature builds. Gate требует exact path и actual timing prefix до Bind.
   Не возвращать копирование обычного cache debug exe: его перезаписывает другой
   build variant и trace снова пропадёт.
5. Exact marker search в streamed binaries заменён с byte-window iterator на
   `memchr::memmem::Finder`, с переносом marker.len−1 bytes между64KiB chunks.
   Весь файл по-прежнему читается до EOF и хэшируется; path/FileID/size и unsupported
   policy сохранены. memchr2.8.3 уже был locked/transitive; добавлена direct edge
   и dev/test opt-level2. Версии зависимостей не обновлялись.
6. После медленного custody/revalidation повторно проверяется исходный deadline.
   Старый trace показал checkpoint/file_bound при remaining0. Новые проверки
   закрывают этот зазор;120000ms не увеличены и не обновляются при duplicate.

Новый run содержит184 timing entries. Actual binding43.346s; remaining budget
109.753→66.407s, bound5. Custody checkpoints20.357s на20 calls плюс initial0.975s;
graph15.856s. Прежний failed run: binding105.960s, custody79.081s на16 calls плюс
initial4.661s, graph14.297s, успел связать4 файла. Это разные по объёму попытки,
нельзя выдавать их за два одинаковых полных workload. Local16MiB marker probe:
912/931ms →110/107ms; native marker regressions также прошли.
Review streaming SHA/marker boundaries, deadline и duplicate/cold scope новых
важных ошибок не выявил. Реальное создание процесса этим review не проверялось.

## Завершённые проверки

| Проверка | Результат |
| --- | --- |
| Workspace, `--features update-validation --locked` |717 passed,0 failed,16 intentionally ignored. Ignored требуют native fixture и не считаются выполненными. |
| Linux Clippy update/worker, all-targets, update-validation, `-D warnings` | Passed. |
| MSVC cross Clippy, те же packages/features/all-targets | Passed; это compilation/lint, не выполнение Windows API. |
| MSVC production Clippy без update-validation | Passed. |
| Marker regression tests |2 passed, включая chunk boundary/full suffix hash и16MiB near-EOF/changed marker. |
| Python observers |5 evidence +9 runner tests passed. |
| Legacy preparation-only87 readonly recheck текущим observer | Passed после расширения suite:26 files twice,72 Main+12 migration mutations; нового submit не было. |
| Новый native Files gate |97/0,30 files twice,72+19+12 mutations, actual17→18/refusal. |
| Формат и документация | Rust fmt прошёл на source producer; final docs-only `git diff --check` и `npm run check:scaffold` прошли,265 Markdown files/local links. |

Протокол/TypeScript/UI в marker fix не менялись; их предыдущие generated/contracts/
TS/UI checks сохранены в [раннем checkpoint подготовки](resume-2026-10-24-activation-preparation.md)
и gate history. Не приписывать их повторный запуск текущему docs-only закрытию.
Local receipts/logs: `.winbridge/update-preparation/activation-marker-*`, включая
`activation-marker-checks.json` и `activation-marker-legacy87-readonly.log`.
Закрывающие receipts в том же каталоге:
`activation-files-passed-checkpoint-scaffold.log` и
`session-close-20261024-activation-files-passed.json`.

## Первые действия следующего агента

1. Прочитать README, актуальные верхи handoff/architecture/roadmap, UX, этот файл
   и [activation plan](runtime-activation.md). Проверить Git status/log, процессы,
   controller IPC, readonly Info и свободное место. Исторические NEXT ниже
   заголовков «История» не выполнять.
2. **Первое новое задание — actor dispatch и actual Main для добавленной launcher/
   receiver library до ReadyForState, без передачи writer.** Read binding закрыт. Изучить
   существующие Windows process/transport/auth APIs перед проектированием channel.
3. `ActivationFilesGuard::require_current()` сейчас строго Prepared/revision0.
   Не ослаблять его для Creating/Created: новая factory потребляет guard и
   возвращает отдельного typed process owner. Failure сохраняет partial resources;
   cold JSON не создаёт новый owner и не продлевает deadline.
4. Сохранить durable Creating **до** OS effect → один suspended CreateProcess в
   fresh owned Job → сохранить actual PID/birth/session/host и Job receipt →
   durable ResumeGranted → resume. Перед новым effect после slow preflight/I/O
   снова проверить authority/deadline. Unknown outcome не равен no-process и
   запрещает второй CreateProcess. Поздний факт сохраняется без нового grant.
5. Private worker entry должен выполняться **до Store writer/dispatch**: exact
   own loaded set/config, actual Created PID, authenticated original supervisor,
   transient nonce с проверкой persisted digest. Nonce не передавать в argv,
   stdout/log или сериализованный grant. IPC design ещё не принят: проверить
   OS peer identity и auth существующего supervisor; нельзя предположить, что
   старый worker token после его exit автоматически годится новому receiver.
   Обычный serve/maintenance guard сохраняется, CLI ignore-guard не добавлять.
6. Только после ReadyForState — следующий блок: durable HandoffIssued, release
   только original instance lock, acquisition **того же FileID** новым worker,
   затем authoritative StateOwned receipt. Anchors/old exited-process custody
   сохранять насколько позволяют sharing rules. Проверить, достаточно ли
   существующего Store.begin_authority; решение ещё не принято. Если нужна
   новая таблица/контракт — явная worker schema17, actual16→17/refusal и новая
   compatibility policy: нынешняя same-schema-only. Не прятать контракт в KV JSON.
7. Затем local ownership/recovery/normal IPC; actual новая controller session,
   generation/build; отдельный durable lifetime handoff activation Job с readback,
   чтобы новый worker пережил успешный exit updater. Не менять глобально task/
   probe kill-on-close, DACL, breakaway. Commit и снятие только своего maintenance
   lease — после всех receipts; независимая user pause сохраняется.

### Карта кода

| Место | Что изучать/расширять |
| --- | --- |
| `apps/update/src/activation/lifecycle.rs` | Prepared→Creating→Created→ResumeGranted→ReadyForState→HandoffIssued→StateOwned→LocalVerified; frozen context и receipts. Сейчас это модель. |
| `apps/update/src/journal/activation.rs` | Authoritative graph reader, CAS/readback, original-custody preparation; здесь связывать trusted execution transitions. |
| `apps/update/src/runtime_replacement/activation_files.rs` | Consuming guard/failure, whole set и original Box; следующая factory должна получить ресурсы отсюда. |
| `crates/platform/src/runtime_files/read_handoff.rs` | Move→read и residual handles при ошибке до/после release. |
| `crates/platform/src/windows_atomic.rs` | Existing suspended native child, Job/handle-list/resume; нужен owned activation adapter. |
| `crates/platform/src/infrastructure.rs` | Сравнить APIs: обычный InfrastructureChild::spawn не suspended, detached не доказывает постоянный Job handoff. |
| `apps/update/src/supervisor/activation_files.rs`, `supervisor.rs` | Typed owner, один blocking slot; сохранить возврат ресурсов, owner thread8MiB и heap boundary после прежних stack overflow. |
| `apps/update/src/journal/supervisor.rs` | Consistent view13, frozen older views. |
| `apps/worker/src/main.rs`, `lib.rs`, `runtime_guard.rs` | Private entry до writer, startup/instance guards и loaded runtime. |
| `apps/worker/src/store/ownership.rs`, `store/update.rs`, `store/shutdown.rs` | Ownership/maintenance/shutdown receipts и будущая передача прежнего state. |
| `crates/protocol/src/update/activation_files.rs`, `update.rs` | Frozen DTO/actions; TypeScript генерируется из Rust. |
| `apps/worker/tests/update_shutdown/activation_preparation.rs`, `activation_files.rs` | Настоящий Main, ACK/duplicate/cold, lock/files/DB/WAL. |
| `apps/update/tests/activation_migration.rs` | Copy-only actual17→18/refusal. |
| `scripts/test-update-health-actor.ps1` | Builds/groups, `-ActivationFilesGate`, frozen diagnostic updater. |
| `scripts/validate-update-activation-preparation-native.py` | Submit/observe/recovery, source/target/reference/mode pins. |
| `scripts/update-activation-files-proof.py`, `update-activation-preparation-proof.py` | Независимые evidence проверки и отрицательные мутации. |
| `apps/update/src/artifact.rs`, `activation/timing.rs` | Exact streamed marker/SHA и feature-only timings; не подменять их cached-success. |

## Как проверять следующие блоки

Текущая target fixture7f6a029 имеет ledger18, **не содержит нового private receiver**.
Для execution gate сначала реализовать receiver/contract и экспортировать clean
fixture с built UI, затем distinct clean producer с launcher/harness. Не запускать
старую fixture с придуманным activation CLI. Export после проверок и commit:

~~~bash
python3 scripts/export-source.py \
  --output-directory /mnt/hgfs/shared/WinBridge/sources --include-built-ui
~~~

Использовать installed worker для запуска отдельного runtime/state/instance
в snapshot-fixture; рабочий runtime не заменять. Каждый native run — новый
namespace/единственный cache lease, disk ≥6GiB до heavy build на обеих сторонах.
Сначала actual create/Job/resume/Ready/no-writer gate, затем state/local/controller/
lifetime gates. Faults: stale owner/epoch/nonce/deadline, changed files/config,
receipt write/ACK failure, crash до/после OS effects, busy lock, unknown handoff.
Проверять preserved tasks/history/keys/settings/user pause и отсутствие duplicate
side effects. Ready set хэшировать дважды, verifier обязан отвергать mutations.

Local команды после соответствующих source изменений:
`cargo test --workspace --features update-validation --locked`, Linux/MSVC Clippy
как выше, `cargo fmt --all -- --check`; Python
`python3 scripts/test-update-activation-preparation-native.py` и
`python3 scripts/test-update-health-native-runner.py`.
При изменении DTO — generated contracts/TS/UI, при schema change — настоящая
migration/refusal. Использовать `PATH=/home/user1/.local/bin:$PATH`,
`CARGO_INCREMENTAL=0`, основной target; не запускать перекрывающиеся Cargo.

Нынешние isolated Main configs имеют controllerURL=None; настоящий controller
confirmation ещё не проверен. Не вводить firewall/relay workaround заранее.
При ordinary isolated build/test failure разрешены исправление и новый run после
terminal диагностики. Если installed worker требует ручной update или ведёт себя
неожиданно — остановить зависимые Windows действия и описать конкретную причину.

## Что остаётся после activation

- U2: завершить fault matrix и rollback boundaries, management contract/CLI/MCP/
  status, opt-in local policy. После unknown external outcome нет автоматического
  повторного install; rollback зависит от доказанного state-owner/effect prefix.
- U3: desktop save-before-exit, visible/tray/absent cases, background без flash,
  один tray, успешные/ошибочные уведомления и report link даже при offline worker;
  migration rehearsal и обновление desktop.
- U4: согласованный один ручной bootstrap новой capability; затем настоящий
  automatic update по стабильным runtime paths без ручной распаковки/rebuild/
  Ctrl+C, с конкретной ручной инструкцией восстановления при неуспехе.
- После U4 обновить user guide/readiness и вернуться к отложенному A4.3.
  [Общий U0–U4 план](README.md). Сейчас не отмечать U2–U4 завершёнными.

## Окружение и возобновление после reboot

Controller при закрытии: PID3922, не service, ws://192.168.44.203:1143;
IPC `.winbridge/controller-session/controller-local-development.sock`, log
`.winbridge/update-preparation/controller-resume-20261023.log`.
PID/session после reboot недействительны: сначала проверить процесс/socket.
Если controller действительно отсутствует, запуск из корня repo:

~~~bash
./target/debug/winbridge-controller \
  --config .winbridge/controller-snapshot.local.json serve
~~~

Не создавать второй listener. Readonly Info/get_task доступны через
`call(endpoint, request)` из `scripts/validate-workflow-native.py`; output читать
страницами от последнего sequence. Не печатать config, private request/input,
control token, nonce или EncodedCommand.

Installed worker: commit `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`, buildId
`1792486531570951100`, manifest
`835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241`, generation
`8f8d1fb9-e5b1-4984-a17e-d067db46268e`. Это наблюдение, не grant; перепроверить Info.
Runtime `K:\_proj-26\winbridge\runtime`, source `K:\_proj-26\winbridge\src`;
project root `K:\_proj-26\winbridge\workspaces\snapshot-fixture`.
`/mnt/hgfs/shared` ↔ `K:\VMs\shared`, только разрешённый namespace WinBridge.

### Диск и выполненная Debian cleanup

Windows free во время native97:20,660,686,848→16,670,916,608 bytes (около15.5GiB).
Debian при финальном readonly закрытии:32,530,321,408 bytes (около30.3GiB).
Это не гарантии на следующую сессию: измерить заново до сборки. Windows validation
directories, reports/config/journals/source archives не удалялись; отдельного
разрешения на Windows cleanup нет.

После завершения всех local Cargo удалены358 unused generated ELF test executables
только из `target/debug/deps`, освобождено22,672,437,248 bytes. Защищены96 current
test paths, root binary inodes и executables работающих процессов; проверены
Cargo-hash filename, `.d` sibling, ELF/nlink1. Libraries/metadata/config/journals/
reports и running controller не удалялись. Receipts:
`.winbridge/update-preparation/activation-marker-cache-prune-plan.json` и
`activation-marker-cache-prune-result.json`. Это воспроизводимые build artifacts;
никакие backups не изменялись. Повторять очистку без новой необходимости не надо.

## Итог для пользователя и следующей сессии

Все начатые native/Cargo/observer проверки завершены; native97 succeeded/exit0,
независимые file/mutation/migration/timing audits пройдены. Закрывающий readonly
Info подтвердил installed d052, connection=connected, activeTaskCount0,
maintenanceError=null, journalRecoveryRequired=null/recoveryCoverage.complete=true.
Installed runtime/config/journal не менялись; ручной update сейчас не требуется.
Controller оставлен работать. UI-окон этот gate не открывал: desktop можно
закрыть, **установленный worker оставить работать** для следующих автоматических
проверок. Перед каждым новым Windows run сообщить worker/desktop/windows условия,
после terminal — что можно закрыть и что оставить.

Следующая сессия начинает с actor dispatch/native gate добавленной launcher/
receiver library, а не с повторной marker диагностики или реализации уже
добавленных modules. Полный updater
остаётся незавершённым до actual state/controller/lifetime, U2 faults и U3/U4.
