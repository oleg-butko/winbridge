# Валидация подготовки — 2026-10-04

## Netwatch

Commit `39409e5`: пять исправлений code review, regression tests и документация. Перед commit прошли 15 Rust tests, 9 Python tests; один тест настоящей Windows junction пропущен на Linux. Ссылки на WinBridge удалены, proposal перенесён в новый repository. Рабочее дерево после commit чистое.

## WinBridge scaffold

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
