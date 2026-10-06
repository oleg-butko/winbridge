# Workflow и приложения — этап 4

Реализован цикл на committed snapshot: import → build → start → stdout readiness → чтение UTF-8 лога. Это один durable запрос Windows worker. Controller/desktop пользуются теми же RPC; закрытие UI или потеря controller не отменяют принятую цепочку. Настоящие Windows проверки запускаются автоматически через подключённый worker; [сценарий](windows-workflow-validation.md).

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

Источник — [Rust DTO](../crates/protocol/src/workflow.rs); TypeScript генерируется. Профиль содержит name/projectId/snapshotId/manifestSha256, build/start CommandSource, общий timeoutMs, readiness `{stdoutContains, timeoutMs}` и относительный logPath. Корреляция: protocolVersion/requestId/idempotencyKey. Рецепты используют фиксированные argv; direct использует разрешённый проект и те же path/argument guards, что обычная команда. Executable запуска проверяется после успешной сборки, потому что до сборки его может не существовать.

Workflow/step IDs и принятый профиль записываются до ACK. Child import/task keys выводятся из workflow/step ID; crash между сохранением child и ссылки на него не дублирует внешнее действие. Manifest hash фиксируется в запросе и каждом child task. Перед build/start проверяется весь immutable snapshot. Ошибка prerequisite отмечает текущий шаг и пропускает последующие; старый exe в cache не запускается после build failure.

В schema 3 добавлены workflows/applications. Перед миграцией существующей schema 1/2 создаётся consistent SQLite backup через VACUUM INTO, один на миграцию. Ни journal, ни backup не попадают в shared. При рестарте внешний task с launch intent получает indeterminate; app отражает этот результат. Неизвестная команда автоматически не повторяется. Queued intent без внешнего эффекта может продолжить свою очередь.

## Владение и deadlines

Project reservation охватывает startup prerequisites, поэтому другой import/build/command не вклинивается между ними. После готовности и подключения лога приложение остаётся управляемым процессом; mutation lease освобождается. Lease хранит operation ID: завершение старого приложения не снимает lease более новой команды. Worker учитывает живые процессы отдельно от leases при shutdown.

Одновременно допускается одно живое приложение на проект; перед новой сборкой/запуском его нужно явно остановить. Это защищает общий Windows build cache от замены работающего exe. Уже принятая следующая цепочка ждёт; deadline включает ожидание в очереди. Границы: 128 активных workflow, 32 живых приложения, последние 10 записей в списках; старые записи доступны по ID. Общая retention журнала остаётся этапом 5.

Общий workflow timeout ограничивает startup, включая import/build/readiness/log. readiness timeout отсчитывается от старта процесса и включает подключение лога. Runtime приложения после успешного startup не заканчивается по этому timeout; оно работает до stop, собственного выхода или остановки worker. При провале готовности/лога worker отменяет управляемое дерево и ждёт подтверждения остановки. Stop intent не считается ни готовностью, ни завершением.

Успех startup и дальнейшая жизнь приложения показаны отдельно. PID подтверждает создание процесса, stdout token этого процесса подтверждает readiness, readable log подтверждает доступ к логу. Падение после готовности меняет ApplicationRecord, сохраняя исторический успешный WorkflowRecord. Отмена завершённого workflow не останавливает приложение; для него есть `stop-application`.

## Чтение лога

`logPath` находится внутри зарегистрированного проекта, вне snapshots/.winbridge. Worker отклоняет traversal, junction/symlink, special files и hard links. LogRead возвращает raw bytes; кодировка профиля — UTF-8. Page ≤64 KiB; byte offset не разрезает данные при последовательном чтении, UI декодирует incremental TextDecoder и держит последние 65 536 Unicode символов.

Cursor включает file identity, offset и hash первых ≤128 bytes. Смена identity означает rotation; уменьшение длины — truncation; изменённый prefix — rewrite. Reset явно показан, новый файл читается с начала. Перезапись области после prefix без смены identity/длины не обнаруживается: для гарантированного restart логгера использовать rotate/truncate. Файл приложения worker не изменяет. Runtime log retention принадлежит приложению; квота stdout/stderr task продолжает drain и отмечает потерю данных.

## Границы этого выпуска

Полный цикл проверяется на snapshot-fixture, включая настоящее Windows исполнение. Production Netwatch не запускался. В его текущем коде readiness не выводится в stdout, log file и named pipe принадлежат Netwatch; запуск второго экземпляра под тем же Windows пользователем конфликтует с его pipe. Для безопасной интеграции нужен отдельный профиль и согласованный probe/isolation. HTTP/IPC readiness, сохранённые именованные editable profiles, отдельный artifact catalog и фильтрация output ещё не реализованы. Этап 4 в roadmap остаётся открытым до этих критериев; принятый durable профиль и UI истории уже работают.
