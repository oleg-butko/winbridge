# Workflow и приложения — этап 4

Реализован цикл на committed snapshot: import → build → start → stdout readiness → чтение UTF-8 лога. Это один durable запрос Windows worker. Controller/desktop пользуются теми же RPC; закрытие UI или потеря controller не отменяют принятую цепочку. Настоящие Windows проверки запускаются автоматически через подключённый worker; [сценарий](../validation/windows-workflow-validation.md).

## Подготовка и запуск

Из Debian подготовить свежий fixture, без изменения работающих конфигураций:

```bash
python3 scripts/prepare-workflow.py --controller-config .winbridge/controller-snapshot.local.json
```

Сценарий создаёт отдельный committed Git root, публикует пакет в shared, возвращает точные Snapshot ID/manifest hash и сохраняет `workflow.request.json` в `.winbridge/workflow-fixture/<run>`. Он читает реальные workspace/recipes подключённого worker. Само приготовление профиля команды не запускает. После обновления worker можно передать подготовленный файл:

```bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json workflow .winbridge/workflow-fixture/<run>/workflow.request.json
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json workflows
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json get-workflow WORKFLOW_ID
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json applications
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json stop-application APPLICATION_ID
```

Либо `prepare-workflow.py --submit` готовит и передаёт новую цепочку. После потерянного ACK повторять **сохранённый файл**, с прежним idempotencyKey; повторная подготовка создаёт новый запрос. В desktop: выбрать пакет → «Посмотреть состав snapshot» → «Использовать в workflow» → проверить JSON профиль → «Запустить workflow». UI предлагает шаблон для snapshot-fixture; произвольное приложение требует своих команд и readiness token. Принятый профиль сохраняется в Windows journal; «Заполнить профиль из workflow» восстанавливает параметры без исполнения. Незаконченный JSON черновик workflow пока не сохраняется в desktop draft.

## Контракт

Источник — [Rust DTO](../../crates/protocol/src/workflow.rs); TypeScript генерируется. Профиль содержит name/projectId/snapshotId/manifestSha256, build/start CommandSource, общий timeoutMs, readiness `{stdoutContains, timeoutMs}` и относительный logPath. Корреляция: protocolVersion/requestId/idempotencyKey. Рецепты используют фиксированные argv; direct использует разрешённый проект и те же path/argument guards, что обычная команда. Executable запуска проверяется после успешной сборки, потому что до сборки его может не существовать.

Workflow/step IDs и принятый профиль записываются до ACK. Child import/task keys выводятся из workflow/step ID; crash между сохранением child и ссылки на него не дублирует внешнее действие. Manifest hash фиксируется в запросе и каждом child task. Перед build/start проверяется весь immutable snapshot. Ошибка prerequisite отмечает текущий шаг и пропускает последующие; старый exe в cache не запускается после build failure.

В schema 3 добавлены workflows/applications. Перед миграцией существующей schema 1/2 создаётся consistent SQLite backup через VACUUM INTO, один на миграцию. Ни journal, ни backup не попадают в shared. При рестарте внешний task с launch intent получает indeterminate; app отражает этот результат. Неизвестная команда автоматически не повторяется. Queued intent без внешнего эффекта может продолжить свою очередь.

## Владение и deadlines

Project reservation охватывает startup prerequisites, поэтому другой import/build/command не вклинивается между ними. После готовности и подключения лога приложение остаётся управляемым процессом; mutation lease освобождается. Lease хранит operation ID: завершение старого приложения не снимает lease более новой команды. Worker учитывает живые процессы отдельно от leases при shutdown.

Одновременно допускается одно живое приложение на проект; перед новой сборкой/запуском его нужно явно остановить. Это защищает общий Windows build cache от замены работающего exe. Уже принятая следующая цепочка ждёт; deadline включает ожидание в очереди. Границы: 128 активных workflow, 32 живых приложения, последние 10 записей в списках; старые записи доступны по ID. Общая retention журнала остаётся этапом 5.

Общий workflow timeout ограничивает startup, включая import/build/readiness/log. readiness timeout отсчитывается от старта процесса и включает подключение лога. Runtime приложения после успешного startup не заканчивается по этому timeout; оно работает до stop, собственного выхода или остановки worker. При провале готовности/лога worker отменяет управляемое дерево и ждёт подтверждения остановки. Stop intent не считается ни готовностью, ни завершением.

Успех startup и дальнейшая жизнь приложения показаны отдельно. PID подтверждает создание процесса, stdout token этого процесса подтверждает readiness, readable log подтверждает доступ к логу. Падение после готовности меняет ApplicationRecord, сохраняя исторический успешный WorkflowRecord. Отмена завершённого workflow не останавливает приложение; для него есть `stop-application`.

## Уведомления о результате

Workflow failure/indeterminate создаёт одно событие с workflow ID, шагом и причиной,
включая import failures до появления Task ID. Owned build/start task не создаёт
дублирующее task notification. После successful startup crash/exit/unknown приложения
даёт отдельный application event; прежний startup остаётся succeeded. Stop/cancel
тихие. Readiness/log failure сигнализирует родительский workflow. Все terminal
records и соответствующие events записываются атомарно в schema 5; journal fault
блокирует выдачу stale результата и новые запуски. [Контракт](attention.md).
[Windows target navigation gate](../done/stage0-7/stage5/windows-workflow-attention-gate-2026-10-08.md) пройден.

## Чтение лога

`logPath` находится внутри зарегистрированного проекта, вне snapshots/.winbridge. Worker отклоняет traversal, junction/symlink, special files и hard links. LogRead возвращает raw bytes; кодировка профиля — UTF-8. Page ≤64 KiB; byte offset не разрезает данные при последовательном чтении, UI декодирует incremental TextDecoder и держит последние 65 536 Unicode символов.

Cursor включает file identity, offset и hash первых ≤128 bytes. Смена identity означает rotation; уменьшение длины — truncation; изменённый prefix — rewrite. Reset явно показан, новый файл читается с начала. Перезапись области после prefix без смены identity/длины не обнаруживается: для гарантированного restart логгера использовать rotate/truncate. Файл приложения worker не изменяет. Runtime log retention принадлежит приложению; квота stdout/stderr task продолжает drain и отмечает потерю данных.

## Профили и результаты сборки

Именованные editable profiles и незаконченные JSON drafts сохраняются в desktop state
рядом с exe по instance/project/workspaceRoot. До 128 записей, JSON до 64 KiB, весь
state до 1 MiB. Выбор, восстановление и сохранение профиля не отправляют команду;
принятая копия в worker journal остаётся неизменной. Close ожидает ACK сохранения всех
изменённых scopes. Старые schemaVersion 1 файлы читаются без преобразования; старый
desktop не понимает новые поля с профилями, поэтому rollback требует совместимого
выпуска или отдельного state файла, а не удаления пользовательских форм.

Необязательное `artifacts` содержит до 8 `{name,path}` относительно root проекта,
вне snapshots/.winbridge. После build exit 0 worker проверяет каждый regular file,
размер (до 256 MiB/file, 512 MiB/total), SHA-256 и стабильность identity/size/mtime.
Отсутствие, замена, symlink/junction или превышение квоты блокируют start. Хеширование
идёт в фоне и не держит journal lock. Cargo hard links допустимы для этого чтения;
защита writable journal/log по-прежнему отклоняет hard links. Каталог фиксирует путь,
bytes/hash, source commit, manifest hash, build task и время проверки. Это историческая
запись; будущая сборка может изменить файл по тому же пути. Хеш не доказывает семантику
компилятора или отсутствие вмешательства другого доверенного процесса.

stdout/stderr и log имеют фильтр строк без учёта регистра и паузу просмотра. Они
работают с последними 65 536 символами загруженного текста; capture worker и byte cursor
продолжают обновляться. Фильтр не обещает поиск по ещё не загруженным данным журнала.

## Готовность Netwatch и совместимость

Вместо stdout token профиль может указать `readiness.netwatch` с `instanceId` и
`runtimeDirectory`. Требуется отдельный `winbridge-*` instance и относительная runtime
directory вне snapshot. Probe сверяет IPC server PID и health PID с принадлежащим
worker процессом. Probe и stdout token взаимоисключающие. См. [изоляцию Netwatch](../validation/netwatch-validation.md).

WorkerInfo объявляет строковые workflowFeatures `artifacts`/`netwatch_health` и build
identity. Новые UI/controller проверяют их перед отправкой расширенного workflow:
старому worker не отправляется неизвестный payload. Старый stdout workflow сохраняет
wire fingerprint и dedup после upgrade. PeerIdentity/Hello не расширены: существующее
соединение остаётся совместимым с предыдущим выпуском. DB schema остаётся 3; новые
record fields читаются с defaults. После новых submissions rollback на старый worker
не поддерживается из-за strict submission contract.

Worker и desktop показывают commit, dirty marker и build ID отдельно от process
generation. В распакованном ZIP identity берётся из source manifest, его SHA-256 также
передаётся; build script сравнивает файлы с manifest и отмечает последующие изменения.
В Git checkout используются HEAD/status. Отсутствие metadata показывается явно.

Полный цикл fixture, process recovery и Netwatch проверяются автоматическими
сценариями; конкретные результаты записываются в Windows gate и handoff. Native
визуальные наблюдения новых панелей не подменяются Unix browser test.
