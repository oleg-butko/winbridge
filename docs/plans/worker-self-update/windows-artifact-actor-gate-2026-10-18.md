# U1: artifact actor и installed-set reader — Windows gate

2026-10-18. Clean source `48b8d21798c3deae2889db94f00f4428986f972c`.
Run `20261018-153922-df4b0630`, Task
`eb05e4e8-7459-4079-a5ef-6341fbe53170`: succeeded, exit 0.

Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261018-153922-df4b0630`.
Все 36 файлов `report.ready.json` проверены по bytes/SHA-256.
SHA-256 `report.json`:
`e739a2b2d3fd5899161d52404d5a1a713152280ffa10e922b7ced45c5b32df23`.
Архив `winbridge-20261018T193913Z-48b8d21-4d43ca40.zip`, SHA-256
`553ef7821490c2eb7f24b9a11573cca7b106e85f7c1f9f4743a542a91555798d`.
Source manifest SHA-256:
`15269fed9e53af667529894248c4061f64cb7f0f9579a96965cd70a74e7f5832`.

Пройдены 156 native tests / 16 groups, полный Rust/native desktop build,
четыре actual artifact queries и readonly exact-file inspection кандидата.
Проверены schema 6 → 7, atomic artifact receipt/FSM, известный installed set,
manifest/file pins и deterministic abort snapshot race. Старый schema-6 updater
отказался открыть новый journal без записи.

Два explicit artifact actor tests подтверждают responsive mailbox, duplicate без
повторного compiler, четыре настоящих metadata queries fixture exe, запрет raw
health/release bypass и coordinated abort с candidate digest. Отдельный negative
actor отклоняет неполный report без принятия artifact receipt, запрещает повтор
и корректно abort/release. Эти fixture exe не являются WinBridge health gate.

Portable PowerShell regression выполняет реальный final try/finally build script
с явно помеченным metadata mock. Финальная exact-file проверка полной сборки
использует настоящий updater и четыре WinBridge exe; её proof сохранён отдельно.
Исправления `$queryHelper` и staging compiler outputs в target подтвердились.
Запрет лишних файлов payload сохранён. Предыдущие failed runs остаются evidence.

Windows free before/after: 22,802,182,144 / 22,593,060,864 bytes.
Debian перед native gate: 32,987,934,720 bytes. Cache owner прежний
`20261017-061527-f4639971`; Windows validation directories не удалялись.

После gate installed worker source `1f3aa9f`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние: activeTaskCount 0, complete
recovery coverage, journalRecoveryRequired/maintenanceError отсутствуют.
Рабочие src/runtime/config/journal не менялись. Окон не было; desktop можно
оставить закрытым, worker — работающим.

Закрыт native gate перечисленных блоков, весь U1 остаётся открытым. Следом:
проверка known runtime при startup, authenticated live identity, baseline до
SourceApply, candidate rehearsal и exact shutdown. Runtime replacement, activation
и U2–U3 впереди; public update и manual bootstrap пока не включены.
