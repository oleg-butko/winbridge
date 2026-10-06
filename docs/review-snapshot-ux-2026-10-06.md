# Ревью snapshot UX — 2026-10-06

Повод: пользователь подтвердил настоящий Windows import/build, но shared содержал много validation packages; delivery project/snapshot не сохранялись, а отказ непроверенной сборки показывал только missing marker path.

Исправления:

- Read-only `list_snapshots` проходит регистрацию проекта и использует только configured delivery namespace. Discovery ограничено 10000 directory entries, page 100; ID policy применяется до чтения metadata. Linked/reparse parent отвергается; такой package показывается с ошибкой без обхода ссылки. Missing shared root выдаёт ошибку, отсутствующий project при доступном root даёт пустой список без создания папок.
- Shared select показывает ID, commit, file count, ZIP bytes или metadata error. Payload при listing не читается: корректный manifest не выдаётся за проверенный ZIP. `.part` каталоги не становятся selectable snapshots. Старый список не переходит между worker/project/root scopes.
- Локальный state принимает прежний schema-1 файл и сохраняет command drafts при обновлении delivery selection. Родитель сериализует обе формы в общей очереди, close ждёт последнее сохранение каждой изменённой scope. Ошибка одной формы/project не скрывается успехом другой; failed close остаётся открытым с явным действием закрытия без сохранения.
- Отказ команды при отсутствии verified marker объясняет необходимость успешного импорта, сохраняет исходную filesystem причину и не создаёт TaskRecord/process. Full source verification перед запуском сохраняется.

Проверки: 48 Rust tests, Clippy -D warnings, fmt, Windows all-targets check, standalone Linux desktop check, generated contracts/strict TS/UI build. UI integration использует настоящие worker/WS/import и файловый draft shim, проверяет discovery/refresh, независимые проекты, fresh WebView restore без import/task, две delayed saves перед close и delivery-save failure после успешного command save. Native Windows baseline прошёл 19 checks до UX update; это не заявляет выполнение новой UI сборки Windows. [Доказательства и оставшаяся проверка](windows-snapshot-gate-2026-10-06.md).

IP filtering, cleanup старых shared packages, services и firewall не добавлялись. Dirty snapshot и pipeline runner остаются последующими этапами.
