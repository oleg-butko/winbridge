# История и диагностический отчёт — план 2026-10-08

Следующий сценарий этапа 5 после workflow/application attention. Ревью исправляет
stale application при незаписанном результате связанной task; startup success
остаётся историческим фактом. Проверяется настоящим процессом и fault injection.

HistoryRead — additive capability: один read-only RPC возвращает до 50 task/import/
workflow/application summaries, с фильтрами project/kind/status и буквальным поиском
по ID/названию. Keyset cursor использует immutable время создания, kind и ID;
mutable status отражает момент чтения, страницы не объявляются frozen snapshot.
Journal recovery отклоняет свежий history read. UI открывает точный ID за пределами
recent ten; чтение/выбор не создаёт команду и не подтверждает attention.

Диагностический отчёт относится к одной task: frozen journal record, её события,
выбранные stdout/stderr, output quota/gaps, build identity и время чтения. Worker
готовит ограниченный JSON preview в одной read transaction. По умолчанию параметры
команды и output исключены. Explicit switches добавляют их; common credential
arguments маскируются, EncodedCommand payload исключается, дополнительные literal
redactions применяются к string values до JSON encoding. До 32 redaction strings,
bounded output; обрезание и неполнота отмечаются явно. Configs/env/SQLite не экспортируются.

Desktop показывает редактируемый preview перед отдельным сохранением. Он сохраняет
только показанный JSON, новым create-only файлом с SHA-256/bytes и проверкой readback.
Directory задаётся локальным winbridge-desktop.local.json; Windows default
K:\VMs\shared\WinBridge\diagnostics. Ошибка/mount loss не показываются как успех;
retry не перезаписывает прежний файл. CLI возвращает тот же preview; worker не пишет
произвольный destination path от RPC клиента. Runtime settings пользователя не меняются.

Storage panel показывает counts, stored output bytes, текущую per-task квоту и
размер локального journal/WAL. Автоматического удаления/retention нет: сначала
пользователь видит размеры. Keyboard shortcuts открывают поиск истории и выбранную
операцию; существующие формы/close-save поведение сохраняются.

Проверки: cursor ties/older pages/literal wildcard search, mixed kinds, filters,
restart, unknown IDs и journal fault; Unicode/redaction/encoded payload, bounded
output и consistency; create-only write, symlink/junction guards и publication
readback. Browser integration с настоящим worker/controller и отдельный автоматический
Windows сценарий проверяют новую history UI и actual сохранённый diagnostic file.
Рабочий worker нужен; desktop необязателен. Тестовое окно обозначается заранее,
показывает шаги и закрывается само. Installed runtime/config/state не заменяются.

Этот сценарий добавляет историю и task diagnostics. MCP и полная политика retention/
projects/settings остаются следующим продолжением; весь этап 5 заранее не закрывается.
