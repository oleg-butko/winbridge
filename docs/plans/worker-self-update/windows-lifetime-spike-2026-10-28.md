# Windows OS spike: время жизни двух поколений

2026-10-28. **Passed**, затем independent readonly reread дал тот же JSON.
Это actual Windows процессы-помощники из platform tests, не интеграция keeper
в production WinBridge. Main26/SDK19+21 пройден отдельно; public install off.

## Pins и результат

Run `20261028-190435-5d9b4e86`, task
`5b90e6fb-34e5-43b0-8f7b-2c5f31707588`, succeeded/exit0.
Producer `1bfaa328e2274bf1f9ee7b8129dc96bb3b7f01b3`, clean:
`WinBridge/sources/winbridge-20261028T190308Z-1bfaa32-606fee73.zip`,
archive SHA `87eaa9b060412d14bfbac220c2dae9731339ce8c61894ae5c96fd564f7caa0c4`,
manifest SHA `9e863a64da3e1d0dbe4835ebb17a0acc518cdc0bfa527c529ff8de666c8deeb6`.

`WinBridge/workflow-validation/20261028-190435-5d9b4e86/result/report.json`
SHA `ff2888d9c7ee2ad679d1e44e3c7d42ba1a1b43bb127791f985e25ec21f86e981`.
Два exact native tests,2 passed/0 failed, тела1.12s и0.54s; первая сборка29.74s.
73 files сверены дважды,36 negative data mutations; повтор без Windows RPC
совпал с исходным audit. Proof SHA:

- two-generation: `7d96d28c9b92dfb6fecff0bc9e5d23262da53e0e69fd0f42b03920460e8079c9`;
- keeper-loss: `c04fdfeab3bc2124aa5552770343033cf41c0dcd21f530f41d6c62540518f315`.

Local run-info:
`.winbridge/update-preparation/lifetime-spike-20261028-190435-5d9b4e86/run-info.json`;
reader output `.winbridge/review-2026-10-28/lifetime-spike-reread.json`.

## Что доказано

Original keeper15312 владел original worker16128/Job. После запроса worker
следующий keeper25596 был запущен из keeper, вне старой worker Job и execution
domain; его worker9848 вошёл в новую соседнюю Job. Next keeper/worker остались
живы после штатного выхода old keeper. PID/birth/host/session и реальные Job
идентичности связаны в отдельно сохранённых native observations. Все helper
процессы оставались под внешней transport Job самого теста; её limits не менялись.
Это не доказательство возможности покинуть чужой Windows Job.

Другая проба завершила keeper18064 кодом73 **без Rust destructors**: OS закрыла
последний handle scoped Job и завершила ожидающий worker24908. Его собственный
300s timer не мог стать причиной выхода в45s+45s observation windows. Ни restart,
ни recovery command не выполнялись. В обоих случаях реальная task+grandchild
отмена проверена до смены поколения. Job/domain flags оставались8192
(KILL_ON_JOB_CLOSE), breakaway не включался, сеть/UI не запускались.

Первый run8f527fb (`20261028-185609-72461f1f`, task5a4d3123) сохранён как failed:
instant Job.empty проверка после exit0 была преждевременной. Bounded wait за
**тем же** original Job в1bfaa32 прошёл без изменения predicate/limits/budget.
Нельзя подменять Job.empty сигналом процесса или новым объектом под старым именем.
Перед повтором read-only preflight дополнительно сверил exit шести PID+birth
первого прогона; чужие/reused PID не завершались.40 old ready files также twice
проверены, старые report/proof не изменялись.

## Что осталось

ОС-механизм позволяет перейти к compact typed lifetime DTO/FSM поверх confirmed
controller/local7: original parent/Job/authority/scope и тот же startup deadline.
Затем durable intent/readback/admission и actual same-child private receiver,
normal-IPC observation, supervised quarantine, trusted final commit/scoped release.
Один завершённый OS test не создаёт live keeper из retained JSON и не разрешает
public install. Root Main26 остаётся AwaitingController, maintenance закрыта.
Next-update delegation/owner epoch transfer и максимальные2 keeper требуют
своего протокола и fault tests; файловые markers этого spike в runtime не переносятся.

Windows free25,021,206,528 before /25,012,371,456 after; Debian10,395,959,296
перед repeat preflight. Ничего из Windows validation/config/journal/report не
удалялось. Final installed Info: worker0.2.0/source48c834c, generation946f1128/
incarnation5a8eb173 прежние, active0/errors=null. Все test processes завершены.
Worker оставить работать; desktop/тестовых окон нет, обновление worker не требуется.

Readonly verification (не запускает Windows команды):

```bash
python3 scripts/verify-update-lifetime-spike-native.py \
  --run-info .winbridge/update-preparation/lifetime-spike-20261028-190435-5d9b4e86/run-info.json \
  --source-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261028T190308Z-1bfaa32-606fee73.zip
```
