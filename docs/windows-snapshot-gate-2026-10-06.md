# Windows snapshot gate — 2026-10-06

Настоящий Windows runtime gate пройден: `debian-20261006-115911-c3629b0a`, `platform: windows`, `passed: true`, 19 проверок. Отчёт находится в `K:\VMs\shared\WinBridge\snapshot-validation\debian-20261006-115911-c3629b0a`; Debian прочитал его и проверил размеры/SHA-256 всех пяти файлов по `report.ready.json`.

SHA-256 `validation-report.json`: `fa10b55472d8df70512637c4918da405d26b2fd09efb9e489800a5783636f7d6`.

Подготовка пользователя: `20261006-181804-b74b6c4b`. Worker generation: `69c79c25-93d2-475b-93ce-53e3af67a92e`; fixed runtime `K:\_proj-26\winbridge\runtime`. Исходный config/state сохранён. Контроллер принимает эту Windows пару без IP фильтрации.

## Реальное исполнение

| Проверка | Доказательство |
| --- | --- |
| Импорт точного commit | Snapshot `s-9c3799cf4a9a4c658b222158dc319b2fb0718279`, import `63d6415a-97cf-4e2d-8578-72b134f28b99`, succeeded/verified. |
| Concurrent duplicate, lost ACK и конфликт payload | Четыре одновременных запроса возвращают один durable Import ID; повтор key сохраняет ID, изменённый payload отклоняется. |
| Windows build | Task `0f9646e9-86b2-4786-8993-b58caaddabb2`, cargo build --locked --offline; cwd внутри verified generation, cache снаружи; exit 0. |
| Запуск собранного exe | Task `102d9bb3-434a-4df7-9bd5-3952d38e3772`, exit 0, stdout `snapshot-fixture: Проверенный commit → Windows build ✓`. |
| Missing ready, corrupt ZIP, unavailable package | Imports failed; ни один не допускает запуск команды из непроверенного поколения. |
| Отмена активного импорта 64 MiB | Cancelled, незавершённое поколение не доступно для сборки. |
| NTFS junction | Настоящий junction под уникальным именем fixture workspace отклонён до записи; canary остался `unchanged`. |

Native gate не отключал VMware mount и не завершал пользовательский worker. Actual crash/recovery отдельного owned Unix worker задокументирован в handoff; Windows crash выполнения импорта этим отчётом не заявляется.

## UI и найденные неудобства

Пользователь подтвердил connected controller, блок доставки, verified import, preview состава, конкретный отказ повреждённого ZIP, чтение imports/tasks после открытия окна и сохранение command snapshot field без автоматического запуска.

Пользователь выбрал `s-516075a603c0e384a96a5522db5d4e61f473283e`: это также корректный fixture commit, но с `padding.dat` для проверки 64 MiB импорта. Его сборка успешно завершилась. Unicode строка появляется при отдельном запуске exe, а не в stdout `cargo build`; агент проверил этот запуск на основном маленьком snapshot.

По наблюдениям исправлены выбор опубликованного snapshot из shared-списка, сохранение проекта доставки/snapshot в локальном desktop state и понятный отказ сборки до успешного импорта. Rust, реальный Unix worker/UI integration, Windows all-targets compile и standalone desktop check проходят. UI test подтверждает restore без import/task, ожидание обеих форм при закрытии и честную ошибку сохранения.

**Осталось:** обновить fixed Windows binaries и подтвердить новые список/сохранение доставки и сообщение отказа. Показ Unicode в native task detail также можно подтвердить, выбрав Task ID запуска выше. Повторять пройденные 19 runtime checks и этапы 1/2 не требуется; этап 3 остаётся на завершающей UX проверке.
