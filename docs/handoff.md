# Следующей сессии: продолжение WinBridge

## Сначала прочитать

Корневой AGENTS.md → README → architecture → roadmap → ux → shared-folders → ADR. Затем `git status --short` и текущую таблицу готовности. Первоначальный proposal в context сохранён для истории; его TLS и общий workspace с Netwatch отменены.

## Цель и явные решения пользователя

Пользователь хочет гибкий локальный каркас Debian VM ↔ Windows host. ИИ агент в Debian должен запускать команды в Windows папке проекта, доставлять последний commit/снимок, собирать, запускать, читать логи и запрашивать popup/звук. Все действия должны быть понятны пользователю в Windows UI.

Принято: имя WinBridge; отдельный `~/winbridge` с Git; нет encrypted connections; shared folder для доставки готовых снимков, отдельный Windows workspace для сборки; recipes + произвольные команды внутри зарегистрированного project. Не возвращаться к этим вопросам без новых требований. Разрешённые рутинные действия в проекте не требуют отдельного подтверждения каждого command.

## Фактическая готовность

| Часть | Состояние |
| --- | --- |
| Git repository и структура | Созданы; история начинается с commit подготовки. Точный hash смотреть в git log. |
| Rust protocol/core/controller/worker | Worker runtime/CLI и platform/transport crates прошли Windows gate; worker implementedCapabilities: task_submit/task_cancel. Controller CLI submit/get/list/output/cancel/watch реально проверен с Windows worker; runtime и native UI gate этапа 2 пройдены, этап завершён. |
| DTO и config | Strict schemaVersion 1 loader, registry/recipes/direct/cwd/timeout policy. LocalRequest/Response, TaskRecord/Summary, output byte pages. TS генерируется из Rust; check:contracts проверяет drift. |
| UI | Пять native Windows шагов подтверждены пользователем. Новый fix формы сохраняет executable/argv/cwd/timeout/recipe по worker/project/root. Browser остаётся demo/report viewer; UI integration проверен на настоящем Unix worker. |
| Tauri desktop | Native host запуск подтверждён. workerConfigPath в winbridge-desktop.local.json, форма в winbridge-desktop-state.local.json рядом с exe. worker_rpc и два local draft commands; close ждёт save ACK. Сохранение формы и dev:ui дополнительно подтверждены пользователем на Windows. |
| Local IPC/runtime tasks | User SID + instance named pipe, user-only protected DACL, exclusive journal и Windows Job Objects подтверждены Windows отчётом. Unix socket/private state также проверены; bounded frames/deadline/connections. |
| HGFS delivery/import | Mapping и shared validation reports работают. Начаты manifest/ready DTO, metadata policy и CLI preflight; publisher/importer, actual payload validation, UI и benchmark ещё не реализованы. workspace_import остаётся planned. |
| SQLite/process tree | Local WAL/FULL, durable acceptance, dedup, queue/cancel, launch intent/recovery, independent pipe drain/quota. Реальные Windows tree timeout/cancel/crash cleanup и сохранённый indeterminate без retry подтверждены. |
| WebSocket | Versioned handshake/RPC/heartbeat/reconnect/watch реализованы; 14 session groups и UI integration проходят в Debian. Шесть групп Windows runtime gate прошли, включая прежний task/PID/generation после offline. Подключение и все три новых native UI наблюдения подтверждены пользователем; этап 2 завершён. |
| Журнал controller | Входящие IP не фильтруются (решение пользователя 2026-10-05). activity.jsonl в stateDirectory, 1 MiB + пять архивов, bounded writer; droppedRecords/lastError видны в CLI status. Legacy allowedWorkerAddresses игнорируется. |
| Log tail/attention | Не реализованы; следующие этапы. |
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
- Runtime CLI: `--config PATH validate|serve|info|submit|get|list|output|cancel`. `output TASK_ID [AFTER_SEQUENCE]` возвращает до 16 chunks, list — последние 10 summary. Desktop читает тот же journal через IPC, не открывает SQLite.
- На этапе 1 recipes имеют фиксированный argv: дополнительные recipe.arguments отклоняются; direct mode принимает массив строк. Один мутирующий процесс на project. maxOutputBytesPerTask по умолчанию 16 MiB; gap отмечается, pipes продолжают drain.
- Rust optional feature `typescript` только для generation. `ui/src/runtime/contracts.ts` — generated file, не править вручную.

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

Текущий рабочий этап — этап 3. Этап 2 завершён: [Windows runtime и native UI gate](windows-session-gate-2026-10-05.md) пройдены. Пользователь подтвердил задачи из Debian, потерю связи при продолжающейся задаче, новый session с прежним Task ID после reconnect и отмену. 14 групп настоящих session checks, 29 Rust tests, UI integration, Windows all-targets compile и standalone Linux desktop check проходят. [Ревью](review-stage2-2026-10-05.md) исправило cleanup прерванной сессии, oversized requests, отражение входящего JSON, Welcome capabilities и handshake budget. Адреса пары подтверждены; не задавать вопрос повторно. IP Windows не блокируется.

Начат [snapshot format](snapshot-format.md): DTO в protocol, pure policy в core, generated TS, controller --check-snapshot-manifest PATH. 33 Rust tests (четыре новых для untrusted metadata), CLI reject/size checks, contracts/TypeScript/UI build, Clippy/fmt, Windows compile и Linux desktop check проходят. CLI честно возвращает metadata_only/payloadVerified false/imported false; actual ZIP/hash/link checks и Windows import ещё впереди. Не выдавать preflight за verified generation и не включать capability до реализации.

В текущей сессии listener уже запущен на выбранном IP; актуальную доступность проверить командой `controller --config .winbridge/controller-session.local.json status`. Runtime config исключён из Git.

Следующее действие — реализовать snapshot import; повторять этап 2 не требуется. Подготовка пользователя `20261005-235244-97785d52`, native Debian run `debian-20261005-170829-53b92c6d`: passed/platform windows/reconnectVerified, семь TaskReport проверены без изменения shared. Task `a5f391a8-dee8-4eeb-8151-efd2e383ccec` сохранила PID 17620 и generation worker после 15 секунд offline controller; затем отменена. Постоянный controller снова работает, worker подключён. Повторять этот runtime gate или gate этапа 1 без новой причины не требуется.

Runtime постоянный: `K:\_proj-26\winbridge\runtime`, runtimeDirectory в локальном settings JSON; старый settings без поля поддерживается. Configs создаются только при отсутствии, затем определяют actual worker settings. Форма/project/state сохраняются; неизменившиеся exe не переписываются, обновление обнаруженного запущенного runtime отклоняется. Дата/ID остаются у отчётов и staging. Старые per-run каталоги не удаляются и автоматически не мигрируют.

Попытка Windows `20261005-224617-fe7ed534` дала TCP/handshake timeout во время запроса файервола, затем socket permission denied 10013. Пользователь сообщил о файерволе с разрешением для каждого нового пути exe. Его Test-NetConnection из pwsh подтвердил доступ к Debian: True, source 192.168.44.1; activity.jsonl содержит tcp_accepted с этого IP. Это подтверждает TCP для PowerShell, но не выполнение Windows worker. Worker теперь различает TCP, WebSocket upgrade и Hello/Welcome в ошибках, а 10013 включает actual exe path. После исправления постоянного пути подключение и gate прошли; firewall/VMware не менялись агентом.

После обновления desktop пользователь подтвердил сохранение формы и dev:ui на Windows: «все проверил включая dev:ui, все работает». [Windows инструкция](windows-validation.md) остаётся для повторных прогонов. Этап 1 закрыт полностью; повторять его gate с нуля не требуется.

Не повторять подготовку или пройденный gate с нуля. Не запускать production Netwatch для проверки этого service; fixture executable достаточен. Новые capabilities включать по фактическим доказательствам.

На этапе 2 plain WebSocket подключает Debian; на этапе 3 HGFS доставляет snapshots. Операции писать только в выбранный namespace после настройки/разрешения реального сценария. Mount/service/firewall настройку не проводить самостоятельно.

## Какие правила Netwatch полезно сохранить

Изучить agent/src/ipc.rs, launcher supervisor/runtime и scripts/unpack_source_archive.py в Netwatch, вместе с tests. Уже исправлены busy named pipe, один heartbeat deadline, bounded pong write, Windows junction обход и restore только собственного backup. Не копировать старые дефекты. PID ownership и instance identity нужно тестировать до lifecycle изменений.

## Готовый запрос для продолжения

> Продолжи WinBridge в /home/user1/winbridge по AGENTS.md и docs/handoff.md. Этап 1 завершён. Windows runtime gate этапа 2 пройден 2026-10-05, доказательства в docs/windows-session-gate-2026-10-05.md. Все три новых native UI наблюдения подтверждены пользователем. Этап 2 завершён; продолжай этап 3 (snapshot capture/shared publication/verified import/UI). Сохраняй отдельный Windows workspace, постоянный путь exe/config/state, durable acceptance/dedup/cancel/recovery и понятный UI. Не вводи TLS или новые adapters без задачи пользователя; пройденные runtime gates без новой причины не повторяй.
