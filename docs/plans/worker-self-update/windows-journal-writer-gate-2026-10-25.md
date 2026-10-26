# Native writer24: история первых прогонов

**Новый run232746 подтверждён:193/0,56 files twice,44+16+18 audits.**
[Актуальные pins, диагностика и продолжение](resume-2026-10-25-writer-verified.md).
Ниже сохранены первые failed runs; их отсутствие writer proof — исторический статус.

2026-10-25 America/New_York. Source19/17, actor24/view16, physical2/private writer3.
**Обе actual migration проверки пройдены; полного native writer proof нет.** Public install off;
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

## Повтор после исправления: exact producer и результат

**Terminal failed/exit1; observer exit1.** Из39 test groups прошли38:
**190 tests passed, Main24 — 1 failed**. Из42 commands41 завершилась exit0,
Main Cargo — exit101. Overall report passed=false; 53 ready files twice verified.
Report SHA256 `d6b7ba81af899bc2de6480f8297b5e13c9c6e9f5a0021f5efdb3c78dcf0b9437`.
Полного writer proof нет, verified-success pin не создан. Прогон не replay.

- Run `20261025-205856-79e1b7c0`.
- Task `f762c5a2-0cf8-474a-8582-45d4bd6c74c1`.
- Producer `772b00ac8a5f4b5a0081cd1014bfc6c65b5b2e5b`, ZIP
  `winbridge-20261025T205458Z-772b00a-e7ad6653.zip`.
- ZIP SHA256 `e780292bde894f437c69c810a9e8a9307596c338923af4d515aa75e8918e25a1`;
  manifest SHA256 `9bb24397934eae13c2a25a8fca3835ae4acc3172be3303b03efd8974071faba3`.
- Target receiver1a2149f, reference Main23 и exclusive cache — как выше.
- Перед admission disk taskac04264e-6e15-45f9-a238-4803ff9e1326 succeeded/0:
  Windows13311016960 / Debian25047293952 bytes; оба≥6GiB.
- Локальный observer log `.winbridge/update-preparation/writer24-native-retry.log`;
  metadata `.winbridge/workflow-validation/20261025-205856-79e1b7c0/activation-preparation-run.json`.
- Shared `/mnt/hgfs/shared/WinBridge/workflow-validation/20261025-205856-79e1b7c0/`.

### Реальные миграционные проверки: обе прошли

Worker fixture **1 passed/0 failed** (7.31s): actual old16 executable в отдельном
instance без controller создал succeeded task с stdout/stderr и dedup key.
CLI task/output сверены с SQLite. Inactive16 consistent copy перешла в17;
rows/authority/backup сохранены, old16 отказывает на17 без новых writes.
Отдельная copy настоящего unfinished Quiesced16 отказала normal17 open **до**
миграции/writes/backup; schema16/authority/accepted shutdown/rows неизменны.
Old fixture process/Job завершены; original reference и installed runtime сохранены.

Updater fixture **1 passed/0 failed** (7.24s): actual old18 physical history из
прежнего успешного Main23 скопирована через SQLite backup API, migrated19 с
fencing epoch и verified backup18. Physical/root/all prior rows сохранены,
writer gate rows0. Terminal replacement остаётся Existing data без fresh permit;
old18 init/status refuse19, DB/WAL bytes и original reference unchanged.

Independent migration verifier **18 negative mutations rejected**; exact source,
reference/history/authority/output связаны, оба proof-файла read twice byte-equal.
Это подтверждение двух компонентов, не весь Main24/install success.

| Artifact | SHA256 |
| --- | --- |
| activation-migration-proof.json | fcd405a037af304534d6f29d017e6783155c77fb0379a806a402dde8356d1616 |
| worker-journal-activation-migration-proof.json | b7348c541d0490730f6f3458df9e253c182ecd2cc097e13ac9c8cf68aa1549a5 |

### Итог Main24 и закрытие

Тест дошёл до replacement FilesMoved5/5, Ready и StateOwned observation, затем
actor writer вернул:

```text
Corrupt: Activation preparation deadline expired; no new execution or adoption
```

Panic: `apps/worker/tests/update_shutdown/activation_writer.rs:212`;
Main test runtime715.65s. Harness сообщает Ready90237ms /StateOwned120352ms от
собственного origin, который начинается **до** baseline/preparation admission.
Это не тот же origin, что original deadline; ошибка подтверждает expiry на writer
пути, а не точный момент expiry относительно saved StateOwned.

Structured `None-supervisor-stderr.log`: bind_files43883ms, remaining после
bound66324ms. Actual before/after-release traces: revision5/HandoffIssued,
тот же state directory/journal/lock, lockAvailable false→true,
observedAt−admittedAt95691ms/102882ms. Hashes:

| Artifact | SHA256 |
| --- | --- |
| None-activation-state-before_release.json | e12bea8995f84ab6b84dd38f210de267a05f89e9fd9a0658663cb334a59425cb |
| None-activation-state-after_release.json | bf558713c1c9004c4cf89a22aa38515c6b93ad5229062951b269601830c4f102 |

Ready set не содержит full Main proof или writer-before-send marker. Успешные
writer CAS/receipt/epoch change не доказаны; отсутствие marker не заменяет final
SQL readback и не доказывает отсутствие эффекта. Source control flow проверяет
custody/window до writer admission, но это отдельный аргумент review, не artifact.

**Следующее действие:** разобрать launch/handoff/observe/admission latency,
убрать только доказанно избыточные slow audits/validation work из original window;
mandatory graph/file/process/lock/fencing/live checks сохранить, original120s
не продлевать. Детальная последовательность и карта кода — в checkpoint.
Изменение кода и новый run оставлены следующей сессии по просьбе пользователя.

Closing process task `b871ac45-a58f-4e8a-8e2b-19ebbd7dd24f` succeeded/0:
по PID+creation FILETIME old worker17920/supervisor17048/new worker22864 уже
отсутствуют. Это process-exit observation, не full native Job-empty proof.
Fresh Info21:25:12Z: тот же installed d052/generation, connected/active0,
maintenance/recovery errors=null, recoveryCoverage.complete=true.
Native report storage Windows13228408832→12158017536 bytes;
Debian closing25040994304 bytes. Cleanup не выполнялся.

Local final receipt `.winbridge/update-preparation/writer24-session-close-20261025.json`.
Observer/native checks завершены; controller оставлен. **Desktop можно закрыть,
installed worker оставить.** UI окна не появлялись. Runtime/config/journal
рабочей установки не менялись; public install выключен.

## Продолжение без повторного эффекта

Exact readonly `--verify-run 20261025-205856-79e1b7c0` команда — в
[checkpoint](resume-2026-10-25-journal-activation.md). Она наблюдает тот же task,
не запускает Windows работу заново. Failed run не превращать в successful reference.
Если task живой/unknown после перезапуска Debian, сначала reconcile его; observation
timeout не даёт права отправить новый key. Новый gate — только после terminal,
исправления source/необходимых checks, clean commit/export и fresh Info/disks.

Full success требует actual39 groups/42 commands, independent Main24,
updater18→19 и worker16→17/active16 refusal, both SQL observations/epoch/receipt,
old-owner fencing, immutable history, actual child/Job exit, no cold replay,
ready hashes twice и negative audits. Source/partial tests этого не заменяют.
После positive gate/review: normal local service/reconciliation → new controller
session → durable Job lifetime/commit/scoped release → remaining U2 faults/
management/opt-in → U3 desktop → U4 actual stable-runtime self-update → A4.3.
