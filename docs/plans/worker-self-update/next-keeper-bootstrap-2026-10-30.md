# Следующий координатор через исходный keeper

**Native exclusion gate passed:** source5f543f0,
run20261030-112100-b8658b84/taskb446d8d3-06a8-4023-9912-d079f75e81d1.
3 exact tests passed: production pin membership/exclusion/exit и два OS helper cases.
74 ready files twice verified,36 OS-proof negative audits. Report
ba8de27d5e4d3b50b0c444866a130a8eae6e35d6c2a423ae1fd9bf44de1fac95;
two-generation proof90d129db6a576bffa41c44a78b50988f76eb1706ba727838384bf54108fd47a6,
keeper-loss proofc2e5487b1992ce35d4038e58b85c6da2b432e761b1e2e95cd584d1e0ed6c5378.
Windows14,119,886,848→14,822,199,296bytes; installed worker unchanged.
Helpers завершены, worker оставить, desktop не нужен/окон нет. Это OS boundary
proof с реальными helpers, не два полноценных updater workflows; durable bootstrap
и mixed-version протокол ещё впереди. Не replay ни failed, ни passed task.

2026-10-30/.204. Продолжение [keeper retirement](keeper-runtime-2026-10-30.md).
Это план и первый platform primitive, не реализованный launch protocol.
Installed worker source48/gen946 не меняется; public install off.

## Текущая граница

Main30/19/23 и actual23→24 copy migration прошли. Main31/19/24 passed:
run20261030-103611-811af539/taskb1cfa7b0-57b5-457e-8296-56c7629a2496,
producerd5a7bab→target830ccb9. Наблюдать тот же task, не replay.
Это отдельное доказательство закрытия writer/освобождения authority при живом Job;
73 files twice/19 keeper negative audits. Следующее поколение ещё не проверено.

Production `Worker::launch_supervisor` уже отказывает worker в external Job.
`InfrastructureChild::spawn` также проверяет собственный execution domain.
Activated worker находится в activation Job keeper, отличной от task execution
domain; проверки только последней недостаточно для делегированного запуска.
Существующий cold-worker отказ сохраняется. Нельзя добавлять breakaway или ослаблять
KILL_ON_JOB_CLOSE/limits. OS experiment двух поколений — вспомогательный опыт,
он не заменяет durable admission и настоящих updater actors.

## Первый реализованный примитив

`PinnedProcess::require_outside_job` query-ит конкретную существующую Job через
тот же exact retained process handle, проверяя liveness до и после наблюдения.
Положительная и отрицательная проверки используют общий query primitive. Missing
Job, access denied, invalid name, exited process и unsupported platform дают ошибку,
не положительное outside evidence. Ни Job, ни процесс не создаются; query handle
немедленно закрывается, limits и полномочия не меняются. Результат не является
разрешением запуска и не сериализуется как capability.

Existing Windows exact-pin test усилен: contained child отвергается как outside,
parent принимается outside только live existing Job; missing/invalid Job и exited
pin отвергаются. OS spike дополнительно проверяет actual parent до CreateProcess
и actual next child после него вне original Job и old task domain. Независимые
низкоуровневые member observations остаются вторым oracle.

Linux infrastructure3 tests/1 ignored и Linux/MSVC platform all-target Clippy passed.
Windows execution нового primitive пока не выполнялся. Новый explicit gate:
`run-update-bootstrap-exclusion-native.py`, reader
`verify-update-bootstrap-exclusion-native.py`. Он выбирает exact production-pin test
и два OS cases, имеет отдельные namespace/report kind/idempotency key. Старый
lifetime-spike default по-прежнему требует ровно2 commands. Selector3 tests и прежние
8 OS reader tests passed; synthetic tests не являются native evidence.
Запускать gate после завершения Main31/освобождения cache. Installed worker оставить,
desktop не нужен;3 локальных tests без новых окон и сети, hashed EXE только local OS.

## Следующий вертикальный сценарий

1. Разделить logical current worker и physical spawner keeper, сохранив старые
   SupervisorSpec/handshake semantics. Новая durable delegation имеет собственный
   versioned контракт/ledger; cold rows никогда не создают live owner или retry.
2. Bootstrap принимает только исходный actual worker peer (PID/birth/host/session,
   membership, loaded generation/context/Active binding), не JSON PID. Фабрика
   запроса принадлежит live worker с freshly admitted update; project command не
   может отправить произвольный вне-Job spawn. Используется pinned trusted updater
   установленного runtime, фиксированные args/config/working directory; нет general
   path/argv API. First round проверяет два поколения с тем же SDK.
3. Старый keeper после writer close не открывает SQLite и не принимает новую
   updater authority. Minimal bootstrap version независима от будущего journal
   format. Одна durable admission и один in-memory creation attempt. Unknown create
   или потеря ответа не дают повторного spawn; readiness выясняется у того же actor.
   Независимость child устанавливается до отправки одноразового start/handoff.
4. До CreateProcess actual keeper обязан быть вне original activation Job и old
   task domain; actual child проверяется там же до acceptance. Query errors остаются
   unknown. Новый координатор получает следующий owner epoch через настоящий OS-lock,
   а current worker подтверждает handoff как logical parent.
5. Одновременно максимум один coordinating actor и два keeper. Это ограничение
   поколений процессов, не числа Debian-контроллеров. В ledger нужны immutable
   связи поколений; live parent keeper проверяется actual pinned handle. Занятый
   предыдущий keeper запрещает создание третьего, без silent adoption/cold restart.
6. Старый keeper выходит только после known ready следующего actor, actual old
   worker exit, positive empty того же original Job и завершения readers. Exit root
   не заменяет Job.empty. Keeper loss закрывает именно свой original Job; новая
   цепочка вне него должна выжить. Повторные команды только наблюдают тот же attempt.

## Смена версии updater до U4

Same-SDK round не доказывает обновление на будущую SQLite schema. До публичного
включения нужна явная стратегия: fresh updater image/descriptor negotiation и
стабильный versioned worker bridge, который не заставляет старый worker/keeper
интерпретировать будущую внутреннюю schema. Нельзя убрать exact-schema проверки
механически, объявить future SDK совместимым по одному номеру или переложить это
на JSON receipt. Новая schema требует consistent retained-copy/backup/old refusal
proof. Native mixed-version gate обязателен отдельно от двух same-SDK поколений.
После этого остаются U2 fault matrix, U3 UI/attention и U4 actual installed update.

## Первый exclusion run: исправлен setup exact-pin test

Source09328f6, task473a983b-bb4c-4985-8dae-05f5a2b05aa0,
run20261030-111128-d13660f4 failed на первом test после0.13s; два OS cases не запускались.
2 ready files twice verified, report12601e60cdfbf13faafdbaea84467858fdf6ed1a4eb6814d22853711d921244c.
`ManagedChild::prepare(None)` создаёт unnamed Job и намеренно не выдаёт recoverable
ProcessIdentity. Existing test затем unwrap-ил её перед membership assertions.
Setup исправлен на уникальный named Job. Сам query primitive и строгие проверки
не менялись, MSVC platform all-target Clippy passed. Это не положительный native
proof; требуется новый isolated run/key. Старый task не replay.
Installed worker unchanged; no windows, Windows directories не удалялись.

## Выбранное направление fresh updater и устойчивого bridge

Первый same-SDK delegation round проверяет lifetime, но не решает будущую schema.
Рекомендуется запускать свежий updater, построенный из нового pinned snapshot,
до изменений пользовательского source/runtime. Старый worker может выполнить этот
stateless build как обычную управляемую задачу, проверить SHA/manifest и frozen
bootstrap descriptor v1 в своём execution domain. Только после успешного build
создаются durable update/delegation intent и independent coordinating actor.
Ошибочный build оставляет работающую установку без изменений.

Old keeper знает только actual current-worker peer, исходный Job и стабильный
bootstrap envelope: fixed helper location, image/config identities, hashes,
logical parent/physical spawner, correlation/one attempt. Он не открывает новый
journal и не проверяет future schema через старую библиотеку. Fresh updater сам
выполняет проверенную миграцию и получает следующий owner; stable ready/handoff и
worker-control bridge не должны заставлять old worker читать будущую SQLite.
Глобальный epoch после migration может увеличиться дважды (migration invalidation
и new-owner acquisition); нельзя подменять это старым parentEpoch+1 либо переписывать
исторические owner fields. Ready нужно проверять через versioned live bridge,
связанный с actual child, preaccepted root и migration result.

Нужен отдельный фиксированный bootstrap query/handshake, а не расширение frozen
SupervisorSpec или снятие exact-schema guards повсюду. Public updater capability
пока закрыта. First same-SDK round и native mixed-version migration/control round —
разные обязательные gates. Старый installed source48 ещё не реализует этот bridge;
его обновление понадобится перед реальным U4, после изолированных проверок.

Очередность source работы: immutable delegation binding/ledger и known-commit
permit → actual worker-to-keeper peer/one creation → stable child handoff/ownership
→ known-empty retirement/max2 → fresh-updater/worker bridge и mixed-version gate
→ auto workflow/U2 fault matrix/U3 UI/actual installed U4. Только выполненные
сценарии помечать готовыми; bootstrap effect сейчас не подключён.
