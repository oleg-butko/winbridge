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
AST прошли. Linux Clippy также passed.

## Успешный повтор

Clean source `5a1e5a82408cfad275686ae2b0b8305831d4be74`, archive
`winbridge-20261014T084103Z-5a1e5a8-95e82178.zip`, SHA-256
`3d4cdfd010d777978254c227e6f49d4ccbeae28eeea55c3bfada34d9c03a7ea7`.
Shared run `20261014-044152-738951fb`, task
`4c1e4e02-59e0-4b99-82f2-8e5acc5079b3`, PID 11284, succeeded/exit 0.
UTC 08:42:01–08:48:22. **197 native tests**, все 7 групп и native desktop check
passed. **9 artifacts** проверены по размеру/SHA-256; report SHA-256
`bbd5bd3818952068452b37bec434c5fc894d82d082d373acab339379256bc6f3`.

Реальные Windows checks: archive rows/output, explicit activation, v1/stale scope
refusal до effect, same-key dedup после restart, partial/corrupt metadata,
unlisted live process при пустой task history и цепочка A→B→C. В canonical-path
regression journal context сохранился; UNC/device syntax запрещён.
Reference schema-6 exe отказал schema 12 с exit 1; isolated journal byte-identical.
Candidate manifest SHA-256
`800e265900efab86623ec205673866b23670f1bfadf7022491c03b75216c80a2`.

Installed worker generation `02939593-8adf-4878-a346-473092697a4a` и session
`631c2e63-edc8-44f8-8cce-89abec7caf8b` сохранились, connected/idle, без journal/
maintenance errors. Desktop закрыт, UI-окон не было; рабочий worker оставить.

## Storage и итог ревью

Compiler cache прежнего failed run переиспользован под lease; новая версия и
состояния отдельные. Windows free before 30,045,970,432 bytes, after
28,515,418,112 (~26.6 GiB): повтор добавил ~1.43 GiB, включая desktop dependencies,
которые первый failed run ещё не собирал. Старые каталоги не удалены.
Debian после разрешённой incremental cleanup: ~25 GiB free, incremental выключен.

A2.4.2b и полный A2 завершены в выбранной политике archive/new pair. Это не
автоматическое обнаружение rollback всех state files и не сохранение прежней
pair identity после потери keys. Внешний coverage registry сохраняется; missing/
uncovered/unknown domain блокирует activation. Archive сохраняет SQLite output,
не исторические копии внешних application logs. Эти ограничения явно описаны
в инструкции; archive viewer и общий read model относятся к A3.
