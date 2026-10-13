# Техники консистентности для фундамента WinBridge

Дата: **2026-10-13**. Анализ кода `a42cd7e`; дополнение к
[Actor Model + FSM](../actor-model-fsm/README.md) и
[анализу консистентности](../consistency-extensibility/README.md).
Новые механизмы ниже — предложения, не внедрённые возможности.

## Рекомендация

**Все восемь техник совместимы с WinBridge и полезны, но в разных границах.**
Их следует объединить общими правилами исполнения, а не навешивать одинаковый
набор полей и таблиц на каждый actor message.

В фундамент сейчас стоит закрепить idempotency, идентичность операций,
области sequence, revision, fencing и reconciliation. Часть уже реализована;
потребуется сделать её единообразной и связать с typed FSM.
Transactional outbox и inbox стоит реализовывать вместе с первым действительно
долговечным межпроцессным обменом reports/events, который меняет состояние
получателя. Для нынешнего polling/read API отдельный broker не нужен.

Основная схема: **один Windows execution authority + локальная SQLite +
типизированные переходы + доставляемые сообщения с доказуемыми границами**.
Сериализация actor handlers помогает внутри процесса, а перечисленные техники
сохраняют смысл работы при повторах, задержках, обновлениях и сбоях.

Подробная [схема реализации, invariants, проверки и порядок внедрения](design.md).

## Сопоставление с нынешним кодом

| Техника | Что уже есть | Рекомендуемое расширение |
| --- | --- | --- |
| Idempotency Key | Task/import/workflow acceptance сохраняется с key и payload; unique ограничения, повтор возвращает прежнюю операцию. | Общие scope/fingerprint/retention правила; pin контракта, совместимость legacy retries. |
| Operation ID | Durable task/import/workflow/application IDs и связи шагов. | Typed OperationRef + Run/Step/Attempt связи; сохранить нынешние IDs. |
| Sequence Number | Task/workflow journal events, task output, общий attention sequence и cursor. | Явные scope/incarnation, обнаружение gaps; отдельный sequence общего обзора при необходимости. |
| Optimistic Concurrency | Settings сравнивает expected SHA-256 и generation, сериализует WinBridge editors. | Revision/CAS в общей точке изменения сущностей и policy; не требовать stale launch revision у completion. |
| Fencing Token / Epoch | UUID generation, instance file lock, project registry и проверка владельца при освобождении. | Durable ownership epoch и проверка на границе защищаемого ресурса. UUID generation сам этого не даёт. |
| Transactional Outbox | Task outcome и attention пишутся одной транзакцией; durable attention читается polling. | Общий event/delivery механизм с pending/ACK, когда требуется гарантированная доставка другому участнику. |
| Inbox / Deduplication | Admission ledger и attention uniqueness уже выполняют часть дедупликации. | Message ID + обработка/state/result одной транзакцией у получателя reports/commands. |
| Reconciliation | Running task → indeterminate; verified import восстанавливается по файлам; application сверяется с task. | Общие typed observations, evidence, resource recovery gate и правила безопасного продолжения. |

Проверенные источники кода:
[Store](../../../apps/worker/src/store.rs),
[scheduler/process owner](../../../apps/worker/src/lib.rs),
[imports](../../../apps/worker/src/imports.rs),
[workflow/application](../../../apps/worker/src/workflows.rs),
[attention](../../../apps/worker/src/attention.rs),
[settings](../../../apps/worker/src/settings.rs),
[protocol](../../../crates/protocol/src/lib.rs),
[companion cursor](../../../apps/desktop/src-tauri/src/companion.rs).

Это наличие отдельных механизмов, **не уже готовый общий consistency framework**.
Например, `Store::save` обновляет record по ID без expected revision, а
project leases находятся в runtime registry, не в таблице durable epochs.

## 1. Idempotency Key: намерение, а не одинаковые аргументы

Key связывает повтор с **тем же логическим запросом**. Две осознанно запрошенные
одинаковые сборки могут иметь разные keys. Одинаковый key с другим значимым payload
должен давать conflict; timeout ответа не разрешает автоматически заменить key.
Такое различение намерения и параметров описано в
[AWS Builders' Library](https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/).

Сейчас task fingerprint — сериализованный TaskSubmission без request ID.
Это защищает reconnect, но изменение сериализации при обновлении может нарушить
legacy replay. Новый canonical fingerprint должен иметь версию; старый сохраняется
для старых records. Method/contract changes не создают новый namespace того же
намерения: версия входит в payload, повтор с другой версией конфликтует.

Key гарантирует отсутствие второго принятия при сохранённом ledger и правильном
исполнителе. Он не делает внутренние действия произвольной команды идемпотентными,
не переживает произвольную потерю базы и не доказывает успешный исход процесса.
Keys/tombstones сохраняются дольше возможного replay; очистка stdout их не удаляет.

## 2. Operation ID: одна идентичность между процессами

Существующие IDs подходят как основа; переименовывать все записи не требуется.
Нужен typed reference с kind и связями Run → Step → Task/Import/Application.
Отдельный attempt обозначает конкретную разрешённую попытку внешнего действия.

Operation ID создаётся при durable admission и повторно возвращается по key.
RPC request ID может меняться между попытками доставки; ActorRef меняется при
restart. Ни один из них не заменяет долговечный Operation ID.
Message ID идентифицирует доставляемое сообщение, а не всю операцию.

Журнал, отчёт, UI, MCP и executor используют одну ссылку на исходную операцию,
source commit и контракт. Это даёт понятную трассу, но не является
аутентификацией отправителя или разрешением выполнить действие.

## 3. Sequence Number: порядок имеет область

Порядок задаётся отдельно для events операции, её output, attention и,
при необходимости, общего worker journal. Нельзя сравнивать task A sequence 12
с task B sequence 9 и делать вывод об очередности исполнения.
Timestamp не заменяет sequence; stdout/stderr не имеют достоверного общего
порядка возникновения только потому, что chunks сохранены последовательно.

На committed event sequence выделяется вместе с записью состояния.
Consumer хранит cursor для конкретного stream и journal incarnation.
Повторы не применяются второй раз; пропуск обнаруживается, восполняется из journal
либо через согласованный snapshot. Нельзя просто взять maximum received sequence
и забыть ещё не обработанное предыдущее сообщение.

Фильтрованная выборка может законно иметь gaps: отсутствие чисел в странице
attention с фильтром само по себе не означает потерю сообщения.
Критерий непрерывности определяется контрактом stream, а не общей арифметикой.

## 4. Optimistic Concurrency: защита от устаревшего решения

Сущность получает revision. Изменение «если по-прежнему версия N» выполняется
условным UPDATE; ноль изменённых строк означает conflict и повторное чтение.
Проверка версии предотвращает lost updates; аналогичный принцип применяется
в [Kubernetes API](https://kubernetes.io/docs/reference/using-api/api-concepts/#updates-to-existing-resources).

SQLite сериализует writes, но это не защищает от решения, рассчитанного на
record, прочитанный раньше вне текущей транзакции.
[SQLite isolation](https://www.sqlite.org/isolation.html).
У нынешнего worker многие пути защищены общим Store mutex; revision особенно
полезна при выделении actors с cached state и при изменении policy из UI/MCP.

Revision операции, policy и конфигурации — разные области. Приём команды не
должен зависеть от revision всех независимых projects или каждой строки output.
Cancel как повторяемое намерение можно применять к актуальной FSM без требования
сохранить revision старого poll.

Completion не обязан иметь revision момента запуска: progress/cancel законно
изменили запись за это время. Проверяются attempt/epoch и допустимость события,
затем reducer применяется к текущему state с локальным CAS.

## 5. Fencing Token / Epoch: устаревший владелец теряет право записи

Владелец ресурса получает возрастающий epoch при передаче владения.
Хранилище/adapter проверяет token непосредственно при защищённом действии;
старая команда не может освободить новый lease или переписать новый результат.
Token без проверки получателем бесполезен.
[Механизм fencing у Martin Kleppmann](https://martin.kleppmann.com/2016/02/08/how-to-do-distributed-locking.html).

Worker generation сейчас случайный UUID: он различает запуски и некоторые stale
запросы settings, но не задаёт порядок владельцев resource.
Предлагается token `journalIncarnation + resourceId + epoch + ownerOperation`,
выдаваемый одной Windows authority транзакционно. Reconnect сессии не отнимает
владение у продолжающего работать процесса.

**Fencing записи SQLite не запрещает старому exe писать в ту же папку.**
Произвольный child process не проверяет token WinBridge при каждом file write.
До повторного использования mutable workspace нужна подтверждённая остановка
старого дерева либо физически отдельный workspace. Истечение таймера lease
не является доказательством остановки. Это существенная граница архитектуры.

## 6. Transactional Outbox: не терять сообщение после commit

State change и исходящий event сохраняются в **одной локальной БД-транзакции**.
Relay отправляет event позже и отмечает durable ACK получателя.
Сбой после доставки до отметки ACK приводит к повтору того же Message ID.
Outbox устраняет окно «state записан, notification потерян», но допускает duplicates.
[AWS: transactional outbox](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/transactional-outbox.html).

Нынешние outcome+attention и polling уже решают часть этой задачи без push:
непрочитанное уведомление можно восстановить из journal.
Отдельная таблица outbox оправдана для actor report/event handoff с обещанием
доставки и состоянием каждого recipient, а не для каждого heartbeat/read/output byte.

В outbox сохраняются неизменяемые payload/contract pin и ID, чтобы обновлённый
serializer не изменил старое сообщение при replay. События больших reports могут
ссылаться на durable artifact с hash и правилами хранения.
**Outbox не делает создание OS-процесса атомарным с SQLite.**

## 7. Inbox / Deduplication: не повторять обработку доставки

Получатель записывает Message ID, изменения своего состояния и receipt
одной транзакцией. Повтор возвращает сохранённый receipt без второй мутации;
тот же ID с другим payload — conflict. ACK отправляется после commit.
Такой механизм совместной фиксации обработки и исходящих сообщений описан в
[NServiceBus Outbox](https://docs.particular.net/nservicebus/outbox/).

Inbox нужен у компонента, действительно обрабатывающего durable messages.
Прозрачному controller forwarder не обязательно создавать вторую бизнес-базу:
ACK просто передаётся от авторитетного получателя. Если controller сам принимает
отчёт долговечно и обещает его дальнейшую доставку, ему уже нужны local inbox/outbox.

Message dedup и business dedup различаются: один report могут повторить
с новым Message ID. Нужен также unique business slot
`operation/attempt/reportKind` с проверкой content digest.
DB inbox не делает запуск exe, звук или произвольный file write exactly-once.

## 8. Reconciliation: установить факты, а не повторить желаемое

Контроллеры сверяют desired и observed state; это общий control-loop принцип.
[Kubernetes controllers](https://kubernetes.io/docs/concepts/architecture/controller/).
Для WinBridge он требует более осторожной политики, чем «нет процесса → запустить».
Процесс мог выполнить действие и исчезнуть; exit code после потери handle
может быть неизвестен. PID может быть переиспользован.

Нужны typed observations и evidence: live managed process identity,
verified snapshot/hash, persisted report, незавершённый launch intent,
подтверждённая остановка дерева. Pure reducer решает, что факт доказывает.
Невозможность доказать итог остаётся `indeterminate`, без автоматического
повторного внешнего запуска.

Сейчас verified import действительно можно восстановить после restart,
а running task переводится в indeterminate. Эти различия следует сохранить:
политика recovery зависит от эффекта. Позднее evidence может дополнять историю
и отдельную reconciliation record, не переписывая старый terminal result незаметно.

## Какую консистентность это даст

| Граница | Гарантия предлагаемого решения |
| --- | --- |
| Один worker journal | Атомарное принятие и переходы, unique intent/message, проверка revision/epoch. |
| Межпроцессная доставка | At-least-once с durable replay и дедупликацией; сходимость при доступном recipient. |
| Осмысленная обработка в одной БД | Один committed effect на key/message/business slot при сохранённом ledger. |
| Windows команда вне БД | Managed ownership и отсутствие автоматического повтора неизвестного запуска; все side effects exactly-once не обещаются. |
| UI/MCP | Один сохранённый факт с revision и freshness; временное отставание объясняется. |

FSM, actors и все восемь техник вместе не дают глобальной транзакции между
Debian, Windows, файловой системой, пользователем и произвольным приложением.
Доступность доставки и полнота наблюдения остаются отдельными от safety свойствами.
Недоступный recipient не меняет успешную сборку в failed; это delivery pending.

## Проверки этой сессии

Выполнен отдельный SQLite prototype: **7 сценариев прошли** — conflict payload/
contract по тому же key, CAS двух connections, stale epoch, atomic state/event/outbox
rollback, committed replay после reopen, atomic inbox/state rollback и lost ACK
с дедупликацией после reopen получателя.

Это проверка предложенной локальной схемы, не внедрение в WinBridge.
В ней нет сети, actor runtime, process spawn и Windows исполнения.
Исследовательские файлы игнорируются Git:
`.winbridge/research/consistency-techniques/probe.py` и `proof.json`.
Повтор: `python3 .winbridge/research/consistency-techniques/probe.py`.

Проверки существующего runtime перечислены в [design.md](design.md#проверки).
Production code, protocol, зависимости и Windows runtime в этой задаче не меняются.
