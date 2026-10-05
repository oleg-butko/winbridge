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
- Headless Chromium запущен с обычным sandbox и временным profile. Проверены отображения success, failure/output и lost freshness. [Screenshot task workbench](screenshots/task-workbench.png).

UI/browser проверка относится к fixtures и presentation, а не к real worker. Никаких операций записи/cleanup в `/mnt/hgfs/shared`, Windows command execution, mount/firewall/service changes или encryption setup в этой сессии не выполнялось.

## Следующие runtime gates

На этапе 1 нужна Windows проверка real execution, stdout/stderr, process tree/cancel, IPC ownership и recovery. На этапе 3 — обе стороны HGFS, неполная доставка, path validation и failure recovery. Скорость HGFS/TCP в этой сессии не измерялась.

Backup paths и exact restore commands: [migration recovery](context/migration-recovery.md).

## Реализация этапа 1

- Rust workspace: три прежних wire contract tests и семь runtime integration tests проходят. Runtime tests действительно запускают fixture executable: success/failure, stdout/stderr, duplicate key/payload, durable result, lock ownership, queued/tree cancel, timeout, recipe и большой output с quota. Launch-intent recovery проверяет indeterminate и отсутствие нового процесса. Unix IPC startup сохраняет существующий regular file вместо удаления как socket.
- `scripts/validate-local.py`: настоящий Unix socket и worker subprocess; 16 конкурентных duplicates, разрыв клиента до ACK, mode 0600/private state, exclusive worker, output/quota, очередь/cancel, invalid frames/paths, worker crash/restart, сохранённый result и graceful shutdown. Отчёты в `.winbridge/validation/<timestamp>-<id>` исключены из Git. Это Linux adapter, Windows cleanup semantics не проверяет.
- `npm run test:ui`: headless Chromium с sandbox; тестовый transport shim передаёт LocalRequest в настоящий Unix worker. Проверены lost-ACK UI и тот же key без второго запуска, output/Unicode, cancel, stale freshness при разрыве, восстановление indeterminate и browser report import/error. [Скриншот реального Linux результата](screenshots/local-worker-linux.png).
- `ui/src/runtime/contracts.ts` генерируется из Rust DTO optional ts-rs feature; `check:contracts` проверяет drift. TypeScript strict и Vite build проходят. npm dependencies/lockfile обновлены для Tauri API/CLI и browser integration checks.
- Local worker Windows MSVC cross-target check проходит, включая Job Object/thread APIs и SID/DACL named pipe код. Это compile check, а не выполнение Windows binary.
- Standalone Tauri package проходит Linux Cargo check. Windows cross-target desktop check остановился на отсутствующем `llvm-rc` для resources; SDK/linker не устанавливались и не обходились.
- `scripts/validate-windows.ps1` и его C# ACL helper проходят syntax/compilation check через portable PowerShell в `.winbridge`. CLI helper также проверен на настоящем Unix worker: multi-argument operations, пути с пробелами, UTF-8 task JSON и byte output. Win32 functions в Debian не выполнялись.

На момент первой подготовки этап 1 оставался открыт: пользователь согласился выполнить [Windows сценарий и native UI шаги](windows-validation.md) на host. implementedCapabilities тогда были пусты. WebSocket/HGFS import/log tail/attention не реализованы. VMware, mounts, firewall, services и глобальные настройки не изменялись.

## Исправления локальных настроек — 2026-10-05

- Desktop читает `winbridge-desktop.local.json` рядом со своим exe. `workerConfigPath` поддерживает абсолютный путь и относительный от папки этого config; неоднозначные Windows пути с drive-relative/root-relative prefix отклоняются. Ошибка настройки возвращается в UI через bridge, окно продолжает открываться.
- PowerShell сохраняет настройки в `winbridge-validation.local.json` рядом с worker exe; исходные defaults — `examples/windows-validation.config.json`. Отчёты направлены в `K:\VMs\shared\WinBridge\validation`, fixture/project/SQLite — в локальную `.winbridge/windows-validation` папку исходников. Existing settings и desktop configs сохраняются; каждый прогон отдельный. При недоступности shared локальный итоговый отчёт содержит ошибку публикации.
- Подстановка прежних переменных окружения удалена. Образец worker config задаёт явный локальный `stateDirectory`; инструкция [windows-validation.md](windows-validation.md) обновлена для двух файлов настроек и самостоятельного чтения отчётов агентом из Debian.
- `cargo test --workspace --locked`: 13 tests проходят в Debian, включая три новых проверки desktop config. `cargo check --workspace --all-targets --target x86_64-pc-windows-msvc --locked` проверяет также Windows-only path test; это компиляция, не запуск. Standalone desktop Linux Cargo check, Clippy и форматирование проходят.
- Через portable PowerShell реально выполнены функции подготовки каталогов и настроек: shared/local paths, пробелы, сохранение пользовательских configs, schema/unknown-field/path rejection. Сам сценарий проверен на раннем отказе при отсутствующем executable: отчёт появляется в настроенной папке и локально; при недоступном report directory остаётся локальный отчёт с ошибкой публикации. Windows processes и Win32 ACL helper в этих проверках не выполнялись.
- Снимки изменённых ранее незафиксированных исходников проверены побайтно; [пути и точные команды восстановления](context/config-fixes-recovery-2026-10-05.md).

После этих исправлений пользователь первоначально сообщил, что Windows запуск в целом работает; readiness по одному этому сообщению не повышалась. Полный отчёт и подтверждение UI получены затем, как описано ниже.

## Windows gate и сохранение формы — 2026-10-05

- Прочитан пользовательский прогон `/mnt/hgfs/shared/WinBridge/validation/20261005-161827-c8deef84`. Все 10 runtime checks passed, `platform: windows`, `ipcAclVerified: true`. Проверены пять TaskReport, task IDs, последовательность/offset chunks и outputBytes. Полный summary и SHA-256 — [Windows gate](windows-gate-2026-10-05.md). Исходные отчёты не изменялись.
- Пользователь подтвердил пять production native UI шагов: Windows worker/история/indeterminate, recipe/output/exit, tree cancel, продолжение после закрытия окна и timeout, затем stop/restart/freshness/тот же Task ID. Пользователь дополнительно подтвердил dev:ui и сохранение формы на Windows. Этап 1 закрыт; worker объявляет task_submit/task_cancel, controller остаётся scaffold.
- Исправлена потеря полей формы: локальный `winbridge-desktop-state.local.json` рядом с exe, отдельные worker/project/root scopes, восстановление executable/исходного JSON argv/cwd/timeout/recipe. DTO генерируются из protocol. State store проверяет schema/size, сохраняет через temporary file + sync + rename, не перезаписывает повреждённый существующий JSON. Закрытие окна ждёт save ACK; ошибка видна, явное закрытие без сохранения доступно. Восстановление не вызывает task.submit.
- `cargo test --workspace --locked`: 16 tests проходят в Debian. Три новых filesystem tests проверяют сохранение при новом store, отдельные scopes и replacement, незавершённый Unicode JSON, сохранность повреждённого файла/отказ oversized draft. Windows all-targets compile check, Clippy, workspace/desktop fmt и standalone Linux desktop check проходят.
- `npm run test:ui`: настоящий Unix worker плюс filesystem IPC shim для local desktop drafts. Проверены новый WebView с восстановленной формой, сохранение последней правки при немедленном закрытии и задержанной записи, отдельные проекты, отсутствие новой task при восстановлении, реальные сообщения ошибки и сохранность последнего файла при отказе записи. Прежние lost-ACK/dedup/output/cancel/offline/crash/report/recipe checks также проходят. TypeScript strict, generated contracts и production UI build проверены.
- После обновления пользователь подтвердил Windows-native перепроверку сохранения формы и dev:ui: «все проверил включая dev:ui, все работает». Этап 1 закрыт полностью.
- [Снимки исходников этой сессии и точные команды восстановления](context/gate-draft-recovery-2026-10-05.md).
