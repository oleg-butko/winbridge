# Следующий блок U1: drain и quiesce

2026-10-20. План после сквозного health gate; блок ещё не реализован.
Public update, shutdown и replacement пока отключены. Установленный worker
для этой разработки менять не требуется: native проверки проходят на отдельной
копии. Рабочий desktop для них не нужен.

Подготовительная реализация: внутренний supervisor v11 / worker control v4
добавляет `ObserveWorkerReceipt` / `ObserveReceipt`. Это readonly lookup старой
immutable command receipt; drain driver, его updater schema и публичный update
этим не реализуются. Текущий Windows health gate использует предыдущий архив
без v11/v4; native проверка нового контракта остаётся отдельной обязательной задачей.

Ответ содержит точный исходный command, optional receipt и время наблюдения.
Клиент проверяет contract, update/context/generation, command payload, resulting
phase, revision и owner/epoch. Отсутствие receipt означает только результат чтения
в данный момент: команда в другом callback ещё может завершиться. Автоматический
повтор внешней команды из этого ответа не следует. Legacy v1–v3 и supervisor v1
не получают скрытый fallback; v4 не принимает мутацию Advance.

Lookup читает сохранённый spec и квитанцию, не сканирует нынешние project/source
directories, не меняет lease, slots или startup guard. Поэтому receipt SourceHeld
остаётся читаемой после Release и после переноса старой папки исходников. Это
историческое доказательство; оно не даёт новое разрешение на запись в файлы.

Внутренний supervisor v12 / worker control v5 подготавливает наблюдение причин
ожидания: matching lease, число принятых операций и logical owners, conservative
in-flight counters, revision/mode пользовательской очереди. Lease/DB counters и
queue mode читаются одним SQLite snapshot; counters registry — отдельное короткое
наблюдение под process-registry lock. Чтение не меняет паузу или lease и не начинает
drain. В DTO намеренно нет physical-owner counts или quiescence approval: нули
не заменяют authenticated ConfirmQuiesced receipt после dispatch closure/OS scan.
Legacy versions не получают эту форму через fallback. Journal schema остаётся10.
Local real-IPC/paused-queue и protocol regressions прошли; native v12/v5 ещё
требуется. Текущий combined health run2feb/5a не содержит этот более новый контракт.
При ошибке чтения показывать unknown/stale с прежним timestamp, а не новые нули.
Фоновая automatic output cleanup уже исключена активной lease под Store lock;
эта гарантия нужна, чтобы после завершения принятой очереди не появлялись новые
maintenance effects перед CloseDispatch.

## Что уже есть

Worker schema 15 хранит SourceHeld → Draining → Quiescing → Quiesced и отдельный
Released. Trusted control v1 поддерживает BeginDrain/CloseDispatch/ConfirmQuiesced,
проверяет process/owner/epoch/config и требует updater parent Draining. Worker
сам вычисляет blockers и делает physical scan; число активных задач от агента
не является доказательством quiescence. User pause хранится отдельно.

Updater root FSM уже имеет CandidateVerified → Draining → Quiesced. Не хватает
долговечных доказательств координации обеих БД и actor orchestration. Worker lease
и updater root находятся в разных SQLite: общей транзакции между ними нет.

## Последовательность реализации

1. Допустить операцию только для свежего health receipt той же installation,
   source/artifacts, supervisor launch и owner/epoch. Проверить текущий config,
   runtime identity и завершение compiler/health процессов. Исторический успех
   после смены владельца не создаёт новую authority.
2. Сохранить immutable drain spec/первый worker command intent и root BeginDrain
   в одной updater transaction до ACK. Вернуть быстрый consistent status;
   один blocking driver продолжает работу под той же физической authority.
3. Каждый command имеет стабильный ID, точный immutable WorkerUpdateSpec и
   expected worker revision. Намерение сохраняется до RPC, typed receipt — до
   следующего шага. Применить BeginDrain, дождаться завершения принятых roots и
   workflow children, затем CloseDispatch и ConfirmQuiesced. Не отменять работу
   ради прохождения gate и не снимать user pause.
4. Получить подтверждение именно сохранённой команды, а не только похожей фазы.
   Worker уже хранит immutable command receipts. Добавить отдельное версионированное
   readonly чтение receipt через authenticated control; отсутствие receipt не
   считать отсутствием эффекта. Legacy control v1–v3 не менять скрытно.
5. Записать последний matching worker receipt и updater Quiesced атомарно в
   updater DB. Root Quiesce получает значения из подтверждения worker после
   закрытия dispatch/physical scan. Не принимать эти значения из CLI/MCP payload.
6. Status читает spec/команды/receipts/root/source/health в одном SQLite snapshot.
   Показывает ожидание принятых задач, закрытие dispatch, physical confirmation,
   stale/unknown outcome и причину остановки. Медленный UI не блокирует driver.

Нужны typed enum состояния driver, immutable history, CAS и одноразовый permit.
Pending после краха остаётся observation/reconciliation, без нового execution
permit. Readonly receipt lookup после потерянного ACK не повторяет внешнюю команду.
Новый контракт запрещено подменять fallback-вызовом старой мутации.

Ревью существующих effect paths: source lease блокирует только связанные проекты,
но BeginDrain блокирует admission новых roots во всём worker. Уже принятые задачи
других проектов и их children продолжают dispatch до CloseDispatch; завершённый
source hold гарантирует отсутствие принятых владельцев затрагиваемого проекта.
Новые requests сейчас получают явный отказ admission, а не принятый queued task:
не обещать пользователю новую серверную очередь без её отдельной реализации.
Если user pause мешает завершиться принятой очереди или приложение остаётся ready,
показывать этот blocker и ждать явного решения, не снимать pause и не останавливать
приложение ради обновления.

Ревью выявило capacity deadlock: прежний SQL учитывал update lease как занятый
work slot во всех фазах. При maxConcurrentWork=1 принятые другие проекты не могли
завершиться во время Draining. Исправление оставляет compiler reservation только
на SourceHeld (и неизвестных фазах, консервативно), а BeginDrain освобождает слот
атомарно со сменой фазы. Источники по-прежнему fenced; новые roots отклоняются,
CloseDispatch ждёт принятые roots/owners, Quiescing закрывает весь dispatch.
Regression воспроизводит прежнее occupied1 вместо0 и проверяет две принятые
очереди других проектов с capacity1. Это unit DB proof, не proof физических
Windows процессов; native проверка включена в следующий health-gate сценарий.

До включения driver запретить raw lifecycle BeginDrain/Quiesce через общий apply:
как для health, только специализированный journal driver с matching intents и
worker receipts вправе двигать root. Значения `acceptedOperations=0` в payload
сами по себе не доказательство. Сохранённая lease receipt описывает конкретную
команду/ревизию, а readonly live lease — более позднее наблюдение: это разные поля
с разной свежестью, их нельзя выдавать за общую транзакцию двух SQLite.

## Версии, отмена и границы

Хранение spec/command intents/receipts требует новой updater schema с consistent
backup и epoch fence. Не упаковывать evidence в человекочитаемое поле reason.
Для native gate обе проверяемые версии должны поддерживать новую schema:
старый target C2aac553 поддерживает только10 и годится для schema-refusal/negative
проверки, но не для положительного same-schema health/drain gate после миграции.
Worker journal не меняется без отдельной необходимости; updater его не мигрирует.

При busy/тайм-ауте не делать force kill и не заменять exe. Отмена/отсрочка должна
сначала доказать завершение своих callbacks/helpers и целостность source, затем
освободить только собственную worker lease. Другие operation/process leases,
config pending и user pause сохраняются. Неизвестное состояние остаётся blocked
с конкретной диагностикой, без TTL или неявного удаления marker.

Exact shutdown — следующий отдельный шаг: durable shutdown intent, flushed ACK,
точный PID/birth exit, instance/file locks и работающий независимый supervisor.
Quiesced и отсутствие named pipe сами по себе не разрешают replacement. Desktop
save/exit, activation, journal migration и controller confirmation остаются U2/U3.

## Доказательства готовности

Native проверки: две-три project queues, выполняющаяся задача и workflow child
завершаются без принудительной отмены; новые roots ждут; user pause не меняется.
Lost ACK и повтор чтения receipt не создают вторую команду. Stale revision/epoch,
чужой scope и неподтверждённые physical owners блокируют quiescence. Статус остаётся
доступным при ожидании; abort не выпускает lease раньше callback retirement.
Отдельно проверить config/cleanup/recovery gates и окончательное освобождение
только своей блокировки. Ни один тест этого блока не устанавливает runtime.
