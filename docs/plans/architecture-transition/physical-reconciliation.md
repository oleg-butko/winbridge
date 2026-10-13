# A2.4.1 — проверяемое владение Windows-процессами

Дата: 2026-10-13. База: `899a7ab`. Реализация готова к native gate;
до его завершения пакет не считается выполненным. Полный A2.4 остаётся открыт.

## Результат ревью

В A2.3.2 процесс успевал начать работу до commit его PID. При аварии между
этими действиями journal сохранял только launch intent. Кроме того, PID
не доказывает, что дочерние процессы остановились: родитель может завершиться,
а PID — перейти другому процессу. Новый epoch защищал SQLite writers, но сам
по себе не давал безопасного способа освободить проект после сбоя.

Исправление закрывает этот промежуток для Windows. Worker создаёт приостановленный
процесс, назначает его уникальному именованному Job Object, затем одной SQLite
transaction сохраняет PID, событие наблюдения и physical binding lease. Только
после успешного commit и проверки execution token вызывается ResumeThread.
Ошибка записи не разрешает resume; RAII пытается остановить именно своё дерево.
Если остановка не подтверждена, проект остаётся заблокированным.

Browser gate выявил ещё одну регрессию A2.3.2: completion импорта затирал фазу
`verified` на `finished` и терял `finishedAtMs`. Completion теперь сохраняет
проверенную фазу/commit и атомарно записывает terminal timestamp вместе с outcome,
attempt completion и освобождением lease. SQLite и UI regressions проверяют это.

## Контракт

`crates/platform::process_identity` содержит внутренний сериализуемый
`ProcessIdentity` и наблюдения `Empty / Occupied / Unknown`. Identity хранит:

- имя Job Object, полученное из journal incarnation и attempt UUID;
- SHA-256 MachineGuid Windows host и Windows session ID;
- PID и creation FILETIME для диагностики и будущих адаптеров.

Это metadata журнала, а не право клиента прислать доказательство остановки.
Новый Job Object нельзя подменить существующим: CreateJobObject с
ERROR_ALREADY_EXISTS отклоняется до изменения его limits. Job handle не
наследуется дочерними процессами; сохраняются прежние kill-on-close limits.
Проверка владения не завершает процессы по PID и не меняет чужие Job Objects.

Жизненный цикл именованного объекта и остановка дерева описаны в официальной
[документации Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
Проверка существующего имени нужна из-за поведения
[CreateJobObjectW](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-createjobobjectw).

При старте worker сначала берёт exclusive instance lock и новый journal authority,
восстанавливает leases, затем проверяет прежние `uncertain` physical bindings.
Только совпадение host/session и подтверждённое отсутствие объекта либо процессов
в нём разрешает записать `process_tree_empty` и освободить lease. Это не меняет
`indeterminate` исход прежней команды и не создаёт новый attempt.

Если остановка дерева ещё завершается, повторная read-only проверка выполняется
при scheduling не чаще одного раза в секунду. Неизменившееся наблюдение не
создаёт journal writes. Освобождение — CAS по исходной revision под текущим
authority guard; stale writer или ignored update не подтверждают release.
Если повторная проверка не может сохранить решение, новые admissions
приостанавливаются и Info/UI показывает journal fault; работающий worker
сохраняет обслуживание уже принятых задач до ремонта журнала и перезапуска.

| Момент сбоя | Восстановление |
| --- | --- |
| До OS creation, сохранён `no_process_created` | Lease можно освободить |
| Между OS creation и binding commit | Ownership неизвестен; автоматического release нет |
| После binding commit, до resume | Проверяется сохранённый Job Object |
| Родитель завершился, потомок жив | `Occupied`; проект продолжает ждать |
| Job Object отсутствует/пуст, host/session совпали | Cleanup receipt; lease освобождена, исход команды остаётся неизвестным |
| Другой host/session, ошибка открытия/чтения, нет binding | `Unknown`; никакого resume, kill по PID или release |

## Журнал, UI и совместимость

Schema 11 добавляет nullable JSON `physical` и immutable binding trigger.
При переходе с schema 10 сначала создаётся consistent `VACUUM INTO` backup;
column/trigger/version изменяются атомарно. Старые payload, record, keys,
attempt IDs и leases не переписываются; старым операциям не придумываются
physical bindings. Receipt `no_process_created` запрещён для уже bound процесса.

Optional `WorkerInfo.resourceRecovery` содержит число unresolved leases и первые
50 записей с project/operation IDs, наличием binding и ограниченной длиной причины.
UI показывает этот список отдельно от глобальной ошибки journal writes.
Проекты с uncertain leases ждут, независимые проекты могут продолжать работу.
Восстановление формы или чтение статуса ничего не запускает. Отсутствующий optional
field сохраняет legacy JSON. Полный resource read model/discovery остаётся в A3.

Unix adapter не обеспечивает эквивалентное физическое владение после сбоя:
отсутствие Unix PID не используется как доказательство освобождения Windows lease.

## Проверки и повторное ревью

Новый `reconciliation` target проверяет atomic PID/binding rollback, immutable
identity, foreign/stale owner rejection, запрет ложного no-process receipt,
schema-10 migration/consistent backup/rollback и bounded recovery inventory.
Windows tests проверяют suspended-before-commit, отказ после ignored binding,
коллизию имени Job Object, живого потомка после выхода родителя, несовпадение
host/session, освобождение после подтверждённой остановки и отсутствие повторного
исполнения прежнего idempotency key. Profile crash regression тоже проверяет
переход от indeterminate к новому явному запросу после physical reconciliation.
Browser regression проверяет UI recovery inventory при реальном Unix worker crash.
Его unknown lease сохраняется в отдельном test project; другие проекты продолжают
проверки. Fixture retention backdating соблюдает revision guard, не отключая его.
Local gate: 250 workspace tests, финальные 30 import-related и 40 guard tests,
полный browser gate с real Unix worker, UI typecheck/build/generated contracts,
Linux/Windows Clippy, Windows-target workspace/tests, PowerShell AST/export/scaffold.

Native сценарий собирает candidate из чистого source archive в отдельном runtime,
исполняет все прежние CAS/lease/workflow tests и новый target. Actual schema-6
reference запускается только на отдельном test journal и обязан отказать schema 11
без изменения его bytes. Installed runtime/config/journal остаются прежними.

Повторное ревью порядка эффектов: before-resume binding атомарен; probe не
использует PID как authority; release не объявляет неизвестный outcome успешным;
scope конкретного lease не освобождает новую задачу; current-owner uncertainty
не снимается старым callback; legacy unknown остаётся conservative.

## Следующий пакет A2.4.2

Этот пакет не решает rollback/restore журнала с потерей новых dedup keys.
Нужны explicit recovery mode, контролируемая новая incarnation, проверка
совместимости восстановленного состояния и понятное пользователю разрешение
старых неизвестных владельцев. Нельзя считать безопасным просто скопировать
старую SQLite и продолжить admission. Legacy execution, пересекающееся с durable
lease, сейчас вызывает отказ startup; отдельный read-only recovery mode должен
дать возможность изучить такое состояние без запуска команд.

До этого installed worker не обновляется на schema 11. A2.4.1 можно проверить
изолированно через старый стабильный worker, но полный A2.4/A2 не закрываются.
Далее A3 объединит observation UI/CLI/MCP, A4 добавит global resources/fairness,
и только после этих contracts A5 будет переносить owners в акторы.
