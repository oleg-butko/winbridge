[Оглавление документации](README.md) · [Планы, ревью и отчёты завершённых этапов](done/stage0-7/README.md).
Материалы прошлых сессий ниже сохраняют хронологию; инструкции запуска находятся в `validation/`, руководства функций — в `guides/`.

## Начало архитектурного перехода, 2026-10-13

Пользователь поручил объединить три аналитических отчёта, сделать повторное ревью,
commit плана и приступить к реализации. [Единый план A0–A9](plans/architecture-transition/README.md)
и [ревью](plans/architecture-transition/review.md) разрешают расхождение порядка:
durable FSM/ownership сначала, queue нескольких проектов перед полной actor миграцией;
outbox/inbox только при durable receiver. Текущая база кода `a42cd7e`; этапы 0–7
сохраняют завершённый статус. План и supporting reports зафиксированы `d524e68`
**до** начала реализации. [A0 завершён](plans/architecture-transition/baseline.md):
ADR 0005, frozen v1/session/task/schema-1 SQL fixtures, 6 protocol + 8 Store tests.
Независимое acceptance чтение, lost ACK/reopen/frozen fingerprint, conflict,
project scope, intent recovery/cancel, terminal guards и output sequence/cursors
passed. 171 workspace tests, Windows protocol/worker `legacy_baseline` cross-check,
fmt/scaffold (114 docs)/diff checks passed. Нет process/network/UI запуска новых
fixtures: fake paths/PID не используются для OS effects. Все execution regressions
в общем прогоне — Unix adapter. Runtime/DB schema/dependencies не менялись.
Следующий пакет A1: чистая typed Task FSM с прежней v1 проекцией; Windows lifecycle
интеграция требует automatic native gate. В scenario с явными `--test` добавить
оба baseline targets, они пока автоматически входят только в workspace test.
Windows worker/controller в этой сессии не проверялись; прошлые generation/session
ниже не считать подтверждением нынешней связи. A0 не требует Windows обновления.

## Исправления закрытия desktop, 2026-10-10

По сообщению пользователя остальные controls работали, close/hide/quit перестали
отвечать после добавления и удаления проекта; отдельно не работала внутренняя
кнопка «Скрыть» в popup. Воспроизведена ложная dirty configuration из-за JSON
форматирования: friendly undo теперь восстанавливает saved bytes. Отказ закрытия
перенесён к tray, save/native ACK ограничены 10 секундами, поздний save не скрывает
окно. State I/O вынесен из native GUI loop; popup hide/open стали async commands.
Добавлен [локальный JSONL desktop log с ротацией](guides/desktop-diagnostics.md),
неблокирующей очередью и независимой проверкой native dispatch. Во втором native
прогоне `20261010-211212-f58ced85` popup bug воспроизведён: hide ACK успешен, но
окно видно. Win32 SW_SHOWNOACTIVATE не обновляет Tao visibility cache, поэтому
Tauri hide — no-op. Popup show/hide теперь используют парные Win32 операции;
hide проверяет actual visibility, popup крестик и навигация используют тот же helper.
157 workspace Rust tests, UI integration (включая зависший save/native ACK),
TypeScript/build/scaffold/Clippy, desktop cargo check/Clippy и Windows platform
all-targets cross-check проходят. [Native Windows gate](done/stage0-7/stage5/windows-desktop-diagnostics-gate-2026-10-10.md)
passed: 42 Rust tests, два native UI запуска, 17 verified artifacts. Run
`20261010-212047-c983fced`, task `9755a19a-8a79-490b-869d-d9ba739c7641`
succeeded/exit 0; report SHA-256
`defa5e348fb31f071289e7ae985035720d774751e3af2e71e1b3f47a1f6cd4d9`.
Native-tested source clean 3d759b3; два неудачных прогона объяснены в receipt.
ACK barrier включается только отдельным validation scenario после реальной записи;
normal launch его не использует. Working worker clean fa7e4d7, healthy/idle прежней
generation/session; runtime/configs не заменялись. Test windows закрыты, рабочий
desktop можно открыть/закрыть, working worker оставить. Новый ручной gate не требуется;
для применения нужен update runtime.

## Ревью source identity после roadmap, 2026-10-10

[Целевое ревью](done/stage0-7/stage7/review-source-identity-2026-10-10.md): воспроизведён и исправлен
false-clean export при скрытых index changes/изменении байтов во время capture.
Exporter сравнивает actual bytes/path set с закреплённым commit, Git build identity
консервативна при assume-unchanged/skip-worktree. Parent checkout/source path
symlinks/duplicate export manifest отклоняются. 8 Python regressions, 153 workspace
Rust tests, Clippy/Windows protocol all-targets/fmt/parser/scaffold проходят.
Узкий automatic Windows `--identity-only` passed: 5 protocol tests, 2 verified
artifacts. Run `20261010-185249-d913cb3f`, task
`ca1d5c9c-6ea0-4a80-a41e-c96aeee23f70` succeeded/exit 0; report SHA-256
`7ada477b60c10f70ddee31997f2d71700c4ef16e2e1998eeefdc22cb4a05d4e6`.
Native-tested clean ef4dd26 archive:
`K:\VMs\shared\WinBridge\sources\winbridge-20261010T225241Z-ef4dd26-9cc9a527.zip`,
265 files/3 654 320 bytes, SHA-256
`591b18c61016cd850ac635c1605c3699fcbb576cbbb85f6412381ba13bd0551c`.
ZIP/source files/report/log проверены readback; последующий commit меняет только docs.
Этапы 0–7 завершены; новое продуктовое расширение пока не выбрано пользователем.
Working worker clean fa7e4d7, прежней generation/healthy/idle; configs/runtime не
заменялись. Gate завершён без test windows: desktop можно открыть/закрыть,
working worker оставить работать. Controller жив, прежний session/peer; новый
manual gate не требуется. Для обычного обновления использовать archive выше
и prepare-windows-snapshot.ps1; новые runtime configs не нужны.

## Этап 7 завершён — два прикладных проекта, 2026-10-10

Пользователь обновил Windows из winbridge-20261010T100754Z-fa7e4d7-827eab23.zip
и оставил worker, desktop закрыт. После reboot controller запущен заново
с .winbridge/controller-snapshot.local.json (exec 16587), generation
f5e6273b-1e7f-490a-bba0-95b648c278a2, session 3cf5eb0b-99a6-440e-a143-27ca028d8340.
Worker clean fa7e4d7, build ID 1791669115612660300, manifest SHA-256
26b0ebb7371588c40579dadd163172ecc128fb5628c7c5b088fa57919a52a11b,
generation 372266b6-3dee-4d87-9de2-ccab896dbcf8; healthy/idle.

[Ревью и план](done/stage0-7/stage7/review-projects-2026-10-10.md): исправлен чужой parent Git commit
в build identity, неизвестные исходники не объявляются clean. Targeted regression
прошла. [Прикладной сценарий](guides/project-integration.md) completed: isolated Netwatch + WinBridge worker через
общий build profile, без новой transport/queue. [Native gate](done/stage0-7/stage7/windows-projects-gate-2026-10-10.md)
passed: 59 Rust tests, два real application cycles, nested tree stop/private IPC/
indeterminate recovery/coordinator restart, 15 verified artifacts. Run
20261010-182417-737310b6, outer task 3f8a8caa-14cc-464d-b8c3-00c7ab2ade24,
report SHA-256 cd11f064976133f16a8143c53e96c2a3ac372fad548c6e7cf8449970f35f2b16.
Две неудачные подготовки/проверки сохранены и объяснены в gate, мутации с неизвестным
исходом не повторялись. Working worker healthy/idle прежней generation; тестовые
процессы остановлены, desktop можно открывать/закрывать, рабочий worker оставить.

153 workspace Rust tests, Clippy, Windows all-targets/final protocol check, fmt,
DTO/scaffold/TypeScript/build проходят. Native-tested Rust/scripts clean 6e38c55;
после gate добавлены docs и небольшая UI подпись unknown provenance (typecheck/build).
Static interfaces закреплены следующим consumer; отдельный SDK пока не нужен.
**Этапы 0–7 завершены.** Новую функцию выбирать по следующей пользовательской задаче,
а не считать deferred dirty snapshots/source cleanup незавершённым текущим этапом.
Пользовательские configs/user-dirty sources/runtime не меняли; backups не требовались.

Готовый archive с built UI:
`K:\VMs\shared\WinBridge\sources\winbridge-20261010T223803Z-e6e2b2e-5ee765aa.zip`.
Clean e6e2b2e, 263 files/3 647 444 bytes, SHA-256
`7f989e1dec88cd5a42e2a1a66b43fd73a718190785d9446040464cd0e93c39e0`.
ZIP и hashes всех manifest files проверены readback. Rust/runtime/scripts совпадают
с native-tested 6e38c55; добавлены docs и UI unknown provenance text. Для следующего
обычного обновления Windows использовать этот archive и prepare-windows-snapshot.ps1;
повторный ручной gate не требуется. Installed Windows worker остаётся fa7e4d7.

Controller после gate собран clean e6e2b2e, build ID 1791671885109194347,
serve exec 6415, generation 149ba1b3-0a9a-4f1c-882e-bb6e7f6041d1;
session 6011e826-d858-4f9a-97ee-2d7b694586b6, peer 192.168.44.1:14256.
Reconnect прошёл, working worker прежней generation/healthy/idle, activity log
droppedRecords=0/lastError=null. Config/listen/IPC/log rotation прежние.
Последний handoff commit записывает receipt, runtime code не меняет. После reboot
эти IDs считать историей и заново проверять live status.

## Этап 7 — общие профили рецептов, 2026-10-10

[Ревью](done/stage0-7/stage7/review-stage7-2026-10-10.md) выполнено; важная ошибка queued workflow
исправлена: recipe definitions/provenance закрепляются до ACK, изменение recipes
после restart не может незаметно изменить принятый build/start. Legacy queued
recipe workflow без definitions требует нового явного запроса. История/keys
сохраняются, source/cache cleanup не добавлена. README больше не противоречит
готовности этапа 6.

[Первый сценарий](guides/project-profiles.md) реализован: статический core CommandHandler,
modules/runner для обычных tasks и workflow steps, config recipeProfiles, один
профиль на проект, fixed argv с workspaceRoot/projectId bindings. Проекты могут
добавлять локальные recipes; collisions отклоняются. Bindings используют registered
root даже при snapshot cwd; shell/environment evaluation отсутствует. Expanded
Info ограничен 512 KiB. Task origin сохранён атомарно с acceptance. UI показывает
profile/metadata/origin, Settings позволяет выбрать профиль, JournalTimeline общий.

152 full workspace Rust tests, Clippy/Windows all-targets, DTO/TypeScript/build/
scaffold/fmt и full browser integration проходят. [Automatic Windows gate](done/stage0-7/stage7/windows-profiles-gate-2026-10-10.md)
passed: 68 native tests, five checksum-verified artifacts. Run
`20261010-060810-0996d143`, outer task `6ef13e91-8cc2-4766-ac25-b6a6097e01b2`,
report SHA-256 `6f8d1abcaa472a0e20a51b2b364d29a675b3de9dd148651f220e93bd399a6581`.
Первый сценарий этапа 7 завершён; весь этап пока не закрыт. Working Windows worker
clean af90ec0 healthy/idle прежней generation; desktop закрыт. Installed runtime
не заменялся, пользовательских runtime configs/user-dirty files не меняли;
backups не требовались.

Source archive: `K:\VMs\shared\WinBridge\sources\winbridge-20261010T100754Z-fa7e4d7-827eab23.zip`,
clean fa7e4d7, built UI, 253 files/3 619 262 bytes, SHA-256
`422d9f07948f5d4833ef54a9c71a05161c46794d3647daa4cc0ce612fb109f77`.
После native gate меняется только документация; runtime code совпадает с archive.
Для применения новых profiles/output/resume нужен общий update worker/desktop
обычным prepare-windows-snapshot.ps1; повторный ручной gate не требуется.

Controller после gate обновлён до clean fa7e4d7: exec session 18450, generation
`eb0d1da7-2c20-4f21-b8da-e61eae5d9bbc`, session
`91e23903-e429-4637-8125-111b76493bb1`, Windows `192.168.44.1:7798`. Installed
worker generation `c182793f-5391-4bf8-9e70-9c94b8b24a5c` прежняя, connected=true,
maintenanceError/journalRecoveryRequired=null, activeTaskCount=0. Listen/IPC/
activity log/config прежние; source IP filtering не добавлен. После новой сессии
проверить live status заново.

## Закрытие этапа 6 — 2026-10-10

Готовый общий release worker/desktop с built UI:
`K:\VMs\shared\WinBridge\sources\winbridge-20261010T092801Z-ff64ebb-5223a958.zip`.
SHA-256 `d06bfe08d593e829ef196d545475b4272a21d6e2f45a1f243e3e0ed92b736fb1`,
243 files, 3 594 138 bytes, clean ff64ebb. Runtime Rust/UI/scripts code совпадает
с native-tested 38f4e5d; после gate менялась только документация. Последний
handoff commit ниже в истории только записывает этот release, archive не переписывается.
Для установки остановить desktop/worker и выполнить prepare-windows-snapshot.ps1
из распакованных исходников; тесты вручную повторять не требуется.

[Automatic Windows resume gate](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md) прошёл:
39 native tests, exact Debian ready/manifest/blob pin, actual cancel/crash/restart,
explicit resumed import и offline pinned build. ZIP read из shared уменьшился
33 555 078 → 8 389 254 bytes; 24 MiB rehashed из local cache. Observed one-pair
import 17 158,97 → 13 388,86 ms; это descriptive sample, не общий speedup.
Report `workflow-validation/20261010-051707-f5ebc3d7`, task
`9ab94af0-988f-4086-961d-4451cc85e162`, report SHA-256
`1b84f21af9050d83cdd6102b5697d8f39ba17a5fb73f9f34b680fe7ffcfc2cb6`.
Native code clean `38f4e5d`, source archive
`winbridge-20261010T091640Z-38f4e5d-4690e508.zip`, SHA-256
`1646f6fc2f88a6f1fe764084b4303d4c1156eaf63a0a89a9f5ed04341ad4f923`.
Первый failed harness run сохранён и объяснён в gate; не является worker failure.

143 workspace tests, final targeted resume tests, Clippy/fmt, Windows all-targets,
DTO/TypeScript/build и full browser integration проходят. Review устранил
разрастание фаз/events, смешение block count с source files и ошибку cmd junction
в test harness. Native pipe instances теперь unique. Source Git tree был чистым;
незащищённых пользовательских конфигов не меняли, backups не требовались.

**Этапы 1–6 завершены. Следующий этап — 7**: стабилизировать handler/module
interfaces и подключение следующего проекта без копирования transport/journal.
SocketDelivery и further list virtualization не нужны по текущим измерениям;
source-cache cleanup и dirty snapshots — отдельные будущие расширения.

Installed worker остаётся clean af90ec0, generation
`c182793f-5391-4bf8-9e70-9c94b8b24a5c`, healthy/idle; resumeAvailable false.
Для новых output/resume функций нужен общий update worker/desktop из нового
архива, но повторный ручной gate не требуется. Desktop закрыт, только worker
оставлен работать. Tests закрыли свои процессы, working runtime не заменялся.
Debian controller обновлён до clean 38f4e5d (code совпадает с native gate),
exec session 57191, generation `ab58dece-3d55-4a0c-860f-bbb5f104268e`,
connected session `2277c073-9c1e-4678-a124-66f2dd6c1c05`, Windows
`192.168.44.1:6282`. Listen/IPC/config/rotating activity log прежние.
После новой сессии всё проверять заново; эти IDs не использовать как live proof.

## Продолжение 2026-10-10 — snapshot chunk/resume в работе

Связь повторно проверена: installed worker clean af90ec0, generation
`c182793f-5391-4bf8-9e70-9c94b8b24a5c`, healthy/idle; controller прежней
running generation `1a3c0c7c-998a-4d35-8ec2-4572c218eb4e` подключён.
Desktop закрыт, для backend проверки нужен только рабочий worker.

[План resume](done/stage0-7/stage6/snapshot-resume-plan.md) и [контракт](guides/snapshot-resume.md): opt-in
schema 3, manifest-pinned 1 MiB blocks, отдельный local cache и новый явный
import/key после cancel/crash. Прежние key/record/staging сохраняются; automatic
retry/build/cleanup не добавлены. Core/sync/worker/CLI/MCP/UI реализация добавлена;
Linux owned crash/restart, cancelled same-key и offline pinned build проходят.
Automatic native Windows gate ещё не выполнен; этап 6 не закрыт.

# Следующей сессии: продолжение WinBridge

## Текущая сессия — 2026-10-10

Debian перезапущена. Пользователь обновил Windows из
`winbridge-20261009T110529Z-af90ec0-59c18ec0.zip`, закрыл desktop и оставил worker.
Controller запущен: exec session 15995, generation
`1a3c0c7c-998a-4d35-8ec2-4572c218eb4e`, session
`80e475d8-26b3-4c44-866a-f2a6ccaf6ea6`, connected=true.
Read-only Windows Info подтвердил clean `af90ec036e641e630583f8753fa92e8f8fba6f04`,
manifest `a36e9f8329cff23c0760063fc66e1c07670f77ddfcb6a4e7c1b831520bf5b528`,
generation `c182793f-5391-4bf8-9e70-9c94b8b24a5c`, delta available,
activeTaskCount=0, maintenanceError/journalRecoveryRequired=null.

[Ревью](done/stage0-7/stage6/review-stage6-2026-10-10.md) не обнаружило новых важных ошибок delta;
исправлены неограниченный desktop output buffer и склейка UTF-8 через gaps.
Следующий сценарий выбран по baseline 2 MiB output: [batching/virtualization](done/stage0-7/stage6/output-batching-plan.md).
Код реализован; 135 workspace tests, Clippy и Windows all-targets compile прошли.
Browser integration с actual output/lost read ACK/virtualization прошла;
[native gate завершён](done/stage0-7/stage6/windows-output-gate-2026-10-10.md): run
`20261010-042006-6c67052d`, task `ddf23ba0-c6b3-4eb8-ae3d-14511f320a79`, 11 tests,
четыре verified artifacts, report SHA-256
`a73633f445734e78bceb9ee28799c74fec97a5db9db478b30d31472f09c9629c`.
Clean b980ac0 backend сравнён с af90ec0 в отдельных state directories: страницы
17→11, median Read RPC 1960.72→1900.94 ms, hashes совпали. Три пары/одиночные
control samples не доказывают общий pipeline speedup; raw timings сохранены.
Installed generation/source прежние, healthy/activeTaskCount=0. Тестовые runtime
завершены, новых окон не было; рабочий worker оставить запущенным, desktop закрыт.
Этап 6 не закрыт: следующий сценарий — snapshot chunk/resume.
Заключительный UI fix сбрасывает frozen view после output cleanup; дополнительная
browser regression и отказ полного отчёта с gaps прошли. Это UI изменение не
меняет native backend b980ac0; повтор backend gate не требуется.

Архив рабочего обновления с финальным UI:
`/mnt/hgfs/shared/WinBridge/sources/winbridge-20261010T083346Z-1055485-519596ba.zip`,
clean `10554850942dac0c6fb2d888bb3914ce328327ad`, 237 files, built UI,
SHA-256 `50ef318f71056fb1b5755cf7a730e6b196cd7a58e3e39e46c88b124d0fe60110`.
Installed worker остаётся af90ec0; обновление runtime автоматическим gate не
выполнялось. Для нового UI используется обычный `prepare-windows-snapshot.ps1`.
Новых ручных проверок не назначено. Позднейший docs-only commit фиксирует архив;
код и native proof остаются указанными выше.

## Предыдущая сессия — 2026-10-09

Пользователь обновил Windows из `winbridge-20261009T084236Z-58f3c14-eb872418.zip`.
Read-only Info: source `58f3c143d41bf5a2137aad45dc9d7650b4bdde94`, clean,
manifest SHA-256 `5612374373c36a48c6b2cbbae83b4aa89b527494e38ada7a7deabdd02aa88b15`,
generation `41623e38-adb1-4a34-a2c4-3a43d9c1cf2c`, maintenance available/healthy,
activeTaskCount=0. После delta gates controller обновлён до clean `af90ec0`: exec
session 57048, generation `95fd938e-feb3-4854-8299-91adf7c7638f`,
worker session `0731567c-81fe-4813-9c52-7d8443671613`, connected=true.
Прежний controller exec session 55195 завершён штатно; worker generation прежняя.
Пользователь поручил ревью и продолжение. Исправлена обработка неверных nested
JSON types в settings UI; backend maintenance regression проходит. Работа по [плану этапа 6](done/stage0-7/stage6/stage6-plan.md): baseline/Git batch/delta завершены;
следующий сценарий resume/output batching/virtualization выбирается по measurements.
Desktop не требуется; рабочий worker оставить запущенным.

Этап 6 реализует [автоматический baseline](validation/performance-baseline.md): monotonic
publisher timings без изменения ready/RPC, commit fixtures/tree I/O/Windows
import/build/output/cancel через installed worker. [Baseline](done/stage0-7/stage6/performance-baseline-2026-10-09.md)
прошёл: run `20261009T093357Z-0dd53063`, 12 samples/59 operations, report SHA-256
`a6d886b28d8c4ecf50ba0ca80b8c14ecb78722d663d11570f1fd006be56b16cd`.
Installed worker healthy/activeTaskCount=0, generation прежняя. Первая выбранная
оптимизация — Git batch capture (518 files: capture 5.6 s, shared write 0.22 s).
Это baseline до delta. Завершённая delta/local source cache описана ниже; resume/output batching/virtualization ещё впереди.

Git batch capture [завершён](done/stage0-7/stage6/performance-capture-gate-2026-10-09.md): один процесс,
bounded header/body, streaming SHA, exact separator и kill/wait при раннем отказе.
123 workspace tests/Clippy/Windows compile проходят. Девять matched pairs на
исходных commits: capture median 7004.6→515.6 ms, 13.58×, все ready/hashes совпали.
Comparison `20261009T095122Z-095becc1`, report SHA-256
`59043c75788028b202f4dc3fe722bac3201f24e7c1b35e86fe098d2832ffe7c2`.
Новый clean b4d355a publisher прошёл 21 Windows operation, run
`20261009T095310Z-d5a9d432`, report SHA-256
`1b8d7e9d0ca0a7f95430a9cb31988b8cfe16f4113e5d69da4333228317d5ea2c`.
Worker unchanged/healthy/activeTaskCount=0; desktop не нужен, TEST окон не было.
Этап 6 не закрыт: delta/local source cache теперь завершена; дальше resume/output batching/virtualization с сохранением immutable generations/
source verification/cancel/recovery. Общий pipeline speedup не заявляется на
основании одного нового Windows repeat; raw timing variation сохранён в отчётах.

Delta завершена по [согласованному плану](done/stage0-7/stage6/snapshot-delta-plan.md): schema 2,
ZIP changed files, pinned local base, новый immutable target и full tree verification.
Добавлены capability/RPC, CLI/MCP публикация/import, понятный preview/history и
автоматический `--delta-only` gate без desktop. 131 workspace tests, browser delta/lost-ACK integration, Clippy, contracts, UI build и Windows all-targets cross-check проходят. При ревью также исправлена recovered import phase/summary после crash между publication и journal result. [Windows gates завершены](done/stage0-7/stage6/windows-delta-gate-2026-10-09.md): native run `20261009-064716-ffce5995`, 40 tests, report SHA-256 `b34285db8c87586ff7655afb513ceea3c03fbc46bcd441d0b8043e25e1a660a8`; cross-host run `20261009T105909Z-c59b2060`, три fresh one-file commit, report SHA-256 `9cdab83a6c5420f99184fda0039393b684f419a536e3b381d5a0520ad837008b`.
ZIP 2 367 994→193 bytes, Debian publication median 1150.73→414.14 ms; Windows observed import 1834.00→1675.17 ms. Full target hashes/build pins/Unicode output совпали. Cancel parent/child 102.88 ms, attention 52.75 ms; worker Info healthy. Installed worker остаётся `58f3c14`, его runtime не меняется.

Архив для рабочего Windows update:
`/mnt/hgfs/shared/WinBridge/sources/winbridge-20261009T110529Z-af90ec0-59c18ec0.zip`,
SHA-256 `6147aa3c8e568e1cfaeffb60dbb86c4ce9aa0080e21941ff853c55657f16965d`,
clean `af90ec0`, 231 files, built UI. Installed worker пока `58f3c14`; delta
поддерживает новый архив. Для update пользователь штатно завершает desktop/worker,
распаковывает архив и запускает `prepare-windows-snapshot.ps1`. Новый controller
уже запущен; Windows runtime в gates не заменялся. Новых ручных проверок нет.
После gates явно сообщено: TEST runtime завершён, desktop можно оставить закрытым,
рабочий worker оставить запущенным. Позднейший handoff commit меняет только
документирование runtime/archive, код release `af90ec0` остаётся тем же.

Ниже — история реализации и прежних версий до этого обновления.

Debian перезапущена; пользователь обновил Windows из
`winbridge-20261008T112855Z-1982de8-82bb6412.zip`, выполнил
`prepare-windows-snapshot.ps1`, закрыл desktop и оставил worker.
Controller запущен с прежним `.winbridge/controller-snapshot.local.json`,
теперь clean controller source `263ec63`, exec session 55195, generation
`62ebf6ca-f139-4c6b-8480-b94fa208c56b`. Exec session 57959 завершён штатно.
Worker source `1982de8b6a7e9a34e6790d5009c30406413ebf08`, sourceDirty=false,
manifest SHA-256 `491b40038ddcdca97199a30cd21c91c7f574a5b4e554341d48e479633d8614d7`,
generation `750f3499-f4a9-42c6-aa43-13d248abb245`;
session `ef6c0389-e5c0-4968-ac58-55b28820fdd3`, connected=true.
HistoryRead/typed attention доступны; journalRecoveryRequired=null, activeTaskCount=0.
[Ревью](done/stage0-7/stage5/review-stage5-2026-10-09.md) исправило неограниченный readback существующего
diagnostic файла. Проверены сохранение/collision/Unicode и MCP contracts.
Проекты/соединение и output retention остаются работой этапа 5. Пользователь выбрал
ручной restart после config save и включаемую automatic output cleanup;
[план](done/stage0-7/stage5/settings-retention-plan.md) фиксируется до реализации. Рабочий worker оставить запущенным,
desktop для текущего backend ревью не требуется. Installed runtime/config/state
не менять в автоматических тестах. Предыдущие gates не повторять без причины.

[Настройки и output retention](guides/settings-retention.md) реализованы по согласованному
плану: actual config path, active/saved JSON, SHA/generation conflict guard,
create-only previous version и atomic replace/readback, ручной restart; identity/state
закреплены. Четыре maintenance RPC local-only, controller и worker WebSocket
отклоняют их, MCP не объявляет. Schema 6 добавляет durable cleanup keys/results
с consistent v5 backup. Preview/apply до 50 tasks защищает pending attention и
active/indeterminate workflow/app; удаления snapshots/reports/history нет.
Opt-in automatic pass раз в 60 секунд фоновый; failures видны в Info/UI.
117 workspace tests и два добавленных теста batches/controller refusal прошли.
Workspace/desktop Clippy, contracts/typecheck/build также прошли. Actual browser UI проверил config/cleanup
lost ACK, same-key reconciliation, dirty close guard и сохранение task metadata.
Windows all-targets compile и native desktop check прошли.
[Settings/retention native gate](done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md)
успешен: clean source adbda09, run `20261009-042415-34160b98`, task
`56ee9fa9-b87d-4392-84c9-efcfd70a70c6` succeeded/exit 0; report SHA-256
`1bfb9e0e9b59ff58d4a050e4a3ec29b24819b1c50ba723b2209d21ba783de5df`.
36 backend tests, два actual UI запуска, 19 verified artifacts; save оставил
PID 19068, restart только тестового worker применил quota/automatic policy.
Cleanup без ошибок. Installed worker по-прежнему 1982de8, generation прежняя,
healthy/activeTaskCount=0. Пользователю сообщено: TEST окна закрылись, desktop можно
завершить, рабочий worker оставить работать. `--settings-retention-only` — единый
изолированный автоматический сценарий; ручной gate не требуется.
Этап 5 завершён в согласованном scope output retention + сохранение metadata/dedup.
Следующая работа — этап 6, измерения существующих capture/publish/import/build и
размеров output/journal до выбора delta/resume/batching/virtualization.
Финальная browser проверка поймала старый history cursor во время refresh;
page теперь сбрасывается до ответа, pagination disabled. Повтор прошёл с новой
keyboard navigation/config/cleanup checks; новый native gate не требовался.

## Предыдущая сессия — 2026-10-08

Debian перезапущена. Пользователь распаковал clean archive
`winbridge-20261007T103234Z-f7c4878-0ac18efc.zip` и выполнил
`prepare-windows-snapshot.ps1`. Read-only worker info подтвердил Windows source
`f7c4878fa5800bec5d8f4ae2781ee9b5108f0b36`, sourceDirty=false,
manifest SHA-256 `c318705bfd7916f98421dda6c467e4edbcc4bc111834dc83ad7219d4f5fc0663`,
generation `8c5d8c94-697c-4e9f-9cdb-765930ee36f3`, attentionAvailable=true,
workflowAvailable=true, features artifacts/netwatch_health и activeTaskCount=0.
Рабочий runtime уже обновлён; записи о `523fc5b` ниже относятся к прежней сессии.

Controller обновлён до `cc18156` для MCP gate и запущен с прежним
`.winbridge/controller-snapshot.local.json`, текущий exec session 27844,
generation `6928c40a-2c66-4bde-8226-a6b59d2fb789`, `ws://192.168.44.203:1143`.
Status connected=true, session `90a2ff78-83e3-435b-975b-f0d78b0e0947`, lastError=null.
Старый exec session 6831 завершён штатно; это был controller предыдущего прогона;
read-only Info по WebSocket успешен. Старые exec session IDs после reboot не действуют.
Доступность всегда проверять заново. Пользователю сообщено: desktop можно завершить
через «Завершить интерфейс»; крестик скрывает его в tray. Worker оставить запущенным.
Восстановление соединения само по себе не требует повторения пройденных gates.
План workflow/app attention зафиксирован коммитом `aeae684` до реализации:
[workflow-attention-plan](done/stage0-7/stage5/workflow-attention-plan.md). Реализованы typed targets и
additive feed/ACK/capability, schema 5 с consistent v4 backup и сохранением sequence,
единый parent workflow event, отдельные runtime outcomes приложения, quiet stop/cancel.
Terminal record/event атомарны; journal fault блокирует stale reads и admission.
UI открывает точный workflow/application ID вне recent 10. [Ревью и исправления](done/stage0-7/stage5/review-workflow-attention-2026-10-08.md).
89 workspace Rust tests, Clippy и browser integration прошли.
[Windows gate](done/stage0-7/stage5/windows-workflow-attention-gate-2026-10-08.md) завершён:
clean source `3c59351`, run `20261008-060121-324c2ef1`, task
`7e2cc850-e9ae-4205-a12f-ea27d399efb2` succeeded/exit 0, report SHA-256
`f5512e89434690ec036636e6f7f03226d78eee1b4e3f2dadbdc3c13c10a39988`.
10 attention + 9 workflow tests, actual native popup → точный workflow → UI ACK,
desktop exit 0 и все 12 artifacts подтверждены. Cleanup без ошибок; установленный
worker прежний, healthy, activeTaskCount=0. Тестовые окна закрылись. Пользователю
сообщено, что рабочий desktop можно завершить, worker оставить работающим. Для него нужен установленный worker, desktop
необязателен; --workflow-attention-only собирает отдельный заметный тестовый desktop.
Installed runtime/config/state не заменять. Следующие сценарии — история/диагностика, MCP.

Продолжение этапа 5: [ревью](done/stage0-7/stage5/review-stage5-history-2026-10-08.md) исправило stale
application read после child result fault; commit `8f9ee71` фиксирует исправление и
[план истории/диагностики](done/stage0-7/stage5/history-diagnostics-plan.md) до реализации.
[История и task diagnostic](guides/history-diagnostics.md) реализованы: additive HistoryRead,
50-record keyset pages/filter/search, точные старые IDs, keyboard actions, read-only
storage usage и editable redacted preview/save. Дополнительно исправлен незаписанный
import result: fault/lease сохраняются до recovery. 103 workspace Rust tests, Clippy, Windows all-targets compile, standalone desktop
check/Clippy, contracts/typecheck/build и actual browser integration прошли;
[Windows history gate](done/stage0-7/stage5/windows-history-gate-2026-10-08.md) завершён: source a64caf7,
run `20261008-065054-f727f7bb`, task `04cf615b-0510-451e-bfd3-e3f8721cc87a`,
succeeded/exit 0, report SHA-256
`d7f39ff0217399261f629992d1814c7a071dd53ff1330dab74d8684820c868b9`.
13 backend tests, actual history/preview/edit/save/retry UI, 17 verified artifacts.
Cleanup без ошибок; installed worker healthy/activeTaskCount=0, generation прежняя.
Пользователю сообщено, что тестовое окно закрылось, desktop можно завершить, worker оставить. Рабочий runtime остаётся f7c4878; новый сценарий
`--history-diagnostics-only` выполняется в отдельном runtime через установленный worker.
Новые ручные Windows наблюдения не требуются. MCP начинается по [плану](done/stage0-7/stage5/mcp-plan.md),
зафиксированному до реализации коммитом `ab0eae0`; retention/settings остаются.
[MCP adapter](guides/mcp.md) реализован: 30 typed tools с schemas из DTO, bounded stdio
lifecycle и forwarding в existing controller IPC. Четыре contract tests и actual
Unix pipeline/lost ACK/dedup/output/history/diagnostic/client exit/cancel прошли.
107 workspace tests, Clippy, Windows all-targets compile и contracts/typecheck прошли.
[Windows MCP gate](done/stage0-7/stage5/windows-mcp-gate-2026-10-08.md) завершён:
run `20261008-072201-391f141c`, report SHA-256
`e401fa6a6b063090c53eb61bde1c78f47f80c87f662346a2ee51ff795e0af2c1`.
Setup/success tasks succeeded, repeat key сохранил task/PID, client exit сохранил
running/PID и explicit cancel завершился cancelled. Старый worker получил только
поддерживаемые запросы: HistoryRead отказал controller с not transmitted.
Installed worker не заменён, healthy, activeTaskCount=0. Следующая работа — GUI
projects/connection settings и полная retention policy; stage 5 пока не закрыт.
Ручные UI gates для мелких правок не назначать; новые worker/desktop обновлять вместе.

## Завершённый сценарий — 2026-10-07

**Native tray/popup/settings gate пройден автоматически на Windows.**
[Доказательства](done/stage0-7/stage5/windows-companion-gate-2026-10-07.md): clean source `f7c4878`,
run `20261007-063240-8e63081e`, task `f4d7db75-db65-4e15-8a75-7d05bd2e21d1`
succeeded/exit 0, report SHA-256
`5b2fa9985438efcab6006ce73c13a8f2bb96a6e2b2cccf988060e54b2af93c86`.
21 core + 9 attention Windows tests, offline worker/desktop builds, два actual native
UI запуска с exit 0 и все 16 checksum artifacts подтверждены. Реальные tray/hide,
защита случайного close, single instance/reopen, popup без focus, sound API,
точная task page без implicit ACK, rate aggregation/deferred ACK и restart cursor/settings
прошли. Тестовые процессы завершены, рабочий worker `523fc5b`/generation прежние.

[Первый прерванный прогон](done/stage0-7/stage5/windows-companion-attempt-2026-10-07.md) сохранён как история.
По разрешению пользователя исправлены UX тестового окна, args/PID/exit/checkpoints,
CreateNoWindow, local concurrent output drain и publication bytes/SHA из одного массива.
Перед каждым Windows прогоном объяснять необходимые worker/desktop и появление тестовых
окон; после явно сообщать, что можно закрыть. Для автоматического gate нужен рабочий
worker, рабочий desktop необязателен. Пользователю уже сообщено: desktop можно закрыть,
worker оставить работающим. Это правило закреплено также в AGENTS.md.

[Tray companion](guides/companion.md) реализован и проверен; рабочий runtime ещё не обновлён,
новые worker/desktop собрать вместе при следующем согласованном обновлении.
Локально прошли standalone desktop Clippy, 20 core tests, contracts/typecheck/UI build,
PowerShell parser/capture smoke и browser integration с настоящим Unix worker.
Workflow/app attention, history/export/MCP остаются незавершёнными, stage 5 не закрыт.

Новое ревью исправило незаписанный process result: journalRecoveryRequired,
stale GetTask/cancel error, остановка admission/queue и сохранение lease до recovery.
72 workspace tests, Clippy, Windows compile и TypeScript прошли.
[Ревью](done/stage0-7/stage5/review-attention-2026-10-07.md), [следующий сценарий](done/stage0-7/stage5/tray-plan.md).
Installed worker `523fc5b` остаётся здоровым; новые native проверки выполнять в
отдельном runtime. План tray/quiet/sound/close фиксируется перед реализацией.

Пользователь обновил worker/desktop. Read-only `info` подтвердил Windows build
`523fc5b`, sourceDirty=false, generation `bff7f8da-af71-432b-83af-739a020d9e37`,
features artifacts/netwatch_health и connected controller. Controller generation
`381e409f-8949-413c-a439-7d0622159614`, session `7fa9c4d3-eae9-42d1-8d83-966e2d79bd13`.
Предыдущие записи runtime ниже — история; доступность всегда проверять заново.

Начинается [этап 5](done/stage0-7/stage5/stage5-plan.md). Замечание пользователя: Base64 `-EncodedCommand`
не объясняет действие. Первым исправлением будет decoded PowerShell script с пояснением
и отдельными точными аргументами. Затем durable attention/acknowledgement, tray/settings,
history/export и MCP. План фиксируется до реализации по инструкции пользователя.
При вопросах или непредвиденном поведении остановиться и уточнить; новые мелкие ручные
Windows gates не назначать.

Первый сценарий этапа 5 реализован: decoded PowerShell с исходными argv отдельно,
worker attention journal (schema 4, consistent v3 migration backup), task outcome
attention и explicit request/dedup/acknowledgement, paged list, capability guards,
CLI и UI task links/explicit confirmation. [Контракт и границы](guides/attention.md).
71 workspace Rust tests, Clippy -D warnings, Windows all-targets compile,
TypeScript/UI build, standalone desktop check и browser integration прошли, включая
generation cursor reset. [Windows attention-only прогон](done/stage0-7/stage5/windows-attention-gate-2026-10-07.md)
успешен: 7 native tests, task `719111f4-cdfd-4ac4-8926-f6bc548eeed2`, report
`workflow-validation/20261007-044046-eebbd202`, SHA-256
`006784bf39af25640bed803eee08d27668ee2855427e0d6df758605e7c000f3f`.
Source `6b855f6` clean, ZIP SHA-256
`27e7510dbba8247d076fdc5bab4b2c7eda2eb3f7e3ecb197eeddcd8265bd800b`.
Final UI decoder принимает то же Base64 padding/trailing-bit поведение, что .NET;
Rust код после native proof не менялся. Основной worker здоров, generation/session прежние.
Stage 5 не закрыт: workflow/app attention, history/export/MCP ещё предстоят;
tray/popup/sound/settings прошли gate, описанный в текущем шаге выше. Installed runtime остаётся `523fc5b`; первый срез не требует нового
ручного gate. Следующий рабочий сценарий — workflow/app outcome signals, затем диагностика/MCP. Не просить
обновлять runtime по каждой небольшой правке; готовые изменения объединить.

## Сначала прочитать

Корневой AGENTS.md → README → architecture → roadmap → ux → shared-folders → ADR. Затем `git status --short` и текущую таблицу готовности. Первоначальный proposal в context сохранён для истории; его TLS и общий workspace с Netwatch отменены.

## Цель и явные решения пользователя

Пользователь хочет гибкий локальный каркас Debian VM ↔ Windows host. ИИ агент в Debian должен запускать команды в Windows папке проекта, доставлять последний commit/снимок, собирать, запускать, читать логи и запрашивать popup/звук. Все действия должны быть понятны пользователю в Windows UI.

Принято: имя WinBridge; отдельный `~/winbridge` с Git; нет encrypted connections; shared folder для доставки готовых снимков, отдельный Windows workspace для сборки; recipes + произвольные команды внутри зарегистрированного project. Не возвращаться к этим вопросам без новых требований. Разрешённые рутинные действия в проекте не требуют отдельного подтверждения каждого command.

## Фактическая готовность

| Часть | Состояние |
| --- | --- |
| Git repository и структура | Созданы; история начинается с commit подготовки. Точный hash смотреть в git log. |
| Rust protocol/core/controller/worker | Worker runtime/CLI и platform/transport crates прошли Windows gate; worker capabilities: task_submit/task_cancel, а workspace_import объявляется при delivery config; native snapshot runtime gate пройден 2026-10-06. Controller CLI submit/get/list/output/cancel/watch реально проверен с Windows worker; runtime и native UI gate этапа 2 пройдены, этап завершён. |
| Stage 7 runner/profiles | Static CommandHandler и общий resolver, reusable recipes, bounded metadata, durable task origin/workflow pins, общий UI timeline. Первый gate: [68 native tests](done/stage0-7/stage7/windows-profiles-gate-2026-10-10.md) + browser. Второй: [реальные Netwatch/WinBridge cycles](done/stage0-7/stage7/windows-projects-gate-2026-10-10.md), 59 native tests/15 artifacts, private IPC/nested stop/recovery/history; 153 workspace tests. Этап завершён, SDK не нужен текущим consumer. |
| DTO и config | Strict schemaVersion 1 loader, registry/recipes/direct/cwd/timeout policy. LocalRequest/Response, TaskRecord/Summary, output byte pages. TS генерируется из Rust; check:contracts проверяет drift. |
| UI | Пять native Windows шагов подтверждены пользователем. Новый fix формы сохраняет executable/argv/cwd/timeout/recipe по worker/project/root. Browser остаётся demo/report viewer; UI integration проверен на настоящем Unix worker. |
| Tauri desktop | Native host запуск подтверждён. workerConfigPath в winbridge-desktop.local.json, форма в winbridge-desktop-state.local.json рядом с exe. worker_rpc, четыре local draft commands и desktop_build; close ждёт save ACK каждой изменённой command/delivery/workflow scope. Shared package select и delivery form restore проверены UI integration; пользователь принял их без нового подробного ручного gate. Сохранение формы и dev:ui дополнительно подтверждены пользователем на Windows. |
| Local IPC/runtime tasks | User SID + instance named pipe, user-only protected DACL, exclusive journal и Windows Job Objects подтверждены Windows отчётом. Unix socket/private state также проверены; bounded frames/deadline/connections. |
| HGFS delivery/import | Git commit capture, deterministic Stored ZIP, actual HGFS .part/readback/ready, durable worker import/lease/pinned build и UI реализованы. 16 actual Unix/HGFS/WebSocket gate checks и actual owned import crash/recovery проходят. 19 actual Windows import/build/run/cancel/NTFS junction checks пройдены 2026-10-06; delivery UX принята пользователем, подробная ручная проверка отложена. Dirty source mode ещё не реализован; stage-6 baseline/Git batch/delta/local source cache/output batching/virtualization и [pinned chunk resume](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md) завершены с automatic native gates. |
| SQLite/process tree | Local WAL/FULL, durable acceptance, dedup, queue/cancel, launch intent/recovery, independent pipe drain/quota. Реальные Windows tree timeout/cancel/crash cleanup и сохранённый indeterminate без retry подтверждены. |
| Task output | Bounded byte/JSON/row pages с прежним cursor; incremental Unicode tails 65 536 characters/stream, gap markers и не более 28 DOM rows. [11 native Windows tests и matched IPC samples](done/stage0-7/stage6/windows-output-gate-2026-10-10.md), browser lost read ACK/filter/pause/loaded bytes проходят. [Delivery resume](done/stage0-7/stage6/windows-resume-gate-2026-10-10.md) прошёл automatic gate. |
| WebSocket | Versioned handshake/RPC/heartbeat/reconnect/watch реализованы; 14 session groups и UI integration проходят в Debian. Шесть групп Windows runtime gate прошли, включая прежний task/PID/generation после offline. Подключение и все три новых native UI наблюдения подтверждены пользователем; этап 2 завершён. |
| Журнал controller | Входящие IP не фильтруются (решение пользователя 2026-10-05). activity.jsonl в stateDirectory, 1 MiB + пять архивов, bounded writer; droppedRecords/lastError видны в CLI status. Legacy allowedWorkerAddresses игнорируется. |
| Workflow/process/log | Реализованы: durable import/build/start/stdout readiness/log, SQLite workflows/applications (schema 3), текущая schema 6 с typed attention/cleanup; consistent upgrade backup, app registry/explicit stop, operation-owned leases, byte cursor rotate/truncate/prefix rewrite. UI steps/timeline/task links/profile restore/log. Actual Windows automatic regression; подробная native UI проверка отложена. См. [workflows](guides/workflows.md). |
| Netwatch profile | Named profiles/drafts, output filters, artifact catalog и build identity реализованы. Этап 4 завершён по общему Windows Netwatch прогону; native визуальная проверка отложена. |
| History/diagnostic/MCP | Mixed 50-record keyset pages, filters/search, exact old IDs, keyboard palette, editable redacted create-only reports и read-only storage usage. History native Windows gate: 13 tests + actual UI + 17 artifacts. MCP: 30 typed tools над controller IPC; actual Unix lost ACK и Windows submit/dedup/client exit/cancel подтверждены. [History](guides/history-diagnostics.md), [MCP](guides/mcp.md). |
| Attention | Durable task outcome/request attention, dedup/acknowledgement, schema 5 + consistent v3/v4 backups, capability guards, CLI и paged UI/typed targets. Tray/popup/sound/settings прошли автоматический native gate с двумя UI запусками. Workflow/app attention прошли автоматический Windows gate: 19 tests и native popup/target/ACK. [Journal](guides/attention.md), [companion](guides/companion.md). |
| Settings/output retention | Local-only active/saved config, checked atomic save/manual restart; schema 6 cleanup preview/apply/durable results, protected targets, opt-in automatic policy. 36 backend Windows tests, два actual UI запуска и 19 verified artifacts; [gate](done/stage0-7/stage5/windows-settings-retention-gate-2026-10-09.md). Этап 5 завершён. |
| Родительский Netwatch | Исправления закоммичены отдельно: 39409e5. Его working tree после commit чистый. |

**Этап 1 завершён 2026-10-05.** [Доказательства](done/stage0-7/stage1/windows-gate-2026-10-05.md): прогон `20261005-161827-c8deef84`, все 10 checks passed, ipcAclVerified true; пять native UI шагов подтверждены пользователем, включая отдельно уточнённый stop/restart worker. JSON uiVerified остаётся false, потому что ручные шаги не оцениваются скриптом; shared report не менялся. Worker --describe возвращает scaffold false и task_submit/task_cancel, в коммите этапа 1 controller оставался scaffold. Теперь controller реализует task_submit/task_cancel через worker; readiness этапа 2 требует отдельного Windows gate.

## Среда и полезные факты

- Debian в VMware (`systemd-detect-virt` → vmware).
- `/mnt/hgfs` — fuse.vmhgfs-fuse; `/mnt/hgfs/shared` существует. Пользовательский mapping на Windows: `K:\VMs\shared`. Native Windows validation reports реально записаны и прочитаны в Debian. Windows workspace пользователя: `K:\_proj-26\winbridge\src`.
- Выбран существующий VMnet8: Debian 192.168.44.203, Windows 192.168.44.1 (подтверждено пользователем). Порт WinBridge 1143; настройки сети не изменялись.
- Имеются Rust stable и Windows MSVC target для cargo check. Windows worker теперь доступен через controller по ws://; registry — только fixture. Он позволяет настоящие Windows команды внутри зарегистрированного project. Глобальные настройки Windows/VMware/firewall не менялись агентом.
- Rust wrappers: `/home/user1/.local/bin`; Node/npm: `/home/user1/.nvm/versions/node/v24.19.0/bin`. В новом workdir они могут отсутствовать в PATH.
- Рабочие repos: `/home/user1/winbridge` и `/home/user1/netwatch`. Не изменять Netwatch для реализации WinBridge и не добавлять path dependency на него.
- Git identity не была настроена глобально. Commits использовали временные `-c user.name=funame -c user.email=user1@debf3.local`, взятые из прежней истории Netwatch; глобальный config не менялся.
- Снапшоты перед переносом и exact restore commands: context/migration-recovery.md. UTC дата снимков 2026-10-05 соответствует локальному вечеру 2026-10-04.
- Отчёты: validation.md и windows-validation.md. Реальные Unix processes/IPC/journal и UI adapter проверены; screenshot local-worker-linux.png относится к Linux adapter.
- Runtime CLI: `--config PATH validate|serve|info|submit|get|list|output|cancel|import|imports|get-import|cancel-import|preview`; controller также publish. `output TASK_ID [AFTER_SEQUENCE]` возвращает bounded page: до 1024 chunks / target 192 KiB raw / conservative 900 KiB encoded, list — последние 10 summary. Desktop читает тот же journal через IPC, не открывает SQLite.
- На этапе 1 recipes имеют фиксированный argv: дополнительные recipe.arguments отклоняются; direct mode принимает массив строк. Один мутирующий процесс на project. maxOutputBytesPerTask по умолчанию 16 MiB; gap отмечается, pipes продолжают drain.
- Rust optional feature `typescript` только для generation. `ui/src/runtime/contracts.ts` — generated file, не править вручную.

## Workflow сессия 2026-10-06

Review fixes закоммичены `700ddd5`: bounded config/desktop state и journal link/reparse guards. Основной цикл этапа 4 реализован и проверен: [контракт](guides/workflows.md), [37 native Windows tests](done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md), [автоматический сценарий](validation/windows-workflow-validation.md). Report run `20261006-132034-a57beb0d`, task `fc247059-8df0-4005-9253-377d328a0fac`, SHA-256 `914c7e54e90ddac912079d6a543c4cc0cc5c0a257f7aaf3c19eea79cba8ba46a`. 59 Unix tests, 15 session groups, actual UI integration, Windows compile/Clippy/contracts/build/standalone desktop проходят. Подробная native UI проверка отложена пользователем; не назначать новый ручной gate для мелких изменений.

Пользователь обновил рабочий Windows runtime выпуском `652635b` через `prepare-windows-snapshot.ps1`; новая generation `210bd4b6-e35f-48e1-85a6-81c973c79605`, workflowAvailable=true, соединение подтверждено. Controller generation `ca6e5e43-3739-4f2a-819c-11e8e8ce63e0`, exec session 9142; доступность проверять status. Не повторять прежний gate; для новых stage-4 features требуется совместимый выпуск. По новой инструкции пользователя при неожиданной ошибке worker остановиться и запросить update. 37 native tests выполнены ранее в isolated source/cache/state через старый worker; дополнительно установленный runtime прошёл actual WebSocket import/build/start/readiness/log/dedup/stop без ручной проверки пользователя. Shared report `workflow-validation/installed-20261006-155439-827cbbac`, SHA-256 `466334303cfa8a2e28483d79a3b6e87c85341f293ad661647d246b6fd4babb8b`; [IDs и доказательства](done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md). Тестовое приложение остановлено, worker оставлен работающим.

Committed fixture `s-e0ee0e50ee6c6de17a487199ca09689dec8f6abd`, manifest `474f1188ec53f7b0492e6a20ec8e5c17295c7c281cdc148706e904547cfb0cb1`; frozen request `.winbridge/workflow-fixture/20261006-131804-d59a5488/workflow.request.json` уже выполнен как workflow `be3004e6-8056-4e50-a5e5-58a41fde2c75`. Повтор key возвращает историю, не запускает приложение снова. `prepare-workflow.py` создаёт новый fixture/config/profile отдельно при необходимости нового цикла. Новые ручные UI checks по мелким исправлениям не назначать.

Этап 4 завершён: 41 native Windows test, native desktop check и отдельный Netwatch validation clone прошли автоматически. Production Netwatch не запускать. Profiles/output filters/artifacts/build identity реализованы; подробнее в последнем разделе. Повторять прежние gates и назначать ручную проверку пользователю не нужно.

## Проверки текущей подготовки

```bash
export PATH=/home/user1/.nvm/versions/node/v24.19.0/bin:/home/user1/.local/bin:$PATH
cargo test --workspace --locked
cargo check --workspace --target x86_64-pc-windows-msvc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo run -p winbridge-controller --locked -- --describe
cargo run -p winbridge-worker --locked -- --help
npm ci
npm run check:scaffold
npm run check:contracts
npm run typecheck
npm run build:ui
python3 scripts/validate-local.py
python3 scripts/validate-session.py
npm run test:ui
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked
```

`validate-local.py` и `test:ui` требуют предварительный `cargo build --workspace --locked`; browser test требует Chromium/Playwright browser с sandbox. Обычный Rust workspace не требует GUI SDK; standalone desktop check в Debian требует GTK/WebKit (они доступны здесь). Windows desktop cross-check остановился на отсутствии llvm-rc; глобальные toolchains не менялись. На Windows `validate-windows.ps1` использует bundled-sqlite feature и проверяет настоящие Win32 primitives. Unix crash cleanup в script явно выполняется самим harness: это не доказательство Windows kill-on-job-close. Browser demo запускается `npm run dev:ui`; можно открыть настоящий сохранённый TaskReport отдельно от fixtures.

## План прежней сессии — 2026-10-06

Этап 7 завершён; актуальная готовность описана в начале. Ниже — план прежней сессии.
Ниже сохранён исторический план этапа 4.

Текущий рабочий этап — этап 4. Этап 3 закрыт по решению пользователя 2026-10-06; новые UI исправления приняты, подробная ручная проверка отложена. Этап 2 завершён: [Windows runtime и native UI gate](done/stage0-7/stage2/windows-session-gate-2026-10-05.md) пройдены. Пользователь подтвердил задачи из Debian, потерю связи при продолжающейся задаче, новый session с прежним Task ID после reconnect и отмену. 14 групп настоящих session checks, 29 Rust tests, UI integration, Windows all-targets compile и standalone Linux desktop check проходят. [Ревью](done/stage0-7/stage2/review-stage2-2026-10-05.md) исправило cleanup прерванной сессии, oversized requests, отражение входящего JSON, Welcome capabilities и handshake budget. Адреса пары подтверждены; не задавать вопрос повторно. IP Windows не блокируется.

[Snapshot runtime](reference/snapshot-format.md) реализован. Protocol: manifest/ready/import/preview/ref DTO и optional TaskSubmission.snapshotId; generated TS. Module workspace-sync: точный Git root/commit (без replacement objects), Stored ZIP, .part files/readback/ready-last на HGFS, bounded ZIP parser, actual hashes, local immutable generation, filesystem link/reparse/hardlink guards. Worker SQLite schema 2, consistent v1 migration backup, durable import dedup/cancel/recovery и одна lease для imports/commands. TaskRecord сохраняет accepted snapshot/commit/manifest hash; под lease перед запуском сверяются этот hash и всё дерево, включая unexpected files. Caches/output должны находиться вне snapshots. Native UI: preview, mapping, phases/progress/history/cancel и сохранённый snapshot field.

44 Rust tests, Clippy/fmt, Windows all-targets compile, contracts/TypeScript/UI build, 14 существующих session groups, новые UI checks и standalone Linux desktop check проходят. Реальный HGFS gate `debian-20261005-183623-f32e6b76`: 16 checks, platform linux, passed true, пять ImportRecord и два TaskRecord. Report SHA-256 `bca344044caa6fcad6d99bab6701ed035e12d134090ce1117cb82df4d20491dc`; в shared `WinBridge/snapshot-validation`, report.ready.json содержит checksums. Actual owned crash/recovery: `.winbridge/sg/c07bd640/crash-report.json`, killed during copy_payload, тот же Import ID `d34f270a-40dd-4bd3-8140-511edc6a5926` recovered interrupted, no final source и no auto build. Прогон выполнен после bound-manifest fix.

Найдено actual HGFS ограничение: rename непустого package directory возвращает ENOENT. Исправлено на per-file .part/readback/rename и ready last. Два первых .part directory оставлены как evidence, не удалять автоматически. Shared ready package исходного fixture: `deliveries/snapshot-fixture/s-9c3799cf4a9a4c658b222158dc319b2fb0718279`, manifest hash `51305c76dd251b4edf27b217634eaef66d72d14ed8a9bb61222ae9a812461131`, payload 885 bytes. Дополнительные 64 MiB gate packages и owned staging сохранены; retention ещё не реализован.

[Windows snapshot runtime gate пройден](done/stage0-7/stage3/windows-snapshot-gate-2026-10-06.md): `debian-20261006-115911-c3629b0a`, actual platform windows, passed true, 19 checks. SHA-256 report `fa10b55472d8df70512637c4918da405d26b2fd09efb9e489800a5783636f7d6`; все пять экспортированных файлов сверены с report.ready.json. Подготовка пользователя `20261006-181804-b74b6c4b`, worker generation `69c79c25-93d2-475b-93ce-53e3af67a92e`. Task build `0f9646e9-86b2-4786-8993-b58caaddabb2`; task run `102d9bb3-434a-4df7-9bd5-3952d38e3772`, оба exit 0, запуск вывел Unicode строку. Пользователь подтвердил import/preview/build/failure/history и command form restore. Найдены UX неудобства: shared package ID был текстом, delivery project/snapshot не сохранялись. Исправлены read-only paged discovery/select, независимый delivery draft в том же локальном файле, общий close flush и понятный отказ непроверенной сборки. 48 Rust tests, UI integration, Windows compile/standalone desktop проходят. **Этап 3 закрыт по решению пользователя 2026-10-06: UI исправления приняты, подробная ручная проверка отложена.** Повторять 19 runtime checks не требуется. Подготовлены create-only `.winbridge/controller-snapshot.local.json`, committed fixture Git root и `prepare-windows-snapshot.ps1`. Windows script сохраняет исходный worker/desktop config, создаёт дополнительный `winbridge-worker.snapshot.local.json` в том же fixed runtime/instance/state, добавляет snapshot-fixture и cargo build-cache recipe. Agent gate `validate-snapshot.py` требует actual platform windows и дополнительно проверяет NTFS junction/canary. Native Linux proof не подменяет эти наблюдения. Следующее действие — review fixes и workflow этапа 4; меньше ручных Windows проверок, автоматические сценарии через уже подключённый worker. Не повторять предыдущие Windows gates.

В текущей сессии listener уже запущен на выбранном IP; актуальную доступность проверить командой `controller --config .winbridge/controller-snapshot.local.json status`. Runtime config исключён из Git.

Основной workflow этапа 4 реализован; осталось безопасно интегрировать отдельный Netwatch profile/probe и закончить editable profiles/output filters/artifacts. Повторять этап 2 и пройденный snapshot runtime gate не требуется. Подготовка пользователя `20261005-235244-97785d52`, native Debian run `debian-20261005-170829-53b92c6d`: passed/platform windows/reconnectVerified, семь TaskReport проверены без изменения shared. Task `a5f391a8-dee8-4eeb-8151-efd2e383ccec` сохранила PID 17620 и generation worker после 15 секунд offline controller; затем отменена. На момент завершения gate этапа 2 controller работал с подключённым worker; текущую связь проверять status. Повторять этот runtime gate или gate этапа 1 без новой причины не требуется.

Runtime постоянный: `K:\_proj-26\winbridge\runtime`, runtimeDirectory в локальном settings JSON; старый settings без поля поддерживается. Configs создаются только при отсутствии, затем определяют actual worker settings. Форма/project/state сохраняются; неизменившиеся exe не переписываются, обновление обнаруженного запущенного runtime отклоняется. Дата/ID остаются у отчётов и staging. Старые per-run каталоги не удаляются и автоматически не мигрируют.

Попытка Windows `20261005-224617-fe7ed534` дала TCP/handshake timeout во время запроса файервола, затем socket permission denied 10013. Пользователь сообщил о файерволе с разрешением для каждого нового пути exe. Его Test-NetConnection из pwsh подтвердил доступ к Debian: True, source 192.168.44.1; activity.jsonl содержит tcp_accepted с этого IP. Это подтверждает TCP для PowerShell, но не выполнение Windows worker. Worker теперь различает TCP, WebSocket upgrade и Hello/Welcome в ошибках, а 10013 включает actual exe path. После исправления постоянного пути подключение и gate прошли; firewall/VMware не менялись агентом.

После обновления desktop пользователь подтвердил сохранение формы и dev:ui на Windows: «все проверил включая dev:ui, все работает». [Windows инструкция](validation/windows-validation.md) остаётся для повторных прогонов. Этап 1 закрыт полностью; повторять его gate с нуля не требуется.

Не повторять подготовку или пройденный gate с нуля. Не запускать production Netwatch для проверки этого service; fixture executable достаточен. Новые capabilities включать по фактическим доказательствам.

На этапе 2 plain WebSocket подключает Debian; на этапе 3 HGFS доставляет snapshots. Операции писать только в выбранный namespace после настройки/разрешения реального сценария. Mount/service/firewall настройку не проводить самостоятельно.

## Какие правила Netwatch полезно сохранить

Изучить agent/src/ipc.rs, launcher supervisor/runtime и scripts/unpack_source_archive.py в Netwatch, вместе с tests. Уже исправлены busy named pipe, один heartbeat deadline, bounded pong write, Windows junction обход и restore только собственного backup. Не копировать старые дефекты. PID ownership и instance identity нужно тестировать до lifecycle изменений.

## Готовый запрос для продолжения

> Продолжи WinBridge в /home/user1/winbridge по AGENTS.md и docs/handoff.md. Этапы 1–6 завершены automatic Windows gates; следующий этап 7 — handler/module interfaces и reusable project profiles. Baseline/Git batch/delta/cache/output batching/virtualization/pinned chunk resume прошли реальные gates; old source generations/history сохраняются, прерванные filesystem mutations и неизвестные внешние команды автоматически не повторяются. Native resume code clean 38f4e5d; installed worker clean af90ec0 healthy/idle, для новых функций нужен общий update worker/desktop, ручной gate не требуется. Актуальную связь/source identity проверять заново; controller был обновлён и connected. Automatic Windows checks проводить в отдельных runtime, заранее обозначая нужные процессы/TEST окна, после сообщать, что можно закрыть. Не повторять gates без причины; при неожиданной ошибке рабочего worker остановиться и уточнить.

## Завершение этапа 4 — 2026-10-06

Добавлены editable named profiles и unfinished drafts в desktop state с close/save ACK,
фильтрация и pause просмотра stdout/stderr/log, исторический artifact catalog с bytes/SHA/source/build task,
фоновые artifact/health checks и build identity worker/desktop. DB schema остаётся 3;
старые stdout workflow fingerprints сохраняются. Controller/UI проверяют workflowFeatures
перед отправкой новых полей. Оригинальный Netwatch repository не меняется.

Пользователь согласовал отдельный clone Netwatch с instance ID. [Сценарий](validation/netwatch-validation.md):
исходный commit 39409e56a26ef438adab3488afea504ef40f1363, validation commit
e295806456a5715510918dc178ff1e7975e8626b, manifest
b9a2cb5d1740057566452ad5acfe5f6c19be45390182a58ceaa470a1b06700ed.
Prepared profile `.winbridge/netwatch-validation/20261006-163106-3ae4e9ff/workflow.request.json`
зарегистрирован для отдельного native test root; не отправлять его установленному worker.
Unix actual Netwatch import/build/artifact/start/IPC/log/stop прошёл:
`.winbridge/netwatch-unix/97d94ee6/report.json`, workflow c11c5856-0568-4887-b383-55376449f49c,
PID 177361 остановлен. 63 Rust tests, Clippy -D warnings, contracts/typecheck/UI build,
Windows all-targets compile и browser integration прошли; автоматический Windows Netwatch прогон также завершён (ниже).

Полный native gate: `WinBridge/workflow-validation/20261006-163840-a97da6e3`,
outer task 98c4a0eb-00f8-4f20-8747-77ecddc8af12, report SHA
f1ccabe0039fadeb3daa7820f1e347896cddb4d70691632579fa9c5ed0ff0195.
Source commit 612b3b2 (clean ZIP), SHA f85b755a24503ba64901e99361de0b2951aa2dfaa58f5471348e028c79855185.
41 Windows tests, native Tauri check, Netwatch workflow
6f647cac-c662-4761-a836-a383757809d1 и application
7cd82e14-5337-49ef-8062-67272ad51128, PID 21416: ready подтверждён IPC,
затем explicit stop; startup history succeeded, runtime и test worker остановлены.
Оригинальные Netwatch files и установленный fixed worker не менялись.

Дополнительная проверка UTF-8 отчёта: `workflow-validation/encoding-20261006-612b3b2.json`,
SHA f530848f81367eb6740faf15ee6a0e14fa7a21166271413182f1f95fac75555a,
task ea16150d-452a-4996-b385-c41628428f10 exit 0. Journal/CLI worker сохраняют
правильный UTF-8; PowerShell native capture портил кириллицу в report по OEM code page.
Общий scripts/worker-cli.ps1 теперь читает stdout/stderr как UTF-8 одновременно,
без изменения console/global settings. Проверен реальным Windows CLI и PowerShell
в Debian. Rust/UI код финального выпуска совпадает с native-tested 612b3b2;
после gate изменены только reader тестового отчёта и документация.

Установленный worker generation 210bd4b6-e35f-48e1-85a6-81c973c79605 остаётся
здоровым; stage-4 Netwatch probe/artifacts требуют одного совместного update worker/desktop.
На этой дате следующий roadmap этап был 5; актуальный следующий этап указан в начале.
