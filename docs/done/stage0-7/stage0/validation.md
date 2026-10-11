# Валидация — 2026-10-05

## Netwatch

Commit `39409e5`: пять исправлений code review, regression tests и документация. Перед commit прошли 15 Rust tests, 9 Python tests; один тест настоящей Windows junction пропущен на Linux. Ссылки на WinBridge удалены, proposal перенесён в новый repository. Рабочее дерево после commit чистое.

## Подготовка scaffold (до этапа 1)

- Rust workspace собирается, три protocol tests проходят: Windows argv round trip, неизвестные mode/fields, example requests.
- Cargo check Windows MSVC target проходит. Это compile check без выполнения Windows binaries.
- Clippy с `-D warnings`, cargo fmt и git whitespace check проходят.
- Controller `--describe` выдаёт version/component и пустые implementedCapabilities; worker `--help` сообщает scaffold status. CLI не запускает процессы или listener.
- npm lockfile, clean install, TypeScript strict и Vite production build проверены.
- `npm run check:scaffold` проверяет Markdown local links, JSON examples, plain ws://, согласованные project IDs, отдельный build workspace и aligned app version.
- Headless Chromium запущен с обычным sandbox и временным profile. Проверены отображения success, failure/output и lost freshness. [Screenshot task workbench](../../../screenshots/task-workbench.png).

UI/browser проверка относится к fixtures и presentation, а не к real worker. Никаких операций записи/cleanup в `/mnt/hgfs/shared`, Windows command execution, mount/firewall/service changes или encryption setup в этой сессии не выполнялось.

## Следующие runtime gates

На этапе 1 нужна Windows проверка real execution, stdout/stderr, process tree/cancel, IPC ownership и recovery. На этапе 3 — обе стороны HGFS, неполная доставка, path validation и failure recovery. Скорость HGFS/TCP в этой сессии не измерялась.

Backup paths и exact restore commands: [migration recovery](../../../context/migration-recovery.md).

## Реализация этапа 1

- Rust workspace: три прежних wire contract tests и семь runtime integration tests проходят. Runtime tests действительно запускают fixture executable: success/failure, stdout/stderr, duplicate key/payload, durable result, lock ownership, queued/tree cancel, timeout, recipe и большой output с quota. Launch-intent recovery проверяет indeterminate и отсутствие нового процесса. Unix IPC startup сохраняет существующий regular file вместо удаления как socket.
- `scripts/validate-local.py`: настоящий Unix socket и worker subprocess; 16 конкурентных duplicates, разрыв клиента до ACK, mode 0600/private state, exclusive worker, output/quota, очередь/cancel, invalid frames/paths, worker crash/restart, сохранённый result и graceful shutdown. Отчёты в `.winbridge/validation/<timestamp>-<id>` исключены из Git. Это Linux adapter, Windows cleanup semantics не проверяет.
- `npm run test:ui`: headless Chromium с sandbox; тестовый transport shim передаёт LocalRequest в настоящий Unix worker. Проверены lost-ACK UI и тот же key без второго запуска, output/Unicode, cancel, stale freshness при разрыве, восстановление indeterminate и browser report import/error. [Скриншот реального Linux результата](../../../screenshots/local-worker-linux.png).
- `ui/src/runtime/contracts.ts` генерируется из Rust DTO optional ts-rs feature; `check:contracts` проверяет drift. TypeScript strict и Vite build проходят. npm dependencies/lockfile обновлены для Tauri API/CLI и browser integration checks.
- Local worker Windows MSVC cross-target check проходит, включая Job Object/thread APIs и SID/DACL named pipe код. Это compile check, а не выполнение Windows binary.
- Standalone Tauri package проходит Linux Cargo check. Windows cross-target desktop check остановился на отсутствующем `llvm-rc` для resources; SDK/linker не устанавливались и не обходились.
- `scripts/validate-windows.ps1` и его C# ACL helper проходят syntax/compilation check через portable PowerShell в `.winbridge`. CLI helper также проверен на настоящем Unix worker: multi-argument operations, пути с пробелами, UTF-8 task JSON и byte output. Win32 functions в Debian не выполнялись.

На момент первой подготовки этап 1 оставался открыт: пользователь согласился выполнить [Windows сценарий и native UI шаги](../../../validation/windows-validation.md) на host. implementedCapabilities тогда были пусты. WebSocket/HGFS import/log tail/attention не реализованы. VMware, mounts, firewall, services и глобальные настройки не изменялись.

## Исправления локальных настроек — 2026-10-05

- Desktop читает `winbridge-desktop.local.json` рядом со своим exe. `workerConfigPath` поддерживает абсолютный путь и относительный от папки этого config; неоднозначные Windows пути с drive-relative/root-relative prefix отклоняются. Ошибка настройки возвращается в UI через bridge, окно продолжает открываться.
- PowerShell сохраняет настройки в `winbridge-validation.local.json` рядом с worker exe; исходные defaults — `examples/windows-validation.config.json`. Отчёты направлены в `K:\VMs\shared\WinBridge\validation`, fixture/project/SQLite — в локальную `.winbridge/windows-validation` папку исходников. Existing settings и desktop configs сохраняются; каждый прогон отдельный. При недоступности shared локальный итоговый отчёт содержит ошибку публикации.
- Подстановка прежних переменных окружения удалена. Образец worker config задаёт явный локальный `stateDirectory`; инструкция [windows-validation.md](../../../validation/windows-validation.md) обновлена для двух файлов настроек и самостоятельного чтения отчётов агентом из Debian.
- `cargo test --workspace --locked`: 13 tests проходят в Debian, включая три новых проверки desktop config. `cargo check --workspace --all-targets --target x86_64-pc-windows-msvc --locked` проверяет также Windows-only path test; это компиляция, не запуск. Standalone desktop Linux Cargo check, Clippy и форматирование проходят.
- Через portable PowerShell реально выполнены функции подготовки каталогов и настроек: shared/local paths, пробелы, сохранение пользовательских configs, schema/unknown-field/path rejection. Сам сценарий проверен на раннем отказе при отсутствующем executable: отчёт появляется в настроенной папке и локально; при недоступном report directory остаётся локальный отчёт с ошибкой публикации. Windows processes и Win32 ACL helper в этих проверках не выполнялись.
- Снимки изменённых ранее незафиксированных исходников проверены побайтно; [пути и точные команды восстановления](../../../context/config-fixes-recovery-2026-10-05.md).

После этих исправлений пользователь первоначально сообщил, что Windows запуск в целом работает; readiness по одному этому сообщению не повышалась. Полный отчёт и подтверждение UI получены затем, как описано ниже.

## Windows gate и сохранение формы — 2026-10-05

- Прочитан пользовательский прогон `/mnt/hgfs/shared/WinBridge/validation/20261005-161827-c8deef84`. Все 10 runtime checks passed, `platform: windows`, `ipcAclVerified: true`. Проверены пять TaskReport, task IDs, последовательность/offset chunks и outputBytes. Полный summary и SHA-256 — [Windows gate](../stage1/windows-gate-2026-10-05.md). Исходные отчёты не изменялись.
- Пользователь подтвердил пять production native UI шагов: Windows worker/история/indeterminate, recipe/output/exit, tree cancel, продолжение после закрытия окна и timeout, затем stop/restart/freshness/тот же Task ID. Пользователь дополнительно подтвердил dev:ui и сохранение формы на Windows. Этап 1 закрыт; worker объявляет task_submit/task_cancel, controller остаётся scaffold.
- Исправлена потеря полей формы: локальный `winbridge-desktop-state.local.json` рядом с exe, отдельные worker/project/root scopes, восстановление executable/исходного JSON argv/cwd/timeout/recipe. DTO генерируются из protocol. State store проверяет schema/size, сохраняет через temporary file + sync + rename, не перезаписывает повреждённый существующий JSON. Закрытие окна ждёт save ACK; ошибка видна, явное закрытие без сохранения доступно. Восстановление не вызывает task.submit.
- `cargo test --workspace --locked`: 16 tests проходят в Debian. Три новых filesystem tests проверяют сохранение при новом store, отдельные scopes и replacement, незавершённый Unicode JSON, сохранность повреждённого файла/отказ oversized draft. Windows all-targets compile check, Clippy, workspace/desktop fmt и standalone Linux desktop check проходят.
- `npm run test:ui`: настоящий Unix worker плюс filesystem IPC shim для local desktop drafts. Проверены новый WebView с восстановленной формой, сохранение последней правки при немедленном закрытии и задержанной записи, отдельные проекты, отсутствие новой task при восстановлении, реальные сообщения ошибки и сохранность последнего файла при отказе записи. Прежние lost-ACK/dedup/output/cancel/offline/crash/report/recipe checks также проходят. TypeScript strict, generated contracts и production UI build проверены.
- После обновления пользователь подтвердил Windows-native перепроверку сохранения формы и dev:ui: «все проверил включая dev:ui, все работает». Этап 1 закрыт полностью.
- [Снимки исходников этой сессии и точные команды восстановления](../../../context/gate-draft-recovery-2026-10-05.md).

## Локальная сессия этапа 2 — 2026-10-05

- `scripts/validate-session.py`: настоящие binaries controller/worker, Unix socket и TCP WebSocket; 13 групп проверок проходят. Private controller IPC; outbound/versioned handshake; duplicate session не вытесняет worker; 12 конкурентных duplicates; lost client ACK; Unicode stdout/stderr; tree cancel; controller exit/restart сохраняет PID/task ID и CLI watch replay без повторов; worker crash/recovery без retry; timeout/version/path policy. Fixture peer дополнительно проверяет неверные wire version/pair/capabilities/IP, обратный порядок response, oversized frame до payload, RPC deadline при живом heartbeat и heartbeat deadline при control traffic. Последний отчёт: `.winbridge/session/20261005-114733-5f5c509b/validation-report.json`, platform linux.
- `cargo test --workspace --locked`: 18 tests; добавлены URL/local pair/schema отказные проверки. Windows all-targets compile, Clippy с `-D warnings`, standalone Linux desktop check и TypeScript strict проходят. Desktop lockfile обновлён для новой зависимости transport.
- `npm run test:ui`: новый отдельный controller status, настоящий stop/restart controller при running task, сохранение task ID/status и reconnection; предыдущие execution/persistence проверки проходят. `validate-local.py` по-прежнему проходит на Unix adapter.
- `prepare-windows-session.ps1` подготовлен; его PowerShell syntax и настоящие JSON/file helpers проверены в Debian: CreateNew сохраняет существующий config, десять отказных schema/address/path cases отклонены. Он собирает runtime на Windows в отдельном локальном каталоге и публикует setup report в shared. Этот report описывает подготовку, а не доказывает удалённое исполнение.
- Адреса существующего VMnet8 подтверждены пользователем: Debian 192.168.44.203, Windows 192.168.44.1. Mounts, VMware, firewall, services и global settings не менялись. Этап 2 остаётся открыт до [native Windows session gate](../../../validation/windows-session-validation.md).

- `validate-remote.py --managed-controller --allow-unix-fixture` проверен с настоящими controller/worker процессами: все шесть групп проходят, включая literal argv и restart/replay. Отчёт сохраняет platform linux; Windows результат не заявлен.

## Ревью этапа 2 и журнал активности — 2026-10-05

По прямому решению пользователя входящие IP не блокируются; legacy allowedWorkerAddresses игнорируется без перезаписи runtime config. Другой источник 127.0.0.2 реально принят, его адрес записан в activity.jsonl. Ротация ограничивает текущий файл и пять архивов; ошибки/пропуски видны через status.activityLog. Доказательства и пять закрытых findings описаны в [ревью](../stage2/review-stage2-2026-10-05.md).

27 Rust tests, 14 групп настоящих session checks, UI integration, Clippy/fmt, generated contracts/TypeScript/UI build, Windows all-targets compile и standalone Linux desktop check проходят. Новые tests проверяют abort→reconnect без retry, oversized envelope без потери session, обе стороны handshake, bounded malformed-JSON diagnostics и filesystem поведение журнала (rotation/reopen/symlink/shutdown/error/recovery/crash fragment). Native Windows gate этапа 2 ещё ожидается; этап 1 по-прежнему закрыт.

## Постоянный Windows runtime и диагностика подключения — 2026-10-05

- Пользовательский setup report `20261005-224617-fe7ed534` прочитан без изменения. Timeout во время запроса файервола сменился на WSAEACCES 10013. Пользователь сообщил о разрешениях файервола по пути exe и подтвердил Test-NetConnection: True, source 192.168.44.1, destination 192.168.44.203:1143. Controller activity.jsonl содержит tcp_accepted с этого IP. Это проверка TCP из PowerShell, а не Windows worker/gate.
- `prepare-windows-session.ps1` использует постоянный runtimeDirectory (`K:\_proj-26\winbridge\runtime` по умолчанию), независимый от source folder/run ID. Configs создаются только при отсутствии, project/state/форма сохраняются; одинаковые exe не переписываются. Изменившиеся exe проходят staging/hash validation; обнаруженный запущенный runtime блокирует замену. Старые per-run каталоги сохранены, их journal автоматически не переносится. Отчёты остаются отдельными в local/shared.
- `test-session-runtime.ps1` реально выполнен через portable PowerShell: стабильные пути при другой папке исходников, побайтное сохранение configs и Unicode draft, сохранение state marker, неизменившийся активный exe, отказ перед любым обновлением при работающем desktop, замена только изменившихся файлов и отказ при несовпадении desktop workerConfigPath. SQLite в этом тесте не открывается и не копируется.
- Read-only PowerShell probe проверен на настоящем Unix worker: отсутствующий, работающий и остановленный процесс; возвращён реальный instance/generation, список tasks остался пустым. Native Windows lifetime/ACL этим тестом не подтверждаются.
- 29 Rust tests проходят: две новые проверки настоящего TCP различают connection refused, задержку HTTP upgrade и задержку Welcome после Hello. Local handler остаётся доступным; общий connection deadline сохраняется. 14 session groups и UI integration вновь проходят с новым worker; reconnect сохраняет task ID без повторного запуска. TypeScript/UI build, contracts, Clippy/fmt, Windows all-targets compile и standalone Linux desktop check проходят.

Инструкции: [Windows session](../../../validation/windows-session-validation.md) и [Windows validation](../../../validation/windows-validation.md). Изменений firewall/VMware/services агент не выполнял. Native Windows gate этапа 2 остаётся открыт.

## Windows runtime gate этапа 2 — 2026-10-05

Пользователь передал setup run `20261005-235244-97785d52` и подтвердил native UI connected с session `42cdb08d-41bb-40c7-a34a-99b2bd3a871b`. Затем `validate-remote.py --managed-controller --reconnect-pause-seconds 15` реально выполнил шесть групп на platform windows, passed/reconnectVerified true. Проверены success/lost ACK/dedup, буквальные Windows argv/Unicode, failure/recipe, timeout/cancel дерева и сохранение PID `17620`/Task ID при restart controller. Watch дочитал output без повторов. Семь TaskReport проверены по sequences/offsets/byte counts.

Shared evidence: `session-validation/debian-20261005-170829-53b92c6d`; подробности и checksum — [Windows session gate](../stage2/windows-session-gate-2026-10-05.md). Автоматический report и подготовка пользователя не редактировались. Постоянный controller снова запущен, Windows worker подключён. Пользователь затем подтвердил все три новых native UI наблюдения: «Все три наблюдения подтвердились». Этап 2 завершён. Harness теперь сохраняет сравниваемые session/generation/PID и убирает только собственный subprocess даже при ошибке старта.

## Начало этапа 3: metadata preflight — 2026-10-05

Общие SnapshotManifest/SnapshotReady/source/file DTO добавлены в protocol, TS bindings сгенерированы. Pure core checks отклоняют traversal/reserved/ADS/noncanonical paths, case collisions и file/directory conflicts, malformed hashes, несогласованные размеры, oversized/unknown JSON и metadata budget overflow. CLI --check-snapshot-manifest читает bounded input и явно возвращает metadata_only/payloadVerified false/imported false.

33 Rust tests проходят, включая четыре новых metadata test groups. Новый CLI реально запущен с valid Unicode example и malicious traversal/reserved/config/oversize inputs; workspace_import проверен как planned, не implemented. Generated contracts, TypeScript/UI build, Clippy/fmt, Windows all-targets compile и standalone Linux desktop check проходят. ZIP bytes/hash verification, local filesystem staging/import/lease и native Windows snapshot gate этими проверками не заменяются. [Формат и следующий сценарий](../../../reference/snapshot-format.md).

## Этап 3: snapshot runtime, 2026-10-05

44 Rust tests включают actual Git object capture, dirty-tree exclusion, deterministic repeat, corrupt/incomplete package, valid-hash ZIP extra/link/content rejection, cancel/read-unavailable staging, local-source extras, real build and bound manifest tamper refusal, durable dedup/retry/recovery, shared lease и consistent schema migration backup.

Новый `validate-snapshot-local.py` запускает owned IPC/WebSocket pair и использует actual HGFS. Gate `debian-20261005-183623-f32e6b76` прошёл 16 checks: actual local import/build/Unicode output, concurrent dedup/conflict, incomplete/read-unavailable/corrupt refusals, active 64 MiB cancellation и no buildable cancelled generation. Отдельный actual owned worker kill during copy_payload восстановил тот же import как interrupted без final source/повторной сборки (`.winbridge/sg/c07bd640/crash-report.json`). Это platform linux, не Windows gate.

Actual HGFS directory rename ENOENT найден и исправлен на per-file .part/readback/ready-last. Existing пакеты не перезаписываются; failed .part evidence оставлены. Source capture и native release archives не включают runtime config/SQLite/secrets.

UI test расширен real manifest/Unicode/mapping/verified files, lost import ACK/dedup и saved snapshot field; [screenshot](../../../screenshots/snapshot-worker-linux.png). Existing 14 session groups сохранены. Contracts/strict TS/UI build, Clippy/fmt, Windows all-targets compile, standalone Linux Tauri и PowerShell parser проверены. Native Windows import/build/NTFS junction/UI gate пока ожидается по [инструкции](../../../validation/windows-snapshot-validation.md).
