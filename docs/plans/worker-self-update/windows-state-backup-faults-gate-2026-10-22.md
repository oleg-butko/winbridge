# U2.1 final backup: fault gate пройден

2026-10-22. Task `b3fd7ab5-c875-49e7-b961-2303a265ebc1`, run
`20261022-203814-bc4258b9`, succeeded/exit0. **23 native tests /8 groups**,
включая один integration test с четырьмя actual Main/source/build/health/drain/
shutdown/backup cases. 31 ready file сверены независимо дважды; 18 искажений
actual proof отклонены. Установленный worker d052 не менялся, UI окон не было.

Отчёт: `K:\VMs\shared\WinBridge\workflow-validation\20261022-203814-bc4258b9`
(`/mnt/hgfs/shared/WinBridge/workflow-validation/20261022-203814-bc4258b9` в Debian).

| File | SHA-256 |
| --- | --- |
| report.json | `0837cea870778b6b9097fe7c76f021f6902431b6e0a12e6421c60e7747beea3e` |
| candidate-health-actor-proof.json | `00fc34c224e06e91869e8ec6ab4a2b9602d4613517ed651e8ec0d21101cb078f` |

Producer clean `6f060f0abe0c29abc325d18779d736cc52b72ff4`, ZIP
`winbridge-20261022T203358Z-6f060f0-c3c330a0.zip`, SHA
`bfb2c1addbc14764a1a03ecf454b3e54450a486b86c3cb169ac9859d5ee16f9c`.
Candidate31fa/source archive и native cache прежние; exact pins сохранены в
`.winbridge/update-preparation/state-backup-faults-native-verified.json`.

| Случай | Подтверждённый результат |
| --- | --- |
| Existing destination | Sentinel не перезаписан, helper exit1, cleanup подтверждён, Indeterminate, повтор сохраняет ту же attempt. |
| Exit after copy | Helper exit79, настоящая SQLite copy/rows сохранены, report не принят, cleanup/Indeterminate, повтор без нового helper. |
| Wrong input report | Helper exit0, actual copy сохранена; binding input SHA отклонён с конкретной причиной, report не принят, cleanup/Indeterminate. |
| Hold before copy + owned crash | Actual helper PID/attempt в Running, destination отсутствует; parent завершает только свой Job, запись остаётся Running, view Indeterminate и custody=false, без повторного исполнения. |

Во всех случаях old Main/serve и роли настоящие, candidate реально собирался.
Перед первым backup request parent readonly-наблюдением подтвердил retirement
shutdown driver, live custody и Root Replacing. Остановка принята ровно один раз,
original instance lock/exit proof сохранены, runtime bytes не заменялись. Worker
source rows/DB/nonempty WAL не менялись; восстановленный saved record после crash
не создаёт physical custody. Proof содержит public pre-crash snapshot; private
supervisor-control capability исключена из shared whitelist и Git.

## Исправления и предыдущие неуспешные прогоны

- `20261022-194847-ec2f7702`/task83efbf45/source5e456d4: первое backup admission
  пришло после DB Exited, пока shutdown driver ещё не retired. Исправлен порядок
  fixture: readonly observation idle+original custody до первого mutation request.
- `20261022-200607-a60a7daf`/task5a34314e/source2fa306e: expected collision/copy
  preservation/dedup прошли, первое readonly-чтение после Job kill получило SQLite
  disk I/O error. Поздняя readonly diagnostic13146cba успешна с неизменными DB/WAL
  SHA; только SHM coordination изменилась. Fixture ограниченно повторяет лишь тот
  же readonly snapshot (Storage/disk I/O,5s/25ms); Corrupt не повторяется. Public
  pre-crash proof сохраняется до kill. Helper/file effects не повторяются.
- Windows run6f060f0 прошёл полностью, но первый local verifier отверг raw stderr
  LF против sanitized reason. Исправлена точная аналогия Rust: убрать Unicode Cc,
  ограничить900 scalar chars. Верифицирован тот же готовый report, Windows не
  перезапускался. 18 отрицательных мутаций включают неверную reason/exit/cleanup,
  input/binding/PID/copy/custody/duplicate/admission/retained history.

Все failed reports сохранены как failed. В итоговом run readonly retry counters
равны0 для всех четырёх cases; selector отдельно проверен native unit test.
Логи прогресса в следующем producer включают оба fault names и completed case,
чтобы одинаковое None больше не скрывало переход к очередному сценарию.

Windows available57571758080→55933362176 bytes, Debian около26GiB. Cache переиспользован,
retained validation directories не удалялись. Все тестовые процессы завершены;
desktop можно открывать/закрывать, installed worker оставить работать.

С [happy и actual12→13 migration gates](windows-state-backup-gate-2026-10-21.md)
это закрывает **U2.1 final consistent backup**. Новые [file/model primitives](windows-runtime-file-moves-gate-2026-10-22.md)
проверены отдельно. Replacement binding/ledger/OS custody handoff, actual file
replacement, activation, desktop и public update остаются следующими U2.2–U4.
