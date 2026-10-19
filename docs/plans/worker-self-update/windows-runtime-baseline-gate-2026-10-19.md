# Windows gate: durable runtime baseline / supervisor v7

2026-10-19. Clean source `b21aa28efbc7ef82f0afaedf1ed15d9b059c5613`,
run `20261019-062511-6aceb224`, Task `84ef11e1-650c-4aa3-89f2-9f2ad2027557`:
**succeeded / exit 0, 175 native tests / 19 groups**.
Updater schema 8, worker schema 15 в candidate; schema-9 preimages сюда не входили.

Архив `winbridge-20261019T102452Z-b21aa28-905a0a84.zip`, SHA
`70483debe6014eefd790076de5600a304bda62487c18cccdc5247db8238aa07f`.
Все 43 файла report.ready.json проверены по размеру/SHA. Report SHA:
`7245c974cc498b5781712744a690a656a81cdd86db304503297c0d2d7d7d9494`.
Windows free before/after: 21,924,835,328 / 21,591,035,904 bytes.
Повторно использован исходный compiler cache `20261017-061527-f4639971`;
все тесты выполнены заново. Validation directories не удалялись.

Проверены immutable baseline intent/receipt с atomic parent transitions, ignored
writes/rollback, forged binding/root/file identities, owner restart, retention и
migration 7→8 без adoption. Previous schema-7 updater отказался от schema 8 без
записи. Legacy wire/projection shapes сохраняются.

Positive copied-process fixture использует собственный Worker actor/guard и real
supervisor/control IPC. Проверены intent до ACK, held worker observation с доступным
status, duplicate без второй попытки, запрет source admission, receipt перед source,
независимость source A / binaries C, отсутствие lease/slot и historical duplicate
при изменённом config. Это test harness; другие role descriptors synthetic.
Он не является health/rehearsal четырёх реальных приложений.

Полная Rust/desktop сборка, четыре artifact queries и exact payload files passed.
Actual normal Main повторно проверил readonly startup и отказ до worker journal
при invalid manifest/descriptor/config. Отдельный serve PID 19268 удерживал пять
pins; после forced owned idle stop они освободились, bytes/updater DB сохранены.
Lifetime 10,914 ms, без задач/network/UI. Graceful shutdown/activation не заявляются.

Первый source-34f39ed run прошёл все 175 tests, но final script всё ещё требовал
schema 7. PS и Python исправлены: expected schema берётся из verified source/archive,
а не из текущего checkout и не из minimum-version условия. Portable mismatched/
ambiguous-version regression passed; этот повтор завершил весь combined gate.

Для будущей проверки отказа старого updater сохранён schema-8 candidate:
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261019-062511-6aceb224\update-candidate\payload\winbridge-update.exe`,
SHA `4b1eefef24a3b658a78bf75241aae08ac3d4ed57723f0fe06edd30251e8ccc26`.
Его image bytes/ID/hash входят в проверенный candidate-files proof.

Installed worker не заменялся: прежний source 1f3aa9f/build 1792228466051801600,
generation `994671b4-e10c-469c-8515-b2aa71a4e3dc`. Fresh Info: connected,
activeTaskCount 0, error null. Окон не было, desktop не нужен; worker оставить.
Далее preimages и их actor, health/rehearsal, approval и exact shutdown. Весь U1,
public update/install и ручной bootstrap пока остаются впереди.
