# Настройки проектов/соединения и хранение output

Реализовано по [плану](settings-retention-plan.md), выбор пользователя 2026-10-09:
config применяется после ручного restart, automatic output cleanup включается явно.
[Native Windows gate](windows-settings-retention-gate-2026-10-09.md) пройден:
36 backend tests, actual UI save/restart и 19 verified artifacts. Рабочий runtime
пользователя не заменён тестами.

## Настройки в desktop

Раздел «Проекты, соединение и хранение» читает путь/config именно запущенного
worker, включая snapshot-конфиг, который может отличаться от desktop.workerConfigPath.
Действующая конфигурация и сохранённый файл показаны отдельно. Можно редактировать
controller URL, projects/root/direct permission, recipes/delivery в JSON, квоту
output и политику хранения. Новая папка проекта должна уже существовать на локальном
диске. Убрать project из config не означает удалить его files/history.
Для Git publication sourceRoot этого проекта регистрируется отдельно в локальном
конфиге Debian controller; Windows registration сразу позволяет выполнять команды.

Instance ID, node ID и stateDirectory редактор не меняет. CLI `--config PATH serve`
закрепляет источник конфигурации; Worker::open без файла даёт actionable read error.
Сохранение проверяет generation, strict JSON/schema/policy/paths/лимиты, SHA-256
прочитанной версии и повторно проверяет файл после staging. Concurrent edit
отклоняется; стороннему редактору всё равно следует избегать одновременной записи:
файловая система не предоставляет общий CAS для некооперирующихся редакторов.

Сохраняются точные bytes показанного JSON, до 64 KiB. Прежний config сохраняется
create-only рядом в `<config>.before-<save UUID>.json`; он содержит те же аргументы,
что оригинал. На Unix новые файлы private 0600; Windows наследует ACL локального
каталога runtime. Backup и staging закрываются/sync/readback до atomic replacement;
результат подтверждается readback конечного файла. При потере ответа повтор использует
тот же UUID/JSON. Изменение JSON получает новый UUID. Ошибки оставляют прежний и
подготовленные files для диагностики; они не удаляются скрытно.

Настройки running worker immutable: save не переключает WebSocket, не останавливает
process tree и не меняет параметры принятых команд. Desktop явно показывает
restartRequired и инструкцию `worker.exe --config <показанный путь> serve` после
Ctrl+C. Остановка worker завершает управляемые процессы; перед ней проверить задачи
и приложения. Повтор `prepare-windows-snapshot.ps1` сохраняет существующий snapshot
config, поэтому UI настройки не теряются при обновлении исходников.

Неполный JSON сохраняется в форме до исправления или явной отмены. Несохранённая
правка/maintenance call удерживает окно открытым; save не выполняется при восстановлении.
После нового generation незаписанная правка остаётся видимой и требует reload.
Клавиатура: Tab/Enter в навигации разделов/controls, Ctrl+K для history,
Ctrl+Shift+P для quick actions; переход ставит focus на выбранный раздел.

## Что очищается и что остаётся

Очистка удаляет только SQLite output rows завершённых tasks: succeeded/failed/
cancelled/interrupted, finishedAt строго раньше выбранной даты (минимум 1 день).
Queued/running/indeterminate, дети незавершённых/indeterminate workflows и активных/
indeterminate applications исключаются. Pending attention защищает task и связанные
дети workflow/application. Read-only preview — до 50 Task IDs и bytes; hasMore
означает следующую отдельную партию. Fresh tasks не попадают в уже просмотренный план.

Apply начинает IMMEDIATE transaction, заново вычисляет выбранную партию и сверяет
fingerprint/IDs. Изменённые state/output/attention требуют нового preview.
Удаление, outputRemovedAtMs/removedOutputBytes и timeline event, durable key/result
фиксируются одной транзакцией. При SQLite error всё откатывается. Повтор key после
потери ACK/restart возвращает прежний result; другой plan с тем же key отвергается.

Сохраняются task status/exit/command/events, original submission/fingerprint/dedup,
workflows/applications/attention/artifacts. Повтор команды с прежним key возвращает
историческую task, а не запускает новый процесс. Task outputBytes — исторический
captured total; UI вычитает removedOutputBytes и явно объясняет очистку. Diagnostic
отмечает truncation/removal, не выдаёт отсутствие сохранённых bytes за полный output.

Snapshots, shared reports, logs приложений, caches и пользовательские файлы вообще
не участвуют. SQLite file может остаться прежнего размера: свободные страницы
переиспользуются. Никакого скрытого VACUUM, shrink или файлового cleanup нет.

## Automatic policy и границы

```json
"outputRetention": { "automatic": false, "olderThanDays": 30 }
```

Legacy config получает false/30. Допустимо 1–3650 дней. После явного включения и
restart фоновая работа не чаще раза в 60 секунд очищает до 50 подходящих задач
по тем же predicates/transaction. Schedule не выполняет SQLite deletion на своём
потоке; shutdown ждёт собственную maintenance работу. Journal recovery блокирует
очистку. Ошибка видна в WorkerInfo.maintenanceError и desktop; следующий successful
pass очищает error. Таблица output_cleanup хранит durable результаты; schema 6
upgrade предваряется consistent v5 VACUUM INTO backup, никаких копий live WAL через HGFS.

Четыре maintenance RPC доступны только local IPC Windows пользователя. Controller
отклоняет их до forwarding, worker дополнительно отвергает их на WebSocket. MCP
не объявляет эти четыре local-only tools; прежние 30 инструментов сохраняются.
Desktop не передаёт произвольный config path. Проект/cwd всё ещё не является sandbox.

Проверки: backend transactions/recovery/protected targets/batches/active PID,
actual browser UI lost ACK и автоматический Windows сценарий
`validate-workflow-native.py --settings-retention-only`. Он создаёт собственный
runtime/worker/TEST desktop и после save перезапускает только тестовый worker.
