# A4.2 завершён — установленная очередь, 2026-10-17

[Native candidate](windows-queue-control-gate-2026-10-16.md),
[план и инварианты](queue-control.md), [поведение](../../guides/queue-control.md).
Actual installed executable, readonly MCP metadata и production renderer прошли.
Полный A4 открыт: далее A4.3 — resource sets, A4.4 — независимые consumers.

## Установленный exe

Worker clean `99351ea21cfbd5a50d37ece5f2968cec1e233b5b`, build
`1792224150478905600`, manifest SHA
`a8bb8a9d7beaa60e33068903ed504a3d3e7ac48a71b4bd8aee7cea9e23262072`.
Exe SHA `62727ba0f7181b2cd9737903afcf326945123323a7f814cc2f21180d04710628`.
Generation `377f2ea2-a0be-4a9d-b2e9-eae49d5ed764`, journal incarnation
`5a8eb173-4193-4332-b00f-67e3850b2c97`: прежняя область и история сохранены.
Queue-control v1/schemas подтверждены actual Info; mode running, revision 0,
policy absent. Рабочие quotas и pause автоматически не менялись.

Run `queue-installed-20261017-041138-3c17d491`, parent task
`afc7a052-08a6-43fd-9d52-36b31af9cec7` succeeded/0. Report:
`K:\VMs\shared\WinBridge\workflow-validation\queue-installed-20261017-041138-3c17d491`.
Report SHA `cfe26e87a92ffb4eb267ed99eeaba1b9c24c427102d9413291dfe0e4f4469c3d`;
size/hash ready marker проверены. Readonly `--finish` повторно прочитал исходный
task/report; новой задачи, сборки или перезаписи completed proof не было.

[`verify-queue-control.ps1`](../../../scripts/verify-queue-control.ps1) запускает
installed exe с новым config/state, двумя test projects, root quota 2/per-project 1
и work budget 1. Пройдено:

- обе независимые pause, root count, existing-key replay при full/paused queue;
- отклонённый key не резервируется: после explicit resume тот же запрос принят;
- running PID сохраняется при pause, output читается, cancel завершает owner;
- queued task не запускается после restart, pause/revision/receipt сохранены;
- exact receipt replay, stale revision отклонён без automatic rebase;
- explicit resume завершает оставшуюся задачу, roots/work slots после cleanup 0.

Тестовый generation изменился, journal scope сохранился. После теста оба owned
test worker процесса остановлены; рабочие exe/config/queue не менялись.
Windows free before/after: 62,572,552,192 / 62,571,175,936 bytes. Сборки не было,
Windows directories не удалялись. Debian около 26 GiB free.

## UI и MCP

Production QueueControlPanel отрисован по checksummed actual Windows observations:
pause labels, root counts/blockers, restart scope, working default и offline.
Browser renderer не получил mutation endpoint, кнопки disabled. Native desktop
окно не запускалось; lost ACK/restore/storage failure уже проверены real Unix UI
в candidate gate. Не заявляется native window automation.

Actual MCP pair с controller clean `29dba9a`, build `1792194184549633653`, прошла.
Provider task `cb7ff49a-85b8-42f9-ad4b-a1ebdf3ae8ab`, pair task
`4bddd22c-1757-42c8-b9e8-5101d8be5a23`: succeeded/0; повтор сохранил IDs.
Receipt `cbb3ac49d20187e3e197739fcfa3d50339766876648661cdad6b6f34a1eb7743`.
Session `e75e3bb3-45ba-4af4-b96a-aa4e01ea4e0f`; worker healthy/idle после проверки.

## Ревью и продолжение

Повторно просмотрены atomic root admission, duplicate-before-quota, queue state/
receipt transaction, callback/authority guards, claim-before-effect и UI recovery.
Важных новых ошибок в проверенных сценариях не найдено. Добавлены повторяемые
installed-exe verifier, immutable parent request/readonly finish и renderer gate.
Рабочий controller повторно запущен exact tested artifact; SHA consumer proof прежний.

A4.2 закрыт. Desktop можно оставить закрытым, worker — работающим.
Следующий пакет выдаёт весь набор общих ресурсов атомарно и сохраняет их до
подтверждённого освобождения; actors/broker для этого не нужны.

Evidence: `.winbridge/architecture-transition/queue-installed-20261017-041138-3c17d491`,
`.winbridge/architecture-transition/a42-installed-*-20261017.log` и shared report.
Runtime evidence не входит в Git/source ZIP.
