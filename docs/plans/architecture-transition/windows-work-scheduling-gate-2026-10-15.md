# A4.1 — work budget и отзывчивость очереди, 2026-10-15

[План и ревью](work-scheduling.md), [поведение и config](../../guides/work-scheduling.md).
**Native candidate проверен; actual installed новый контракт ещё ожидается.**
Полный A4 открыт: backlog/pause, полные resource sets и проверка независимых
потребителей относятся к следующим пакетам.

## Windows candidate

Code `bff20cde70aef4c7bca12cc0e3d01797b3fc2f35`, build `1792075131808810200`,
manifest SHA `03f3659da8ff01fcae11a839868e9b3d004c360e1b5a29c968403f30cfbe77fd`.
Run `20261015-103809-d68fa193`, task `63ba3507-02b9-48dc-b18f-7b2e9ea22d32`
succeeded/0. Report в `K:\VMs\shared\WinBridge\workflow-validation\20261015-103809-d68fa193`;
SHA-256 `9fbfdc39cda96abf7a8149d7d9cdbf7009f228d27f908c39fb0ea6d398cb89d7`.
**57 native tests / 12 groups, desktop compile и 15 verified artifacts passed.**
Ready manifest проверен по размерам и SHA каждого файла. Desktop не запускался:
это native compile и actual Windows IPC/process tests, без UI automation.

Проверены гонка двух SQLite connections за один слот, rollback claim/attempt/lease,
сохранение uncertainty после restart, task/import capacity, fair projects,
typed metadata и прежние frozen wire/readiness/operation pages. Workflow root
не занимает второй слот; Ready application освобождает work capacity и сохраняет
application lease. Другая команда после этого запускается, приложение завершается
через свой прежний ownership path.

В каждом pool scenario три проекта имеют по 15 принятых задач **до старта IPC**.
Проверены slots, ожидание старой задачи за пределами recent 10, actual named pipe
Info/operation page/fenced cancel, следующий проект и физический cleanup.
Это 135 admissions, а не 135 завершённых исполнений: ненужный backlog отменяется.

| Режим | Начальные work slots | Loaded Info | Cancel → terminal | После cleanup |
| --- | --- | --- | --- | --- |
| Limit 1 | 1 | 12 ms | 46 / 52 ms | 0 |
| Limit 2 | 2 | 4 ms | 66 ms | 0 |
| Legacy, общего лимита нет | 3 | 8 ms | — | 0 |

Это измерения одного прогона, не обещание latency для любой нагрузки. Regression
требует loaded Info менее 5 s при IPC deadline 10 s; proof хранит времена и scope.
В повторе использован только проверенный Cargo cache; source/state/reports новые,
старые результаты тестов не засчитывались. Windows free before/after:
45,532,835,840 / 45,516,910,592 bytes (около 42.4 GiB). Debian около 27 GiB,
incremental выключен; Windows directories и пользовательские данные не удалялись.

## Найденная и исправленная проблема

Первый run `20261015-060547-6ba88f51` получил IPC timeout. Диагностический
`20261015-102946-8f40dd67` прошёл тесты, но loaded Info занял 9.5 s,
cancel — 4–8 s. Scheduler повторно проверял Windows paths у задач, для которых
общий pool уже заполнен, удерживая Store и задерживая обслуживание IPC.

Теперь advisory check выполняется перед path validation и один раз на project
pass. Повторная неизменившаяся wait reason не записывается; timer пропускает
missed ticks. Atomic claim по-прежнему повторяет capacity check в BEGIN IMMEDIATE:
предварительная проверка не выдаёт права на запуск. Проверка путей перед реальным
запуском сохранена. Финальный test снова использует холодный старт с полной
очередью, без диагностического прогрева IPC.

326 workspace tests прошли; после последнего сокращения повторных scans отдельно
прошли durable pool tests, actual pool/workflow и Linux/Windows Clippy. Ранее
прошли typecheck/build, 23 общих Rust/TS metadata cases, actual Unix worker UI с
queue/cancel/freshness и полный прежний browser regression.

## Debian controller и установленная пара

Запущен clean controller `bff20cd`, build `1792075089522830086`,
artifact SHA `002b5664e9c0a12643953a7287884fef37493b80a061f9a0bb8578dc15974946`.
Exact binary прошёл consumer tests CLI/MCP/WS/IPC; SHA proof совпал с `/proc/PID/exe`.
Старый artifact сохранён локально, остановка была graceful. Serve session `26577`,
PID 44875; WS session `dbe5ce5a-3e1e-4813-b074-1a57d171aba0`.
Рабочие endpoints/configs не менялись.

Actual MCP gate `paired-20261015-104142-e21dcc73` прошёл с установленным worker
`a8cf019`, build `1792055908607920500`. Provider task
`d798ba0f-c36d-4923-a347-29dd7618a5ee` и pair task
`adea5c85-4cf2-4337-a5f6-922ff21cbb75` succeeded/0; duplicate сохраняет обе задачи.
Receipt `64877717460d085593828f07a6f07c0a0e6fc20c5b694e04437d8b22e7709d9e`.
Worker generation `69b65ecc-fe9d-42f2-a4cc-210b75127a66` и journal incarnation
`5a8eb173-4193-4332-b00f-67e3850b2c97` сохранены. Это проверка RPC2 пары,
а не включение нового work-scheduling контракта на старом worker.

RPC 1/2 и journal schema 12 прежние. Новый work-scheduling v1 schema SHA:
`d89abdc35d712b7cced4a8c922ece1c7dca469b330c7160ec1df356f12a00bc8`.
Default mode сохраняется, production budget не включался. Следующая граница —
[одно обновление worker/desktop](../../validation/architecture-worker-update.md)
и автоматическая проверка actual metadata, UI и установленного executable в
изолированном pool. Ручной Windows checklist не нужен. Desktop можно закрыть,
worker оставить работать до обновления.

Local evidence: `.winbridge/architecture-transition/a41-*`,
`.winbridge/architecture-transition/paired-20261015-104142-e21dcc73`,
`.winbridge/workflow-validation/20261015-103809-d68fa193`; это runtime evidence,
оно не входит в Git/source ZIP.
