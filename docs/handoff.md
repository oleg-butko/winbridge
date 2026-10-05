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
| Rust protocol/core/controller/worker | Worker runtime/CLI и platform/transport crates прошли Windows gate; worker implementedCapabilities: task_submit/task_cancel. Controller: ws:// listener, private IPC и CLI submit/get/list/output/cancel/watch; Windows gate этапа 2 ожидается. |
| DTO и config | Strict schemaVersion 1 loader, registry/recipes/direct/cwd/timeout policy. LocalRequest/Response, TaskRecord/Summary, output byte pages. TS генерируется из Rust; check:contracts проверяет drift. |
| UI | Пять native Windows шагов подтверждены пользователем. Новый fix формы сохраняет executable/argv/cwd/timeout/recipe по worker/project/root. Browser остаётся demo/report viewer; UI integration проверен на настоящем Unix worker. |
| Tauri desktop | Native host запуск подтверждён. workerConfigPath в winbridge-desktop.local.json, форма в winbridge-desktop-state.local.json рядом с exe. worker_rpc и два local draft commands; close ждёт save ACK. Сохранение формы и dev:ui дополнительно подтверждены пользователем на Windows. |
| Local IPC/runtime tasks | User SID + instance named pipe, user-only protected DACL, exclusive journal и Windows Job Objects подтверждены Windows отчётом. Unix socket/private state также проверены; bounded frames/deadline/connections. |
| HGFS delivery/import | Дизайн snapshot import и mapping. Windows validation reports читаются из shared; snapshot import и benchmark ещё не реализованы. |
| SQLite/process tree | Local WAL/FULL, durable acceptance, dedup, queue/cancel, launch intent/recovery, independent pipe drain/quota. Реальные Windows tree timeout/cancel/crash cleanup и сохранённый indeterminate без retry подтверждены. |
| WebSocket | Versioned pair handshake, bounded RPC/correlation/heartbeat, reconnect, CLI watch replay и controller freshness UI реализованы; 14 групп real-process session checks и UI integration проходят в Debian. Ревью и исправления этапа 2 завершены. Windows gate этапа 2 подготовлен, ещё не выполнен. |
| Журнал controller | Входящие IP не фильтруются (решение пользователя 2026-10-05). activity.jsonl в stateDirectory, 1 MiB + пять архивов, bounded writer; droppedRecords/lastError видны в CLI status. Legacy allowedWorkerAddresses игнорируется. |
| Log tail/attention | Не реализованы; следующие этапы. |
| Родительский Netwatch | Исправления закоммичены отдельно: 39409e5. Его working tree после commit чистый. |

**Этап 1 завершён 2026-10-05.** [Доказательства](windows-gate-2026-10-05.md): прогон `20261005-161827-c8deef84`, все 10 checks passed, ipcAclVerified true; пять native UI шагов подтверждены пользователем, включая отдельно уточнённый stop/restart worker. JSON uiVerified остаётся false, потому что ручные шаги не оцениваются скриптом; shared report не менялся. Worker --describe возвращает scaffold false и task_submit/task_cancel, в коммите этапа 1 controller оставался scaffold. Теперь controller реализует task_submit/task_cancel через worker; readiness этапа 2 требует отдельного Windows gate.

## Среда и полезные факты

- Debian в VMware (`systemd-detect-virt` → vmware).
- `/mnt/hgfs` — fuse.vmhgfs-fuse; `/mnt/hgfs/shared` существует. Пользовательский mapping на Windows: `K:\VMs\shared`. Native Windows validation reports реально записаны и прочитаны в Debian. Windows workspace пользователя: `K:\_proj-26\winbridge\src`.
- Выбран существующий VMnet8: Debian 192.168.44.203, Windows 192.168.44.1 (подтверждено пользователем). Порт WinBridge 1143; настройки сети не изменялись.
- Имеются Rust stable и Windows MSVC target для cargo check. Windows runner и удалённого command канала в этой сессии нет.
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

Текущий рабочий этап — этап 2. Код bounded ws:// сессии, CLI submit/get/list/output/cancel/watch, reconnect/reconcile и UI controller freshness готов. 14 групп настоящих session checks, 27 Rust tests, UI integration, Windows all-targets compile и standalone Linux desktop check проходят. [Ревью](review-stage2-2026-10-05.md) исправило cleanup прерванной сессии, отказ oversized исходящего запроса без disconnect, отражение входящего JSON в ошибке, пропущенные Welcome capabilities и общий handshake budget. Native Windows gate ещё не выполнен. Адреса пары уже подтверждены; не задавать этот вопрос повторно. IP Windows теперь не блокируется: менять локальный config для нового исходного IP не требуется.

В текущей сессии listener уже запущен на выбранном IP; актуальную доступность проверить командой `controller --config .winbridge/controller-session.local.json status`. Runtime config исключён из Git.

Следующее действие — [Windows session проверка](windows-session-validation.md): пользователь запускает `scripts/prepare-windows-session.ps1` на host из нового source snapshot, агент проверяет task submission/output/cancel/reconnect из Debian и читает setup/report из shared. Сценарий создаёт отдельные fixture/state/runtime и adjacent exe configs; старые Windows конфиги и journal сохраняются. После доказанного Windows gate закрыть этап 2 и перейти к snapshot import, а не повторять этап 1.

После обновления desktop пользователь подтвердил сохранение формы и dev:ui на Windows: «все проверил включая dev:ui, все работает». [Windows инструкция](windows-validation.md) остаётся для повторных прогонов. Этап 1 закрыт полностью; повторять его gate с нуля не требуется.

Не повторять подготовку или пройденный gate с нуля. Не запускать production Netwatch для проверки этого service; fixture executable достаточен. Новые capabilities включать по фактическим доказательствам.

На этапе 2 plain WebSocket подключает Debian; на этапе 3 HGFS доставляет snapshots. Операции писать только в выбранный namespace после настройки/разрешения реального сценария. Mount/service/firewall настройку не проводить самостоятельно.

## Какие правила Netwatch полезно сохранить

Изучить agent/src/ipc.rs, launcher supervisor/runtime и scripts/unpack_source_archive.py в Netwatch, вместе с tests. Уже исправлены busy named pipe, один heartbeat deadline, bounded pong write, Windows junction обход и restore только собственного backup. Не копировать старые дефекты. PID ownership и instance identity нужно тестировать до lifecycle изменений.

## Готовый запрос для продолжения

> Продолжи WinBridge в /home/user1/winbridge по AGENTS.md и docs/handoff.md. Этап 1 прошёл Windows runtime/native UI gate 2026-10-05; доказательства в docs/windows-gate-2026-10-05.md. Код этапа 2 готов и проверен в Debian; продолжи его подготовленный native Windows gate по docs/windows-session-validation.md, затем перейди к этапу 3. Сохраняй отдельный Windows workspace, durable acceptance/dedup/cancel/recovery и понятный UI. Не вводи TLS или новые adapters без задачи пользователя. Сохранение формы и dev:ui также подтверждены пользователем на Windows; повторять gate этапа 1 не требуется.
