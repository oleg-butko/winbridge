# U1: compiler actor, abort, output и artifact identity — Windows gate

2026-10-18. Clean source `a91b904b7f120adad8aa669e1e485f0b65b746f7`.
Run `20261018-103821-c88c4d20`, Task
`5c1e0a9b-c20c-4642-b596-653eebc1468d`: succeeded, exit 0.

Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261018-103821-c88c4d20`.
`report.ready.json` содержит 28 файлов; их размеры и SHA-256 проверены на Debian.
SHA-256 `report.json`:
`31997fe35ce54e8f77232b4b8f01659bf5a814c686db7321963cef4ff07c702a`.
Архив: `winbridge-20261018T143811Z-a91b904-68ef48f3.zip`, SHA-256
`8bcb75119d8151adbf7d5a1b64194e6455f328744ac664163432e88940a5ca0f`.

Пройдены 133 native tests в 15 группах: source/FSM, journal/preflight,
legacy contracts, worker coordination, previous-version refusal, no-clobber,
supervisor IPC/source staging, infrastructure/atomic process launch, compiler
actor, coordinated abort и bounded output. Полная сборка Rust и native desktop
прошла. Четыре независимые payload copies успешно ответили на `--artifact-info`:
правильные роли, Windows x86_64, clean commit и единый source manifest
`49d6238dca557e9a4ee6590494f3c2a31cebe8a34adad3a41c02e55cb723b145`.
У каждой query подтверждены PID identity, cleanup, размер и SHA-256 файла.
Desktop metadata query не создавала окон.

Предыдущий run `20261018-101634-46ac2235` прошёл те же тесты и сборку, но
правильно отклонил hard-linked Cargo output. Исправление проверяет отдельные
create-only payload copies; запрет hard links сохранён. Portable regression
подтверждает, что изменение build-cache alias не меняет payload.

Свободное место Windows до/после: 25,633,300,480 / 25,443,999,744 bytes.
Debian перед сборкой: 33,091,391,488 bytes. Переиспользован проверенный cache
`20261017-061527-f4639971`; Windows validation directories не удалялись.

Installed worker не изменялся: source `1f3aa9f`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc`. После gate: activeTaskCount 0,
complete recovery coverage, journalRecoveryRequired/maintenanceError отсутствуют.
Desktop можно оставить закрытым, installed worker — работающим.

Это завершает проверку перечисленных блоков, **не U1 целиком**. Candidate proof
остаётся `candidateVerified: false`, `runtimeApplied: false`. Далее: durable
binding compiler report/artifact set, exact shutdown, activation/health и U2–U3.
Public update и изменение рабочего runtime отключены; bootstrap пока не нужен.
