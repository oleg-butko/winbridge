# A4.1 — установленная Windows версия проверена, 2026-10-15

[Native candidate](windows-work-scheduling-gate-2026-10-15.md),
[семантика](../../guides/work-scheduling.md). **A4.1 завершён; полный A4 открыт.**

Worker `7e49cfc50ac0741336c598df453fcbeb295bfd70`, build `1792075647649211100`,
manifest SHA `efce08cf5adf68d7837de6282176634235c8b993c278d2d280c92f617dfadf56`.
Generation `5cfc53f0-5cfc-435a-a50e-b8f2ee7fb4eb`; прежняя journal incarnation
`5a8eb173-4193-4332-b00f-67e3850b2c97`. Контракт work-scheduling 1 и его schema
объявлены; default mode подтверждён actual MCP Info: общий лимит выключен,
occupied/queued/uncertain равны нулю. Рабочие configs не изменялись.

Controller clean `bff20cd`, build `1792075089522830086`, работает на прежних
endpoints. Actual RPC2 pair `paired-20261015-105307-c1cfd9d5` прошла: provider task
`8fd9fcf0-c5a5-4b82-b39f-51742d8afde0`, pair task
`f3b9f9d8-fca4-4643-b44d-2bc27af7e36f`, обе succeeded/0. Receipt
`b564d23d4d573535f65a9d072b496ab33bd93b4b1556716667170dd9b063f5ba`.
Исторические receipts не выдаются за новую пару.

## Проверка установленного executable

`scripts/validate-work-scheduling-installed.py` отправляет один pinned parent
request через RPC2, сохраняет его до submit и проверяет повтор того же key.
`--finish GATE_DIRECTORY` только наблюдает прежний task/report, без новой команды.
`scripts/verify-work-scheduling.ps1` запускает **существующий runtime exe** с
тремя новыми isolated configs/state. Никакой сборки/замены runtime нет. Test worker
не подключается к controller; остальные проекты пользователя не исполняются.

Run `work-installed-20261015-105708-bea99042`; parent task
`5ac141e6-de96-4011-9ac5-e6f2c545c4ab` succeeded/0. Report в
`K:\VMs\shared\WinBridge\workflow-validation\work-installed-20261015-105708-bea99042`;
SHA `8ca689487eee038e0e3d5b26483cc54be585e58633fc981dc3fb4c4dfba94ba3`.
Размер и SHA проверены после чтения shared. Executable SHA before/after:
`3f37ed4481d186a7d4e4aad72bd8c99bb5ade74f71a736a3b043706c72fea7f4`.

| Режим | Admissions / проекты | Work slots | Loaded Info | Cancel → terminal | Cleanup |
| --- | --- | --- | --- | --- | --- |
| Limit 1 | 45 / 3 | 1 | 7 ms | 95 / 51 ms | 0 slots, test worker stopped |
| Limit 2 | 45 / 3 | 2 | 5 ms | 52 ms | 0 slots, test worker stopped |
| Legacy | 45 / 3 | 3 | 5 ms | — | 0 slots, test worker stopped |

Проверены actual named pipe/fenced requests, old backlog за пределами recent 10,
wait reason через operation page, следующий проект после cancel, duplicate с
прежними Task ID/PID и одной записью физического запуска. Это 135 admissions,
не 135 завершённых прогонов: backlog явно отменён перед остановкой test workers.
Первый run обнаружил harness-only ошибку FileShare при чтении живого runs.log;
совместное чтение исправлено, полный повтор прошёл. Рабочий worker ошибок не имел.

Production `WorkSchedulingSummary` проверен в Debian Chromium на checksummed
observations установленного Windows exe: три режима, queue после cancel,
scope, default working mode и предупреждение last-known при потере свежести.
Это проверка renderer, не native window automation. Browser не имеет submit/config
endpoint. Evidence `ui-completed.json` и `installed-summary.png` рядом с local gate.

## Ревью после завершения

Проверены atomic capacity/attempt/lease claim, rollback и epoch guards,
неосвобождение uncertain owners по одному terminal/таймеру, durable fairness,
workflow child без второго слота и Ready application handoff. Важных runtime
ошибок не найдено. Limits остаются startup policy единственного Store owner;
до независимых actor writers потребуется persisted policy generation (A4.3).
Текущий backlog не ограничен и live pause отсутствует — это следующая A4.2,
а не скрытые возможности A4.1.

Рабочий worker здоров/idle, generation/scope/config/policy прежние. Новых Windows
окон не было; desktop можно держать закрытым, worker оставить работать.
Local evidence: `.winbridge/architecture-transition/work-installed-20261015-105708-bea99042`,
`paired-20261015-105307-c1cfd9d5`, `a41-installed-*`.
