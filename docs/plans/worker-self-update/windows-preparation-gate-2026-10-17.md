# U0: Windows preparation gate — 2026-10-17

**U0 завершён. Самообновление установки ещё не реализовано.** Проверены readonly
source planning, Update FSM, native CLI и build-only staging. Worker shutdown,
source apply, binary replacement и уведомления обновления относятся к U1–U4.
[План](README.md), [инструкция](../../guides/worker-update-preparation.md).

## Проверенный код и прогон

- Final clean code: `7a7d15f52f252e9498bc0aa634b4435ed8f84a11`.
- ZIP: `winbridge-20261017T102503Z-7a7d15f-81c68745.zip`, 4,417,424 bytes;
  SHA-256 `a87f81a657a32ec0d20aeaffe59ec2048321fa467d2abeb86159bf89e1c5c30d`.
- Run: `20261017-062536-7f2b8b26`; parent task
  `f4009480-b995-4b27-834e-f86f00178a3b` succeeded/0.
- Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261017-062536-7f2b8b26`.
- Report SHA-256: `48448556ffb2574b4421a69255ab9a172697700d46761af01912f46ffd29a925`.
- Candidate proof SHA-256: `952e65e4e3ffd2223244488ebcb6033a02627320a11640d4380e853545a3da51`.
- Build ID: `1792232788024065700`, source dirty false, source manifest SHA
  `61a43b3a9f6492d0ad173551d8e7b56d2a9b6125dc50a6983d236fe752df8784`.

Checksummed report artifacts проверены Debian orchestrator. Все четыре native
groups завершены успешно: 7 core source/FSM tests, 6 native CLI/filesystem tests,
6 frozen legacy contract tests и verifier build. Дополнительно PowerShell stage
test проверяет complete/create-only payload, missing artifact, runtime preservation
и отказ до записи при junction в generated output.

Полная сборка использует `cargo build --workspace --locked` с bundled SQLite и
desktop release/custom-protocol. Source проверяется до и после build; actual
worker descriptor сверяется с clean commit/manifest. Четыре exe копируются в новый
payload с size/hash readback. Desktop и candidate worker service не запускались.
`runtimeApplied: false`, `candidateVerified: false`: тест не выдаёт preparation за
health/contract/install gate новой execution authority.

| Artifact | Bytes | SHA-256 |
| --- | --- | --- |
| winbridge-worker.exe | 16,192,000 | `ef2cee18af78b80b575379e8a1bb9db46928f49013614fbc2eab27a13196b65a` |
| winbridge-fixture.exe | 309,760 | `3216cd962f9b9e685198375d06f8747e8c0135e5a8b357e1dbe577876a80970e` |
| winbridge-desktop.exe | 14,056,448 | `fa60e65dfaabb7c710ae8e7549835ccc16b4372e32c0963f7b4b55764f556e6e` |
| winbridge-update.exe | 1,185,280 | `f569a8b08e55b38daeb2b1e8ef349a065e21e57182a5372ef855d6e13d4e953d` |

## Исправления по результатам ревью и исполнения

Первый run `20261017-061527-f4639971` на `b321719` прошёл 19 native tests и оба
build шага, но корректно отказал на post-build verification: Tauri создал четыре
`gen/schemas` файла. Они ошибочно считались unmanaged inputs. `6cb2574` исключает
узкий generated Tauri namespace и Python/root release caches из source ownership;
manifest не может включать эти paths. Relevant 13 Rust tests/Clippy повторены.

Run `20261017-062225-b5cc22df` на `6cb2574` прошёл полностью, report SHA
`aef4c6aa73e371cecef88e529ddcb939d1896c0f44ad7293334857e571004160`.
Дополнительный review выявил разницу между сохранением cache и разрешением
compiler writes через links. `7a7d15f` добавляет pre-build проверку известных output
roots и отдельный junction/symlink regression. Final run проверил это на Windows.

Повторы используют **только compiler caches** первого terminal run, по проверенному
report/accepted task identity и с exclusive lease. Новые source/state/staging/report
создаются отдельно; исходный report не переписывается. Artifact identity проверяется
по новому source manifest, cache не считается доказательством соответствия binaries.

Локально `b321719` прошёл 360 workspace tests; затем 13 relevant tests после cache
fix. Clippy, fmt, Rust→TS contracts, typecheck/build:ui/scaffold, Python compilation
и portable PowerShell checks passed. Native final проверяет тот же core code и
последние PowerShell guards. Ни schema worker journal, ни frozen RPC не менялись.

## Рабочая установка и место

Read-only наблюдение до/после: working worker clean `1f3aa9f`, build
`1792228466051801600`, generation `994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние.
После прогона active tasks 0, maintenance/recovery errors отсутствуют. Installed
runtime/source/configs не изменялись. Windows UI-окон не было; ручного checklist нет.

Windows free final before/after: 46,392,893,440 / 45,821,747,200 bytes.
Первый cold build: 50,997,256,192 / 47,137,906,688 bytes; compiler caches сохранены
и переиспользованы. Debian before/after: 27,626,741,760 / 27,201,228,800 bytes.
Incremental выключен, новых Windows directories не удаляли; порог 6 GiB не достигнут.

Worker оставить работать, desktop можно держать закрытым. Ручное обновление сейчас
не требуется. Следующий пакет — U1 durable source transaction/supervisor/maintenance;
production bootstrap выполняется только после isolated U1–U3 gates. A4.3 installed
gate остаётся отложенным по решению пользователя.
