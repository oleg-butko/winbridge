# Восстановление до исправлений конфигурации — 2026-10-05

Перед изменениями создан и проверен один снимок каждого файла, прежняя версия которого отсутствует в Git. Снимки находятся под `/home/user1/backup/codex/snapshots/`; исходные файлы в snapshot store не изменялись.

Точные команды восстановления отдельных файлов:

```bash
cp -a -- /home/user1/backup/codex/snapshots/20261005T125440.005065033Z-46080/home/user1/winbridge/scripts/validate-windows.ps1 /home/user1/winbridge/scripts/validate-windows.ps1
cp -a -- /home/user1/backup/codex/snapshots/20261005T125440.119843368Z-46095/home/user1/winbridge/apps/desktop/src-tauri/src/main.rs /home/user1/winbridge/apps/desktop/src-tauri/src/main.rs
cp -a -- /home/user1/backup/codex/snapshots/20261005T125440.262580638Z-46110/home/user1/winbridge/crates/core/src/config.rs /home/user1/winbridge/crates/core/src/config.rs
cp -a -- /home/user1/backup/codex/snapshots/20261005T125440.380915472Z-46125/home/user1/winbridge/apps/desktop/README.md /home/user1/winbridge/apps/desktop/README.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T125440.496152480Z-46140/home/user1/winbridge/examples/README.md /home/user1/winbridge/examples/README.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T125440.615604280Z-46155/home/user1/winbridge/docs/windows-validation.md /home/user1/winbridge/docs/windows-validation.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T125440.730374638Z-46170/home/user1/winbridge/docs/handoff.md /home/user1/winbridge/docs/handoff.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T125440.825771548Z-46185/home/user1/winbridge/docs/validation.md /home/user1/winbridge/docs/validation.md
cp -a -- /home/user1/backup/codex/snapshots/20261005T125440.900588730Z-46200/home/user1/winbridge/docs/roadmap.md /home/user1/winbridge/docs/roadmap.md
```

Команды восстанавливают указанные исходники в репозитории. Они не меняют содержимое backup store. Новые образцы конфигурации и файлы, восстановимые из Git, в этот список не входят.
