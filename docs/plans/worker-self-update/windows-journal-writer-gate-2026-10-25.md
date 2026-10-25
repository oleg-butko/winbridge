# Native writer24: прогоны и восстановление

2026-10-25 America/New_York. Source19/17, actor24/view16, physical2/private writer3.
**Положительного native writer/migration proof пока нет.** Public install off;
installed d052/runtime/config/journal не менялись. [План продолжения](resume-2026-10-25-journal-activation.md).

## Первый run: terminal failed, не replay

- Run `20261025-203011-cffafea9`, task `abf1250f-9876-435c-83ea-672c0462d8b0`.
- Producer `0d5d0e34a3992193e4e7bbe1d5f3c2272ca78a5f`, ZIP
  `winbridge-20261025T202603Z-0d5d0e3-9de4e71c.zip`.
- ZIP SHA256 `947de6aa2e8ad21070bf2247aa7b55461057960d0ba6cde0f7d5e156d069d2eb`;
  manifest SHA256 `857b78af153d270fe64de281f854225fe2b1619f45e3fa2fd68dc89e738329a0`.
- Report SHA256 `235af900c28d5fe669d58071a8cda68d1b6ceea7a31e038d11f6b7d7df05ab7f`;
  11 ready files twice verified. Task failed/exit1, observer terminal/exit1.
- **41 preliminary tests passed:** writer model/ledger19, worker SQL11,
  existing opening7, view contract2, actual mailbox1, create-new send trace1.
- Actual worker16 migration fixture failed before backup/old16 server/task:
  `SELECT receipt` requested an absent column. Frozen schema16 stores the
  receipt in `worker_shutdown_receipts.record`, alongside `command`.
- Updater migration, candidate/old real set build and actual Main24 were not reached.
  No live writer effect, migration or installed cutover is inferred from these tests.

Shared report directory:
`/mnt/hgfs/shared/WinBridge/workflow-validation/20261025-203011-cffafea9/`.
Local metadata `.winbridge/workflow-validation/20261025-203011-cffafea9/activation-preparation-run.json`.
Observer log `.winbridge/update-preparation/writer24-native-main.log`;
verified failure summary `writer24-first-failed.json` in that local directory.

## Исправление сценария перед повтором

Projection вынесена в
`apps/worker/tests/support/journal_migration_queries.rs`: одна readonly statement
читает `command,record`, проверяет typed command/receipt и update index.
Нативная fixture использует её и для original, и для consistent active16 copy.
Original directory/journal/lock identities закреплены read pins; lock не приобретается
как runtime authority. Новый переносимый regression открывает настоящий Store/SQLite
и проверяет SQL projection ещё в Debian, не создавая synthetic migration success.

Regression **1/0**, MSVC worker all-tests validation Clippy passed; source fmt/
scaffold/diff passed. Это исправление fixture, не runtime migration proof.
Следом нужен distinct clean producer/export и новый run; первый failed namespace
не изменять и не использовать как reference успешного сценария.

## Target, reference и cache

Receiver остаётся clean `1a2149f88407fd9c33d101e12d07d7c4b64599d3`:
`winbridge-20261025T163918Z-1a2149f-8b8c082c.zip`, ZIP SHA
`abc74d43ee4e43f0b60a86f0cd3bc9dd35679fcf8b6f010cb1ca3c5c72573c1b`,
manifest SHA `b27808d7f52abd9fdbb4c5fe01e078493950d22e5f5a46c8b59b992bb453bed1`.
Он не требует ручного installed update.

Reference pin — successful old physical Main23:
`.winbridge/workflow-validation/20261025-100933-d8a50807/activation-preparation-verified.json`.
Helper заново проверяет old132 proof/ready bytes и строит separate18/16 refs;
image source1370d6b — установленный target старого run, не producer7b270c6.
Composite reference SHA256
`bd1e71f1181236e4b8e3c4f140e8b716abe3c8908c10d11651132252c25f3772`.
Schema17 compatible-drain pin нужен старому mode, его не подменять в writer mode.

Compiler cache `20261017-061527-f4639971/test-build-cache` verified before run:
owner report SHA256 `22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`.
Old owner task `ac5724f1-c38a-4a50-a446-f9b57ae5473b` terminal failed, но четыре
compiler groups завершились exit0; cache используется только как воспроизводимый
compiler cache, **не как test-success evidence**. Одна exclusive lease обязательна.
Local cache receipt `.winbridge/update-preparation/writer24-cache-verified.json`.

## PowerShell syntax и окружение

Native parse-only task `bb8e0890-89c6-4f9a-b6ee-6550bd4bbaec` succeeded/0.
ZIP читался под deny-write/delete pin; `Parser.ParseInput` подтвердил отсутствие
syntax errors и параметр ActivationWriterGate. **Script не исполнялся.**
PS file SHA256 `b2b1566d7d40f273dad9c6f20a1af5703bb6a17c931a4ed326fa65a3ab06bad6`.
После SQL fix байты PS не менялись; reuse syntax evidence допустим только после
сравнения PS entry нового проверенного ZIP с этими байтами. Локальная receipt:
`.winbridge/update-preparation/writer-ps-preflight-20261025T202749Z-1f6c0024.json`.

Перед первым run Windows free13696921600 / Debian25138630656 bytes.
Failed run report: Windows before13614120960 / after13202550784 bytes.
Closing observation20:43:07Z: Debian25047887872 bytes, тот же installed generation
8f8d1fb9-e5b1-4984-a17e-d067db46268e, controller connected/active0/errors=null.
Это historical measurements, не admission для следующей тяжёлой сборки.
Новых UI окон не было. **Desktop можно закрыть; installed worker оставить.**
Windows validation directories не чистить. Source script fixes и повтор после
terminal разрешены; unexpected installed worker/manual update останавливают
зависимые Windows действия. Нельзя retry неизвестный/live task по новому key.

## Новый run и безопасное продолжение

После clean fixed producer export использовать команду из checkpoint с
`--activation-writer`, receiver1a2149f, cache above и old Main23 reference pin.
Fresh Info/диски≥6GiB и объявление worker-only/no UI обязательны.
Записать новый task/run до ожидания результата. Observer сохраняет request и scope
до dispatch; после потери Debian context сначала get_task, затем тот же argv с
`--verify-run RUN`. Не resubmit по observation timeout и не relaunch старый run.

Success требует actual39 groups/42 commands, independent Main24, updater18→19
и worker16→17/active16 refusal, both SQL observations/epoch/receipt/old-owner fencing,
retained immutable history, actual child/Job exit, no cold replay, ready hashes twice
и negative mutation audits. Не выдавать source checks или partial41 за этот success.
После positive gate/review — normal local service/reconciliation/controller/lifetime,
remaining U2 faults/management, U3 desktop и U4 actual stable-runtime update.
