# Ревью и закрытие этапа 3 — 2026-10-06

Пользователь принял UI исправления, отложил подробную ручную проверку и попросил перейти к более крупным итерациям с автоматическими Windows сценариями. Этап 3 закрыт на основании уже выполненных 19 Windows runtime checks, UI integration и этого решения. Отдельный новый ручной UI gate не заявляется.

В ревью исправлены две существенные ошибки:

- Config/desktop state читались целиком до проверки лимита 1 MiB. Теперь Read::take ограничивает фактическое чтение до limit+1; oversized или некорректный файл не перезаписывается.
- Journal path проверял только file_type.is_symlink, без Windows reparse attributes и hard links. Теперь до создания state и открытия lock/journal/WAL/SHM используется общий safe_path с проверкой всех ancestors, junction/reparse/special entries и single-link файлов. Linked target не открывается для записи.

50 Rust tests ревью проходят, включая oversized state preservation и journal linked ancestor/hardlink canary. [Автоматический Windows harness этапа 4](../stage4/windows-workflow-gate-2026-10-06.md) дополнительно подтвердил bounded config/state и настоящий NTFS junction/hardlink journal guard; все 37 native tests прошли. Compile отдельно от native execution.

Новый crash test обнаружил побочный эффект общего regular-file guard: старый Unix IPC socket отвергался до stale endpoint verification. Исправлено в следующем workflow commit: проверяются ancestors, затем тип socket и отсутствие живого server; неизвестный файл сохраняется. Owned crash/restart test проходит с прежними IDs и без повторения команды.

Следующий этап: persisted workflow import → build → start → readiness → log tail; process registry отделяет работающий app от завершённой сборки. Readiness и crash не подменяются фактом создания PID. Workflow/step IDs и accepted parameters сохраняются; неизвестная внешняя команда не повторяется при восстановлении. Изолированные новые Windows тесты собираются/запускаются через текущий разрешённый worker без обновления его exe, firewall и пользовательского journal. Native desktop smoke обновляется крупным общим выпуском, а не после каждого мелкого fix.
