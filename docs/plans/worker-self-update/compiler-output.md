# Bounded compiler output — U1, 2026-10-18

`build_output::read_compiler_output` — trusted readonly API для следующего UI.
Пока нет public update/MCP command или нового supervisor command. Он читает
сохранённые файлы независимо от доступности worker, ничего не исполняет и не
снимает барьеры.

Rust/TypeScript DTO задают update/attempt scope, stream, byte offset, limit
1–65 536 и идентичность файла. Caller не передаёт путь. После первой страницы
курсор сохраняет File ID; новая попытка, другой stream, замена файла, обрезка,
неверный root/installation или слишком большой запрос отклоняются. Возвращаются
байты, чтобы UI мог декодировать UTF-8 постепенно без потери разделённых символов.

`phase`/`buildRevision` относятся к согласованному journal snapshot с
`observedAtMs`. `availableBytes` — размер при чтении, `hasMore` — оставшиеся
страницы этого наблюдения. Пока capture выполняется, отсутствие новых байтов не
означает завершения; `captureComplete=false`, окончательное число наблюдённых
байтов и quota truncation ещё неизвестны.

После terminal receipt проверяются полный retained size/SHA-256 в пределах
16 MiB. Возвращаемая страница берётся из **того же hash pass**, чтобы повторное
чтение после проверки не могло вернуть другую версию. `proofVerified` отделён от
`captureComplete`: capture error сохраняется явным полем. Ошибка запуска до
создания output объясняет отсутствие файла. Indeterminate не выдаёт завершённый
capture; terminal success не выводится из пустого файла.

Windows reader допускает продолжающуюся запись capture и запрещает replacement
на время чтения. UI не удерживает этот handle между страницами. На Unix после
чтения дополнительно сверяется File ID пути. Это наблюдение trusted local files,
а не sandbox против параллельного редактирования файлов тем же пользователем.

Проверки используют реальные compiler processes: paging по три байта/Unicode,
readonly scope, bounded request, SHA tamper той же длины, replacement, отсутствие
File ID при ненулевом offset, wrong-stream cursor и live/terminal capture.
Native proof добавлен в общий автоматический сценарий; Windows gate ожидается.

18 compiler regressions, Linux/Windows Clippy all-targets, contracts, typecheck/build:ui/scaffold и syntax checks passed. Native proof включён в повтор PowerShell gate.
