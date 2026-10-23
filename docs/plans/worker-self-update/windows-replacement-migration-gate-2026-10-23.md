# Actual Prepared14 →15 и old14 refusal: Windows gate пройден

2026-10-23. Task `11987f79-3786-4155-aa39-6277d74d6950`, run
`20261023-092901-d1c837e1`, terminal succeeded/exit0. **1 native integration test**,
6 ready files проверены независимо дважды,27 искажений actual proof отклонены.

Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261023-092901-d1c837e1`
(`/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-092901-d1c837e1`).

| Файл | SHA-256 |
| --- | --- |
| report.json | `9721059757178c2e835e74322e31a4e5682b649d10725e653ec7d0eb98fc9ddd` |
| runtime-replacement-migration-proof.json | `1f186da42fa2eefd7d823c9be93e1da6395911ebb386e656f533cbb8ac4d7093` |

Clean producer `aaf721f2fbccacdb3ea15c90485fa755917e2a44`, archive
`winbridge-20261023T092826Z-aaf721f-f4eb7d96.zip`, SHA
`ad01c14b70cb7eab455d64b9777393f120c4dc9f3488437c68c5878ace4d6a0b`,
manifest SHA `0ac1ec63ed5bcf4b447e6aafa81d0365160bbef33da40deb4fbf9039ce12e842`.
Mode `--replacement-migration-only` / `-ReplacementMigrationOnly`, updater15.

## Исходный14 reference и сохранность

Reference — actual successful Main14 run20261023-012740-9feb95b9/source9a02ca9,
полный proof SHA `1bdf7d61ed4da331c4c5d3a3516d31b3872c0db452635e617acaa046a3d56c8b`.
Перед pin снова сверены16 ready files и approved preparation verifier. Original
known manifest SHA `d115eb72489b7af1729e6aaba4b1cc9fbb7c575eaff6d75eed099eeb60e11200`.
Canonical SHA всего Prepared record:
`a2b823d95c1a6cee157cbb8f9387df309d8dccca4eadec08b0c9877ee1638029`.
Этим SHA связаны original14 observation, migrated15 record и public proof.

SQLite backup API создал отдельную consistent-копию реального14 journal. Migration
сохранила все old SQL rows, включая preparation intents/current/history, и exact
typed Prepared/Verified-backup/shutdown. Installation/scope прежние, migration
epoch2→3; backup14 имеет исходные rows/epoch, integrity ok. После нового owner
backup и preparation вернулись как Existing; fresh replacement без original guard
отказан. Все три replacement tables остались пустыми. Saved state не стало custody.

Actual old14 updater проверен через artifact query и exact raw known manifest.
Он запущен только с новой изолированной15 config для `journal init/status`: оба
оператора отказали UnsupportedSchema. Bytes DB/nonempty WAL копии не изменились.
Original14 reference оставался readonly; его DB/WAL bytes/info сохранились.

Python независимо проверил manifest/executable/source pins, original Prepared SHA,
schema/epoch и flags сохранности/no permission/refusal.27 negative mutations
покрывают raw manifest/updater/source proof, epoch/schema, record/receipt SHA,
owner, history digest и false preservation/refusal/custody/runtime/UI outcomes.
Пустой newly-created WAL трактуется как отсутствие payload по прежнему правилу;
SHM coordination не объявляется изменением journal data.

## Границы и продолжение

Это закрывает **actual14→15 migration/refusal**, не positive fresh admission под
живым original guard, consuming handoff, rename IO/actor или activation. Следом
передать original exited-process/state/instance-lock custody единственному
replacement driver, добавить transition-aware physical checks и только затем
success receipts. Не ослаблять старый whole-runtime `ShutdownExitGuard::require`.
Public installation и U3/U4 ещё не готовы.

Windows available37,879,934,976→37,704,318,976 bytes; Debian при сверке
26,805,424,128 bytes. Approved cache reused; retained directories не удалялись.
Local Windows all-target Clippy/fmt/Python7/0/scaffold прошли; production code
после647/0 workspace не менялся. Тестовых UI окон/desktop/candidate build не было.
Installed d052 worker прежний, connected/active0/no errors. Desktop можно закрыть,
worker оставить; ручного обновления сейчас не требуется.

Полные pins/public proof:
`.winbridge/update-preparation/replacement-migration-native-verified.json`;
source reference — `replacement-schema14-reference-pin.json` в том же namespace.
