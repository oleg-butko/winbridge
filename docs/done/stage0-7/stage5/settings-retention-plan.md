# Настройки и хранение output — план 2026-10-09

Пользователь выбрал сохранение локального worker config с ручным перезапуском и
включаемую автоматическую очистку старого output. Изменение адреса/проекта/квот
не меняет параметры работающего worker и принятых задач до его перезапуска.

Desktop читает конфиг именно запущенного worker, различает active и saved версии,
показывает путь, изменения и инструкцию перезапуска. Instance/node/state остаются
фиксированными: смена хранилища/IPC — отдельная настройка установки. Изменяемые
поля — controller URL, delivery mapping, projects/recipes/direct policy, output
quota и retention. Перед записью проверяются JSON, filesystem policy и исходный
hash; concurrent edit не перезаписывается. Сохранение локальное, доступно только
desktop/local IPC; сетевой controller/MCP не может менять конфиг. Путь берётся
из CLI worker, не из аргумента UI. Конфиг записывается атомарно и перечитывается.

Очистка затрагивает только SQLite stdout/stderr завершённых задач старше
выбранного срока. История, команды, dedup payload/keys, attention и artifacts
сохраняются; snapshots, shared reports, приложение/его log и caches не удаляются.
Active/indeterminate задачи, операции активного workflow/application и output
с непрочитанным attention защищены. Предпросмотр показывает конкретные Task IDs
и bytes, явная очистка сверяет fingerprint выбранной партии. Удаление и отметка
в task journal фиксируются одной транзакцией; повтор key возвращает прежний
результат. UI и diagnostic объясняют отсутствие удалённого output.

Automatic output cleanup по умолчанию выключен. Включённая в config политика
начинает действовать после перезапуска; интервал и партия ограничены. Ошибки
видны, результат не выдаётся за success; journal recovery останавливает cleanup.
SQLite schema upgrade предваряется consistent backup. Физический файл SQLite
может не уменьшиться сразу; освобождённые страницы переиспользуются, VACUUM не
запускается скрытно.

Проверки: stale config/hash, validation/link guards, активный процесс во время
save, restart, protected output, preview/apply race, atomic rollback, lost ACK
и restart dedup. Browser/native UI проверяют формы, keyboard access, сохранение
и явный preview/apply. Один автоматический Windows сценарий выполняется в новом
runtime с заметным TEST окном; installed exe/config/state остаются прежними.
