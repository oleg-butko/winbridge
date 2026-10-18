# Executable identity и bounded probes — U1, 2026-10-18

Четыре устанавливаемых exe получают readonly `--artifact-info`: role
(worker/fixture/desktop/updater), query contract 1, прежний BuildIdentity,
compiled OS/architecture. BuildIdentity/legacy RPC shape не меняются. Query
обрабатывается до config, journal, fixture counter, diagnostics, tray и Tauri.
Неподдерживаемые аргументы этого query завершаются ошибкой, не запускают UI/work.
У desktop прежде не было такого CLI; Native build/query проверка ещё впереди.

В поддерживающем exe сохраняется framed capability marker. `artifact::inspect_image`
читает local regular file потоково (до 256 MiB), связывает SHA-256/размер/File ID и
проверяет marker в том числе на границе блоков. `artifact-query IMAGE ROLE` не
читает updater config и **не запускает** файл без marker. Это предотвращает
открытие старого desktop при неизвестном ему CLI флаге. Marker обозначает поддержку,
не заменяет trusted source/build policy и проверку SHA.

Поддерживаемый query запускается с единственным `--artifact-info`, null stdin и
раздельными stdout/stderr. Windows использует atomic suspended Job launch и
stdio whitelist. Timeout 10 секунд, максимум 64 KiB каждого потока; дерево
закрывается и проверяется, readers должны закончить. Wrong role/target/schema,
invalid JSON, unexpected stderr, quota, timeout или unknown cleanup не возвращают
успешную квитанцию. Файл pinned на время проверки; hash/File ID сверяются повторно.
Query report всегда имеет `candidateVerified=false`: это identity, не health check
нового worker и не разрешение остановить установленную версию.

Build-only script выполняет эти probes **внутри managed compiler tree**, после
компиляции и до его завершения. Отдельный probe journal не нужен: compiler intent,
PID/Job, terminal/output и физический cleanup покрывают всех этих потомков.
Script проверяет role/source commit/manifest/clean flag/version всех четырёх exe,
пишет command/log и identity report, затем сверяет staged payload SHA/bytes с
проверенными файлами. Отдельные build IDs debug/release exe могут различаться.
Новый updater helper берётся из только что собранного target; installed runtime
не запускается и не заменяется.

Следом нужен независимый durable artifact-set receipt в updater, включающий
идентичность runtime/candidate roots, полный новый/старый набор и guards перед
cutover. Успешный script/report сам по себе не переводит parent в CandidateVerified.
Verification boot, worker/desktop shutdown, file replacement, recovery UI и public
MCP update всё ещё отключены. Bootstrap установленного worker пока не требуется.

Local gate: 460 workspace tests, 11 preflight tests, metadata queries worker/fixture, Linux/Windows Clippy all-targets, fmt, contracts, typecheck/build:ui/scaffold, 8 export tests и script syntax passed. Native gate ожидается.
