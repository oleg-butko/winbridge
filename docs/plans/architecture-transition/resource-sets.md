# A4.3 — атомарные наборы общих ресурсов

2026-10-17. A4.2 installed gate завершён. План проверен до реализации;
этот документ не объявляет A4.3 готовым. [Общий порядок](README.md#A4--очередь-двух-трёх-проектов-и-ограничение-нагрузки).

## Результат и границы

Два-три проекта могут запрашивать разные наборы shared resources. Например,
два build slots допускают две сборки, общий cache допускает только одного writer,
а binary/IPC/port/interactive slot удерживается живым приложением после Ready.
Пользователь видит capacity, owners, uncertainty и причину ожидания. Один запрос
получает весь набор вместе с прежним project/work lease либо не получает ничего.

Это статическая opt-in policy локального worker, без broker/actor runtime и нового
сетевого mutation API. Legacy RPC 1/2 и operation-page v1 сохраняются. Resource IDs
обозначают объявленные пользователем конфликты; WinBridge не выводит все побочные
эффекты произвольной команды из argv и не подменяет OS port/IPC registration.
Изоляция ОС и разрешённые direct commands остаются прежними.

## Policy и контракт

Optional root `resourceScheduling`: definitions `{id, kind, capacity}` и bindings
по project ID: общие task, import, дополнительные workflow build/start requirements.
Kind enum: build/cache/binary/ipc/port/interactive. Build допускает capacity 1..64,
остальные exclusive, capacity 1. Requirement содержит resource ID и units.
Build/cache отпускаются при application Ready; остальные принадлежат физическому
owner до cleanup. Direct task/import без handoff держат набор до завершения.

Policy отсутствует по умолчанию; рабочие configs не менять. Сохранённая настройка
вступает в силу после manual restart. Validations ограничивают размер, запрещают
unknown/duplicate IDs/projects, неизвестные виды, impossible units и дубли между
общим task и role-specific набором. Named resources дополняют обязательные project
lease и global work pool, а не заменяют их. Отсутствие policy сохраняет legacy mode.

До ACK сохраняется immutable resource request и его provenance: обычный task/import
и весь workflow step plan. Children получают набор из frozen parent в одной
transaction с admission/link. Config edit/restart не перепривязывает принятый запрос.
Новая capacity применяется при claim; удалённый/несовместимый resource блокирует
старый frozen request с понятной причиной, а не запускает его без защиты.

Loaded resource policy сохраняется с owner epoch в journal. Второй Store connection
использует persisted policy; собственный empty in-memory default не обходит лимит.
Policy generation привязана к activation authority, независима от command revision
и wire version. Inspection читает прежнюю policy, ничего не активирует.

## Durable claim, handoff и recovery

Schema 14: singleton loaded policy, immutable requests и immutable per-lease claims.
Migration 13→14 создаёт consistent backup; прежние admissions получают explicit
empty legacy plan. Старый schema-13 exe отказывается открыть новую тестовую БД.
Рабочая migration — только после candidate gate и обычного user update.

В BEGIN IMMEDIATE проверяются pause, global capacity, project reservation и все
frozen requirements. Затем intent/attempt/project lease/claims фиксируются вместе.
Ошибка любой строки откатывает весь claim; нет частичного набора или hold-and-wait.
Count выводится из claims JOIN retained execution leases, отдельного счётчика нет.

Claims наследуют authority/attempt и state существующего execution lease. Terminal
status не является physical cleanup; uncertainty сохраняет ресурсы. Recovery и
reconciliation используют прежний Job Object/process/import ownership path.
TTL не освобождает ресурс. При Ready application durable handoff освобождает только
build/cache; process resources остаются до подтверждённого завершения. Уже отпущенный
build/cache не захватывается заново, если application позднее станет uncertain.

Legacy workflow/project reservation сохраняется; расширение параллельных workflows
одного project допускается только при отдельно доказанной полной защите binary/IPC.
Первый A4.3 даёт общие resources между независимыми projects и безопасные direct
commands после application handoff, не обещает несколько произвольных приложений
в одном неразделённом workspace.

## Наблюдение и проверки

Новый readonly `winbridge.resource-scheduling` v1 и schema hash, optional opaque Info
metadata с provider/context/policy generation, capacities/used/uncertain и owners.
Inventory ограничен, truncation явная, counts полные. UI показывает actual snapshot,
freshness/inspection/default mode; unknown contract не объявляется активной policy.
Operation-page v1 сохраняет прежние leases, reason включает новый resource blocker.

Проверки: разные наборы на трёх projects, contention за общий resource при свободном
work pool, конкурентные SQLite claims, полный rollback, frozen child inheritance,
lowered/removed policy, duplicate acceptance, restart/epoch, uncertainty, no-process
failure, cancel и application handoff. Общие Rust/TS metadata fixtures и actual UI.
Full local regression, isolated native Windows gate через здоровый installed worker,
затем review, exact controller proof и один update для actual installed gate.

## Повторное ревью плана

SQLite transaction и прежний execution lease сохраняют единую authority; отдельный
resource daemon/счётчик не нужен. Immutable requests не дают config edit ослабить
принятый контракт; persisted policy устраняет bypass новым Store connection.
Handoff использует scope существующего lease, чтобы release и readiness не расходились.
New observation contract не расширяет frozen operation-page v1. Explicit resource
IDs честно ограничивают обещание: произвольные external effects нельзя автоматически
классифицировать как безопасные. Actors A5 остаются последующим внутренним refactoring.
