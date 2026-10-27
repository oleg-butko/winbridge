# Review при подключении actor25: освобождение Unix log lock

2026-10-26. Во время полного workspace повторился прежний редкий WouldBlock
в `event_log::rotation_and_partial_crash_recovery_preserve_complete_records`
после закрытия RollingFile и немедленного повторного открытия.

Детерминированно воспроизведён механизм open file description: если сохраняется
дубликат lock FD, закрытие исходного FD не освобождает flock. Такой дубликат может
кратковременно существовать в child между fork и exec даже при CLOEXEC. Это
вероятное объяснение сбоя при параллельных process tests; tracing конкретного
fork во время исходного сбоя не выполнялся.

`RollingFile::drop` на Unix теперь сначала закрывает log file, затем явно
освобождает flock. Это завершение lifetime именно диагностического writer;
worker/update authority locks не менялись. Windows implementation не менялась.
Новый regression test удерживает cloned description после drop writer, проверяет
повторное открытие и то, что закрытие старого alias не освобождает lock нового
writer. Первоначальная эксклюзивность по-прежнему обязательна.

Проверки: **5 event-log tests passed**; отдельный OS experiment подтвердил
WouldBlock с retained alias, успешный новый owner после LOCK_UN и сохранение
новой эксклюзивности после закрытия старого alias. Evidence:
`.winbridge/update-preparation/actor25-event-log.log` и
`actor25-event-log-inheritance.json`. Failed full run:
`actor25-workspace.log`; он не записывается как успешный. Полный повтор и обе
Clippy относятся к следующему commit actor25 и будут записаны после завершения.

Рабочий Windows worker0.1.1/source59249e6 не заменялся и не проявлял неисправностей.
