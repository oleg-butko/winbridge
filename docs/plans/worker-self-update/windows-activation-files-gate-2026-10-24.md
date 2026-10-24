# Actor21 whole-set read binding: повторный Main выполняется

2026-10-24. **ACTIVE retry после исправления612df2e, результата ещё нет.**
Task `46d57134-48fc-4188-81cd-b94f59074d15`, run `20261024-162445-229e2220`.
Первое authoritative get_task: running. Ожидаются95 tests/19 groups/21 commands,
новый Bound/cold proof и scalar timing trace. Не resubmit при observation timeout.

## Текущий повтор: pins и восстановление наблюдения

- Producer `612df2e562c3a13b1006237a2db3937f553e94f5`:
  `winbridge-20261024T162421Z-612df2e-1f018014.zip`, SHA256
  `0679f07d9851451efd66547f1a99a726b0579a8ae260f20e839899f83e7688ce`, manifest
  `25396d675245bbe1ad0eda41719b135329ed005b733c87465955b65fa3a4a9cf`.
- Fixture7f6a029 и successful17 reference те же, что в первой попытке ниже.
  Старый failed audit не используется как reference.
- Metadata `.winbridge/workflow-validation/20261024-162445-229e2220/activation-preparation-run.json`,
  `activationFiles=true`; request/meta записаны до ACK, task ID сохранён.
- Shared `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-162445-229e2220`.
- Observer exec11882, лог `.winbridge/update-preparation/activation-budget-native-main.log`.
  Проверять handle/get_task заново, не считать observation timeout terminal.
- Cache20261017-061527-f4639971 с единственным lease. Debian free перед
  submit16,465,453,056 bytes; Windows измеряется самим сценарием до build.
- Local focused31/0, Python5+9, Linux/MSVC validation и MSVC production
  all-target Clippy/fmt/scaffold265 passed. Логи/receipts `activation-budget-*`.
  Full workspace716/0 относится к предыдущему producer74e8839, повторно не заявляется.

Readonly наблюдение **новой task**, без submit:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T162421Z-612df2e-1f018014.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T103055Z-7f6a029-3e94038a.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-files --verify-run 20261024-162445-229e2220
~~~

Исправление возвращает тот же validated replacement из одного graph snapshot
вместо повторного full history read, добавляет deadline recheck после I/O и
переносит тестовые duplicate/history audits после Bind. Исходные120000ms не
изменены. Timings включены только в validation build; `trace_end` не success.
После terminal проверить actual task/report/ready/proof, negative mutations,
timing trace и unchanged installed worker. До positive gate execution не начинать.

Нужен только installed d052 worker; desktop не нужен, UI-окон не будет. После
terminal явно сообщить, что desktop можно закрыть и какой worker оставить.
Исторические counts/pins/ошибка ниже относятся **только к первой попытке**.

## Первая попытка74e8839 — terminal failed по deadline

2026-10-24. **Terminal failed/exit1, gate не пройден.** Task
`01d7bf76-6c53-427c-8cd9-81d8f8798962`, run `20261024-153856-fe2e4f76`,
producer `74e8839`. **94 preliminary tests passed,1 Main test failed**;
21 commands, последний Cargo command exit101, остальные exit0.
Задача завершилась за1575.266s, Main test —1044.21s. Это не общий timeout
30-минутной задачи и не ошибка только Debian observer. Старый task не resubmit.

Последний успешный positive native gate остаётся
[preparation20/actual17→18,87/0](windows-activation-preparation-gate-2026-10-24.md),
отдельный one-file handoff также прошёл4/0. Whole-set21 реализован, но positive
сквозного proof пока нет. [Подробная передача контекста](resume-2026-10-24-activation-files.md).

## Ошибка и границы диагностики

Main дошёл до FilesMoved/revision22,5/5 installed, затем prepare20 и binding21.
В `apps/worker/tests/update_shutdown/activation_files.rs:60` проверка driver_error
получила:

~~~text
Runtime read-pin handoff stopped: PreconditionsChanged: Activation preparation deadline expired; no new execution or adoption
~~~

`None-supervisor-stderr.log` содержит ту же причину и указание сохранить journals/
barrier без automatic retry. Это сработавший live monotonic guard120000ms,
а не отказ firewall, потеря controller или сбой установленного worker.
Весь процесс **не** объявляется прошедшим на основании94 preliminary successes.

Точный источник расхода времени ещё не измерен. В текущем harness между
durable Prepared и Bind есть тяжёлые observe/duplicate/inspect/history проверки;
сам driver также повторяет graph/custody/hash preflights. По имеющимся логам
нельзя установить номер файла/checkpoint, на котором исчерпан бюджет, или
доказать, что достаточно только переставить assertions.

Следом нужны scoped timings без private material: preparation commit/ACK wait,
observe/duplicate/graph reads, начальный original.require_progress и каждый
file checkpoint, elapsed/remaining monotonic budget. Сначала выяснить, можно ли
перенести тяжёлые тестовые observations после live handoff без потери проверок,
и убрать повторное дорогое чтение без ослабления authority/identity checks.
Если потребуется изменение политики сроков — отдельный явный контракт; нельзя
молча увеличить120000, обновить исходный deadline при duplicate/cold read или
отключить stale-owner/nonce/FileID guard.

Success/cold assertions **после** места падения не выполнены; не считать
доказанными whole-set Bound, полный write/delete denial, duplicate после Bound
или cold no-adoption именно в этом run. `candidate-health-actor-proof.json`
отсутствует; успешного `activation-preparation-verified.json` для этого run нет.
Fixture выполняет catch_unwind → terminate_tree → tree_is_empty → resume_unwind;
нового cleanup panic в логе нет, но полноценного post-crash success proof тоже нет.

Новый worker этим режимом не создаётся и state не передаётся. Public install/
activation off; installed runtime не менялся. В задаче checkpoint дальнейших
Windows-прогонов не запускалось; следующий этап реализации не начат.

## Что действительно проверено

Все18 preliminary groups прошли:4 handoff primitive,2 strict21 contract,
1 IPC21 rejection,24 activation model/ledger/timer,2 strict20 contract,
1 IPC20 rejection,1 actual17→18 migration и прежние replacement/preparation/
backup groups. Это94 tests, **не95/0**.

27 опубликованных ready files прочитаны/хэшированы дважды; failed report,
все preliminary logs и migration proof сохранены. Migration verifier отдельно
проверил actual17→18/refusal и отверг12 negative mutations. Успешные Main/file
mutation suites не запускались против отсутствующего positive proof; counts72/19
для нового run не заявляются.

- Report SHA256 `650f1779f77130e3b4e7158f923172320c29f90893fcd5ee066de926091de538`.
- Failed Main log SHA256 `1e55ba7ae515a64ed70b6a8ac760e3393132c53018920db7533470fdb82eac40`.
- Migration proof SHA256 `6aedf35ceaac7115aab43b41824b4acabf00342770317f834538a2ecb23c4891`.
- Local readonly failure audit:
  `.winbridge/workflow-validation/20261024-153856-fe2e4f76/activation-files-failed-audit.json`.
  `passed=false`; не использовать его как successful migration reference pin.

## Immutable pins и наблюдение

- Producer `74e8839634a36bc56508df2d5f3b9eb11397982f`:
  `winbridge-20261024T153800Z-74e8839-fe777660.zip`, SHA256
  `af1ff79024b1e88dd20440a2a2329bad59116e0e35fbe7d220338cd27020b3d8`, manifest
  `959daf2e7303ee227723d6b4352000eab574e96a600fc506506eee34a844e820`.
- Fixture `7f6a029c1fd626077adc034a96b54d63da551df5`:
  `winbridge-20261024T103055Z-7f6a029-3e94038a.zip`, SHA256
  `4f17e0e0c5f4be899a3e4456b53240c0a9bd9b02b5a5aad2dee458ba29e7a788`, manifest
  `7deaebcf66a9e447169bdd65d8991572bcfb6625719ba5e7945b9552a7fbe86b`.
- Migration reference:
  `.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
- Metadata:
  `.winbridge/workflow-validation/20261024-153856-fe2e4f76/activation-preparation-run.json`.
  `activationFiles=true`; successful verified result **не создан**.
  Private request/EncodedCommand/
  config/token не печатать.
- Shared reports:
  `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-153856-fe2e4f76`.
- Observer log `.winbridge/update-preparation/activation-files-native-main.log`;
  observer terminal exit1 после корректного отклонения failed report. Прежние
  exec62989/PID123026 завершены, не возобновлять.
- Cache `20261017-061527-f4639971`, один build lease.
  Debian free перед submit16,472,887,296 bytes, после audit16,469,987,328 bytes
  (около15.3GiB). Windows free27,462,344,704→27,239,272,448 bytes. Cleanup не было.

Readonly recheck **прежнего task**, без submit; ожидаемо завершится ошибкой
`Native preparation failed`, потому что actual Main failed. Это не команда повтора:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T153800Z-74e8839-fe777660.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T103055Z-7f6a029-3e94038a.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-files --verify-run 20261024-153856-fe2e4f76
~~~

Не терять `--activation-files`: mode является частью сохранённого scope.
Observation timeout не означает terminal. Сначала get_task по Task ID, затем
наблюдать тот же run. Обычные исправления изолированных build/test scripts и
fresh retries разрешены после terminal диагностики. Installed worker unexpected
behavior/update requirement останавливает зависимые Windows действия.

## Что уже проверено, а что нет

Для producer74e8839: workspace716/0 (16 ignored), final Linux/MSVC all-target
Clippy, protocol-jsonschema55/0, generated contracts/TS/UI, fmt/diff/scaffold263,
Python5+9 passed. Логи `activation-files-*` в `.winbridge/update-preparation/`.
Legacy preparation87/0 readonly recheck сохранён, он не доказывает новую ветку21.
[Native20 preparation87/0](windows-activation-preparation-gate-2026-10-24.md) и
[one-file native4/0 pins](runtime-activation.md) относятся к прежним producers.

**Первый следующий шаг:** диагностировать и исправить расход deadline в Main21,
затем clean producer/new namespace и успешный whole-set gate. Старую задачу
не replay; завершённые reports/runtime/state/reference данные не удалять.
Только после positive gate/review продолжать [activation plan](runtime-activation.md):
process creation/private receiver, original state handoff, local/controller/
lifetime, U2 faults/U3/U4. JSON и cold Snapshot не дают живых ресурсов.

Закрывающий Info: installed d052 healthy, generation
`8f8d1fb9-e5b1-4984-a17e-d067db46268e`, activeTaskCount0,
maintenanceError=null/journalRecoveryRequired=null/recoveryCoverage.complete=true.
Активных native/Cargo/observer runs нет; controller PID3922 оставлен работать.
UI-окон не было. Desktop можно закрыть; installed worker оставить работать.
Ручное обновление установленного worker сейчас не требуется.
