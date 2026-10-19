# Windows gate: previous-runtime preimages library

2026-10-19. Clean source `835bc14dbb5583177ccfd222218719108d229f7d`,
run `20261019-070223-c48d2a35`, Task `6b6c13ce-f7e4-4863-bd48-78ff8783d60b`:
**succeeded / exit 0, 189 native tests / 19 groups**.
Updater schema 9, candidate worker schema 15. Supervisor v8 WIP не входил в архив.

Архив `winbridge-20261019T110205Z-835bc14-2e98ced8.zip`, SHA
`ce5b647bc50ee774e96b8f97fb1d8a1267fba211c5df5760ab3af2ebd6ee9c57`.
Все 43 файла из report.ready.json проверены по размеру/SHA. Report SHA:
`3d5d84ca7ed6757f5976a8e591559ea44c6c1fb3040b9ef33dd5f08736226498`.
Windows free before/after: 21,363,748,864 / 20,961,271,808 bytes.
Cache owner `20261017-061527-f4639971` reused; тесты выполнены заново,
validation directories не удалялись.

Пройдены 11 preimages tests: five independent create-only copies/receipt/source
barrier, existing/partial/extra/tampered generations, one-use permit/drop/no replay,
owner/parent fencing, ignored inserts/rollback, retained records и migration 8→9.
Actual NTFS handles запрещали запись в пять copies до receipt; после завершения
они открывались на запись, bytes/hash/File IDs сохранились. Descriptors этих
fixtures synthetic: тесты не подтверждают здоровье приложений.

Previous schema-8 updater отверг schema 9 без записи. Новые path checks проверяют
удаление sidecar между metadata/open, отказ от hardlink replacement и отсутствие
retry для изначально unsafe path. Unix symlink case прошёл отдельно локально.
Точный отсутствовавший путь в предыдущем failed run не был записан: воспроизведённая
optional WAL/SHM гонка исправлена, этот повтор прошёл полный pipeline.

Полный Rust/desktop build, queries четырёх exe и exact payload files passed.
Actual worker Main readonly startup/negative guard и normal serve/pins повторены.
PID 13660 держал пять pins; после forced owned idle stop handles освобождены,
bytes/updater DB сохранены. Lifetime 10,365 ms, без UI/network/tasks.
Graceful shutdown/activation ещё не подтверждаются этим тестом.

Для будущего schema>9 refusal сохранён updater из этого candidate:
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261019-070223-c48d2a35\update-candidate\payload\winbridge-update.exe`,
SHA `b3d1c13773ea1a0e11800a6c856890e0cad7122044ee475527da0781326d755d`.
Текущий actor v8 остаётся schema 9: его предыдущим-schema reference остаётся
schema-8 updater из baseline gate.

Installed worker не заменён: source 1f3aa9f/build 1792228466051801600,
generation `994671b4-e10c-469c-8515-b2aa71a4e3dc`, fresh Info connected/idle0/error null.
Окон не было; desktop не нужен, worker оставить. Далее actor v8 real IPC/cancel,
health/rehearsal/approval, exact shutdown и runtime install. Весь U1 остаётся открыт.
