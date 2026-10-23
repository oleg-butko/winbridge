# Preparation model/ledger: Windows gate пройден

2026-10-22 America/New_York (run timestamp UTC2026-10-23).
Task `1ecc73f3-4170-49c5-97fa-7f7e29b4fee1`, run
`20261023-005847-b276db76`, succeeded/exit0. **10 native tests / 1 group**,
все 5 ready files проверены независимо дважды.

Report directory:
`K:\VMs\shared\WinBridge\workflow-validation\20261023-005847-b276db76`
(`/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-005847-b276db76` в Debian).
SHA-256 `report.json`:
`92a3119ee3ca3a2049260bc4cbe03e46046744b2300ba8b3c385f862cf2cc17a`.
Локальная сводка: `.winbridge/update-preparation/runtime-preparation-native-verified.json`.

Producer clean `cee06a7f5a8be621532a3859ba98a32883b6d05c`, архив
`winbridge-20261023T005835Z-cee06a7-88d49ac8.zip`, archive SHA
`ce86846bbe82153286744a769b188eb3ff4dece82656e9af62d5328a29543272`,
manifest SHA `6038e61686046c1ea45fa8307c2b10ee5aeacba9484b71dacc2d06f8db599b22`.
Updater14/worker16; supervisor остаётся v15/view7.

Проверены реальные Windows SQLite transactions для synthetic ledger metadata:
immutable intent/history, CAS, полный causal readback, rollback ignored writes,
reopen issued copy и terminal uncertainty, tampering detection. Тест13→14
проверяет consistent backup synthetic schema13 и сохранение прежних root rows.
Data FSM проверяет directory/file intent-before-receipt, fixed five-file order,
independent File IDs, точные bytes/hash/type/manifest, unknown fields и no reissue.

Это **не** actual13 executable refusal и **не** положительный сквозной admission
полного Main/source/build/health/shutdown/backup графа. Fresh preparation driver,
actor dispatch, actual copies и faults при IO ещё не реализованы. Данный gate
также не проверяет rename/activation/desktop/bootstrap и не закрывает весь U2.2.

Локально перед export: `cargo test -p winbridge-update --locked`199/0,6 ignored;
Linux/Windows all-target Clippy с update-validation passed; fmt/Python compile/
scaffold links passed. Сначала6 прежних migration fixtures оставили новые14 tables
при synthetic downgrade; fixtures исправлены, production checks не ослаблены.

Windows available48,141,598,720→48,128,479,232 bytes. Debian после проверки
27,234,304,000 bytes. Reused approved cache `20261017-061527-f4639971`;
validation directories/reports не удалялись. Full desktop/candidate build не нужен.
Новых UI окон не было. Installed d052 worker здоров, active0, generation прежняя;
runtime/config/journal не изменялись. Desktop можно открывать/закрывать, worker
оставить работать. [Следующий блок](runtime-preparation.md#следующая-вертикаль).
