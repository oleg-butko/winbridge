# A2.4.2a — диагностика восстановленного журнала

Дата: 2026-10-13. База: `d1f3090`. Реализация и local/native gates проверяются;
полный A2.4.2/A2 остаётся открытым.

## Ревью и выбранная граница

Ревью A2.4.1 обнаружило два важных дефекта. `serve` завершал worker при ошибке
планировщика, хотя уже принятые процессы принадлежали своим OS owners.
Теперь ошибка приостанавливает scheduling/admission, сохраняет IPC и обслуживание
принятых задач; real CLI test удерживает running process первого проекта,
внедряет ignored claim второго, читает Info/output и подтверждает настоящую
отмену первого без повторного запуска второго. Вторая проблема: startup восстанавливал task, но
первый GetApplication мог вернуть прежний Ready до первого scheduler tick.
Сверка приложения с task теперь выполняется до открытия IPC, без advancement
workflow или spawn. Ошибка сохранения application блокирует новые запуски,
а чтение конкретного незаписанного результата возвращает ошибку.

После копирования старой SQLite могут исчезнуть новые idempotency keys. Ни новый
epoch, ни проверка Job Object не обнаружат утрату этих ключей только по старой
базе. Поэтому первый пакет восстановления — явное, сохраняющееся запрещение
эффектов и доступ к исходным свидетельствам. Он не объявляет rollback detection
или безопасное возобновление реализованными.

## Контракт

`winbridge-worker --config PATH serve-recovery` берёт прежний exclusive instance
lock, создаёт и синхронизирует `stateDirectory/journal-recovery.required`, затем
открывает **существующий** journal с SQLite READ_ONLY и query_only. Schema должна
быть ровно 11. Другие версии получают отказ без миграции; marker сохраняется.
Не создаётся новая база, не меняются incarnation/epoch/attempts/leases, не
выполняются task/import/application recovery, physical probes, retention или
scheduling. Даже успешно проверенное отсутствие Windows Job Object здесь
не используется для release: это режим чтения свидетельств.

Normal `serve` проверяет marker **до** Store::open/migration/begin_authority.
Наличие даже пустого marker запрещает запуск; повторный `serve-recovery` не
переписывает его. Ни desktop, ни CLI, ни MCP не имеют команды очистки marker.
Он сохраняется при обычном перезапуске и замене только SQLite. Это не внешний
high-water anchor: удаление marker или восстановление всего stateDirectory
в обход процедуры пока не обнаруживаются автоматически.

Допущенные RPC перечислены явно: Info, чтение tasks/imports/workflows/applications,
output/log, history, attention, размеров, локального конфига и snapshot previews.
Остальные методы, включая cancel/stop, ACK, config save, import/build/workflow
и cleanup, отклоняются **до** выполнения обработчика. Новые RPC по умолчанию
тоже запрещены. Guard Store дополнительно отказывает writers; SQLite READ_ONLY
остаётся последней защитой от случайного обхода application allowlist.

Транспорт остаётся прежним. Legacy Hello advertises семейства v1 RPC, а не
текущее разрешение на запуск. Info содержит `journalRecoveryRequired`, optional
`journalInspection {schemaVersion,reason}`, unknown activeTaskCount и отключённые
mutation/maintenance flags. В обычном режиме optional field отсутствует;
frozen legacy fixtures и wire enums не меняются. A3 должен объединить эту
динамическую доступность с discovery/read model.

Inventory включает все unreleased leases и running/indeterminate legacy tasks,
running imports без lease/member link. Legacy overlap не требует успешного
capture при startup: диагностика показывает обе записи без создания ownership
или cleanup proof. В ответе первые 50 записей, полный count, truncated и bounded
reason. UI сохраняет просмотр истории и показывает, что Running/Ready —
исторические записи. Кнопки запуска, отмены, ACK и сохранения выключены.

## Повторное ревью и проверки

Проверяем реальные SQLite records, keys, output, revisions и authority до/после
режима; несовместимые версии и пустой marker; exclusive lock; lost-key restore
consistent backup при закрытых test connections; legacy overlap и 52/50 inventory;
повторный actual CLI restart/IPC; scheduler fault и первый application read.
На Windows отдельно удерживаем собственный suspended bound process: inspection
не делает resume/kill, Job Object остаётся occupied, journal bytes/records прежние.
Только test owner освобождает его после проверки.

Browser gate использует real Unix worker, переходит в serve-recovery, проверяет
history, banner и disabled actions, настоящий отказ normal restart и повторную
диагностику. Это не native desktop execution. Native gate запускает новый
`recovery_mode` вместе с прежними CAS/ownership/lease/workflow targets из чистого
candidate archive; installed schema-6 binary/config/journal не обновляются.

## Следующий пакет — A2.4.2b

Нужен проверяемый административный переход к новой incarnation/паре после
восстановления: подтвердить изоляцию от прежних физических owners и от retries
клиентов, чьи keys могли быть потеряны. Новая incarnation сама по себе не позволяет
освободить uncertain lease или принять старый неподтверждённый запрос.
Следует выбрать механизм сохранения restore/admission evidence вне откатываемой
SQLite либо новую изолированную instance/workspace с явным переподключением
клиентов. Пока этот контракт не реализован и не прошёл fault/native gates,
выход из latched recovery не предоставляется; installed worker не обновляется.
