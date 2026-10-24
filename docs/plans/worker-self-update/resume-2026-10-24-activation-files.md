# Продолжение updater: original custody и read binding — 2026-10-24

Это инструкция для следующей сессии без контекста чата. Она заменяет NEXT/ACTIVE
из прежних checkpoint и `resume-2026-10-24-activation-preparation.md`.
Пользователь поручил закончить начатые проверки, подробно сохранить результаты
и порядок дальнейшей реализации, затем сделать коммит. Полное самообновление
ещё не готово; остановка на checkpoint не означает завершение U2–U4.

## Где остановились

Реализована factory удержания **полного проверенного набора новых runtime-файлов**
внутри original supervisor, однако её positive Main21 gate **не пройден**:
94 preliminary tests passed,1 Main failed из-за истечения live preparation
deadline120000ms во время read-pin handoff. В изолированном сценарии старый worker
завершён и файлы заменены; полного Bound/cold proof нет. Новый worker не создаётся,
его журнал не открывается. Установленный рабочий Windows runtime не менялся.

**Первое следующее задание — диагностика/исправление этого gate, затем успешный
новый native run. К launcher/state handoff пока не переходить.**

| Компонент | Реализовано | Ещё не реализовано |
| --- | --- | --- |
| Updater journal | Schema18; activation source/intent/FSM, graph-bound current/history, CAS/readback, actual17→18 migration/refusal. | Подключение execution transitions к реальным процессам и receipts. |
| Worker journal | Schema16; существующая coordination/maintenance, shutdown receipts и ownership fencing. | Private activation receiver и квитанция передачи state новому worker. |
| Supervisor | Internal21/view13; подготовка20 и whole-set read binding21, один blocking slot, resident original guard. | Suspended worker launcher, state handoff, local/controller/lifetime confirmation. |
| Файлы | Manifest-last replacement; one-file move→read native4/0. Whole-set factory/actor реализованы; Main21 failed по deadline, positive proof отсутствует. | Исправление Main21 и положительный whole-set gate; затем допуск однократного запуска. |
| Production capability | Missing-contract/driver решение Blocked до drain; activation/controller_confirmation/desktop_update не объявлены готовыми. | Положительный production update, management CLI/MCP/policy и bootstrap. |
| Desktop | Нынешний desktop не менялся в этом блоке. | U3 update notification/save/exit/background/tray и обновление desktop. |

## Что изменено и зачем

1. Закрыт preparation20 gate на настоящем Main: durable Prepared записывается
   до получения ACK вызывающей стороной; duplicate возвращает тот же attempt.
   Чтение после завершения supervisor видит историю, но не получает его handles
   или nonce. Actual17→18 сохраняет настоящую старую историю и запрещает старому
   executable открывать более новую схему для записи.
2. Исправлен Debian observer: FilesVerified в контракте — tagged object
   `{"kind":"files_verified"}`, а не строка. Windows task уже была succeeded;
   её доказательства проверили повторно readonly, без повторного исполнения.
3. Добавлен consuming `OwnedRuntimeFile::into_read_pin_checked`. Move handle
   содержит DELETE access и мешает открыть обычные строгие runtime read pins.
   Промежуточный read handle сохраняет запрет WRITE; после закрытия DELETE handle
   открывается строгий pin и снова проверяются exact path/File ID/size/SHA256.
   Подмена файла теми же байтами, но другим File ID, отвергается.
4. Ошибка перехода сохраняет оставшиеся kernel resources. До release это
   original move owner; после release — readonly handle, который запрещает WRITE,
   но **может допускать rename/delete**. Последний нельзя назвать успешным strict
   pin или восстановленным move owner. Автоматического повторного adoption нет.
5. `ActivationFilesGuard` потребляет свежую non-Serde preparation и **тот же
   Box original RuntimeReplacementGuard**. Он сохраняет exited-process/state/
   instance custody, anchors и старые displaced handles. Все пять installed
   файлов переходят в read pins, затем полный `PinnedInstalledSet` сравнивается
   с authoritative replacement/source. Ошибка хранит partial/pending resources
   внутри `ActivationFilesFailure`, а не освобождает их ради повтора.
6. Перед переходами проверяются owner/epoch/root/replacement, actual supervisor
   PID, nonce digest и исходный monotonic deadline. JSON или повторный Prepared
   не продлевают окно. Сейчас timeout preparation —120000ms; это действующий
   предел, его нельзя молча обновить при duplicate или cold reopen.
7. `FilesActor` задаёт Idle → Binding → Bound/Failed. ACK Binding означает
   начало volatile resource binding после durable Prepared, а не право запустить
   процесс. View13 показывает live resources отдельно от сохранённого состояния.
   Cold view: NotBound/heldFiles0/originalStateHeld=false. Views1–12 frozen.
8. Native runner получил явный `--activation-files`; отдельный verifier проверяет
   реальный read set, lock, duplicate/cold observations и отвергает испорченные
   proof. Старый preparation-only `--verify-run` продолжает работать.

## Проверки и сохранённые доказательства

Итог нового whole-set прогона и exact pins находятся в
[отдельном gate report](windows-activation-files-gate-2026-10-24.md).
Его статус нужно читать вместе с этим checkpoint; прежние pending-записи
не являются указанием запускать тест повторно.

| Проверка | Результат / область |
| --- | --- |
| Workspace | `cargo test --workspace --features update-validation --locked`:716 passed,0 failed,16 intentionally ignored. Ignored требуют native fixture; это не выполненные тесты. |
| Protocol JSON Schema | `cargo test -p winbridge-protocol --features jsonschema --locked`:55 passed. |
| Linux Clippy | `cargo clippy -p winbridge-update -p winbridge-worker --all-targets --features update-validation --locked -- -D warnings`:passed. |
| MSVC cross Clippy | Та же команда с `--target x86_64-pc-windows-msvc`:passed. Это compile/lint, а не выполнение Windows API. |
| Generated contracts/UI | `npm run generate:contracts`, `npm run check:contracts`, `npm run typecheck`, `npm run build:ui`:passed. |
| Python |5 evidence tests и9 существующих runner tests:passed; legacy87 readonly recheck passed. |
| Новый Windows Main21 |94 preliminary passed /1 Main failed,21 commands;27 ready files twice. Actual17→18 proof отдельно принят с12 negative mutations. Успешного whole-set proof нет. |
| Формат/документация | `cargo fmt --all -- --check`:passed до native export; после checkpoint `git diff --check` и `npm run check:scaffold`:passed,265 Markdown files/local links. Rust в закрывающей задаче не менялся. |

Локальные логи/receipts находятся в `.winbridge/update-preparation/`:
`activation-files-workspace.log`, `activation-files-final-*.log`,
`activation-files-final-checks.json`, `activation-files-legacy18-recheck.log`.
Закрывающий docs log: `activation-files-checkpoint-scaffold-final.log`;
machine summary: `session-close-20261024-activation-files.json` того же каталога.
Они generated/ignored и не входят в Git. Все Cargo проверки выполнялись с
`PATH=/home/user1/.local/bin:$PATH`, `CARGO_INCREMENTAL=0` и основным target.
Native desktop собирается как часть candidate set, но его окна/tray этим gate
не проверяются; никаких тестовых UI окон не требуется.

Прежние terminal runs не resubmit:

| Gate | Run / Task | Что подтверждает |
| --- | --- | --- |
| Production17/actor19 | `20261024-085627-3a106020` / `a4f17a0d-4eea-4af2-8f2c-78bc6235c0af` |33 tests; production Blocked до drain и actual16→17/refusal. [Report](windows-compatible-drain-gate-2026-10-24.md). Источник следующей actual17→18 migration. |
| Preparation18/actor20 | `20261024-111115-9272d0e2` / `c1675a54-b250-4fa9-b2d3-66c653033180` |87 tests/16 groups/18 commands;26 ready files twice,72 Main+12 migration mutations. [Report](windows-activation-preparation-gate-2026-10-24.md). |
| One-file read handoff | `20261024-143036-03cf8cbb` / `ade260bf-de0a-436b-9920-589eb26ee86c` |4 native tests;5 ready files twice; success/failure-before/failure-after/foreign-FileID. [Pins](runtime-activation.md). |
| Whole-set actor21 | `20261024-153856-fe2e4f76` / `01d7bf76-6c53-427c-8cd9-81d8f8798962` |**Failed/exit1**,94 preliminary passed/1 Main failed. Exact hashes, readonly failure audit и дальнейшая диагностика — в [report](windows-activation-files-gate-2026-10-24.md). |

Счётчики `workerLaunchCount0/stateHandoffCount0` в preparation/read-binding proof
описывают scope сценария, а не универсальное наблюдение за процессами Windows.
Будущий execution gate обязан подтвердить настоящий PID/creation time/Job и
receipts; нельзя считать нулевые счётчики доказательством готового launcher.

## Ближайшая задача: исправить deadline failure Main21

Ошибка зафиксирована в `activation_files.rs:60` во время ObserveActivationFiles:
driver вернул `Runtime read-pin handoff stopped: PreconditionsChanged: Activation
preparation deadline expired; no new execution or adoption`. Supervisor log
подтверждает ту же причину. Это не 30-минутный task timeout, не observer mismatch
и не неожиданное поведение installed worker. Точный файл/checkpoint и доли
времени из текущих логов неизвестны. **Без новых измерений причина медленного
перехода не установлена**, известен механизм остановки.

Порядок продолжения:

1. Изучить участок `activation_preparation.rs` между durable record и вызовом
   `activation_files::execute`: observe/duplicate/inspect/history сейчас
   выполняются до Bind. Сохранить доказательство durable-before-ACK и duplicates,
   но оценить перенос тяжёлых наблюдений после live binding.
2. Добавить scoped timings в harness/driver: commit preparation, получение ACK,
   наблюдения, full graph reads, original.require_progress, этапы каждого file
   handoff и оставшийся monotonic budget. Не логировать nonce/token/private input,
   не добавлять поля в frozen view13 ради диагностики.
3. Исправлять измеренный bottleneck. Повторные hashes/graph reads можно менять
   только при сохранении live fencing и exact identity под удержанными handles.
   Если требуется разделить бюджеты подготовки и startup, оформить явный
   контракт с immutable bound deadlines; не обновлять старое окно при duplicate.
   Простое увеличение120000 или отключение проверки не является исправлением.
4. Локальные focused/Clippy/Python/contracts checks, commit исправления и новый
   clean producer. Старый terminal failed task не replay. Fixture7f6a029 и
   successful17 migration reference остаются пригодными для read-binding gate;
   новый namespace/cache lease, тот же explicit `--activation-files`.
5. Требовать complete success:95 tests/19 groups/21 commands, actual Bound/
   strict set/lock/write-delete denial/duplicate/cold proof, ready files twice,
   Main/file/migration negative mutations. Только после review этого результата
   начинать execution блок ниже. Не ожидать фиксированное число ready files,
   если diagnostic logs добавят опубликованные файлы; проверять полный ready set.

## Карта кода

| Файл / модуль | Назначение и место следующего расширения |
| --- | --- |
| `apps/update/src/activation.rs`, `activation/lifecycle.rs` | Flat source/intent, frozen startup FSM и transient preparation/deadline. Данные не дают OS authority. |
| `apps/update/src/journal/activation.rs` | Graph reader/admission; `require_activation_prepared` проверяет fresh resident object и authoritative graph. Здесь связывать новые guarded transitions с receipt. |
| `apps/update/src/runtime_replacement/activation_files.rs` | Whole-set consuming guard/failure. Основа будущей consuming factory запуска; private fields не обходить. |
| `crates/platform/src/runtime_files/read_handoff.rs` | Windows move→read primitive и четыре реальных sharing/identity tests. |
| `crates/platform/src/windows_atomic.rs`, `infrastructure.rs` | Existing process adapters для сравнения. Обычный InfrastructureChild не является suspended activation launcher; kill-on-close task/probe lifecycle нельзя менять глобально. |
| `apps/update/src/supervisor/activation_files.rs` | Typed owner resident Binding/Bound/Failed. Добавлять отдельное состояние/owner для процесса, сохраняя failed custody. |
| `apps/update/src/supervisor.rs` | Internal21, один blocking slot и возврат resources. Сохранить owner thread8MiB и heap boundary после native stack overflow. |
| `apps/update/src/journal/supervisor.rs` | Согласованный readonly snapshot/view13, frozen older views. |
| `crates/protocol/src/update/activation_files.rs`, `update.rs` | Strict DTO/actions. TypeScript в `ui/src/runtime/contracts.ts` генерируется из Rust. |
| `apps/worker/src/main.rs`, worker startup/Store | Будущий private activation entry **до** обычного открытия writer; нынешние startup maintenance guards сохраняются. |
| `apps/worker/src/lib.rs`, `runtime_guard.rs` | Порядок instance/runtime guards и Store open; здесь сохранять loaded-set pins и запускать только после authenticated state handoff. |
| `apps/worker/src/store/ownership.rs`, `store/update.rs`, `store/shutdown.rs` | Существующие fencing/leases/receipts; проверить достаточность для нового state-owner receipt, не прятать новый контракт в произвольный metadata JSON. |
| `apps/worker/tests/update_shutdown/activation_preparation.rs`, `activation_files.rs` | Actual Main original supervisor, duplicates, cold observations, lock/files/DB/WAL. |
| `apps/update/tests/activation_migration.rs` | Actual17→18 copy-only migration/refusal, originals preserved. |
| `scripts/test-update-health-actor.ps1` | Native groups, builds и explicit `-ActivationFilesGate`. |
| `scripts/validate-update-activation-preparation-native.py` | Submission/recovery, immutable source/target/reference/mode pins и readonly `--verify-run`. |
| `scripts/update-activation-files-proof.py`, `update-activation-preparation-proof.py` | Независимые causal/identity/scope проверки и negative mutations. |

Локальные Python команды: `python3 scripts/test-update-activation-preparation-native.py`
и `python3 scripts/test-update-health-native-runner.py`. Это unit tests observers,
не команды отправки Windows задач.

## Первые действия следующего агента

1. Прочитать README, tops handoff/architecture/roadmap/ux, этот checkpoint и
   [activation plan](runtime-activation.md). Проверить `git status --short`,
   `git log -8 --oneline`, процессы/IPC/Info и disk. Не принимать нижние старые
   NEXT/ACTIVE в длинных документах за текущее задание.
2. Подтвердить terminal failed/exit1 сохранённого whole-set task. Readonly команда
   из report тоже вернёт failure; она не повторяет Windows команду. Успешного
   verified pin нет. Выполнить ближайшую задачу диагностики выше и получить
   positive gate в **новом** namespace; никакого повторного submit старого run.
3. После успешного gate review live binding вместе с будущим creation boundary: срок, authority,
   exact set и сохранение resources на всех ошибках. `require_current()` сейчас
   проверяет строго Prepared/revision0; не ослаблять его ради следующих фаз.
   Новая factory должна потребить `ActivationFilesGuard` и вернуть отдельного
   typed process owner с новыми guarded transitions.
4. Только затем первый вертикальный execution блок: durable Creating → **один suspended
   CreateProcess** в fresh owned Job → сохранить actual PID/birth/session/host
   и Job receipt → ResumeGranted → resume. Подтверждённое отсутствие процесса
   отличается от неизвестного исхода. При unknown нельзя создать второго.
   После длительного preflight/I/O повторно проверить deadline и authority
   непосредственно перед новым OS effect, а не только при начале команды.
   Nonce не передавать через command line/stdout или сериализованный grant.
5. Реализовать парный private worker receiver: exact loaded set/config,
   authenticated actual supervisor и nonce через ограниченный private channel;
   ReadyForState **до** writer/dispatch. Обычный `serve` должен продолжать
   отказываться при незавершённом update. Не добавлять CLI `ignore guard`.
6. Следующий блок — durable HandoffIssued и передача **того же instance lock**:
   старый owner освобождает только свой lock, новый доказывает acquisition того
   же file ID и сохраняет receipt нового владельца. Сохранить anchors и old
   exited-process custody насколько позволяют sharing rules. При необходимости
   новой таблицы/контракта — явная worker schema17, actual16→17/refusal и
   обновлённая compatibility policy. Нынешняя политика same-schema-only;
   нельзя незаметно пропустить её ради нового receipt.
7. Проверить local ownership/recovery/resources/normal IPC. Затем подтвердить
   actual новую controller session и scoped generation/build. Доступный TCP
   порт сам по себе не подтверждает успешное обновление.
8. Отдельно реализовать durable lifetime handoff fresh activation Job и readback
   limits, чтобы новый worker пережил успешный выход updater. Не менять общую
   kill-on-close семантику task/probe Jobs, DACL или breakaway policy. Только
   после всех receipts разрешены commit и снятие **своего** maintenance lease;
   независимая user pause сохраняется.

## Как проверять следующие блоки

Использовать существующий установленный worker и отдельные runtime/state/instance
в `snapshot-fixture`, без изменения рабочего runtime. Перед новым run измерить
Debian и Windows disk, получить единственный cache lease. До новой тяжёлой сборки
не менее6GiB на обеих сторонах. Windows validation directories не очищать.

Текущая target fixture `7f6a029` подходит только для read-binding теста: в ней
есть ledger18, **нет нового private activation receiver**. Для execution gate
сначала реализовать receiver и экспортировать clean fixture с built UI, затем
distinct clean producer с driver/harness. Не запускать старую fixture с новым
придуманным CLI. Export после проверок и commit:

~~~bash
python3 scripts/export-source.py \
  --output-directory /mnt/hgfs/shared/WinBridge/sources --include-built-ui
~~~

Положительный gate должен доказывать actual create/Job/resume, отсутствие writer
до handshake, state ownership и preserved tasks/history/keys/settings/user pause,
normal IPC, controller generation и survival после успешного updater exit.
Fault gates: stale owner/epoch/nonce/deadline, changed file/config, receipt write
failure, crash/ACK loss до и после OS effects, busy lock, unknown handoff/lifetime.
Cold reader не восстанавливает grants. Ready files хэшировать дважды, verifier
обязан отвергать отрицательные мутации. При ошибке isolated build/test сценария
исправление и автоматический повтор разрешены после terminal диагностики;
новый запуск получает новый namespace и clean producer.

Нынешние isolated Main configs намеренно имеют controllerURL=None. Настоящее
controller confirmation ещё не проверено. Новый executable path может затронуть
правила firewall; не придумывать relay/обход или менять firewall заранее.
При конкретной проблеме описать факты и необходимое действие пользователя.

## Что остаётся после execution

- Завершить U2 fault matrix, management contract/CLI/MCP/status и opt-in policy.
  Rollback допустим лишь до нового journal owner/effects при доказанных
  preconditions; после неизвестного исхода нет автоматической повторной установки.
- U3: save-before-exit desktop, visible/tray/absent cases, background без flash,
  один tray, уведомления и report link при недоступном worker; migration rehearsal.
- U4: один согласованный ручной bootstrap новой capability; затем настоящий
  автоматический update по стабильным runtime paths без ручной распаковки/rebuild/
  Ctrl+C, с ручной инструкцией восстановления при ошибке.
- Лишь после U4 обновить пользовательский guide/readiness и вернуться к
  отложенному архитектурному installed A4.3 gate. [Общий план](README.md).

## Окружение и правила возобновления

Controller при запуске этого checkpoint: PID3922, не service,
`ws://192.168.44.203:1143`; local IPC
`.winbridge/controller-session/controller-local-development.sock`.
После reboot проверить процесс/socket и не запустить второй controller.
Команда при отсутствии работающего процесса:

~~~bash
./target/debug/winbridge-controller \
  --config .winbridge/controller-snapshot.local.json serve
~~~

Лог `.winbridge/update-preparation/controller-resume-20261023.log`. Не печатать
config, private request/input, control token, nonce или EncodedCommand. Для
readonly Info/get_task доступен `call(endpoint, request)` из
`scripts/validate-workflow-native.py`. Output постраничный: продолжать от
последнего `sequence`, а не считать первый ответ полным tail.

Установленный worker: commit `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`, buildId
`1792486531570951100`, manifest
`835fdbc174813a2401896d98b198891915641d894d7be07e5496b15ab37e5241`, generation
`8f8d1fb9-e5b1-4984-a17e-d067db46268e`. Generation после restart меняется;
каждую сессию перепроверять Info. Исторические PID/build значения не grants.

- Runtime `K:\_proj-26\winbridge\runtime`, source `K:\_proj-26\winbridge\src`.
- `/mnt/hgfs/shared` ↔ `K:\VMs\shared`; менять только разрешённый namespace
  WinBridge. Reports/config/journals и исходные reference данные не удалять.
- Проект `snapshot-fixture`:
  `K:\_proj-26\winbridge\workspaces\snapshot-fixture`.
- Cache run `20261017-061527-f4639971`, `test-build-cache`; один lease/native build.
  В Debian reuse основной target, `CARGO_INCREMENTAL=0`; не удалять бинарник
  работающего controller. Cleanup только unused reproducible Debian build caches.
- Перед каждым Windows run сообщить требуемые worker/desktop и тестовые окна;
  после завершения явно сказать, что можно закрыть и что оставить работать.
- Если установленный worker требует ручного update или проявляет неожиданное
  поведение, остановить зависящие Windows действия и сообщить конкретную причину.
  Обычный isolated test failure не является поводом повторно просить разрешение
  исправить сценарий, но не разрешает системные изменения/ослабление guards.

## Итог закрытия

Все начатые проверки завершены. Native Main21 terminal failed/exit1;94 preliminary
passed/1 Main failed,27 ready files twice, migration audit12 passed. Успешного
whole-set proof нет. Механизм ошибки и диагностика для продолжения записаны;
в этой закрывающей задаче timeout/driver не менялись, новые Windows tasks не
отправлялись. Новый execution блок не начат.

Installed d052/generation прежние, activeTaskCount0, maintenanceError=null,
journalRecoveryRequired=null/recoveryCoverage.complete=true. Observer62989 /
PID123026 завершён; controller PID3922 оставлен работать. Активных native/Cargo
runs нет. UI-окон не было; desktop можно закрыть, установленный worker оставить
работать. Ручное обновление worker сейчас не требуется.

Debian free перед submit16,472,887,296 bytes, после failure audit16,469,987,328
bytes (около15.3GiB). Windows free27,462,344,704→27,239,272,448 bytes. Это
измерения завершённого run; перед следующей сборкой обновить. Cleanup не было.
Рабочий runtime/config/journal не менялись. В Git входят только документация
и ссылки на ignored receipts, не runtime данные или приватные запросы.
