# A2.3.2 — durable ownership ресурсов проекта

Дата: 2026-10-13. База: `dd04276`, завершённый A2.3.1.
Статус: реализация готова к native gate; весь A2 остаётся открытым.

## Что показало ревью

1. После ошибки исполнения fallback мог сохранить `indeterminate` и убрать проект
   из in-memory map, хотя остановка дерева не подтверждена. Terminal outcome сам по
   себе не даёт права запускать следующую команду в этом проекте.
2. Ошибка `spawn` не всегда означает отсутствие процесса: ошибки назначения Job
   Object, resume или получения pipe происходят уже после OS creation. Platform
   теперь различает эти случаи; частичная инициализация оставляет uncertainty.
3. Root reservation и task/import claim прежде не имели общей durable проверки.
   Теперь проверка ресурса и решение о запуске находятся в одной SQLite transaction.
   Root writer использует `BEGIN IMMEDIATE`, исключая read-to-write lock upgrade race.

## Контракт

Schema 10 добавляет `resource_leases`. Lease привязана к project, operation kind/ID,
execution attempt и journal incarnation/epoch/authority. Claim сохраняет launch
intent, attempt и lease атомарно до запуска. Partial unique index допускает только
одну незавершённую `project_mutation` lease на логический project ID.

| Scope | Что защищает | Когда меняется |
|---|---|---|
| `project_mutation` | Import, обычную команду, startup приложения | Claim → held; подтверждённая остановка + terminal commit → released |
| `application_lifetime` | Управляемый процесс после readiness | Committed readiness передаёт владение из mutation scope; остановка + terminal commit → released |

Передача scope сохраняет прежние operation/attempt/authority и не записывает cleanup
proof: приложение продолжает работать. Она проверяет persisted readiness, Ready
application и совпадающий PID Running task; CAS и SQL guard не допускают обратной
передачи. Ошибка handoff сохраняет mutation reservation и сообщает recovery required.
Так сохраняется существующее поведение: после startup обычные команды могут идти
рядом с приложением, а поздняя остановка приложения не снимает lease новой команды.

Root workflow продолжает резервировать проект своим persisted Running record,
а не вторым execution slot. Его собственные linked children могут получать lease;
посторонний claim и конкурирующий root отклоняются внутри write transaction.
После terminal root процесс приложения остаётся отдельно учтённым.

| Lease state | Значение | Что допускается |
|---|---|---|
| `held` | Исполнитель получил ресурс | Mutation блокирует следующую mutation; Ready application допускает команды проекта |
| `uncertain` | Физическое освобождение не доказано | Новые mutations этого проекта ожидают, включая unknown application owner |
| `released` | Есть физический receipt | История неизменна; разрешён новый claim |

Task receipts: `NoProcessCreated` или `ProcessTreeEmpty`; import receipt:
`ImportThreadStopped`. Доверенный executor подтверждает их; RPC/MCP не принимает
произвольный «ресурс свободен». Task завершение, attempt completion, cleanup proof,
release и outcome attention коммитятся вместе. При ошибке outcome writer executor
может отдельно сохранить receipt: lease ещё held, следующий запуск не разрешён.

`ProcessOwner` владеет managed tree и pipe readers уже до PID commit. При ранней
ошибке он завершает дерево, проверяет parent exit и empty Windows Job Object,
дожидается readers; fallback сохраняет полученный receipt. Если доказательства нет,
result становится indeterminate, lease — uncertain. Unix adapter не подменяет
native Windows проверку Job Object.

## Перезапуск и совместимость

Worker получает прежнюю exclusive OS instance lock, активирует authority,
восстанавливает leases и только затем допускает scheduling. Новый epoch сам по
себе не освобождает ресурс. Сохранённый receipt может освободить старую lease;
task при этом восстанавливается в indeterminate, успех не выдумывается.

Import выполняется потоком того же процесса, удерживающего instance lock.
После получения lock нового worker и reconciliation import record прежний поток
точно завершён: допускается `ImportThreadStopped`. Эта логика неприменима к
внешнему процессу или будущему отдельному actor executor.

Legacy Running и Indeterminate tasks без leases собираются в одну retained группу
на проект, включая все известные IDs/PIDs и unknown PID. Им не создаются
вымышленные attempts/receipts. Повторный restart не теряет и не дублирует группу.
Ранее terminal success/failure/cancelled записи задним числом не переоцениваются.

Миграция 9→10 выполняет consistent `VACUUM INTO` backup и атомарное создание таблицы,
indexes и guards. Старые JSON, idempotency keys, output и identities сохраняются.
Frozen wire v1 не изменён. Старый worker должен отказать новой schema до её изменения.

## Наблюдаемое поведение

Queued task/import/workflow получает persisted reason с project и owner ID, если
durable ресурс мешает запуску. Повтор scheduler не плодит одинаковые events;
после получения ресурса причина ожидания очищается. Queued cancel остаётся доступен.
Неопределённость одного проекта не блокирует независимые проекты.

Автоматически снимать unknown process lease по timeout, epoch или terminal result
нельзя. Проверка физического состояния, recovery mode и явное разрешение неизвестных
ресурсов относятся к A2.4. Полноценный обзор ресурсов в UI/MCP относится к A3.
Пока этот путь не завершён, candidate с такой блокировкой не заменяет installed runtime.

## Цена следующего расширения и проверки

Scopes сейчас используют логический project ID; overlapping workspace roots,
отдельные binary/IPC keys, global capacity и fairness будут реализованы в A4.
Понадобится additive migration resource keys/policy, а не отмена token, CAS или
cleanup receipt контракта. SQLite остаётся durable authority; HashMap — runtime cache.
Actor migration сможет использовать те же decisions и fenced callbacks.

Добавлены 15 tests leases и fault test readiness handoff. Проверены конкурирующие
task/import/root claims, ignored insert/release, atomic rollback, cleanup receipt
после failed outcome, stale/forged proofs, retained legacy identities, schema-9
backup/failed migration, последовательные настоящие процессы и copy recovery.
Existing PID fault дополнительно проверяет actual cleanup и следующий запуск;
profile crash проверяет ожидание в пострадавшем проекте и работу независимого.

Native `--task-cas-only` включает leases target и schema-10 refusal actual reference
worker. На Windows требуется только installed worker; desktop и UI окна не нужны.
Local gate: 245 workspace tests passed (reference refusal test intentionally ignored),
финальные leases/task_fsm/workflows targets: 44 tests passed. Windows-target
workspace и worker tests, Clippy all-targets, fmt/diff checks, PowerShell AST,
Python syntax/pre-IPC argument guards и scaffold passed.
Готовность будет отмечена после автоматического native gate.
