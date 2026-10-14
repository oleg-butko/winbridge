# A3.2 — evidence конкретной пары и разрешение контрактов

Дата: 2026-10-14. После [A3.1 catalog](discovery.md).
A3.2 начат; [A3.2a foundation](readiness-foundation.md) реализован локально.
Runtime producer/readiness API/enable ещё не подключены; полный A3.2/A3 открыт.

## Последовательность исполнения

1. Обновить installed Windows worker по постоянному пути, перезапустить Debian
   controller новой версией. Проверить фактические build identities и прохождение
   catalog/context через Windows → WS → controller IPC → MCP. Old/mixed route
   показывает только доступный legacy режим; отсутствие catalog не восполнять
   предположением по version string.
2. Ввести typed identity проверяемой пары: contract/schema digest, consumer build,
   provider build, request context, route/hops и fingerprint policy проекта.
   Build evidence и paired evidence отдельные. Receipt сохраняется create-only
   на local disk с checksum/readback; failure/partial receipt не даёт enable.
3. Добавить read-only readiness projection и общий evaluator: supported →
   build verified → pair tested → enabled текущей policy/fresh route. UI/CLI/MCP
   показывают каждое основание и конкретную причину запрета. Existing A2 RPC
   policy не заменяется catalog и не получает скрытого ограничения задним числом.
4. Провести actual Windows consumer/provider tests на изолированных данных,
   сохранить evidence. До этого новые execution contracts отключены. Затем A3.3
   вводит bounded consistent operation pages и отдельный archive read model.

## Инварианты и ревью

Receipt — evidence проверенного поведения, не security credential. Само
advertisement никогда не выдаёт permission. Scope, policy, artifact/hash или hop
не совпали — readiness объясняет mismatch до нового effect. Unknown contract/
schema не включается через legacy fallback. Idempotency namespace остаётся по
stable operation family; negotiated contract сохраняется с логической операцией.

Disconnect/reconnect сбрасывает live enable. Историческая квитанция остаётся
историей; возвращение разрешения требует свежей проверки identities/context/
route/policy. Старые принятые запросы не перепривязываются и не перезапускаются.
Для changed build/route/context нужна новая pair verification, а не перенос флага.
Session UUID — freshness, не durable journal incarnation и не proof тестирования.

Native A3.1 проверяет candidate IPC и reference compatibility, не объявляет
pair-tested installed route. Исторический installed worker schema 6 не передавал
fenced scope/catalog. Пользователь обновил его до `f128c9a`; actual Windows
WS/IPC/MCP smoke прошёл. Для A3.2b нужен следующий worker с policy fingerprints. Использовать
новый временный exe для outbound WS означало бы снова менять firewall path;
по решению пользователя обновляется `K:\_proj-26\winbridge\runtime\winbridge-worker.exe`.
Автоматического self-updater вне managed process tree сейчас нет; обновление
делает пользователь после остановки worker, без изменения firewall/services.

Проверки: wrong/missing build, unknown metadata, old/new hops, bad receipt/hash,
reopen/dedup receipt, changed context/policy, reconnect freshness, no effect при
disabled contract. Настоящая paired проверка обязательна после ручного обновления;
повторять ручной UI checklist для этого пакета не требуется.
