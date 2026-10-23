# Runtime preparation v16: actual Main gate пройден

2026-10-22 America/New_York (run UTC2026-10-23).
Task `d9805159-8804-486a-8aae-95ddb6e12e79`, run `20261023-012740-9feb95b9`,
succeeded/exit0. **40 native tests /10 command groups**, включая один полный
Main/source/build/health/drain/shutdown/backup/preparation case.
Все **16 ready files** проверены независимо дважды; **22 искажения actual proof**
отклонены. Сам actual Main integration test занял561.78s.

Отчёт: `K:\VMs\shared\WinBridge\workflow-validation\20261023-012740-9feb95b9`
(`/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-012740-9feb95b9` в Debian).

| Файл | SHA-256 |
| --- | --- |
| report.json | `db9230ef1402bf95290a53db7949f6fb59b642f6cd28ed1dd3b8dba6f73009c5` |
| candidate-health-actor-proof.json | `1bdf7d61ed4da331c4c5d3a3516d31b3872c0db452635e617acaa046a3d56c8b` |

Producer clean `9a02ca92d6ae7aa0b631bf06c5f30444fae0e050`:
`winbridge-20261023T012726Z-9a02ca9-42d7b394.zip`, SHA
`31ffafa6ac8f4bdfdffd41e2fc632a12d0f56cd401ed4c874a36ea67539029fc`,
manifest SHA `ae34117a9c5292116dfc5d26a4a4ec7d0e98eef76123ff4a060c75703cb4e4a2`.
Target clean `cee06a7f5a8be621532a3859ba98a32883b6d05c`:
`winbridge-20261023T005835Z-cee06a7-88d49ac8.zip`, SHA
`ce86846bbe82153286744a769b188eb3ff4dece82656e9af62d5328a29543272`,
manifest SHA `6038e61686046c1ea45fa8307c2b10ee5aeacba9484b71dacc2d06f8db599b22`.
Оба updater14/worker16, проверяемый supervisor v16/view8.

## Что доказано

39 preliminary tests включают16 preparation model/ledger/actual filesystem cases,
1 actual Windows directory pin/rebind test и22 backup/entry/WAL/cleanup regressions.
После них собраны реальные old roles и candidate, выполнены source transaction,
artifact verification и health, drain/quiesce, exact Main exit и consistent backup.

Под живой **оригинальной** custody fresh v16 admission связал сохранённые baseline,
preimages, candidate/health, shutdown и Verified backup. Перед первым запросом
fixture readonly-дождался retirement backup driver. Неверная версия15 нового
action отклонена до создания preparation record.

Driver создал каталог и пять независимых copies, сохранив revisions0→12. Native
status показал Admitted/CreatingDirectory/PreparingFiles/Prepared и files0→5.
Проверены реальные File IDs/bytes/SHA и fixed manifest encoding. Original runtime,
candidate payload, previous copies и worker journal bytes сохранились. Запрос
дубликата вернул ту же attempt/receipt; extra create не выполнялся. V15 view не
сериализует новое поле. После завершения собственного тестового Job сохранились
тот же record и Prepared history, а physical custody=false; normal startup по-прежнему
блокируется maintenance. Raw FilesVerified/install этим не разрешены.

Python verifier дополнительно связал actual file receipts с plan/target, canonical
Set UUID, exact source/parent/root/owner scope, duplicate и post-crash history.
Независимо восстановлены typed manifest bytes и SHA.22 mutations проверяют phase/
revision/attempt/root/epoch/parent/path, source/copy IDs/SHA, counts, active driver,
duplicate, manifest role/build, retained graph и отсутствие custody после exit.

## Ограничения и продолжение

Это **happy actual preparation**, не native IO fault matrix и не whole U2.2.
Synthetic/core faults уже проверены, но actual supervisor crash/unknown outcome
на directory/file intents/receipts ещё нужно проверить отдельно. Actual old13
executable refusal новой14 БД также впереди. Target cee06a7 — limited same-schema
fixture, а не готовый production target со всеми новыми updater capabilities.
До public cutover нужен versioned candidate-updater compatibility check до drain.
Затем consuming replacement custody/rename/manifest-last FilesVerified, activation,
U3 desktop/migration/rollback и U4 bootstrap. [Текущий блок](runtime-preparation-actor.md).

Локально до export: workspace636/0,16 ignored; после path review повторены16
preparation/2 directory tests, обе all-target Clippy; generated contracts/typecheck/
UI build и Windows all-target validation-fixture Clippy passed. Исправлены tmpfs
placement тестовых файлов, canonical config/runtime comparison и lint findings.

Windows available42,525,798,400→41,510,203,392 bytes. Debian перед прогоном около26GiB,
после проверки27,127,623,680 bytes. Approved cache20261017-061527-f4639971 reused;
retained Windows validation dirs/reports не удалялись. Private control/token не
экспортировались. Полный public proof/pins/storage и локальная recheck summary:
`.winbridge/update-preparation/runtime-preparation-main-native-verified.json`.

Тестовых UI окон не было, все тестовые процессы завершены. Установленный d052
worker прежний, connected/active0/no errors; production runtime/config/journal
не менялись. Desktop можно открывать/закрывать; worker оставить работать.
