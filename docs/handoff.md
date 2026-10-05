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
| Rust protocol/core/controller/worker | Собираются; help/describe работают. Реальных capabilities нет, descriptor это показывает. |
| DTO и примеры requests | Recipe/direct contracts, TaskStatus/Event; Windows argv и unknown fields проверяются tests. Config examples пока не имеют loader. |
| Browser UI | Рабочий prototype на fixtures: success/failure/lost freshness, timeline, command/argv/cwd, output и copy. Это не connected worker. |
| Tauri desktop | Только границы в apps/desktop/README; package/IPC bridge ещё нет. |
| Network/IPC/runtime tasks | Не реализованы. ws:// решение принято; capabilities должны рекламироваться только после реализации. |
| HGFS delivery/import | Дизайн и mapping. Shared mount прочитан; файлы там не создавались, benchmark не запускался. |
| SQLite/process tree/log tail/attention | Спроектированы, но runtime кода пока нет. |
| Родительский Netwatch | Исправления закоммичены отдельно: 39409e5. Его working tree после commit чистый. |

Нельзя говорить, что WinBridge уже исполняет Windows команды. `--describe` возвращает `scaffold: true`, пустые implementedCapabilities и отдельно plannedCapabilities.

## Среда и полезные факты

- Debian в VMware (`systemd-detect-virt` → vmware).
- `/mnt/hgfs` — fuse.vmhgfs-fuse; `/mnt/hgfs/shared` существует. Пользовательский mapping на Windows: `K:\VMs\shared`. Windows сторона не проверялась исполнением программы.
- Имеются Rust stable и Windows MSVC target для cargo check. Windows runner и удалённого command канала в этой сессии нет.
- Rust wrappers: `/home/user1/.local/bin`; Node/npm: `/home/user1/.nvm/versions/node/v24.19.0/bin`. В новом workdir они могут отсутствовать в PATH.
- Рабочие repos: `/home/user1/winbridge` и `/home/user1/netwatch`. Не изменять Netwatch для реализации WinBridge и не добавлять path dependency на него.
- Git identity не была настроена глобально. Commits использовали временные `-c user.name=funame -c user.email=user1@debf3.local`, взятые из прежней истории Netwatch; глобальный config не менялся.
- Снапшоты перед переносом и exact restore commands: context/migration-recovery.md. UTC дата снимков 2026-10-05 соответствует локальному вечеру 2026-10-04.
- Полный отчёт проверок и screenshot: validation.md. Browser состояния success/failure/unknown проверены в headless Chromium; real execution пока отсутствует.

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
npm run typecheck
npm run build:ui
```

Это проверки подготовленного каркаса, не проверка реальных Windows execution/IPC/HGFS функций. Native runtime gate появится на этапе 1. Браузерная демонстрация запускается `npm run dev:ui`; fixtures имеют явный badge и сообщение об отсутствии worker. Можно выбрать scenario в UI или URL `?scenario=failure&tab=output`.

## Следующий разумный объём работы

Этап 1 roadmap: config loader, project registry и один local Windows command service с durable task/result/output, затем IPC/read model и native shell для существующего task workbench. Использовать локальный fixture для success/failure/timeout/process tree. Сначала сделать маленький полный сценарий, затем расширять.

При отсутствии Windows execution environment продолжить независимые contracts/config/core/interfaces и подготовить точный Windows validation сценарий. Cross-target check позволяет найти compile errors, но не подтверждает запуск или ACL. Не включать новую capability как implemented до real runtime проверки.

На этапе 2 plain WebSocket подключает Debian; на этапе 3 HGFS доставляет snapshots. Операции писать только в выбранный namespace после настройки/разрешения реального сценария. Mount/service/firewall настройку не проводить самостоятельно.

## Какие правила Netwatch полезно сохранить

Изучить agent/src/ipc.rs, launcher supervisor/runtime и scripts/unpack_source_archive.py в Netwatch, вместе с tests. Уже исправлены busy named pipe, один heartbeat deadline, bounded pong write, Windows junction обход и restore только собственного backup. Не копировать старые дефекты. PID ownership и instance identity нужно тестировать до lifecycle изменений.

## Готовый запрос для продолжения

> Продолжи WinBridge в /home/user1/winbridge по AGENTS.md и docs/handoff.md. Начни этап 1 из docs/roadmap.md: зарегистрированный проект, рецепты/direct command, реальное выполнение и журнал на Windows, отображение command/argv/cwd/output/exit code в task workbench. Соблюдай принятые решения: локальный ws:// без TLS, HGFS для готовых снимков и отдельный Windows build workspace. Не выдавай fixtures за реальное исполнение. Обнови roadmap и handoff по фактическим результатам.
