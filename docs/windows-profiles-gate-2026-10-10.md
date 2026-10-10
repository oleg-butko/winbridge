# Windows gate: общие профили рецептов — 2026-10-10

**Passed: 68 native tests**, пять файлов отчёта проверены по actual bytes/SHA-256
из Debian. Запуск через installed worker; новый isolated runtime, без desktop
окон и без замены рабочего exe/config/state. Первый сценарий этапа 7 завершён;
это не закрытие всего этапа. [Контракт](project-profiles.md), [ревью](review-stage7-2026-10-10.md).

Run: `20261010-060810-0996d143`.
Outer task: `6ef13e91-8cc2-4766-ac25-b6a6097e01b2`, succeeded, exit 0.
Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261010-060810-0996d143`.

Source clean `fa7e4d79d9dda440dd84180117e9ecf30af97600`:
`winbridge-20261010T100754Z-fa7e4d7-827eab23.zip`, 253 files, built UI,
3 619 262 bytes. ZIP SHA-256
`422d9f07948f5d4833ef54a9c71a05161c46794d3647daa4cc0ce612fb109f77`.
Native worker source manifest SHA-256
`26b0ebb7371588c40579dadd163172ecc128fb5628c7c5b088fa57919a52a11b`,
buildId `1791626927805572800`. Source/archive identity сверены driver/harness.

| Группа | Native tests | Результат |
| --- | ---: | --- |
| Core и runner profile policy | 29 | Boundaries, bindings, argv/timeout, collisions, metadata budget |
| Два проекта через настоящий IPC | 1 | Реальное выполнение, dedup/conflict, cancel, owned crash/restart, provenance |
| Runtime/snapshots/workflows | 38 | Tree/timeout, pinned source, workflow lifecycle и recipe pin regressions |

В общем профиле `common` два проекта имеют разные cwd/bindings. Успешные tasks:
first `4b469217-49f1-489a-b031-f588bcac0ac8` (PID 14996), second
`48c66225-7270-4195-a857-a7a9949c8b9c` (PID 2292). Exact argv включают Unicode,
пробелы, кавычку и literal `$(literal); & quote"`; fixture проверяет их как отдельные
аргументы, без shell interpretation. Same project-scoped key возвращает прежнюю
task и не добавляет run; другой payload с тем же key отклоняется. Direct запрещён.

Tree task `016559ec-4dff-4451-a219-8bea5c15ba66` cancelled. После намеренного
падения только тестового worker task `5658a894-3e6c-44e0-b4fa-04249ccfbcb2`
восстановлена как indeterminate, прежний key не повторяет процесс. После изменения
рецепта queued task `fc77c187-83ef-4436-b75e-51c8d41f1244` failed без PID;
исторические commands/origins сохранились. Новый явный запрос
`de64a4b1-655b-4b3d-a7a7-b531f7ae7480` succeeded с новой profile label.

Native workflow tests отдельно подтвердили recipe pins до ACK: changed queued
workflow не начинает import/build/start, legacy queued recipe требует нового
запроса, recovered running workflow после завершённого import не раскрывает
изменённый build. Уже accepted child сверяется по durable key до этой проверки.
Успешный profile build использует общий runner и сохраняет происхождение child task.

Report SHA-256:
`6f8d1abcaa472a0e20a51b2b364d29a675b3de9dd148651f220e93bd399a6581`.
Ready SHA-256:
`ce25fd777ca41249acc559d692fc30d0e83e7842c89d1a7f1d0cffa6795609bc`.
Profiles proof SHA-256:
`12d9140a31f01d951d536bd40e32a3d9a7677c69bbb96d69dd178b8387f73528`.
Пять verified artifacts — report, proof и три group logs. Ready появляется после
завершения; прежние shared reports не менялись. Наблюдения не являются benchmark.

Debian: **152 full workspace tests**, Clippy `-D warnings`, Windows all-targets
compile, fmt, DTO, TypeScript, UI build и full browser integration проходят.
Browser с настоящим Unix worker проверил profile preview/origin, recipe workflow
build/pins, malformed Settings JSON, shared timeline и сохранение форм. Новый
Windows UI visual gate не заявляется; desktop окна не открывались, ручная проверка
не требуется для этого backend gate.

Installed worker остался clean af90ec0, generation
`c182793f-5391-4bf8-9e70-9c94b8b24a5c`, healthy/activeTaskCount=0; тестовые процессы
завершены. Рабочий worker оставить запущенным; desktop можно держать закрытым.
После gate controller Debian обновлён до clean fa7e4d7; worker reconnect подтвердил
прежнюю generation/health. Для применения новых profiles/output/resume в рабочем
runtime нужен обычный общий update из указанного ZIP и prepare-windows-snapshot.ps1;
установка не выполнялась тестовым сценарием.
