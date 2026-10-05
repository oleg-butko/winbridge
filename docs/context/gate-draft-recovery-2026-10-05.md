# Восстановление до закрытия gate и fix формы — 2026-10-05

Перед первой правкой каждого важного незафиксированного исходника создан и побайтно проверен timestamped snapshot. Файлы в `/home/user1/backup/codex/snapshots/` не изменялись.

Точные команды восстановления отдельных файлов:

```bash
cp -a -- /home/user1/backup/codex/snapshots/20261005T141850.376258967Z-49950/home/user1/winbridge/ui/src/features/tasks/NativeWorkbench.tsx /home/user1/winbridge/ui/src/features/tasks/NativeWorkbench.tsx
cp -a -- /home/user1/backup/codex/snapshots/20261005T141850.445387828Z-49966/home/user1/winbridge/scripts/validate-ui.mjs /home/user1/winbridge/scripts/validate-ui.mjs
cp -a -- /home/user1/backup/codex/snapshots/20261005T141850.533366200Z-49981/home/user1/winbridge/crates/protocol/src/lib.rs /home/user1/winbridge/crates/protocol/src/lib.rs
cp -a -- /home/user1/backup/codex/snapshots/20261005T141850.617717238Z-49996/home/user1/winbridge/crates/protocol/examples/export-types.rs /home/user1/winbridge/crates/protocol/examples/export-types.rs
cp -a -- /home/user1/backup/codex/snapshots/20261005T141850.701635066Z-50011/home/user1/winbridge/crates/core/src/lib.rs /home/user1/winbridge/crates/core/src/lib.rs
cp -a -- /home/user1/backup/codex/snapshots/20261005T141850.799435113Z-50026/home/user1/winbridge/crates/core/Cargo.toml /home/user1/winbridge/crates/core/Cargo.toml
cp -a -- /home/user1/backup/codex/snapshots/20261005T141850.892465202Z-50041/home/user1/winbridge/apps/desktop/src-tauri/src/main.rs /home/user1/winbridge/apps/desktop/src-tauri/src/main.rs
cp -a -- /home/user1/backup/codex/snapshots/20261005T141850.975776471Z-50056/home/user1/winbridge/apps/desktop/src-tauri/capabilities/main.json /home/user1/winbridge/apps/desktop/src-tauri/capabilities/main.json
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.058243939Z-50071/home/user1/winbridge/apps/desktop/README.md /home/user1/winbridge/apps/desktop/README.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.152414925Z-50086/home/user1/winbridge/apps/worker/src/main.rs /home/user1/winbridge/apps/worker/src/main.rs
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.243095396Z-50101/home/user1/winbridge/apps/worker/src/lib.rs /home/user1/winbridge/apps/worker/src/lib.rs
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.345611295Z-50116/home/user1/winbridge/README.md /home/user1/winbridge/README.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.449752645Z-50131/home/user1/winbridge/docs/architecture.md /home/user1/winbridge/docs/architecture.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.538421123Z-50146/home/user1/winbridge/docs/handoff.md /home/user1/winbridge/docs/handoff.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.638762800Z-50161/home/user1/winbridge/docs/roadmap.md /home/user1/winbridge/docs/roadmap.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.733167357Z-50176/home/user1/winbridge/docs/ux.md /home/user1/winbridge/docs/ux.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.818275104Z-50191/home/user1/winbridge/docs/validation.md /home/user1/winbridge/docs/validation.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T141851.930369455Z-50206/home/user1/winbridge/docs/windows-validation.md /home/user1/winbridge/docs/windows-validation.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T142929.568463068Z-52638/home/user1/winbridge/modules/runner/README.md /home/user1/winbridge/modules/runner/README.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T142929.630748976Z-52657/home/user1/winbridge/docs/structure.md /home/user1/winbridge/docs/structure.md
```

Это снимки отдельных исходников перед этой задачей. Они не меняют отчёты пользователя в shared или содержимое backup store; новые файлы этой сессии не входят в список.
