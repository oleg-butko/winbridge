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
Диагностический прогон ещё не выполнен. По результатам решается следующий шаг;
автоматический повтор полного gate до анализа причины не назначен.

До native diagnostic: 23 local runtime/reconciliation/recovery tests passed;
Windows-target tests compile, Linux/Windows Clippy с `-D warnings`, fmt и
PowerShell AST passed. UI не изменён относительно clean browser gate.
