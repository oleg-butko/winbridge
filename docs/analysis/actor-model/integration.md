# Границы акторов, протокол проекта и переносимость

Часть [оценки Actor Model](README.md). Ниже предлагаемые контракты, а не готовые API.

## Главный актор: точка доступа, а не исполнитель всех команд

Логический главный актор WinBridge принимает обращения, проверяет capabilities
и направляет их владельцу соответствующей операции. Controller остаётся входом
на Debian; Windows worker остаётся authority исполнения. Не нужно искусственно
объявлять один из этих процессов владельцем всей памяти на двух ОС.

Внутри worker разумно выделить несколько ролей:

| Роль | Чем владеет | Что не выполнять внутри долгого handler |
| --- | --- | --- |
| Root supervisor / registry | Жизненный цикл внутренних actors и сильные ссылки на них | Сборку, тесты и ожидание завершения процесса. |
| State authority / scheduler | Короткие переходы, атомарный admission ресурсов и journal | Полное хеширование дерева и блокирующий process wait. |
| Project/run coordinator | Порядок шагов конкретного проекта/прогона | Независимую вторую запись task state в обход authority. |
| Process owner | Handle управляемого дерева и команды отмены | Монопольное ожидание exit, мешающее обработать cancel. |
| Report ingestion | Проверку формата, размеров, привязки и сохранения отчётов | Произвольный код внешнего проекта в процессе WinBridge. |
| Connection/query adapters | RPC correlation, свежесть связи и выдачу read model | Самостоятельный повтор команды при неизвестном принятии. |

Это роли, а не обязательные шесть новых actors с первого дня. Начать достаточно
с state owner и одного coordinator; существующие process/I/O adapters можно
оставить обычными threads/tasks. Actor Model не требует превращать каждый module,
каждую строку output или UI кнопку в отдельного актора.

Native UI сохраняет привязку к GUI thread: команды акторов проходят через
Tauri/native dispatch. Actor Model не разрешает перенести произвольные window
handles в Tokio task и не заменяет существующие ACK deadlines/watchdog.

Долгая работа запускается отдельно. Handler сохраняет намерение, возвращает ID,
а позднее обрабатывает `ProcessStarted`, `OutputAvailable`, `ProcessExited`.
Эти observations проверяются по operation ID и incarnation/generation: поздний
ответ старого actor не должен перезаписать новый итог.

## Как сохранить консистентность

Последовательность mailbox защищает **состояние одного актора**, но не даёт
общей транзакции между project actor, scheduler, БД и процессом.
Порядок одновременных отправителей также не задаёт справедливую очередь проектов.

Предлагается оставить одно место атомарной записи admission/переходов:
state authority, владеющий SQLite, проверяет expected revision и сохраняет
операцию с ресурсами до ACK. Coordinator получает подтверждённую версию и может
держать производную копию для планирования. Общий обзор читается из journal/read
model с revision; не собирается как якобы атомарный снимок нескольких mailbox.

Для синхронного SQLite использовать выделенный поток или существующий Store
adapter. Отдельные actors не должны открывать независимые competing writers и
каждый хранить «свою истинную очередь». Это бы усложнило нынешнюю транзакционную
границу вместо улучшения.

Ключи принятия, task IDs и commit/snapshot pins остаются долговечными.
Kameo ActorRef/ActorId — runtime-адреса; они не заменяют run ID или task ID в journal.
После actor restart состояние восстанавливается из БД, а не из `Default`.
Root registry удерживает нужные actors независимо от клиентских соединений:
потеря последней ссылки MCP/desktop не должна остановить принятую работу.

Не допускать циклических ожиданий `A.ask(B)` → `B.ask(A)` в handlers. Например,
Store не должен ждать callback от project actor, который уже ждёт Store.
Внутренняя очередь, долговечная queue и process capacity — отдельные лимиты.
Медленный output не должен занимать всю полосу control/status сообщений.

## Внешний актор — отдельный project executor

WinBridge импортирует исходники из commit, собирает executor в Windows execution
workspace и запускает как обычный управляемый процесс. Так уже проверенные
Job Object, tree cancel и output capture остаются в силе.

На Debian проект запускает coordinator, который использует controller/connector
API. Запуск Debian координатора не должен требовать доступа к исходникам WinBridge.
Общий протокол можно оформить небольшой библиотекой DTO/codec, когда появится
первый потребитель; actor framework types в её публичный API не входят.

Предпочтительная начальная топология:

```text
Project coordinator (Debian)
  → WinBridge controller → ws:// → WinBridge worker
    → local child protocol → Project executor (Windows)
    ← validated reports    ←
  ← durable report/status ←
```

Путь `winbridge-worker.exe` остаётся постоянным сетевым endpoint. Project executor
не обязан открывать новое соединение с Debian: локальный pipe/stdio/file report
передаётся существующим worker. Поэтому обновление child binary в другом поколении
не должно создавать новую потребность разрешать его сетевой путь в firewall.
Сетевые операции тестируемого приложения — отдельное свойство самого теста.

Сейчас у managed command `stdin = null`, а stdout/stderr — обычные byte streams.
Полноценное двустороннее actor messaging потребует нового child transport adapter,
не одной зависимости на Kameo.
[Текущий process adapter](../../../crates/platform/src/lib.rs).

Для первого one-shot executor достаточно job manifest в отдельном run workspace
и ограниченного результата в отдельном report channel/файле. Worker читает его
после достоверного завершения, проверяет и копирует в своё report storage.
Если нужен живой progress, добавить выделенный pipe или framed channel.
Не смешивать произвольный stdout программы с управляющим JSON без framing,
лимитов и явного разделения логов/событий.

## Контракт запуска и отчёта

Версионировать общий envelope отдельно от предметной схемы проекта:

| Сообщение / данные | Необходимый смысл |
| --- | --- |
| Bootstrap/job | Protocol version, project/run/step IDs, incarnation, command ID, source pins, параметры, deadline и допустимые capabilities. |
| `Started` / `Ready` | Executor загрузил нужную версию, понял job и объявил реальные capabilities; PID сверяется с процессом worker. |
| `Progress` / `StageResult` | Sequence, конкретный этап, наблюдаемый результат, ссылка на output; не самовольное изменение состояния родителя. |
| `FinalReport` | Verdict, test counts, версии схем, завершённость отчёта и ссылки на проверяемые artifacts. |
| `ReportAccepted` | Worker подтвердил сохранение отчёта; доставка клиенту и подтверждение показа пользователем — отдельные факты. |
| Status/report query | Чтение уже сохранённых данных без повторного запуска executor. |

Отчёт о старте и финальный отчёт — разные вещи. Запуск executor не подтверждает
успех теста, а `ready` тестируемого приложения не означает конец его жизни.
Команды и результаты должны иметь человекочитаемые названия и параметры;
не превращать UI в список непрозрачных encoded message payload.

Worker привязывает канал к **известному запущенному процессу и run**, проверяет
допустимую версию/sequence/размер. Он не доверяет произвольным project/task IDs,
которые исполнитель написал в payload. Artifact paths проверяются относительно
разрешённого run output root, без traversal/junctions; байты копируются в
контролируемое хранилище с hash/size, чтобы последующие изменения исходного файла
не меняли исторический отчёт.

Idempotency отчёта — например `(runId, incarnation, reportSequence)` плюс hash:
повтор тех же байтов подтверждается, конфликтующий payload отвергается.
Повтор доставки **данных** безопасно разрешать; повтор исполнения с неизвестным
эффектом по-прежнему запрещён. Progress/history и final record сохраняются
с соблюдением выбранной атомарной границы journal.

Полезны независимые поля `executionOutcome`, `reportedVerdict`, `reportState`.
Если процесс завершился с exit 0, но отчёт отсутствует, исход процесса известен,
а результат тестов неполон. Если actor написал «всё прошло», но worker подтвердил
принудительное завершение, это противоречие, а не общий успех. После crash worker
может иметь сохранённый отчёт и неизвестный последующий исход процесса:
показывать оба факта, не подменять один другим.

UI/MCP читают общий сохранённый отчёт. Popup/звук относится к началу run и важным
итогам; повтор получения report или polling не создаёт новое событие внимания.
Существующая история дополняется связями run/report, а не заменяется ещё одним
независимым хранилищем.

## Можно ли сделать один executor для Windows и Debian

**Одну общую codebase — да; один native executable для обеих ОС — нет.**
Rust actor logic, DTO, очереди и разбор отчётов можно использовать повторно.
Для каждой ОС собирается свой binary. Роли могут задаваться через
`--role coordinator|executor` или отдельные binaries одного workspace.

| Часть | Переносимость |
| --- | --- |
| Actor state machine, messages, report schemas | Обычно общая Rust библиотека. |
| Coordinator и executor orchestration | Общие части плюс разные роли и capabilities. |
| Filesystem/process/network abstraction | Общий trait, разные OS adapters и проверки путей. |
| Windows service, registry, UI automation, named pipe semantics | Windows implementation; на Debian явный `unsupported`, а не ложный успех. |
| Зависимости тестируемого приложения и toolchain | Проверять отдельно; portable actor library не делает их portable. |

Можно иметь `ProjectBackend` с Windows/Linux реализациями. Но не нужно создавать
Linux имитацию Windows функций только ради одинакового binary API. Если executor
нужен исключительно для Windows UI, Windows-only binary и общий DTO crate —
более простой и честный результат.

Actor Model изолирует владение памятью при корректном проектировании. Отдельный
процесс также имеет своё адресное пространство. Ни то ни другое не является
изоляцией файлов/реестра/сети: команды по-прежнему выполняются с правами пользователя.
Нужно соблюдать project policy, отдельные test instances, resource claims и
проверенные workspace, как в [анализе Netwatch](../netwatch-agent-pipeline/README.md).

## Расширение протокола без постоянного изменения WinBridge

Разделить стабильные операции WinBridge — принять run, запустить executor,
прочитать статус/отчёт, отменить — и versioned domain commands проекта,
например `netwatch.test_suite` или `netwatch.collect_diagnostics`.

Проект объявляет schema/version/capabilities и параметры своих команд.
WinBridge проверяет разрешённый набор, отображение и ресурсную политику.
Новая команда в имеющемся envelope не обязательно требует нового Rust DTO
WinBridge, но новый системный эффект требует его поддержки и видимой семантики.
Один непрозрачный `executeAnything(json)` не решит задачу прозрачности и контроля.

Большие отчёты и attachments передаются отдельно или читаются страницами:
нынешний transport ограничивает frame 1 MiB и имеет RPC timeout 8 s.
Submit/actor command возвращает долговечный ID, а не держит RPC до конца тестов.
Новый child/actor protocol включается через capability guard; старому worker
не отправляется неизвестный вариант строгого DTO. Предметные capabilities
проекта следует версионировать в его envelope, не раздувая системный handshake
одной capability на каждый тест или команду.
[Текущий transport](../../../crates/transport/src/websocket.rs).

Данные actor не становятся инструкциями для ИИ агента или исполняемым HTML для UI.
Сначала достаточно стандартного summary/test renderer и raw JSON просмотра;
предметные UI renderers добавляются отдельно. Обновление схемы проекта не должно
требовать одинаковой версии actor framework у всех участников.

## Безопасная миграция

1. Добавить report contract и один тестовый one-shot executor поверх текущих tasks.
   Проверить весь round trip с видимым UI/MCP результатом.
2. Вынести один короткий state/coordinator actor, сохраняя старые RPC и task keys.
   SQLite остаётся источником восстановления; изменения fingerprints версионируются.
3. Расширить scheduler/resources и типизированные pipeline steps по реальным кейсам.
4. Добавить duplex channel только после потребности в нескольких командах
   одному живому executor; определить сохранение каждой принятой команды,
   heartbeat, остановку и reconciliation без автоматического повторения.

Supervision рестартует технический actor, а не автоматически повторяет run.
Восстановление process owner сначала сверяет journal и OS ownership. Нельзя
в `on_start` безусловно исполнять последнее `StartJob`. При критическом отказе
journal сохраняется нынешний admission stop, даже если supervisor умеет restart.
