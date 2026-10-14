# A2.4.2b.2/b.3 — Windows recovery pair gate

Дата: 2026-10-14. [Контракт](recovery-pair.md).

## Первый прогон и причина остановки

Clean source `c2c58c5e05601f68d781b1a9d79529d7ea456399`, archive
`winbridge-20261014T081854Z-c2c58c5-f3064a0a.zip`, SHA-256
`22eb49293397feaaffb271b303acde0acea36555ef0996ea05f0193f7e2a87c4`.
Shared run `20261014-042057-8075f8f1`, task
`d43ccf22-2378-4d64-bbfb-d3ef3a0bcab7`, PID 18096, failed/exit 1.
Шесть report artifacts проверены по размеру/SHA-256.

Компиляция успешна, первые четыре группы passed. В `recovery_pair` 7 passed,
3 failed при открытии активированного target: canonical Windows path
`\\?\K:\...` ошибочно распознан как UNC. Это candidate bug; gate остановлен,
reference-worker и desktop check ещё не выполнены. Installed worker исправен.
Пользователь разрешил исправление и автоматический повтор с compiler cache.

## Исправления и подготовка повтора

Windows `Prefix::Disk` / `Prefix::VerbatimDisk` принимаются как локальные,
`UNC` / `VerbatimUNC` / device prefixes отвергаются **до filesystem probe**.
Actual local fixed-volume check сохраняется. Native regression открывает тот же
journal через canonical path, проверяет неизменный context и pure refusal UNC.
Ревью также исправило ложный повторный activation success у retired source:
retirement теперь проверяется и в CLI activation до чтения старого receipt.

Cache reuse не использует прежние исходники, состояния или evidence. Driver
проверяет checksum receipts и terminal task; Windows проверяет source identity,
cache ancestors/local disk и exclusive lease. Cargo пересобирает изменённые
компоненты. Новый shared report создаётся отдельно, прежний failed report остаётся.
280 workspace tests, Windows-target Clippy, 6 cache receipt tests и PowerShell
AST прошли. Native повтор подготовлен; этап до его завершения не закрыт.
