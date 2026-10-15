# A4 — поэтапная очередь и общая нагрузка

2026-10-15. Зависимость: A3 actual installed gate завершён. Первый пакет A4.1
начат; весь A4 не объявляется реализованным заранее.

## A4.1: work slots и выбор готовых проектов

Opt-in `workScheduling.maxConcurrentWork` (1..32), absent — прежний scheduler.
Текущие Windows configs не менять. Производственные значения выбираются при
включении пользователем, с обычным ручным restart worker. Tests используют
свои явные значения 1/2. Admission новых задач ещё не ограничивается: это A4.2.

Work slot занимает task/import на стадии project mutation; workflow использует
слот исполняющегося child, родитель второй slot не занимает. После committed
application readiness mutation slot освобождается; живое приложение сохраняет
свою прежнюю application-lifetime lease. Отдельные binary/IPC/application capacity
будут A4.3. Uncertain lease считается занятой, включая unknown application:
terminal result/TTL/epoch сами по себе не освобождают ёмкость.

Проверка общего бюджета, LaunchIntent/attempt и получение project lease должны
быть в одной BEGIN IMMEDIATE transaction. Счётчик берётся из durable leases,
не active HashMap и не числа terminal/running JSON records. Ошибка/rollback
не оставляет половину reservation; restart восстанавливает старые владельцы
перед новым scheduling. Снижение лимита не отменяет уже принятые/живые работы.

Fair selection: между готовыми проектами сначала тот, у кого раньше был последний
durable dispatch. Источник порядка — retained execution_attempts rowid, не часы.
Проект без grant идёт первым, tie — стабильный project ID. Busy project пропускается;
неудачный claim не обновляет очередь fairness. Внутри прежнего сравнимого класса
сохраняется FIFO существующего журнала. Полноценная единая admission sequence и
arbitration разных классов — отдельный пакет, текущие timestamps ей не подменяются.

Политика immutable для Worker/его единственного Store owner; remote request не
передаёт свой limit. При переносе на actors Store executor остаётся общим.
Новый Store/actor не должен открывать независимый writer с другой политикой.
Дополнительная migration БД для первого work budget не нужна: leases/attempts
уже durable. Если позже writers станут независимыми, policy generation и resource
allocation ledger придётся фиксировать в БД до такого разделения.

Наблюдение: provider-bound optional readonly metadata в catalog, known interpreter
на Rust/TS, mode/limit/used/uncertain/queued и точные причины ожидания в operation
pages. Metadata не меняет frozen RPC 1/2 и не является permission. UI показывает
scope/freshness и объясняет, что это лимит работ, а не всех Windows процессов.
Unknown/future metadata не интерпретируется как действующая политика.

Проверки: 3 проекта, limit 1/2, slot claim race двух SQLite connections, rollback,
uncertain/ready app handoff, workflow child без двойного учёта, waiting history
старше recent 10, cancel/status во время занятости, restart и schema/wire baseline.
Native gate — изолированный config/state через установленный worker, без UI окон.
После готового кандидата нужен один installed update для actual workload gate.

## Следующие вертикальные пакеты

- A4.2: bounded root backlog и pause admission/dispatch, сохранение duplicate keys,
  исключение child из повторного учёта, reject до ACK и видимые typed blockers.
- A4.3: atomic full resource sets для overlapping roots/binary/cache/IPC/port/desktop
  и application lifetime; persistent policy/queue generation при необходимости.
- A4.4: общий обзор и native тест трёх независимых потребителей, throughput и
  latency cancel/status, overload/recovery и mixed-version gates. После него A4 закрывается.

## Ревью плана

Work capacity нельзя считать по active_jobs: приложение может освободить mutation
slot, а процесс жить; unknown owner может пережить worker. Root workflow не является
исполняющимся child. Fairness выбирает кандидатов, но никогда не выдаёт permission
и не обходит durable claim. Absent config сохраняет production semantics; новая
миграция, Kafka, distributed actor transport и изменение execution контрактов
для первого budget не нужны. Точные binary/IPC keys нельзя обещать до A4.3.


## Реализация кандидата

Policy загружается один раз в Worker/его Store. Task/import claim проверяет budget
до launch intent в той же serialized transaction; failed lease insert откатывает
attempt/status. Fair project selection использует max execution_attempts.rowid;
кандидаты фильтруются до клонирования, завершённая история не копируется повторно.
Slot summary — собственный read snapshot, не часть observation другой страницы.
Queue count включает queued task/import и queued workflow roots без child; после
перехода root в running ожидание считается по child. Это не новый backlog cap.

Protocol добавляет provider/scope-bound opaque workScheduling metadata и contract
`winbridge.work-scheduling` v1. Known readers нормализуют числовые metadata как JS,
но строгие config/command DTO не ослабляются. 23 общих fixtures проходят на Rust/TS.
RPC-2 schema остаётся прежней. Running config fingerprint включает активный budget,
а absent policy сохраняет прежний hash.

326 workspace tests, capacity race/rollback/restart, actual 3-project pool с 45
admissions для limit 1/2/legacy, native local IPC API и workflow readiness handoff
прошли на Debian. UI проверен отдельно с настоящим Unix worker (limit 1, queued
counts/cancel/freshness) и полным прежним browser scenario. Linux/Windows Clippy,
typecheck/build и отдельный desktop locked metadata resolution прошли.
Native Windows gate и actual installed новый pool пока не выполнены.

Первый native run `20261015-060547-6ba88f51`: policy/transaction/operation
группы прошли, pool test получил timeout первого local IPC Info под backlog.
Рабочий установленный worker остался здоров/idle. Тест теперь отдельно проверяет
готовность IPC до admissions и пишет времена admissions/initial slots/loaded
Info; fixture timeout 120 s отделён от 10 s IPC deadline. Это диагностический
повтор, не доказательство исправления scheduler; результаты ожидаются.
