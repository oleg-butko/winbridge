# Ревью журнала внимания — 2026-10-07

Проверены durable acceptance/dedup/ack, paging, task outcome transaction,
generation changes, PowerShell presentation, CLI/capability guards и desktop lifecycle.
Working tree был чистым; read-only запрос подтвердил прежний здоровый worker `523fc5b`.

## Существенная ошибка и исправление

После завершения процесса Store.finish может отказать при записи task outcome/attention.
Если попытка сохранить indeterminate тоже не проходит, прежний код только печатал
stderr и освобождал workspace: task оставалась running, а GetTask возвращал её как
обычный свежий ответ. Последующая команда могла начать изменять workspace до reconciliation.

Worker теперь публикует journalRecoveryRequired и сохраняет in-memory fault для
незаписанного результата. GetTask/cancel этой task возвращают конкретную ошибку,
новые execution/import/workflow submissions и запуск очереди приостановлены, lease
не освобождается. Fault публикуется под journal lock, admission повторно проверяет
его под тем же lock, исключая race после failed commit. Уже работающие задачи
сохраняют управление процессами; чтение доступных journal records остаётся доступным.
UI отделяет доступный worker от необходимости восстановить journal и блокирует новые
запуски. После ремонта storage и restart прежняя running task восстанавливается как
indeterminate; команда не повторяется. Volatile fault не выдаётся за persisted state.

Новый regression test запускает реальный fixture failure, намеренно отклоняет запись
SQL trigger, проверяет stale-result error, очередь/lease и отказ новой команды.
После устранения injected fault и restart сохраняются indeterminate и один execution.
Это предусмотренный отрицательный тест, основной Windows journal не менялся.

Проверки: 72 workspace tests, Clippy -D warnings, Windows all-targets compile и
TypeScript прошли. Browser/нативный regression будет проверен вместе со следующим
сценарием, без повторного ручного gate.
