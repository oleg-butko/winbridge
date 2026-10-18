# U1: точная связь compiler completion и candidate report

2026-10-18. Trusted API; public update и runtime replacement отключены.

Compiler с настроенной build policy должен создать фиксированный
`runtime/update/<update-id>/candidate/candidate-report.json`. До запуска в одной
транзакции с Build Intent сохраняются отдельное report intent, SHA-256 frozen
BuildSpec, File ID runtime и каталога update. Уже существующий candidate staging
отклоняется до создания процесса. Report intent не разрешает отдельное исполнение.

После фактического root exit, пустого compiler Job, окончания обоих output readers
и source/config readback driver читает report. Проверяются прежние runtime/update
File IDs, candidate directory identity, single-link regular file, отсутствие
symlinks/reparse points, размер 1…4 MiB, file identity и SHA-256. На Windows read
handle запрещает конкурентные write/delete; повторная проверка пути и каталогов
выполняется до завершения capture. Успешный Build Completed и report evidence
сохраняются **в одной SQLite transaction**. Потерянная запись любого из них
откатывает обе записи. Отсутствующий/непроверяемый обязательный report оставляет
Indeterminate; исходный key только читает прежний attempt, не запускает compiler.

`build_report::read_verified` читает согласованный journal snapshot и возвращает
именно те bytes, которые сверены с сохранённым receipt. Повторное открытие JSON
после проверки SHA не требуется. Замена файла с теми же bytes, правка без изменения
размера, замена candidate/update directory или hard-link alias отклоняются.
Обычный/legacy compiler без report intent не может задним числом присвоить
появившийся report. Чтение не запускает процессов и не меняет outcome.

Updater schema 6 добавляет две append-only таблицы, с запретом update/delete:
`update_build_report_intents`, `update_build_reports`. Миграция требует owner lock,
проверенный consistent SQLite backup и новую epoch; существующие grants не
перепривязываются. История schemas 1–5 читается без присвоения новых доказательств.
Старый updater обязан отказаться от schema 6 до записи. Worker schema остаётся 15;
BuildRecord/BuildSpec и supervisor v1–v4 wire shape не меняются. Отдельный readonly
snapshot имеет собственный contract, пока только в trusted library.

Report receipt доказывает происхождение bytes от одного управляемого compiler
attempt. Он **не доказывает** правильность JSON, artifact hashes/roles, пригодность
worker journal, worker health или разрешение установки. Следующий driver должен
проверять содержимое этих exact bytes, текущие файлы payload и старый runtime,
сохранять artifact-set receipt и только затем продвигать candidate FSM.

Регрессии: intent до dispatch; duplicate без повторного процесса; atomic completion
при ignored SQLite writes; file/parent replacement и same-size edit; hard links;
missing/empty/oversized report; отказ от существующего staging; schema-5 backup /
epoch fence и запрет присвоения позднего report. Compiler actor fixture также
создаёт и проверяет обязательный report перед coordinated abort.

Local/native verification status находится в handoff и отдельном gate report.
