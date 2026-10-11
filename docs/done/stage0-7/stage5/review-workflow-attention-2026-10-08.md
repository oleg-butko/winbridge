# Ревью workflow/application attention — 2026-10-08

План зафиксирован `aeae684` до реализации. Новый срез продолжает этап 5:
durable typed targets, точный переход из popup/inbox, явный ACK. Native gate
[пройден автоматически](windows-workflow-attention-gate-2026-10-08.md): 19 backend
tests, actual workflow popup/navigation/ACK и 12 verified artifacts. Installed
Windows runtime остаётся `f7c4878`.

## Важные исправления

- Результат workflow/application и notification сохраняются одной SQLite
  транзакцией. Ошибка INSERT attention откатывает terminal state; повтор reconcile
  не создаёт второй event и не сбрасывает ACK.
- Ошибка записи больше не выдаёт прежнее running/ready как свежее подтверждение.
  Workflow/application reads отклоняются; admission/queue ждут repair и restart.
  При сбое записи readiness блокируются обе связанные записи, а не только workflow.
- Child build/start task определяется настоящей связью в journal, не строковым
  префиксом idempotency key. Сигнал принадлежит parent workflow; readiness failure
  не создаёт второе application event. Late runtime failure оставляет startup success.
- Schema 5 migration сохраняет IDs, ACK, explicit dedup и sequence high-water даже
  после отсутствующей записи с большим sequence. Перед v4 upgrade — consistent
  VACUUM INTO backup. Legacy ACK не может подтвердить taskless event.
- AttentionTargets — отдельная additive capability. Старый task-only worker не
  получает неизвестные методы; desktop использует legacy feed. Старые task DTO
  сохраняют wire shape.
- UI открывает target по точному ID вне recent ten; ответ предыдущего poll не
  подменяет выбранную запись. Открытие страницы не выполняет ACK.

## Проверки

Workspace Rust tests и Clippy -D warnings, standalone Linux desktop Clippy,
Windows MSVC all-targets compile, generated contracts, TypeScript/UI build,
PowerShell parser и Python syntax прошли. Browser integration использует настоящий
Unix worker и WebSocket controller: taskless import failure → workflow page → UI ACK,
actual app crash → application page, старый target вне последних десяти workflow.

Windows сценарий `--workflow-attention-only` собирает новый worker и desktop в
отдельном local runtime. Он проверяет attention migration/backend outcomes и
настоящий popup → workflow page → UI ACK. Живые logs/journal остаются на локальном
диске; в shared публикуются закрытые immutable файлы с bytes/SHA из одного массива.
Рабочий runtime и production Netwatch не заменяются. История/diagnostics/MCP ещё
не реализованы; весь этап 5 не закрыт.
