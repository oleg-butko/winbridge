# U2.2: четыре actual replacement faults — Windows gate пройден

2026-10-23. Task `63232320-8bf6-4826-b13c-315ead0b4433`, run
`20261023-173625-e4eea0a6`, terminal **succeeded / exit0**.
**60 native tests /0 failed /12 test groups**, включая один integration test
с четырьмя полными последовательными Main cycles. Сам integration test занял
2182.68 секунд. Все 32 ready files проверены дважды; independent verifier принял
actual proof и отверг **68 намеренных искажений**. Новых Windows tasks при
закрывающем audit не отправлялось.

Это проверка корректного отказа/сохранения неопределённости, **не успешная
установка всего updater**. Root каждого case остаётся Replacing;
`runtimeApplied:null`, `installedRuntimeApplied:false`, `activationStarted:false`.
Установленный runtime не менялся, GUI-окон не открывалось.

## Проверенная пара и файлы

| Поле | Значение |
| --- | --- |
| Producer | `e5678273f0a3f913c9c1aa88702e96ef81b8835b` |
| Source ZIP | `winbridge-20261023T173516Z-e567827-79b806bb.zip` |
| Source ZIP SHA256 | `19698c773f60ffff2901ec6535357a88e41e75669deee1c4b6db85cbf1a8b66e` |
| Source manifest SHA256 | `205e39d548244a7a02d42ba1ca29493867a9b59b66b9fdc33febd95872253972` |
| Target fixture | `5fbc1cf2e37086dad8e65667ed3af31b1a16d9f7` |
| Target ZIP | `winbridge-20261023T091226Z-5fbc1cf-b4ac44a5.zip` |
| Target ZIP SHA256 | `3891ad4689779c2eb2507b70be4ad5d2f599cf4816fb4b882d64f2de0006c6fb` |
| Target manifest SHA256 | `98e52e10e99e25fbc8cf9864b4af3e4d83419e8a1297cbcd330dc61e5c4546f3` |
| `report.ready.json` SHA256 | `624b128c53b7ac5667d31274d56f90a2f3a590522c7e26ba42c1e39b37ba3bc1` |
| `report.json` SHA256 | `adbcbfd771f13c3aa9d270b07b884008c3d7ae4395bfdbbd0c42d9cb5dbf7773` |
| `candidate-health-actor-proof.json` SHA256 | `8a5b1963a0c7d9eeca69a29aeb3cef6776ed90e9f5997a8e03559008523c7ed0` |

Схемы updater15/worker16, supervisor17/view9. Target — limited fixture:
совпадение schema не делает её готовым production updater. Новая readonly
[support query](updater-runtime-support.md) из commit `d1b2bea` **не входит** в
immutable producer e567827 и этим native прогоном не проверялась.

## Что подтвердилось

Каждый case начинает отдельный normal Main и supervisor, выполняет source/build/
artifact/health, drain/quiesce, exact exit, final backup и runtime preparation.
Затем fresh replacement admission передаёт original shutdown custody; fault
инъецируется только в собственном isolated namespace.

| Case | Update ID / replacement attempt | Сохранённый и физический результат |
| --- | --- | --- |
| ExistingDirectory | `0f1c62e4-6279-41b2-8c7b-830be40adca2` / `770adc7b-77bd-4e6a-af66-a6612d545024` | Indeterminate1, previous Planned; чужие directory/sentinel сохранены, old/prepared files неизменны. |
| LockedOldFile | `e11ba59f-16e0-4eb2-b04f-580461af7527` / `6e31518b-fb93-4895-8773-e1a9e6fbf7d6` | Indeterminate1 из-за error32; original state/instance locks удерживаются; release foreign handle не разрешает retry. |
| ReceiptWriteDenied | `8f8f0135-c18b-4428-9530-bc1a6989ab00` / `434ded38-b791-49bf-b6c5-9fa814e6b045` | Old worker фактически retired, receipt4 отвергнута SQL trigger; Indeterminate4 сохраняет RetireIssued3. После снятия trigger duplicate не переносит файлы. |
| HoldBeforeReceipt | `667ac28b-f037-442f-8754-53fd2774efff` / `9e80dfae-cd07-477f-8a61-07ddfbd7a357` | Owned supervisor Job завершается после verified OS retire до receipt; SQL остаётся Replacing3/RetireIssued, marker afterEffect4 не становится квитанцией. |

Во всех случаях actual File IDs/paths/bytes/SHA соответствуют сохранённому prefix
и известному единственному переносу. Prepared directory остаётся полной; source,
preimages, consistent backup и worker DB/nonempty WAL сохранены. Live failed guard
держит custody; после собственного Job exit cold projection показывает
Indeterminate/custody=false, не выдаёт новую permission. Обычный startup отказал
по maintenance до DB effects. Если old worker уже retired, проверялся настоящий
retained executable с тем же config, а не ошибка отсутствующего файла.

Barrier последнего case существует только в clean native Windows
`update-validation` build; release product не получает fault API. Подробные
инъекции и правила сохранения — в [описании harness](runtime-replacement-faults.md).

Audit68 включает неверные scope/kind/types, paths/File IDs/digests, causal revision,
root/custody/driver/duplicate state, preimages/backup/worker DB, marker input/PID/
afterEffect и выдуманные activation/cleanup. Проверяется original actual proof,
затем каждое искажение по отдельности. Исходники и manifests обоих ZIP при
закрытии также повторно проверены полностью.

## Где доказательства и как продолжить

Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261023-173625-e4eea0a6`;
Debian: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-173625-e4eea0a6`.
Full proof/pins/counts в ignored
`.winbridge/update-preparation/replacement-faults-native-verified.json`;
закрывающая сводка обеих последних задач —
`session-close-20261023-replacement-verification.json` там же.
Повторить только readonly proof audit можно так:

```bash
python3 scripts/test-update-replacement-proof.py \
  --proof /mnt/hgfs/shared/WinBridge/workflow-validation/20261023-173625-e4eea0a6/candidate-health-actor-proof.json \
  --pin .winbridge/update-preparation/replacement-faults-native-verified.json
```

Native observer завершился exit0. При закрытии повторно проверен и прежний
positive gate:20 ready files/44 mutations; вместе **52 ready files дважды и 112
отклонённых mutations**. Это два прогона с пересекающимися prerequisite tests,
а не120 различных tests.

Windows free 37,855,776,768→35,630,841,856 bytes; Debian before 26,569,707,520,
при закрывающей сверке26,487,005,184 bytes. Минимум6 GiB соблюдён; cache
20261017-061527-f4639971 переиспользован, очистки не было. Windows validation
directories/reports/config/journals сохранены.

Installed worker d052, generation 8f8d1fb9-e5b1-4984-a17e-d067db46268e,
connected, active0, maintenance/recovery errors отсутствуют, recovery coverage
complete. Desktop можно закрыть; установленный worker оставить работать.

Не resubmit эту terminal task. Следующая реализация подробно описана в
[актуальном handoff](resume-2026-10-23-replacement.md): native support query и
durable pre-drain compatibility, затем activation/local health/controller receipt,
оставшиеся U2 faults, U3/U4. Полный updater и public install ещё не готовы.
