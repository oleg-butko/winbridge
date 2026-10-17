# A4.3 — atomic resource sets, 2026-10-17

[План и ревью](resource-sets.md), [поведение и config](../../guides/resource-scheduling.md).
**Native candidate проверен; actual installed новый контракт ожидает update.**
A4.2 завершён; полный A4 открыт: после installed gate остаётся A4.4 multi-consumer.

## Реализация и инварианты

Optional resourceScheduling объявляет build/cache/binary/ipc/port/interactive
resources и project bindings. Требования task/import и весь workflow step plan
сохраняются до ACK; children получают frozen parent plan, включая после config
edit/restart. Старый key возвращает прежнюю операцию, не меняя её требований.

Loaded policy хранится с owner epoch. Peer Store connection не обходит capacity
пустым in-memory default. В BEGIN IMMEDIATE выдаётся весь набор вместе с
intent/attempt/project lease либо откатывается всё. Нет частичного hold-and-wait
и независимого mutable counter: counts читаются из claims JOIN execution leases.
Fair project order действует и при named policy без общего work limit.

Claims наследуют authority/attempt/physical cleanup существующего lease. Ready
handoff освобождает build/cache через durable scope; binary/IPC/port/interactive
остаются у application. Recovery меняет state, сохраняя scope: ранее отпущенные
work resources не захватываются снова при последующей uncertainty application.
Terminal status без physical proof ресурс не освобождает, TTL не используется.

Schema 14 добавляет policy/immutable requests/claims, migration 13→14 создаёт
consistent SQLite backup. Native test запускает предыдущий actual exe с НОВОЙ
тестовой БД: schema 14 отклонена, bytes БД не изменились. Working schema пока 13.
RPC 1/2 и operation-page v1 сохранены, их hashes проверяются frozen tests.

Новый readonly contract `winbridge.resource-scheduling` v1, schema SHA
`9c5fbd6e588e8bd550b5cbc33c911d516619a61e87bfa41c67c91d564eed1e19`.
Info/MCP передают scoped capacities/used/uncertain/owners/policy generation.
Inventory bounded, truncation явная, счётчики показанных ресурсов полные.
UI показывает владельцев, причины ожидания — в общем журнале, freshness и inspection.

## Native Windows

Final code `fc5c90ef4ab1afdbe962f786dcc85859f4a6c1c2`, build
`1792227527487797400`, source manifest SHA
`76ab1be29d7d69f0fcc59043c82b0ba2705aa1f3771ea73b851fc1b85db7b508`.
Archive SHA `89c8182ff366660ea1410e80202712619c2d64584fecba5e73f4186e886d4878`.
Run `20261017-045801-304be26a`, parent task
`dbca6dff-12e4-4f9b-a13b-b8cacd2386e9` succeeded/0.
Report: `K:\VMs\shared\WinBridge\workflow-validation\20261017-045801-304be26a`;
report SHA `3a0a20de827ca8f3578f12253f702797ebf3f13312e6b5d507886a001c5327f4`.
**80 native tests / 20 groups, desktop compile и 25 verified artifacts passed.**
Readonly recheck повторно прочитал исходный task/report, без нового submit/build.

Проверены complete sets на трёх projects, shared cache blocker при свободном pool,
fair next project перед backlog прежнего owner, native Info/operation page/fenced
cancel, duplicate с прежним PID, frozen request после config edit/restart и cleanup 0.
Workflow отдельно проверяет frozen start resources после edit до создания children:
после Ready четыре process resources удерживаются, build/cache свободны;
после stop все resources свободны. Root quota 1/work limit 1 действуют одновременно.

Native Store tests: два concurrent connections, rollback подавленного INSERT,
peer persisted policy, policy removal, uncertainty, migration/backfill/backup и
guard legacy → named activation. Сохранён прежний трёхпроектный pool regression:
135 admissions, loaded Info 8/11/5 ms, cancel 55/89/95 ms, cleanup slots 0.
Это измерения одного прогона, а не обещание latency при любой нагрузке.

Previous schema-13 exe SHA
`62727ba0f7181b2cd9737903afcf326945123323a7f814cc2f21180d04710628`;
retained copy:
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261017-045801-304be26a\previous-schema13-worker.exe`.
После update для повторов передавать этот путь через `--resource-reference-worker`.
Queue schema-12 reference по-прежнему нужен отдельно; он сохранён в предыдущем
run `20261016-194546-f0e20b76`, report.queueReferenceWorker.

Windows free before/after: 51,842,183,168 / 51,738,898,432 bytes.
Debian before final clean build: 27,654,430,720 bytes, около 25.8 GiB.
Incremental выключен, проверенный compiler cache переиспользован; source/test state/
reports новые. Windows directories не удалялись. Desktop окон не было.

## Ревью, исправления и UI

Первый candidate `46c9445` прошёл 346 workspace tests и 79 native tests, run
`20261017-044841-da6e9215`. При последующем ревью исправлен project policy fingerprint:
чужие bindings и unused definitions не сбрасывают квитанции другого project.
Новый конкурент за общий cache меняет availability, а не permission. Собственный
binding и referenced capacity/kind по-прежнему входят в fingerprint. Final native
повтор включает этот regression на Windows.

Также ревью добавило guards: нельзя включить named policy при unclassified legacy
admissions/physical owners; нельзя disable/remove/change kind удерживаемого ресурса.
Это сохраняет принятый контракт и не выдумывает требования из произвольной команды.
Новых важных ошибок после исправлений в проверенных сценариях не найдено.

Final clean code прошёл **347 workspace tests**; core/protocol/store/runtime/workflow,
38 общих Rust/TS metadata cases, Linux/Windows Clippy, typecheck/build и contracts
прошли. Полный прежний UI regression прошёл на `46c9445`; frontend после scope-only
исправления не менялся. Final production ResourceSchedulingSummary проверен по
checksummed actual Windows observations: capacities/owners, frozen owner после
restart, cleanup 0 и stale snapshot. Native window automation не заявляется;
desktop прошёл native compile, renderer — headless Chromium с sandbox.

## Controller и граница продолжения

Debian controller exact clean `fc5c90e`, build `1792227308688010918`, SHA
`6f0ec2888ccf4b009bdbd59483b8d0ad452b61509675f7cac9a99c78fd451f51`
прошёл CLI/MCP/WS/IPC consumer tests. Proof SHA совпал с `/proc/PID/exe`.
Прежний exact `29dba9a` artifact сохранён; shutdown graceful, endpoints/configs прежние.
Serve `35492`, PID 24440, session `c0cc059c-1401-491d-be16-ab74960ef689`.

Actual MCP RPC2 pair с installed worker `99351ea`, build `1792224150478905600`,
прошла: tasks `1c71c3af-d35c-453d-95db-7418c824e716` и
`0d7707f0-4463-45b5-8c79-c7100fc02be5` succeeded/0, duplicate сохранил IDs.
Receipt `418729e49f17d13a6f79879df4a95c0c894ff6ebfa376f4474236be1665fc34f`.
Worker generation `377f2ea2-a0be-4a9d-b2e9-eae49d5ed764` и journal incarnation
`5a8eb173-4193-4332-b00f-67e3850b2c97` прежние, health/idle подтверждены.
Resource contract у installed provider отсутствует: RPC2 pair не объявляет его
включённым. Production resource policy/quotas не менялись.

Следующее действие — [один update worker/desktop](../../validation/architecture-worker-update.md),
затем automatic actual installed claims/metadata/UI gate. До него A4.3 не закрыт.
Desktop можно закрыть, worker оставить до update. Ручной UI checklist не нужен.

Named IDs — объявленные конфликты внутри одной authority, не OS sandbox и не
автоматическое определение побочных эффектов argv. Консервативная workflow
reservation одного project сохраняется. Эти ограничения описаны в guide.

Evidence: `.winbridge/architecture-transition/a43-*`, shared final run,
`.winbridge/architecture-transition/resources-final-6d2fe418` и local renderer proofs.
В Git/source ZIP runtime evidence не включается.
