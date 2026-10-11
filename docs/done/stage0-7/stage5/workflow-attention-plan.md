# Workflow/application attention — план 2026-10-08

Продолжение этапа 5 после подтверждённого tray gate. На Windows установлен `f7c4878`,
controller после reboot восстановлен. План фиксируется до реализации.

Workflow failed получает error, indeterminate/interrupted — warning. Уведомление
содержит имя цепочки, неуспешный шаг, причину и настоящую ссылку на workflow, в том
числе когда import/очередь не создали Task ID. Success и явная cancellation тихие.
Child build/start failure не создаёт второе task notification для того же сбоя.

После подтверждённого startup приложение получает error при failure и warning при
indeterminate либо самостоятельном выходе, включая exit 0. Для управляемого долгого
приложения выход означает остановку сервиса, а успешный startup остаётся историческим
фактом. Явный stop тихий. Ошибка readiness/log до завершения startup сигнализируется
одним workflow event, без дублирующего application event.

Новый typed target: task, workflow либо application. Legacy task attention DTO/RPC
сохраняются. Additive event feed/ack RPC и capability обслуживают все targets; старый
worker не получает неизвестных запросов. Desktop выбирает новый feed по capability
и открывает точный workflow/application, даже за пределами последних десяти записей.
Открытие не подменяет acknowledgement. Tray/popup используют те же настройки/cursor.

SQLite schema 5 допускает attention без task FK и сохраняет typed target. Перед
upgrade v4 выполняется consistent backup. Все прежние IDs, sequence, ACK и explicit
dedup сохраняются. Terminal record и event записываются одной транзакцией; повтор
reconcile/restart не создаёт новое событие. Ошибка записи результата не выдаётся за
свежее состояние, admission/queue приостанавливаются до восстановления журнала.

Проверки: failures до task, build/readiness/log, runtime exit/crash, explicit stop/cancel,
lost ACK/dedup/restart, v4 migration, atomic rollback и отказ выдавать stale состояние.
Browser integration с настоящим worker проверяет target navigation и ACK. Новые Windows
tests запускаются через текущий worker в отдельном runtime, без повторения старого gate;
тестовые окна обозначаются заранее. Рабочий worker оставить запущенным, desktop необязателен.

Workflow/app attention завершает этот сценарий, но history/audit/export и MCP остаются
дальнейшей работой этапа 5. Рабочие runtime/config/journal и Netwatch тест не заменяет.
