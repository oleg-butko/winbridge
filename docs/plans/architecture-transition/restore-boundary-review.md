# A2.4.2b — ревью границы восстановления

Дата: 2026-10-13, код `8877cf2`. Выбор [restore policy](restore-admission-options.md)
согласован: новая пара/журнал и новый request contract. Это уточнение обязательных условий перехода, не реализованный выход
из inspection. Windows-прогоны не выполнялись, рабочие config/runtime не менялись.

## Вывод

Для варианта с новой парой одного нового instance ID недостаточно: передача
старого логического запроса новой паре должна явно отклоняться. После восстановления
нужен контракт запроса с закреплённой journal incarnation и запрет legacy writes
в новом режиме. Нельзя добавлять свежую incarnation к старому запросу незаметно
для вызывающего агента. Read-only history и доказательства физических owners
остаются отдельными условиями.

Для варианта с независимым ledger legacy admission можно сохранить только при
проверенном полном охвате прежних relevant keys до restore. Новый пустой ledger
не доказывает, что неизвестный key никогда не исполнялся.

## Что установлено по текущему коду

| Элемент | Реализованный контракт | Следствие для cutover |
| --- | --- | --- |
| `TaskSubmission`, `LocalRequest` в `crates/protocol/src/lib.rs` | v1 содержит request/project/key/command, без ожидаемой journal incarnation; неизвестные поля запрещены | Старое содержимое запроса само по себе не доказывает намерение выполнить новую операцию в восстановленной паре |
| CLI `apps/worker/src/main.rs`, `apps/controller/src/main.rs` | Config загружается при каждом запуске; request JSON читается отдельно | Изменение config по прежнему пути может направить сохранённый request в новую пару |
| `apps/controller/src/mcp.rs` | Схемы и requests получаются из v1 DTO | Прозрачное добавление controller текущего fence к любому старому tool call не решает проблему; caller должен закреплять scope логической операции |
| WebSocket handshake | instance/node проверяются при Hello/Welcome | Защищает соединение с неправильной парой, но не payload старого запроса, отправленный уже через правильное новое соединение |
| `crates/transport/src/lib.rs` | Windows pipe использует SID + instance ID; stateDirectory в имя не входит. Local exchange не проверяет ожидаемую incarnation | Новая stateDirectory с прежним именем pipe не создаёт новую границу клиента |
| `apps/worker/src/store/ownership.rs` | Guards сверяют durable authority/attempt, созданные внутри worker | Это защита старых callbacks; она не определяет происхождение входящего v1 запроса, который может создать новый attempt |

Windows pipe names не различают регистр, тогда как `valid_id` допускает верхний
и нижний регистр. Поэтому `Example` → `example` нельзя считать новой Windows
IPC-областью. [Microsoft: Pipe Names](https://learn.microsoft.com/en-us/windows/win32/ipc/pipe-names).
Новый recovery instance ID должен быть свежим canonical ID, например UUID в
нижнем регистре, и не совпадать с прежним без учёта регистра. Менять правила
приёма существующих v1 config IDs задним числом для этого не требуется.

## Обязательная граница нового request contract

При выборе новой пары:

1. Caller закрепляет instance/journal incarnation при создании логической
   операции, сохраняет их вместе с idempotency key и не обновляет при retry.
   Новый явный запрос может использовать свежий scope после cutover; старый retry
   не превращается в него только из-за нового Info, session или config.
2. Worker сравнивает expected scope до dedup/admission и любых внешних эффектов.
   Отсутствующий/устаревший context в восстановленной паре даёт понятный отказ.
   Правило распространяется на import/workflow и административные mutations,
   а не только на запуск direct task.
3. Новый contract/envelope имеет отдельную версию. Frozen v1 JSON/enum values
   остаются прежними; обычный legacy runtime продолжает прежнее поведение.
   После recovery v1 reads доступны, v1 mutations не получают неявный доступ
   к новой области. Fallback после отказа нового контракта не запускает v1 команду.
4. CLI/MCP передают context вызывающего клиента. Desktop закрепляет его при
   явном действии пользователя; восстановление формы не отправляет запрос.
   Точная форма DTO и схема negotiation выбираются при реализации с tests обеих
   сторон; полноценный общий discovery/read model остаётся в A3.

При выборе ledger необходимо дополнительно доказать bootstrap completeness,
охват всех внешне значимых admission keys, порядок durable reserve/commit/effect
и recovery при частичной записи. В отсутствие этих свидетельств сохранение
прежнего pairing не означает разрешение legacy admission.

Оба варианта требуют physical evidence для прежних owners, включая операции,
записи которых могли исчезнуть. Проверка только строк восстановленной БД, смена
epoch/session или новый workspace при произвольных командах не являются общим
доказательством остановки. Нельзя разрешить работу только потому, что исторический
inventory пуст. Если необходимых свидетельств нет, процедура остаётся blocked.

## Проверки будущей реализации

- Lost ACK → restore с утратой key → retry старого logical request: ни одного
  нового процесса, даже после чтения нового config/Info и reconnect.
- Старая incarnation в новом соединении; отсутствующий context; неподдерживаемый
  новый contract с попыткой v1 fallback: отказ до любого эффекта.
- Windows IDs, различающиеся только регистром, и новая stateDirectory с тем же
  pipe ID: preparation не выдаёт это за независимую пару.
- Независимые guards для task/import/workflow/settings/cleanup; child steps
  наследуют принятую область родителя, stale callback остаётся запрещён.
- Lost records + live неперечисленный owner; partial cutover; crash между каждым
  durable шагом; legacy ledger без completeness: admission закрыт.
- Новый **явный** запрос после подтверждённого cutover проходит один раз;
  повтор с тем же context/key возвращает прежнюю операцию. Архив остаётся историей.

## Изменение порядка реализации

Ревью делает request fence обязательным для новой пары, а не условным улучшением
после первого восстановления. Минимальный versioned request boundary входит в
A2.4.2b до открытия admission; A3 затем объединит наблюдение и discovery.
Actor pilot/Run/Step остаются после этих границ. Решение о новой паре либо ledger
получено; persistent recovery mode реализуется после request fence.
