# A4.2 — очередь и durable pause, 2026-10-16

[План и ревью](queue-control.md), [поведение и настройки](../../guides/queue-control.md).
**Native candidate проверен; новый installed контракт ещё ожидается.**
Полный A4 открыт: далее полные resource sets и независимые consumers.

## Реализовано и проверено

Опциональный лимит принятых незавершённых roots: task/import/workflow считаются
по одному разу, workflow children не занимают второе место. Existing key проверяется
до quota/pause; отказ не сохраняет новый ключ и не подтверждает приём. Root marker
и domain record фиксируются в одной BEGIN IMMEDIATE. Counts читаются из фактических
статусов журнала, отдельного изменяемого счётчика нет.

Типизированная QueueMode разделяет pause admission и pause dispatch. Пауза запуска
проверяется в atomic claim до intent/attempt/lease, а уже начатый запуск продолжается.
Статус, output и cancel доступны. State, CAS revision и immutable command receipt
сохраняются атомарно. Повтор возвращает прежнюю квитанцию с текущим наблюдением;
потеря ACK/reopen не запускают новую команду и не выполняют automatic rebase.

Контракт `winbridge.queue-control` v1 обслуживается только local IPC с прежней
SID/permissions границей. WebSocket/MCP получают readonly metadata. Desktop
сохраняет исходную команду до send и предлагает отдельные pause-кнопки, проверку
квитанции и точный повтор. Это доверенный локальный интерфейс пользователя;
разрешённые direct commands не становятся sandbox для недоверенных программ.

Schema 13 добавляет root index, queue state и receipts. Migration 12→13 создаёт
consistent SQLite backup. На новом тестовом journal проверено, что фактический
предыдущий exe отказывается открывать schema 13 и не меняет bytes БД.
Рабочий journal пока остаётся schema 12; production quotas не включались.

## Windows candidate

Code `29dba9a2e41cad0786c2486c3658b5b14e07ae6e`, clean build
`1792194415716690600`, source manifest SHA
`0c09a3d312ac3d4d43e76b678add28e265628ff60a9acdd7a5acc69f1df0e2e7`.
Source archive SHA
`93d19a63bf673d3752caea4829b10358c6e7a0e5c4cab01e4ee7c98843addadc`.
Run `20261016-194546-f0e20b76`, parent task
`0d86baed-b6c4-4349-a3bc-e8b77d74363c` succeeded/0.
Report: `K:\VMs\shared\WinBridge\workflow-validation\20261016-194546-f0e20b76`;
report SHA `b0bd8dac7210b9556d62e8e7bfac957a464eeed3f184c1d79ddcbc32791ed5ce`.
**69 native tests / 16 groups, desktop compile и 20 verified artifacts passed.**
Это actual Windows named pipe/process/runtime tests и native desktop compile.
Desktop UI не открывался; его новое управление проверено через production panel
и настоящий Unix worker. Actual installed Windows UI/metadata ещё не заявляются.

Проверены mixed roots, конкурентные SQLite admission/CAS, rollback root/receipt,
child exemption, lowered quota, terminal release, removed-project fairness,
local-only envelope, dispatch pause без PID, live owner после pause, output/cancel,
restart receipt replay и отсутствие automatic resume. В queue proof после явного
resume: mode running, revision 4, pending roots 0, work cleanup 0.
Workflow handoff проверен одновременно с root quota 1 и work budget 1.

Прежний трёхпроектный pool также прошёл: 135 admissions при limits 1/2/legacy.
Loaded Info 14/5/4 ms, cancel → terminal 60/89/106 ms, cleanup slots 0.
Это измерения одного прогона. Worker остаётся authority физических процессов;
terminal root не освобождает неопределённого физического owner от work lease.

Previous installed exe SHA
`3f37ed4481d186a7d4e4aad72bd8c99bb5ade74f71a736a3b043706c72fea7f4`.
Проверенная копия сохранена в локальном Windows test run:
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261016-194546-f0e20b76\previous-schema12-worker.exe`.
После обновления runtime повторный native gate должен передавать этот путь через
`--queue-reference-worker`, а не использовать новый installed exe как старый.

Windows free before/after: 49,530,875,904 / 47,879,094,272 bytes.
Debian before clean build: 28,365,783,040 bytes, около 26.4 GiB.
Incremental выключен, основной Debian target и проверенный Windows compiler cache
переиспользованы. Исходники, test state и reports новые; старые результаты не
засчитывались. Windows validation directories и пользовательские файлы не удалялись.

## Ревью и локальная проверка

Ревью обнаружило retained queued tasks удалённого project, скрытые fair selection.
Исправлено: такой project участвует в scheduler и получает прежний policy failure,
а root quota освобождается по terminal status. Добавлен отдельный regression.
Также проверены immutable key payload, rollback при подавленном INSERT, scope/
schema guards, historical receipts и stale callback/epoch. Важных нерешённых
ошибок в проверенных сценариях не найдено.

Финальный clean commit прошёл **337 workspace tests**. Дополнительно прошли
Linux/Windows Clippy, fmt, generated contracts, typecheck/build, 46 общих Rust/TS
metadata cases, real-worker queue UI и полный прежний UI regression. Queue UI
проверяет lost ACK, persist-before-send, reopen без отправки, receipt query,
revision conflict без rebase, реальные counts/blockers, storage failure и offline.

RPC2 / operation-read-model / work-scheduling schema hashes сохранены.
Новый queue-control schema SHA:
`d33836aaf9d79201ccc19c21c56d3007de9cb46bcb47fd1f7e29e57a239e0722`.

## Debian controller и установленная пара

Clean controller `29dba9a`, build `1792194184549633653`, artifact SHA
`c36d56200ec0d67f3b9d0c3c6c3f557205f76a91cc61089a5ff66beccba9e3e5`
прошёл exact-binary consumer tests CLI/MCP/WS/IPC; proof SHA совпал с `/proc/PID/exe`.
Прежний `bff20cd` artifact сохранён, остановка graceful. Новый serve `7327`, PID
31211, session `90f93baa-71b1-4b3c-9287-8c83c41a3690`; endpoints/configs прежние.

Actual MCP pair `paired-20261016-195112-6991e4b3` прошла с installed `7e49cfc`,
build `1792075647649211100`. Tasks `ef4c877a-8507-473e-9780-b621124f1de2` и
`a387c9c6-bbc0-45b6-a884-905e7ae6ca1c` succeeded/0; duplicate сохранил IDs.
Receipt `c0ef052edcdcd86cc417fa51e03eca634b872963407cf323573adc8c56055e45`.
Worker generation `c2320f8e-e903-4668-8699-48e83e13f413`, journal incarnation
`5a8eb173-4193-4332-b00f-67e3850b2c97` прежние. Это RPC2 compatibility gate;
старый provider не рекламирует queue-control, новый контракт не включён заранее.

Следующая граница — [одно обновление worker/desktop](../../validation/architecture-worker-update.md),
затем actual installed metadata/control/receipt проверка автоматически. Рабочий
config менять не нужно; production quota absent, состояние новой queue running.
Desktop можно закрыть; worker оставить до обновления. Ручной UI checklist не нужен.

Local evidence: `.winbridge/architecture-transition/a42-*`,
`.winbridge/architecture-transition/paired-20261016-195112-6991e4b3`,
`.winbridge/workflow-validation/20261016-194546-f0e20b76`; в Git/ZIP не включается.
