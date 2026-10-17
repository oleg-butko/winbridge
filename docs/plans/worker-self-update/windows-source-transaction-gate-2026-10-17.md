# U1: native Windows gate файловой транзакции

2026-10-17. [Контракт, crash inspection и ограничения](source-transaction.md).
**Passed: 65 native tests / 9 groups**, actual ZIP verification, script staging
checks и полный build/staging четырёх candidate exe. Установленные source/runtime
не менялись; desktop не запускался. Полное самообновление U1–U4 ещё не готово.

## Идентичность и отчёты

- Clean code: `fb3752c863746ceb68e21ac0eb95640f502fe845` (`feat(update): journal guarded source file transactions`).
- Run: `20261017-145136-0c69deed`.
- Installed worker task: `8cf6da5f-007b-4775-8646-1d86e9e75747`, succeeded / exit 0.
- Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261017-145136-0c69deed`.
- Source ZIP: `sources/winbridge-20261017T185128Z-fb3752c-a41daad4.zip`, 4,521,701 bytes,
  SHA-256 `3c6ae5b76d91907b044ef5febdd2e1c5d495423656792ca636781075cb1a1a23`.
- Manifest SHA-256 `3bcafcf84683e8fa7b9e4b97d4a108e60a2869a15ec0e4c57068a89f0774e966`;
  499 files / 8,701,165 uncompressed source bytes.
- Native test provider build `1792263126878489200`;
  candidate worker build `1792263247986733000`.
  Оба clean / exact source commit и manifest.

Все 14 artifacts прочитаны из shared и проверены по bytes/SHA из `report.ready.json`.

| JSON artifact | SHA-256 |
| --- | --- |
| report.json | 386a16889ce65af867cb304106c4219e1fe4053bc248c151a0b891644282be24 |
| source-transaction-proof.json | 3fc516529991ecb5d412b9796c52da5c129b3ba8ab48e7fce8c8cfb633af2f63 |
| update-archive-proof.json | cffe0fae5c029e90999db762676caec9eb22c20e9e024376f6308841544aef9e |
| update-candidate-proof.json | 8c6fa06128e259a5f95fdb123e28ee37067866f6ffde61df193d4ca909822dda |
| update-coordination-proof.json | f7e1fdc5a0b4e1d88650bba1d407c08679572ecfe71b7fb04cb5a739a2bbd12c |

## Что проверено

| Group | Passed tests |
| --- | ---: |
| Source ownership / pure update FSM | 9 |
| Updater CLI, journal, ZIP и source transaction | 39 |
| Frozen legacy contracts | 6 |
| Updater build | 0 (compile) |
| Durable worker coordination | 5 |
| Native source/drain/quiesce/private IPC | 3 |
| Schema-14 worker refuses schema 15 | 1 |
| Native no-clobber move | 1 |
| Schema-1 updater refuses schema 2 | 1 |

В 39 updater tests входят 11 новых source tests: Add/Replace/Remove/Keep,
manifest-last, сохранение caches/local config, local edits и staged/config tamper,
stale owner/lease и root replacement; ignored intent/receipt/final CAS, notification
failure, lock/backup/rollback migration и шесть реальных process-crash checkpoints.
SourceHeld и startup guard сохраняются; повтор не меняет source и не запускает
worker. Физически совпавший target без receipt/Verified не считается успехом.

`source-transaction-proof.json` содержит полный test plan/receipts/progress и
readonly inspection; четыре изменений завершены Verified, parent Building,
sourceMatchesTarget true, canReplay false, workerLeaseRetained true. CLI pages
source-status/source-inspect прочитаны без захвата владельца. UI для этих страниц
и live IPC driver ещё не реализованы.

Прежний schema-1 updater взят из проверенного candidate run
`20261017-102946-eea3af7d`: SHA-256
`a082c2518737b3187f8768b56679653dd3d4de7f25233e890c2742cf5e63132d`. Create-only copy сверена до/после захвата;
старый updater отказал от новой тестовой БД до записи, сравнение DB bytes/history
прошло. Рабочий worker reference SHA-256
`f462add07d0d0e2a64cae0da4c055149a4c92d9b394efcad96bb680bb02ce251` также копировался и запускался только с
новым тестовым state. Production journals этими exe не открывались.

## Candidate build и границы

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| winbridge-worker.exe | 16,479,744 | `e810f945b2910b1f3c1ec55057e8246f572f0909f85420b891bd9c1cfdf2dd18` |
| winbridge-fixture.exe | 309,760 | `f85c81cc11d8c75def06ed634334d6e66e364586713452b111854bd607912a2f` |
| winbridge-desktop.exe | 13,964,288 | `d868bdf32530f20b9086eb23d73f14f86bebd19456dba59a2bbdad62a61899c9` |
| winbridge-update.exe | 4,510,208 | `889bf3dff626f65317cfe609d265a71ca6acfd1972efba294f03c4a5383f0b2f` |

Actual source ZIP проверен current updater; source hashes проверены до и после
build. Rust workspace и native Tauri desktop release собраны. Script tests
проверили complete payload, SHA, create-only staging, missing artifact,
неизменность тестового runtime и generated-output link guard.
`runtimeApplied: false`, `candidateVerified: false`, `windowsOpened: false`:
это build/staging evidence, а не production install/health/reconnect gate.

Compiler caches повторно использованы из original owner
`20261017-061527-f4639971`, receipt SHA-256
`22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`.
Проверены terminal owner task, checksummed report и exclusive leases. Test state,
source, staging и proof новые; прошлый test outcome не наследовался. Windows
validation directories не удалялись.

Windows K free before/after: 34,595,745,792 /
33,953,144,832 bytes. Debian перед native build:
22,484,049,920 bytes; final 22,479,953,920 bytes.
Оба выше 6 GiB; очистка не требовалась, `CARGO_INCREMENTAL=0`, основной Debian
и проверенные Windows compiler caches переиспользованы.

Installed worker до/после: clean `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние. Итоговый Info: activeTaskCount 0,
maintenanceError/journalRecoveryRequired/updateCoordination отсутствуют.
Рабочий worker оставить запущенным; desktop можно закрыть. Ручной gate и обновление
установленной версии сейчас не требуются.

U1 source transaction завершена. Следующий блок — guarded infrastructure supervisor
вне managed task Jobs, затем authenticated bounded local IPC и shutdown. Установка
и bootstrap остаются после isolated U1–U3 gates. A4 временно отложен.
