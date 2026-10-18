# U1: artifact set — native результаты и исправление сценария

2026-10-18. Clean source `94d9a35d76bb00b69c7ca2d5790d2ee08855ffe3`.
Run `20261018-145111-7277b3db`, Task
`cabd8b2c-1b85-4be7-a24f-bd70dd0a7075`: failed/1.

Пройдены все 149 native tests / 15 groups, включая schema 6 → 7 migration,
previous updater refusal, exact file/role contracts и atomic artifact receipt/FSM.
Полная сборка Rust/native desktop и четыре actual executable queries прошли.
Финальный вызов `inspect-candidate-files` не состоялся: PowerShell использовал
несуществующую переменную `$candidateHelper` вместо `$queryHelper`.
Поэтому **overall gate не пройден**, несмотря на успешные предыдущие проверки.

Все 29 ready files сверены по размерам/SHA. Shared:
`K:\VMs\shared\WinBridge\workflow-validation\20261018-145111-7277b3db`.
Windows free before/after: 24,547,835,904 / 24,146,214,912 bytes.
Debian перед gate: 33,028,091,904 bytes. Cache owner прежний
`20261017-061527-f4639971`; Windows validation directories не очищались.

Отдельная readonly диагностика Task
`b95883dd-ce36-4875-9da8-44e8f2bc31e9` succeeded/0: уже собранный candidate updater
выполнил `inspect-candidate-files` на прежних actual четырёх exe и точном report.
`candidate-files-diagnostic.json` содержит stdout proof; исходный report скопирован
в `diagnostic-candidate-report.json`. Эти два поздних файла **не входят** в прежний
ready manifest, который не менялся. `journalBound`, `candidateVerified`,
`runtimeApplied` false; проверка не заменяет production worker health.

Исправлен helper variable. Новый portable regression исполняет настоящий final
try/finally statement build script с metadata mock: bound helper/argv, rejection
неверного контракта и неизменность опубликованного report. Прошёл на Debian
PowerShell; включён в следующий native run до полной сборки. Full повтор будет
объединён с новым supervisor v5 artifact actor gate.

Installed worker source `1f3aa9f`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` не менялся; после отказа idle 0,
complete recovery coverage, journalRecoveryRequired/maintenanceError отсутствуют.
Диагностика не создавала окон. Desktop можно держать закрытым, worker оставить.
U1/install/public capability не завершены; bootstrap пока не требуется.
