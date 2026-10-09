# Ревью этапа 5 — 2026-10-09

После перезапуска Debian восстановлен controller с прежним локальным конфигом.
Read-only Info подтвердил установленный Windows worker source `1982de8`,
sourceDirty=false, history/typed attention доступны, журнал исправен, activeTaskCount=0.
Рабочий desktop пользователь закрыл; для ревью он не требуется.

Проверены MCP framing/lifecycle/schema validation и отсутствие automatic replay,
history cursor/filter/diagnostic transaction, redaction и create-only сохранение
отчёта. В сохранении найден важный дефект: повтор save ID читал существующий файл
целиком через `fs::read`. Лимит нового JSON 512 KiB не ограничивал этот путь;
совпадение имени с большим посторонним файлом могло исчерпать память desktop.

Readback теперь читает не более ожидаемого размера плюс один байт. Длинный файл
даёт mismatch; он не изменяется, сообщение об успешном сохранении не появляется.
Regression проверяет 2 GiB файл с совпадающим JSON prefix, отказ и сохранность
содержимого/длины. Остальные проверки Unicode/SHA-256/idempotency/collision и link
guards продолжают проходить. MCP contract tests также прошли.

Это локальная проверка исправления; новый Windows gate ещё не выполнялся.
Дальнейший сценарий этапа 5 — projects/connection settings и output retention.
Пользователь выбрал сохранение config с ручным перезапуском worker и отдельную
включаемую автоматическую очистку output. Установленные конфиги, журнал и старые
shared артефакты не изменены.
