# Следующий координатор через исходный keeper

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
