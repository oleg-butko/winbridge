# История и диагностические отчёты

Сценарий этапа 5, реализован 2026-10-08 по [плану](../done/stage0-7/stage5/history-diagnostics-plan.md).
[Windows gate](../done/stage0-7/stage5/windows-history-gate-2026-10-08.md) пройден автоматически: 13 тестов и actual UI. MCP, настройки
projects/connection и политика удаления старых данных остаются дальнейшей работой.

## История

Worker читает tasks, imports, workflows и applications из своего SQLite journal.
`history_read` — additive capability. Controller отклоняет новые RPC до передачи
старому worker; desktop показывает необходимость совместного обновления.
Схема базы остаётся 5, миграция для этого сценария не требуется.

`history` возвращает до 50 summary и `hasMore`/`nextCursor`. Фильтры: projectId,
kind, status и буквальный search по ID/названию/проекту. `%` и `_` не являются
wildcards. SQLite case folding используется для ASCII; Unicode читается без потерь.
Search/cursor ID ограничены 200 UTF-8 bytes, управляющие символы запрещены.
Порядок: createdAtMs, kind, operationId по убыванию. Keyset cursor сохраняет
границу при совпадающих timestamps; состояния показывают момент чтения, страницы
не являются замороженным снимком всех операций. Journal recovery отклоняет свежее
чтение истории вместо подтверждения старого running/ready.

UI ищет и открывает точную task/import/workflow/application даже за пределами
последних десяти записей. Связь application → workflow берётся из журнала.
Ctrl+K фокусирует поиск, Ctrl+Shift+P открывает быстрые действия по ID.
Просмотр не выполняет команду и не подтверждает событие внимания.

## Диагностический отчёт

`prepare_task_diagnostic` читает одну task и выбранный output в одной SQLite
read transaction. JSON содержит build identity, время чтения, сохранённые события
task и признак terminal. Это исторический отчёт; он не проверяет живость внешнего
процесса и не повторяет команду. Configs, environment и сама SQLite не выгружаются.

По умолчанию `includeCommand=false`, `includeOutput=false`; request/idempotency
keys исключены. При включении команды распространённые credential fields/arguments
маскируются. PowerShell EncodedCommand, включая сокращения и inline payload,
также маскируется. До 32 непустых literal redactions, каждая до 4096 UTF-8 bytes,
применяются к строкам перед JSON encoding, включая output. Это не универсальный
детектор секретов: пользователь проверяет редактируемый preview перед сохранением.

Output ограничен первыми 128 chunks и 64 KiB суммарно; общий JSON — 512 KiB.
Байты каждого stream объединяются перед UTF-8 decoding, чтобы граница chunk не
ломала кириллицу. Некорректные UTF-8 bytes заменяются. Metadata сохраняет sequence,
offset и capturedBytes; quota/неполный output обозначается `outputTruncated`.
Оригинальный journal и output не изменяются.

Desktop сохраняет именно текст preview отдельной кнопкой. Во время сохранения
preview защищён от редактирования. `reportDirectory` задаётся локальным
`winbridge-desktop.local.json`; Windows default —
`K:\VMs\shared\WinBridge\diagnostics`. Старый конфиг с двумя полями поддерживается
и автоматически не переписывается. Worker не принимает произвольный destination
path от RPC клиента.

Имя — `diagnostic-<save ID>.json`, запись create-only с проверкой путей на links/
reparse points, sync/закрытием и readback bytes/SHA-256. Повтор с тем же ID и
текстом проверяет прежний файл; другой текст не перезаписывает его. Изменение
preview получает новый ID. При ошибке успех не показывается; файл сохраняется
для диагностики. Частично записанный файл не заменяется повтором: для новой
попытки после устранения причины подготовьте новый preview с новым save ID.

## Размеры и CLI

`storage_usage` возвращает counts операций/attention, pending count, stored output
bytes, текущую per-task output quota и размеры journal/WAL. UI только читает эти
значения; автоматическое удаление выключено. Полная retention policy ещё не
реализована.

CLI worker/controller: `history QUERY.json`, `diagnostic REQUEST.json`,
`storage-usage`. Diagnostic CLI возвращает preview, запись файла выполняется
desktop после просмотра.

```json
{"projectId":null,"kind":"task","status":null,"search":"","before":null}
```

```json
{"taskId":"<Task ID>","includeCommand":false,"includeOutput":false,"redactValues":[]}
```

## Проверки

Rust проверяет cursor ties/фильтры/перезапуск, смешанную историю реального pipeline,
unknown IDs, import journal fault/recovery, Unicode/output quota/redaction и
create-only collision/link guards. Browser integration использует настоящий
Unix worker/controller: старые страницы, точный ID, клавиатура, editable preview,
потерянный save ACK, повтор того же ID и сохранность предыдущего файла.

`--history-diagnostics-only` запускает новые тесты и настоящий desktop на Windows.
Отдельный instance/runtime/report directory не заменяет рабочие exe/config/state.
TEST окно показывает ход проверки, открывает старую task, сохраняет проверенный
JSON и закрывается само. Предыдущие gates полностью не повторяются.
[Запуск и назначение сценария](../validation/windows-workflow-validation.md).
