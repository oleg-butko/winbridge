# A2.4.2a: незавершённый Windows gate и диагностика процессов

Дата: 2026-10-13. Полный gate не пройден; read-only inspection не объявлен
завершённым, installed schema-6 worker не обновляется.

## Проверенные наблюдения

- Local `84c1159`: 260 workspace tests; clean browser gate на real Unix worker;
  Windows-target tests, Clippy, UI typecheck/build/contracts passed.
- `20261013-161404-134b44cc`, source `23c26c2`, task
  `6008e106-3ed3-426f-be0f-64a8f71a69c3`: ошибочная assertion attention test
  привязывалась к прежней формулировке диагностики. `84c1159` заменил её проверкой
  failed task ID, причины fault injection и приостановки; все 20 attention tests passed.
- `20261013-161810-d7049c65`, source `84c1159`, task
  `3845d288-e60c-476f-ad0a-eda67540d4d8`: все **11** новых Windows recovery tests
  passed. Resume crash/restart test passed. Два прежних runtime tests не завершились
  в пределах наблюдения; партия `runtime` дала 5 passed / 2 failed.
- Failure task `df8a2976-eb6f-4666-8eb4-e4c7b7631585`, PID 7924, сохранил
  stdout/stderr (57 bytes), но не terminal outcome. Позднее cleanup сообщил,
  что отсутствие дерева не подтверждено за 5 секунд. Flood task
  `65fea721-0b0f-4bc7-a60b-be18f4128496` остался Running с launch intent,
  без startedAt/PID/output. Из этих фактов нельзя установить одну общую причину.
- Пользователь подтвердил отсутствие видимых запросов защиты и заметной нагрузки.
  Installed worker сохранил source `b5d39f9`, generation
  `02939593-8adf-4878-a346-473092697a4a`, session
  `631c2e63-edc8-44f8-8cce-89abec7caf8b`; connected/idle, maintenance/journal fault нет.

Отчёты и incremental logs: `WinBridge/workflow-validation/<run>/` в shared.
Оба run завершились failed; ready marker подтверждает публикацию отчёта, а не
успех тестов. Полный reference-worker proof во втором run ещё не выполнялся.

## Исправление после ревью и следующий прогон

Прежний resume выбирал первый поток нужного PID и считал любой результат, кроме
ошибки API, успешным. Это недостаточно: ResumeThread возвращает прежний suspend
count; только 1 подтверждает переход к нулю. [Microsoft API contract](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-resumethread).
Новый guard требует ровно один поток, проверяет PID открытого handle и count 1.
При неоднозначности запуск не продолжается; cleanup остаётся обязанностью OS owner.
Это исправление отдельного риска, **не доказанная причина** двух deadlines.
Stable Rust Child не предоставляет primary thread handle; полноценный adapter
CreateProcessW с владением hThread остаётся возможным следующим решением, если
наблюдения покажут недостаточность conservative guard.

Diagnostic mode отделён от полного gate. Trace различает prepare, OS spawn,
identity, journal binding, resume, parent exit, tree cleanup и output reader drain.
Timeout cleanup указывает независимо parent_reaped и tree_empty. Дополнительный
Windows test повышает suspend count до 2 и проверяет отказ, отсутствие user output
и cleanup только своего тестового процесса. Deadline не увеличен.

Нужен только прежний установленный worker; desktop закрыт, UI-окон не будет.
Сценарий использует новый isolated source/build/state и измеряет свободное место.
Диагностический прогон завершён; результаты ниже. Причина прежних двух deadlines
не доказана, и увеличение timeouts не применялось.

До native diagnostic: 23 local runtime/reconciliation/recovery tests passed;
Windows-target tests compile, Linux/Windows Clippy с `-D warnings`, fmt и
PowerShell AST passed. UI не изменён относительно clean browser gate.

## Узкий native diagnostic — passed

Source `b90a708d6a6178a65af77a89a40f275b0b636d2f`, clean archive
`winbridge-20261013T204006Z-b90a708-f8bdc612.zip`, SHA-256
`a5f9096ae650add7f9c240bb5b98f35fe843bc5e6761ccbf149a5fcd7d2a4251`.
Run `20261013-164022-975ce2c4`, task `c5d5373f-e832-45e3-ab8a-c6cf51a0c9bf`
succeeded/exit 0. `diagnosticOnly: true`; две groups exit 0,
**26 Windows tests** (1 resume guard + 7 reconciliation + 11 recovery + 7 runtime).
Три artifacts, включая report и оба logs, проверены по size/SHA-256.

Прежние delays не воспроизвелись. По восьми task traces максимумы:
prepare→bind 19 ms, bind commit 7 ms, resume 70 ms, cleanup→outcome commit 46 ms;
cleanup timeout нет. Installed worker build/generation/session прежние,
connected/idle, maintenance/journal errors нет. Desktop можно оставить закрытым,
worker оставить работать.

Диск K: available bytes до/после: `31820439552` → `29670723584`
(~2 GiB на isolated build; осталось ~27.6 GiB). Для full regression используется
этот же чистый source/build cache, а новые test states и shared report создаются
отдельно. Admission проверяет успешный diagnostic receipt и SHA каждого artifact,
точный archive SHA/source identity; Windows заново сверяет manifest и entrypoint
с archive, затем все source files. Reparse ancestors source/cache запрещены.
Полный gate сохраняет trace; diagnostic success не принимается вместо него.

Четыре Python fault tests проверяют wrong archive, changed artifact, failed/dirty/
неполную партию, traversal и linked artifact. Ни один прежний run не удаляется.
Полный gate пока не выполнен.
