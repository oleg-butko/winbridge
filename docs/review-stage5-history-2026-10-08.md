# Ревью перед историей этапа 5 — 2026-10-08

Проверены attention transactions/migration/capabilities, workflow/application
reconcile, guards свежести, exact target navigation и сохранение форм UI.

Найдена существенная ошибка: при failure UPDATE tasks результата owned процесса
fault регистрировался для task ID. GetApplication/StopApplication проверяли лишь
application ID и могли вернуть прежнее ready. Теперь они проверяют связанную task.
Незавершённый workflow также проверяет результаты своих child operations; завершённый
startup остаётся доступным как исторический succeeded даже при позднем app fault.

Регрессия запускает реальный fixture с поздним crash, запрещает terminal UPDATE
tasks SQLite trigger, проверяет stale read/stop отказ и сохранение startup success.
После repair/restart приложение становится indeterminate с одним событием, без
повторного внешнего запуска. Этот targeted Linux тест прошёл.

Следующая работа зафиксирована в [плане](history-diagnostics-plan.md) до реализации.
Installed Windows runtime остаётся f7c4878; новые функции проверяются в отдельном
runtime через уже разрешённый worker. Новый Windows gate пока не выполнен.
