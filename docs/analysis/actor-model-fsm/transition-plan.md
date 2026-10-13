# План постепенного перехода к Actor Model + FSM

Часть [отчёта](README.md), 2026-10-13.
Это предлагаемая последовательность работ; она не начата изменением runtime
и не заменяет закрытые этапы 0–7 новым обещанием готовности.

## Цель и первый сценарий

Первый результат: Debian coordinator одного тестового проекта запрашивает
Windows проверку по явному контракту; WinBridge принимает её долговечно,
собирает и запускает отдельный executor, сохраняет типизированный report
и показывает тот же результат в desktop/MCP.

Сначала executor выполняет **один прогон и завершается**. Его report содержит
source pins, список/количество tests, passed/failed/skipped и отдельный verdict.
Exit code и достоверность execution outcome сохраняются независимо.
Этот сценарий соответствует
[первой итерации Actor Model](../actor-model/integration.md),
но добавляет FSM, consumer tests и совместный gate.

В качестве первого consumer подходит snapshot-fixture/небольшой test executor.
После доказательства recovery и compatibility подключается тестовый Netwatch.
Не нужно одновременно переводить все modules или вводить универсальный DAG.
Стабильный contract adapter желательно отделить от изменяемого приложения:
обычный новый commit Netwatch проходит application tests, а не новую процедуру
согласования контракта между всеми компонентами.
Новый контракт называется и версионируется при согласовании спецификации;
имена test-report/v1/v2 в отчёте — примеры, не существующий API.

## Шаг 0. Зафиксировать legacy baseline и правила

**Работа.** Составить таблицу текущих task/workflow/application переходов
и внешних обещаний: acceptance, key, ACK, timeout, cancel, unknown outcome,
output/report. Сохранить golden fixtures настоящего v1 wire и старых persisted
records. Выделить поддерживаемые update paths; оформить ADR о разделении
internal FSM, wire contract и persistence version.

Requester первого расширения добавляет цель, ошибки и consumer examples
в versioned contract pack. Provider согласует semantics, особенно то,
что он не может гарантировать после crash или RPC timeout.
Baseline версии/кейсов затем служит основой для каждого следующего пакета.

**Приёмка.** Тесты описывают реальное нынешнее поведение, включая running
до появления PID, terminal outcome, duplicate original key, неоднозначный
результат после restart. Предложение не переименовывает legacy состояния.

**Контроль объёма.** Не документировать все возможные будущие transitions.
Зафиксировать текущие и те, которые нужны первому executor/report.

## Шаг 1. Выделить typed reducer без изменения wire API

**Работа.** В crates/core добавить типы State/Event/Effect/TransitionError
для обычной команды и чистую функцию переходов. В worker создать одну точку
применения решений к Store. Переносить по одному пути: acceptance/start,
completion, cancel, recovery. Определить ownership каждого перехода.

Internal Starting/Cancelling можно сначала хранить через совместимые поля/intent
и проекцию нынешнего record, либо добавить явно версионированное представление.
Для любого изменения SQLite отдельно проверяются migration и recovery.
Значения нового internal enum не попадают напрямую в старый TaskStatus.

**Приёмка.** Прежние API и idempotency fingerprints сохранены.
Тесты таблицы переходов и последовательностей покрывают cancel/exit race,
stale completion, failure до/после launch intent, journal failure и crash.
Accepted ACK появляется после commit; неизвестная команда не перезапускается.

**Откат.** Пока persistent format не менялся — вернуть прежнюю реализацию
handler. Если формат менялся — использовать только проверенный migration path;
«это всего лишь рефакторинг enum» не является доказательством отката.

## Шаг 2. Ввести discovery и раздельную готовность контрактов

**Работа.** Сохранить legacy Hello. Добавить optional catalog через Info,
проверить Rust reader и всю цепочку controller/MCP/local IPC/desktop.
В каталоге различать поддержку, готовность provider/executor, compatibility
receipt и enabled profile. Нужен registry версий статически подключённых handlers,
а не динамическая загрузка Rust-кода проекта.

Новый caller читает catalog, выбирает разрешённый контракт и pin; provider
перепроверяет его до admission. Отсутствующий catalog означает legacy набор.
Прежде чем добавить новый request variant, все нужные readers/forwarders
должны поддержать его.

**Приёмка.** Old/new матрица работает на настоящих codecs, а не только на
mock JSON. Неизвестные contract/version/digest отклоняются до effect.
Новый WorkerInfo не ломает старый reader; старый промежуточный controller
не создаёт ложное обещание поддержки. Schema генерация Rust → MCP/TS согласована.

**Откат.** Отключить catalog/candidate advertisement; legacy методы сохраняются.
Не повышать transport version только для каталога.

## Шаг 3. Перенести одного владельца состояния в локальный актор

**Работа.** Завернуть reducer и его persistence interface в ограниченный
state/run actor. Process owner и тяжёлый I/O остаются вне короткого handler.
Store/SQLite обслуживаются так, чтобы синхронный disk I/O не блокировал
все Tokio tasks; mailbox ограничен, status/cancel не вытесняются output flood.

Kameo используется локально за interface. Registry worker удерживает ActorRef.
Actor stop/restart восстанавливает технический объект и journal view;
он не повторяет побочный эффект. Result message имеет operation/attempt/generation.
Принятые IDs остаются долговечными и не заменяются runtime ActorRef.

**Приёмка.** Настоящий Store + actor проходят lost reply, mailbox saturation,
длительный executor, закрытие UI, сетевой reconnect и actor fault tests.
Под нагрузкой status/cancel доступны, прежний v1 сценарий не изменился.
Метрики задержек измеряются на этой реализации; ускорение заранее не обещается.

**Контроль объёма.** Пока не доказана надёжность одного владельца, остальные
actors не переводятся. Internal FSM можно сохранить и без Kameo, если pilot
не даёт достаточного выигрыша в ясности и lifecycle.

## Шаг 4. Реализовать первый candidate contract на обеих сторонах

**Работа.** Provider WinBridge добавляет typed command/report envelope,
Run/Report FSM, persistence/query и внешний executor adapter.
Consumer проекта реализует свой coordinator/codec и consumer tests.
Общий contract pack содержит fixtures, schema и переходы; provider добавляет
fault/policy tests. Добавляется небольшой автоматический contract harness.

На Windows WinBridge доставляет verified source, собирает native executor
и запускает его как managed child. На Debian проект запускает собственный
coordinator. Shared portable Rust logic возможна, но binaries и локальные
OS adapters разные. Не добавлять зависимость WinBridge на соседний ~/netwatch.

Report принимается с лимитами и привязкой к run/source/contract; неверный
или отсутствующий report даёт отдельную понятную причину.
В stdout/stderr остаётся доступен исходный diagnostic output.
UI и MCP используют один сохранённый typed view, а не раздельные выводы.

**Приёмка.** Consumer tests проходят против actual provider codec/handler;
provider invariants проходят независимо. Candidate включён только в test profile.
Production всё ещё использует legacy контракты.
Exit 0 + invalid/missing report не отображается как «тесты прошли».

**Откат.** Выключить новые admissions; принятый candidate run завершать
совместимой новой реализацией. Старые функции продолжают обслуживаться.

## Шаг 5. Прогнать новую пару и включить один профиль

**Работа.** Автоматизированный сценарий собирает и запускает обе новые стороны
в test mode с isolated instance IDs, journal, IPC и project roots.
Рабочий runtime не подменяется. Windows validation runtime имеет стабильный путь,
source/build caches локальные; mutable state разных версий не разделяется.

Совместный suite включает:

- Legacy сценарий и consumer/provider cases нового контракта.
- Partial update matrix и unsupported version/digest до acceptance.
- Lost ACK, original-key duplicate, caller restart и reconnect после acceptance.
- Worker/actor сбой, отсутствие автоматического повтора неизвестного launch.
- Cancel до старта и во время исполнения, process-tree timeout, delayed observation.
- Valid/invalid/missing report, paused admission и чтение статуса при нагрузке.
- UI/MCP совпадение результата и явную неизвестность/staleness.

Receipt связывает case outcomes с конкретными builds, contract/test digests
и environment. После успеха профиль разрешает contract для одного проекта.
Другие проекты остаются на v1; обновление одного project coordinator
не требует синхронного изменения всех клиентов.

**Приёмка.** Новый запрос использует выбранную версию; retry использует original
request. Выключение candidate запрещает новые runs и сохраняет обслуживание
принятых. Нет нового окна без предварительного объяснения, звука на каждый poll
или скрытого выполнения команды при восстановлении формы.

**Windows взаимодействие.** Перед прогоном сообщить, какой worker оставить,
нужен ли рабочий desktop и какие тестовые окна появятся.
По завершении сообщить, какие окна закрыть и какой worker оставить работать.
Ручные действия требуются только для неавтоматизированного поведения;
повторяемые execution/recovery проверки выполняет suite.

## Шаг 6. Расширять после первого доказанного маршрута

Общий scheduler с ресурсами/справедливостью для 2–3 проектов развивается поверх
тех же reducers и transaction rules. Каждая операция получает typed resource
claims, ожидание объясняется пользователю; чтение не требует execution slot.
См. [ресурсную модель](../consistency-extensibility/proposed-design.md).

Новый проект с обычным build/test подключается профилем и executor/report adapter,
без отдельной core FSM на каждое имя теста.
Новый вид OS effect, долговременный duplex child RPC либо новая гарантия recovery
требует отдельного расширения общего контракта и fault tests.
Это существенно дороже добавления нового case в существующий test runner.

Долгоживущие project actors, произвольный workflow graph и remote actor transport
не входят в первый переход. Они рассматриваются после конкретной потребности
и проходят тот же consumer → provider → paired gate.

## Какие части кода затронутся

| Место | Изменения | Что остаётся за прежней границей |
| --- | --- | --- |
| crates/protocol | Версионированные public DTO, catalog, typed errors, schemas/TS fixtures. | Kameo ActorRef и внутренние mutable states не становятся public DTO. |
| crates/core | Pure reducers, guards, переходы и interfaces effects/resources. | Никаких Tauri, HGFS путей или прямого управления Windows-процессами. |
| apps/worker | Apply transaction, scheduler ownership, actor registry, Run/Report persistence. | Windows execution authority, accepted-before-ACK и recovery сохраняются. |
| apps/controller | Discovery всего маршрута, routing guards, MCP schemas/query. | Не становится владельцем живых Windows processes. |
| crates/transport | Совместимый framing/IPC, ограничения и доставляемые сообщения. | Не вводится второй P2P stack для передачи project reports. |
| crates/platform | Process/file observations и OS adapters, если их требует новый effect. | Не принимает решение, прошли ли предметные тесты проекта. |
| modules | Предметные handlers import/build/test/report по interfaces. | Не пишут произвольное состояние в journal в обход transition authority. |
| ui / apps/desktop | Проекции состояния, contract readiness, commands/reports и attention. | Не объявляют success оптимистически до подтверждения worker. |
| Проект-consumer | Coordinator/executor, test pack, профиль интеграции. | Не меняет implementation WinBridge ради каждого теста приложения. |

## Стоимость и цена будущего расширения

Это ориентиры инженерных дней одного разработчика с tests/docs, **не обещание
срока**. Шаги частично пересекаются с оценкой прошлого Actor Model отчёта;
их нельзя складывать как независимые сметы.

| Пакет | Ориентир | Главная неопределённость |
| --- | --- | --- |
| Baseline и согласование первого контракта | 2–4 дня | Сколько нынешних неявных переходов обнаружится. |
| Typed task reducer и persistence integration | 3–6 дней | Crash windows, legacy fingerprint и migrations. |
| Совместимый catalog/selection через полный маршрут | 4–8 дней | Поведение старых intermediate readers и MCP/TS. |
| Один local state actor с Store | 4–8 дней | Lifecycle, ownership, backpressure; пересекается с reducer работой. |
| Contract pack и общий test harness | 4–8 дней | Автоматизация mixed versions и точность fixtures. |
| One-shot executor/report с Run FSM, UI/MCP | 8–15 дней | Размер нового public contract и recovery; тот же вертикальный пакет, что в прошлом отчёте. |
| Native paired gate и включение первого профиля | 3–6 дней | Изоляция Windows, GUI automation и реальные failure cases. |

Сначала следует сделать шаги 0–1 и уточнить смету: иначе стоимость каталогов
и actors будет оцениваться при ещё неизвестной семантике.
FSM не даёт немедленного выигрыша в скорости сборки, но снижает цену проверки
новых веток, расследования отказов и совместимости.

| Будущее изменение | Ожидаемая цена при предложенном фундаменте |
| --- | --- |
| Новый тест или recipe существующего проекта | Профиль/runner/tests; без protocol version change при прежних semantics. |
| Новый проект с поддерживаемым executor contract | Project config, OS isolation, consumer pack и integration gate. |
| Новые diagnostics/metadata | Обычно локальный reducer/view change плюс compatibility tests additive fields. |
| Новая публичная команда или несовместимый результат | Новый contract version/adapter, consumer/provider tests и paired receipt. |
| Новый эффект: interactive app, duplex RPC, иные recovery гарантии | Общий core/platform lifecycle и новые fault tests; цена остаётся высокой. |

Фундамент не станет абсолютно неизменяемым. Его задача — сделать место и цену
изменения явными и не заставлять переписывать весь pipeline для каждого проекта.
Не следует строить декларативный универсальный движок FSM со скриптовыми actions:
typed Rust reducers и небольшой registry пока дешевле и проще проверяются.

## Критерий завершения первого перехода

Один проект выполняет реальную Windows проверку через managed executor.
Машины переходов тестируются отдельно, reports сохраняются и читаются одинаково
агентом и пользователем. Поддерживаемые old/new сочетания подтверждены suite;
новая функция включается по receipt и профилю.

Закрытие UI, reconnect, timeout ответа и restart actor не создают повторный
внешний запуск. Принятый run сохраняет contract pin и понятную историю.
До следующего конкретного расширения сохраняются v1 adapters и прежние
доказанные guarantees, а не только одинаковые названия статусов.
