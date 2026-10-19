# U1: supervisor v8 — предыдущий runtime и отмена

2026-10-19. Internal trusted API, updater schema 9 / worker schema 15.
Реализация добавлена; local/native проверки указаны в handoff. Public update/install
и activation ещё не включены.

`ArchiveRuntimePreimages` выполняет быстрое metadata admission до ACK: verified
baseline, frozen config, свободное место, immutable intent/parent transition и
одноразовый permit. Hashing, process-authenticated ObserveRuntime и пять copies
исполняются на blocking driver. `ObserveRuntimePreimages` независимо читает parent,
baseline и preimages в одном SQLite snapshot. Typed states: NotAdmitted, Pending,
Verified; view schema 3. Supervisor v7 сохраняет schema-2 view, v1–v6 — schema 1;
новое JSON поле пропускается в старых контрактах. Сохранённая Verified — история
проверки, не новое разрешение на установку или гарантия текущего наличия файлов.

Active duplicate читает ту же попытку; completed duplicate не повторяет I/O.
Pending без fresh permit не возобновляет запись. Другая preparation work откладывает
новый driver. Source admission deferred во время проверки/копирования, а journal
guard запрещает source при pending intent после failure/crash.

Для отмены active copy v4 AbortBeforeCutover сохраняет прежний контракт ответа.
Внутри supervisor один `Arc<UpdateAuthority>` удерживает тот же OS lock/epoch до
завершения mailbox и callback. Дополнительное соединение сохраняет только abort
intent через прежние CAS/authority checks; AcquireOwner/новый epoch не выполняются.
Durable parent Aborting останавливает copier на следующем checkpoint. Свежий abort
permit остаётся у actor; verification/release запускаются после фактического
завершения callback и освобождения копировщиком файлов. Partial copies сохраняются.
Source/compiler abort использует прежние условия cleanup.

Нативный positive fixture использует один настоящий test Worker actor/guard и две
последовательные операции: successful five-file generation, затем новый request/
owner epoch с cancellation при удержанном worker reply. Проверяются durable intent
до ACK, status/duplicate/source defer, exact copies/разные File IDs, history без
повторного hashing и abort intent без раннего cleanup/release. Source A и runtime C
не изменяются. Other-role descriptors synthetic; candidate health не заявляется.
Установленные exe/journals и пользовательские окна не участвуют.

Review отдельно исправляет порядок уничтожения fixture: SQLite/owner handles
закрываются до TempDir, чтобы Windows не оставляла незаметные временные каталоги.
Проверка actual NTFS pin/release сохранена в предыдущем library gate; новый actor
сценарий требует собственного native execution.

Локально прошли 518 workspace tests, отдельный повтор actual no-lease abort,
Linux/Windows Clippy all-targets, fmt/contracts, typecheck/scaffold, exporter и
runner regressions. Positive two-operation Windows fixture compiled; native execution
пока впереди. No-lease outcome теперь явно сообщает отсутствие lease, не выдуманное
освобождение. Worker task/source/compiler ownership и их cleanup guards сохранены.

Pending описывает отсутствие verified receipt; после отмены окончательный результат
берётся из parent/AbortRecord, а не трактуется как продолжающийся copy driver.
