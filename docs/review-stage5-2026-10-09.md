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

Исправление также прошло [Windows gate](windows-settings-retention-gate-2026-10-09.md):
4 report tests, включая oversized collision и junction guard. Projects/connection
settings и output retention завершены тем же автоматическим gate.
Пользователь выбрал сохранение config с ручным перезапуском worker и отдельную
включаемую автоматическую очистку output. Установленные конфиги, журнал и старые
shared артефакты не изменены.

Финальная browser проверка выявила гонку refresh истории: старые page/cursor
оставались доступны до нового ответа, поэтому «Более ранние 50» могла использовать
прежний cursor после «Свежие записи». Refresh теперь очищает page и выключает
pagination до подтверждённого ответа. Повтор actual browser integration прошёл,
включая keyboard navigation новых разделов, config/cleanup lost ACK и retention
metadata. Повтор native gate для этого изменения read-only состояния не назначался.

## Повторное ревью после обновления worker

Read-only Info подтвердил clean installed source `58f3c14`, generation
`41623e38-adb1-4a34-a2c4-3a43d9c1cf2c`, maintenanceAvailable=true,
maintenanceError/journalRecoveryRequired=null. Проверены config identity/restart,
bounded reads/create-only backups, retention predicates/atomic result/dedup и
local-only RPC guards. Десять maintenance regression tests проходят.

В JSON редакторе найден UI crash: syntactically valid JSON с неверными nested
types (например `recipes: {length: {invalid: true}}`) попадал в friendly controls
и React пытался отобразить object. Type cast не обеспечивал runtime validation.
Теперь controls получают только проверенные формы полей; неверный/неполный JSON
остаётся в textarea с объяснением, worker отклоняет save до записи. Browser
regression проверяет wrong worker/recipes/retention types, отсутствие crash,
сохранность исходного файла и последующее успешное исправление/lost ACK.

После ревью начинается [baseline этапа 6](stage6-plan.md). Измерения идут через
работающий worker без обновления его runtime и без дополнительных ручных UI gates.
