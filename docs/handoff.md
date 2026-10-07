# Следующей сессии: продолжение WinBridge

## Текущий шаг — 2026-10-07

**Остановлено для уточнения после неожиданного native результата.**
Implementation `c1e9402`, автоматический run `20261007-055534-610692b2`:
21 core + 9 attention Windows tests и worker/desktop builds exit 0, но initial desktop
не создал phase report; stderr не устанавливает причину. Ready manifest имеет неверные
bytes одного runtime stderr log (SHA совпадает). Gate не пройден, повтор не запускать
до ответа пользователя. [Точные результаты и следующий шаг](windows-companion-attempt-2026-10-07.md).
Рабочий worker/desktop не обновлялись, installed worker здоров и generation прежняя.

После review/plan commit `bbba964` реализован [tray companion](companion.md):
close/save/hide/explicit quit, runtime-scoped single instance, Rust background poll,
popup task link без ACK, built-in sound/volume/quiet/rate и durable cursor/settings.
Native gate попытка завершилась отказом; сценарий `--companion-only` подготовлен для отдельного
runtime с bundled UI, без замены установленного worker/desktop и без debugger port.
77 workspace tests, Clippy, Windows workspace compile, TypeScript/UI build и
Linux standalone desktop check прошли. Browser integration проверил settings/save/hide/quit,
tray failure/task navigation и настоящий result journal fault. Native проверки
этого среза пока не считать пройденными. Workflow/app attention/history/export/MCP
остаются незавершёнными, stage 5 не закрыт.

Новое ревью исправило незаписанный process result: journalRecoveryRequired,
stale GetTask/cancel error, остановка admission/queue и сохранение lease до recovery.
72 workspace tests, Clippy, Windows compile и TypeScript прошли.
[Ревью](review-attention-2026-10-07.md), [следующий сценарий](tray-plan.md).
Installed worker `523fc5b` остаётся здоровым; новые native проверки выполнять в
отдельном runtime. План tray/quiet/sound/close фиксируется перед реализацией.

Пользователь обновил worker/desktop. Read-only `info` подтвердил Windows build
`523fc5b`, sourceDirty=false, generation `bff7f8da-af71-432b-83af-739a020d9e37`,
features artifacts/netwatch_health и connected controller. Controller generation
`381e409f-8949-413c-a439-7d0622159614`, session `7fa9c4d3-eae9-42d1-8d83-966e2d79bd13`.
Предыдущие записи runtime ниже — история; доступность всегда проверять заново.

Начинается [этап 5](stage5-plan.md). Замечание пользователя: Base64 `-EncodedCommand`
не объясняет действие. Первым исправлением будет decoded PowerShell script с пояснением
и отдельными точными аргументами. Затем durable attention/acknowledgement, tray/settings,
history/export и MCP. План фиксируется до реализации по инструкции пользователя.
При вопросах или непредвиденном поведении остановиться и уточнить; новые мелкие ручные
Windows gates не назначать.

Первый сценарий этапа 5 реализован: decoded PowerShell с исходными argv отдельно,
worker attention journal (schema 4, consistent v3 migration backup), task outcome
attention и explicit request/dedup/acknowledgement, paged list, capability guards,
CLI и UI task links/explicit confirmation. [Контракт и границы](attention.md).
71 workspace Rust tests, Clippy -D warnings, Windows all-targets compile,
TypeScript/UI build, standalone desktop check и browser integration прошли, включая
generation cursor reset. [Windows attention-only прогон](windows-attention-gate-2026-10-07.md)
успешен: 7 native tests, task `719111f4-cdfd-4ac4-8926-f6bc548eeed2`, report
`workflow-validation/20261007-044046-eebbd202`, SHA-256
`006784bf39af25640bed803eee08d27668ee2855427e0d6df758605e7c000f3f`.
Source `6b855f6` clean, ZIP SHA-256
`27e7510dbba8247d076fdc5bab4b2c7eda2eb3f7e3ecb197eeddcd8265bd800b`.
Final UI decoder принимает то же Base64 padding/trailing-bit поведение, что .NET;
Rust код после native proof не менялся. Основной worker здоров, generation/session прежние.
Stage 5 не закрыт: workflow/app attention, history/export/MCP ещё предстоят;
tray/popup/sound/settings описаны в текущем шаге выше. Installed runtime остаётся `523fc5b`; первый срез не требует нового
ручного gate. Текущий рабочий сценарий — native tray/popup/sound/preferences поверх
durable attention, затем workflow/app outcome signals и диагностика/MCP. Не просить
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
| DTO и config | Strict schemaVersion 1 loader, registry/recipes/direct/cwd/timeout policy. LocalRequest/Response, TaskRecord/Summary, output byte pages. TS генерируется из Rust; check:contracts проверяет drift. |
| UI | Пять native Windows шагов подтверждены пользователем. Новый fix формы сохраняет executable/argv/cwd/timeout/recipe по worker/project/root. Browser остаётся demo/report viewer; UI integration проверен на настоящем Unix worker. |
| Tauri desktop | Native host запуск подтверждён. workerConfigPath в winbridge-desktop.local.json, форма в winbridge-desktop-state.local.json рядом с exe. worker_rpc, четыре local draft commands и desktop_build; close ждёт save ACK каждой изменённой command/delivery/workflow scope. Shared package select и delivery form restore проверены UI integration; пользователь принял их без нового подробного ручного gate. Сохранение формы и dev:ui дополнительно подтверждены пользователем на Windows. |
| Local IPC/runtime tasks | User SID + instance named pipe, user-only protected DACL, exclusive journal и Windows Job Objects подтверждены Windows отчётом. Unix socket/private state также проверены; bounded frames/deadline/connections. |
| HGFS delivery/import | Git commit capture, deterministic Stored ZIP, actual HGFS .part/readback/ready, durable worker import/lease/pinned build и UI реализованы. 16 actual Unix/HGFS/WebSocket gate checks и actual owned import crash/recovery проходят. 19 actual Windows import/build/run/cancel/NTFS junction checks пройдены 2026-10-06; delivery UX принята пользователем, подробная ручная проверка отложена. Dirty/incremental/benchmark ещё не реализованы. |
| SQLite/process tree | Local WAL/FULL, durable acceptance, dedup, queue/cancel, launch intent/recovery, independent pipe drain/quota. Реальные Windows tree timeout/cancel/crash cleanup и сохранённый indeterminate без retry подтверждены. |
| WebSocket | Versioned handshake/RPC/heartbeat/reconnect/watch реализованы; 14 session groups и UI integration проходят в Debian. Шесть групп Windows runtime gate прошли, включая прежний task/PID/generation после offline. Подключение и все три новых native UI наблюдения подтверждены пользователем; этап 2 завершён. |
| Журнал controller | Входящие IP не фильтруются (решение пользователя 2026-10-05). activity.jsonl в stateDirectory, 1 MiB + пять архивов, bounded writer; droppedRecords/lastError видны в CLI status. Legacy allowedWorkerAddresses игнорируется. |
| Workflow/process/log | Реализованы: durable import/build/start/stdout readiness/log, SQLite workflows/applications (schema 3), текущая schema 4 с attention; consistent upgrade backup, app registry/explicit stop, operation-owned leases, byte cursor rotate/truncate/prefix rewrite. UI steps/timeline/task links/profile restore/log. Actual Windows automatic regression; подробная native UI проверка отложена. См. [workflows](workflows.md). |
| Netwatch profile | Named profiles/drafts, output filters, artifact catalog и build identity реализованы. Этап 4 завершён по общему Windows Netwatch прогону; native визуальная проверка отложена. |
| Attention | Durable task outcome/request attention, dedup/acknowledgement, schema 4 + consistent v3 backup, capability guards, CLI и paged UI/task links. Tray/popup/sound/settings реализованы в исходниках, native gate ещё не завершён. Workflow/app attention ещё предстоит. [Journal](attention.md), [companion](companion.md). |
| Родительский Netwatch | Исправления закоммичены отдельно: 39409e5. Его working tree после commit чистый. |

**Этап 1 завершён 2026-10-05.** [Доказательства](windows-gate-2026-10-05.md): прогон `20261005-161827-c8deef84`, все 10 checks passed, ipcAclVerified true; пять native UI шагов подтверждены пользователем, включая отдельно уточнённый stop/restart worker. JSON uiVerified остаётся false, потому что ручные шаги не оцениваются скриптом; shared report не менялся. Worker --describe возвращает scaffold false и task_submit/task_cancel, в коммите этапа 1 controller оставался scaffold. Теперь controller реализует task_submit/task_cancel через worker; readiness этапа 2 требует отдельного Windows gate.

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
- Runtime CLI: `--config PATH validate|serve|info|submit|get|list|output|cancel|import|imports|get-import|cancel-import|preview`; controller также publish. `output TASK_ID [AFTER_SEQUENCE]` возвращает до 16 chunks, list — последние 10 summary. Desktop читает тот же journal через IPC, не открывает SQLite.
- На этапе 1 recipes имеют фиксированный argv: дополнительные recipe.arguments отклоняются; direct mode принимает массив строк. Один мутирующий процесс на project. maxOutputBytesPerTask по умолчанию 16 MiB; gap отмечается, pipes продолжают drain.
- Rust optional feature `typescript` только для generation. `ui/src/runtime/contracts.ts` — generated file, не править вручную.

## Workflow сессия 2026-10-06

Review fixes закоммичены `700ddd5`: bounded config/desktop state и journal link/reparse guards. Основной цикл этапа 4 реализован и проверен: [контракт](workflows.md), [37 native Windows tests](windows-workflow-gate-2026-10-06.md), [автоматический сценарий](windows-workflow-validation.md). Report run `20261006-132034-a57beb0d`, task `fc247059-8df0-4005-9253-377d328a0fac`, SHA-256 `914c7e54e90ddac912079d6a543c4cc0cc5c0a257f7aaf3c19eea79cba8ba46a`. 59 Unix tests, 15 session groups, actual UI integration, Windows compile/Clippy/contracts/build/standalone desktop проходят. Подробная native UI проверка отложена пользователем; не назначать новый ручной gate для мелких изменений.

Пользователь обновил рабочий Windows runtime выпуском `652635b` через `prepare-windows-snapshot.ps1`; новая generation `210bd4b6-e35f-48e1-85a6-81c973c79605`, workflowAvailable=true, соединение подтверждено. Controller generation `ca6e5e43-3739-4f2a-819c-11e8e8ce63e0`, exec session 9142; доступность проверять status. Не повторять прежний gate; для новых stage-4 features требуется совместимый выпуск. По новой инструкции пользователя при неожиданной ошибке worker остановиться и запросить update. 37 native tests выполнены ранее в isolated source/cache/state через старый worker; дополнительно установленный runtime прошёл actual WebSocket import/build/start/readiness/log/dedup/stop без ручной проверки пользователя. Shared report `workflow-validation/installed-20261006-155439-827cbbac`, SHA-256 `466334303cfa8a2e28483d79a3b6e87c85341f293ad661647d246b6fd4babb8b`; [IDs и доказательства](windows-workflow-gate-2026-10-06.md). Тестовое приложение остановлено, worker оставлен работающим.

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

## Следующий разумный объём работы

Текущий рабочий этап — этап 4. Этап 3 закрыт по решению пользователя 2026-10-06; новые UI исправления приняты, подробная ручная проверка отложена. Этап 2 завершён: [Windows runtime и native UI gate](windows-session-gate-2026-10-05.md) пройдены. Пользователь подтвердил задачи из Debian, потерю связи при продолжающейся задаче, новый session с прежним Task ID после reconnect и отмену. 14 групп настоящих session checks, 29 Rust tests, UI integration, Windows all-targets compile и standalone Linux desktop check проходят. [Ревью](review-stage2-2026-10-05.md) исправило cleanup прерванной сессии, oversized requests, отражение входящего JSON, Welcome capabilities и handshake budget. Адреса пары подтверждены; не задавать вопрос повторно. IP Windows не блокируется.

[Snapshot runtime](snapshot-format.md) реализован. Protocol: manifest/ready/import/preview/ref DTO и optional TaskSubmission.snapshotId; generated TS. Module workspace-sync: точный Git root/commit (без replacement objects), Stored ZIP, .part files/readback/ready-last на HGFS, bounded ZIP parser, actual hashes, local immutable generation, filesystem link/reparse/hardlink guards. Worker SQLite schema 2, consistent v1 migration backup, durable import dedup/cancel/recovery и одна lease для imports/commands. TaskRecord сохраняет accepted snapshot/commit/manifest hash; под lease перед запуском сверяются этот hash и всё дерево, включая unexpected files. Caches/output должны находиться вне snapshots. Native UI: preview, mapping, phases/progress/history/cancel и сохранённый snapshot field.

44 Rust tests, Clippy/fmt, Windows all-targets compile, contracts/TypeScript/UI build, 14 существующих session groups, новые UI checks и standalone Linux desktop check проходят. Реальный HGFS gate `debian-20261005-183623-f32e6b76`: 16 checks, platform linux, passed true, пять ImportRecord и два TaskRecord. Report SHA-256 `bca344044caa6fcad6d99bab6701ed035e12d134090ce1117cb82df4d20491dc`; в shared `WinBridge/snapshot-validation`, report.ready.json содержит checksums. Actual owned crash/recovery: `.winbridge/sg/c07bd640/crash-report.json`, killed during copy_payload, тот же Import ID `d34f270a-40dd-4bd3-8140-511edc6a5926` recovered interrupted, no final source и no auto build. Прогон выполнен после bound-manifest fix.

Найдено actual HGFS ограничение: rename непустого package directory возвращает ENOENT. Исправлено на per-file .part/readback/rename и ready last. Два первых .part directory оставлены как evidence, не удалять автоматически. Shared ready package исходного fixture: `deliveries/snapshot-fixture/s-9c3799cf4a9a4c658b222158dc319b2fb0718279`, manifest hash `51305c76dd251b4edf27b217634eaef66d72d14ed8a9bb61222ae9a812461131`, payload 885 bytes. Дополнительные 64 MiB gate packages и owned staging сохранены; retention ещё не реализован.

[Windows snapshot runtime gate пройден](windows-snapshot-gate-2026-10-06.md): `debian-20261006-115911-c3629b0a`, actual platform windows, passed true, 19 checks. SHA-256 report `fa10b55472d8df70512637c4918da405d26b2fd09efb9e489800a5783636f7d6`; все пять экспортированных файлов сверены с report.ready.json. Подготовка пользователя `20261006-181804-b74b6c4b`, worker generation `69c79c25-93d2-475b-93ce-53e3af67a92e`. Task build `0f9646e9-86b2-4786-8993-b58caaddabb2`; task run `102d9bb3-434a-4df7-9bd5-3952d38e3772`, оба exit 0, запуск вывел Unicode строку. Пользователь подтвердил import/preview/build/failure/history и command form restore. Найдены UX неудобства: shared package ID был текстом, delivery project/snapshot не сохранялись. Исправлены read-only paged discovery/select, независимый delivery draft в том же локальном файле, общий close flush и понятный отказ непроверенной сборки. 48 Rust tests, UI integration, Windows compile/standalone desktop проходят. **Этап 3 закрыт по решению пользователя 2026-10-06: UI исправления приняты, подробная ручная проверка отложена.** Повторять 19 runtime checks не требуется. Подготовлены create-only `.winbridge/controller-snapshot.local.json`, committed fixture Git root и `prepare-windows-snapshot.ps1`. Windows script сохраняет исходный worker/desktop config, создаёт дополнительный `winbridge-worker.snapshot.local.json` в том же fixed runtime/instance/state, добавляет snapshot-fixture и cargo build-cache recipe. Agent gate `validate-snapshot.py` требует actual platform windows и дополнительно проверяет NTFS junction/canary. Native Linux proof не подменяет эти наблюдения. Следующее действие — review fixes и workflow этапа 4; меньше ручных Windows проверок, автоматические сценарии через уже подключённый worker. Не повторять предыдущие Windows gates.

В текущей сессии listener уже запущен на выбранном IP; актуальную доступность проверить командой `controller --config .winbridge/controller-snapshot.local.json status`. Runtime config исключён из Git.

Основной workflow этапа 4 реализован; осталось безопасно интегрировать отдельный Netwatch profile/probe и закончить editable profiles/output filters/artifacts. Повторять этап 2 и пройденный snapshot runtime gate не требуется. Подготовка пользователя `20261005-235244-97785d52`, native Debian run `debian-20261005-170829-53b92c6d`: passed/platform windows/reconnectVerified, семь TaskReport проверены без изменения shared. Task `a5f391a8-dee8-4eeb-8151-efd2e383ccec` сохранила PID 17620 и generation worker после 15 секунд offline controller; затем отменена. На момент завершения gate этапа 2 controller работал с подключённым worker; текущую связь проверять status. Повторять этот runtime gate или gate этапа 1 без новой причины не требуется.

Runtime постоянный: `K:\_proj-26\winbridge\runtime`, runtimeDirectory в локальном settings JSON; старый settings без поля поддерживается. Configs создаются только при отсутствии, затем определяют actual worker settings. Форма/project/state сохраняются; неизменившиеся exe не переписываются, обновление обнаруженного запущенного runtime отклоняется. Дата/ID остаются у отчётов и staging. Старые per-run каталоги не удаляются и автоматически не мигрируют.

Попытка Windows `20261005-224617-fe7ed534` дала TCP/handshake timeout во время запроса файервола, затем socket permission denied 10013. Пользователь сообщил о файерволе с разрешением для каждого нового пути exe. Его Test-NetConnection из pwsh подтвердил доступ к Debian: True, source 192.168.44.1; activity.jsonl содержит tcp_accepted с этого IP. Это подтверждает TCP для PowerShell, но не выполнение Windows worker. Worker теперь различает TCP, WebSocket upgrade и Hello/Welcome в ошибках, а 10013 включает actual exe path. После исправления постоянного пути подключение и gate прошли; firewall/VMware не менялись агентом.

После обновления desktop пользователь подтвердил сохранение формы и dev:ui на Windows: «все проверил включая dev:ui, все работает». [Windows инструкция](windows-validation.md) остаётся для повторных прогонов. Этап 1 закрыт полностью; повторять его gate с нуля не требуется.

Не повторять подготовку или пройденный gate с нуля. Не запускать production Netwatch для проверки этого service; fixture executable достаточен. Новые capabilities включать по фактическим доказательствам.

На этапе 2 plain WebSocket подключает Debian; на этапе 3 HGFS доставляет snapshots. Операции писать только в выбранный namespace после настройки/разрешения реального сценария. Mount/service/firewall настройку не проводить самостоятельно.

## Какие правила Netwatch полезно сохранить

Изучить agent/src/ipc.rs, launcher supervisor/runtime и scripts/unpack_source_archive.py в Netwatch, вместе с tests. Уже исправлены busy named pipe, один heartbeat deadline, bounded pong write, Windows junction обход и restore только собственного backup. Не копировать старые дефекты. PID ownership и instance identity нужно тестировать до lifecycle изменений.

## Готовый запрос для продолжения

> Продолжи WinBridge в /home/user1/winbridge по AGENTS.md и docs/handoff.md. Этап 1 завершён. Windows runtime gate этапа 2 пройден 2026-10-05, доказательства в docs/windows-session-gate-2026-10-05.md. Все три новых native UI наблюдения подтверждены пользователем. Этап 2 завершён. Windows snapshot runtime gate этапа 3 пройден 2026-10-06: docs/windows-snapshot-gate-2026-10-06.md. Пользователь принял UI исправления и закрыл этап 3, подробную ручную проверку отложил. Этап 4 завершён 2026-10-06 по автоматическому Netwatch gate: 41 native test и Windows desktop check. Следующий этап — 5; приоритет автоматических Windows сценариев через подключённый worker; не останавливаться на новых ручных UI gates. Сохраняй отдельный Windows workspace, постоянный путь exe/config/state, durable acceptance/dedup/cancel/recovery и понятный UI. Не вводи TLS или новые adapters без задачи пользователя; пройденные runtime gates без новой причины не повторяй.

## Завершение этапа 4 — текущая работа 2026-10-06

Добавлены editable named profiles и unfinished drafts в desktop state с close/save ACK,
фильтрация и pause просмотра stdout/stderr/log, исторический artifact catalog с bytes/SHA/source/build task,
фоновые artifact/health checks и build identity worker/desktop. DB schema остаётся 3;
старые stdout workflow fingerprints сохраняются. Controller/UI проверяют workflowFeatures
перед отправкой новых полей. Оригинальный Netwatch repository не меняется.

Пользователь согласовал отдельный clone Netwatch с instance ID. [Сценарий](netwatch-validation.md):
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
Новый ручной gate не нужен. Следующий незавершённый roadmap этап — 5.
