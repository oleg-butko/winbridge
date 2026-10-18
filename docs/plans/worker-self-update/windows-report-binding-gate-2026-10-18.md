# U1: durable compiler report — Windows gate

2026-10-18. Clean source `914df7c5b8217044768ec1aa162e2ff2928728dd`.
Run `20261018-110211-6ecc2e54`, Task
`65d0be5f-474f-43e0-8341-a6ae4025a164`: succeeded, exit 0.

Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261018-110211-6ecc2e54`.
Все 29 файлов `report.ready.json` сверены по bytes/SHA-256.
SHA-256 `report.json`:
`82571060e9c279b8fefa8d7524be0c3fdb6be4e17ce87cabb0d974be87dd452c`.
Архив `winbridge-20261018T150202Z-914df7c-bd1b5353.zip`, SHA-256
`82d8e857186f1179ab3b959db5c85c605d482422cf944ce7233313b8eec7228f`.

Пройдены 139 native tests / 15 groups, полный Rust/desktop build и четыре exe
artifact queries. Шесть report regressions проверяют intent до dispatch,
atomic completion при ignored report/completion writes, duplicate без replay,
same-size edit/file/parent replacement/hard links, missing/empty/oversized report,
существующий staging и migration 5 → 6 с backup/epoch/no-adoption. Отдельный
PowerShell compiler actor подтвердил собственный report receipt до safe abort.
Schema-5 reference updater из прежнего successful candidate отказался открыть
schema 6 без записи. Worker journal schema не менялась.

`compiler-report-binding-proof.json` содержит schema-6 snapshot и exact-byte
receipt fixture; `compiler-actor-proof.json` — receipt настоящего PowerShell
fixture. Полная candidate сборка отдельным build-only сценарием проверена отдельно;
она ещё не является full self-update через compiler actor. Это различие сохраняется.

Windows free before/after: 25,319,215,104 / 25,107,931,136 bytes.
Debian до native gate: 33,075,818,496 bytes. Проверенный cache owner прежний
`20261017-061527-f4639971`; Windows validation directories не удалялись.

Installed worker source `1f3aa9f`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` остался прежним. После gate: activeTaskCount 0,
complete recovery coverage, journalRecoveryRequired/maintenanceError отсутствуют.
Окон не было. Desktop можно оставить закрытым, worker — работающим.

U1 целиком остаётся открытым. Далее: exact artifact set / old runtime guards,
candidate rehearsal, drain/exact shutdown и U2–U3. Public install и bootstrap
пока отключены; рабочие src/runtime/config/journal не изменялись.
