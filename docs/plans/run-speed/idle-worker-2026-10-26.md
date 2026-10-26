# Нагрузка worker в idle, 2026-10-26

Работа в `perf/run-speed-2026-10-25`, основной worktree не менять. Updater остаётся
вне этой задачи. Рабочие Windows executable/config/journal не обновлять.

## Подтверждённое наблюдение

Readonly process snapshots для installed worker PID19064, start
2026-10-25T23:01:07.9609820Z, source d052c6c, generation66c20413:
две пары20.557334/18.394257s дали CPU102.61/102.53% одного логического ядра.
Основной поток9916 потребил16.766/14.984s CPU. User CPU преобладает.
Измерение включает границы двух коротких diagnostic tasks; между ними15s нет
диагностической задачи. Это не hardware-normalized общий CPU компьютера.
Ещё одна пара9.212132s дала101.77%. Raw records/requests/terminal states:
`.winbridge/idle-diagnostics/20261026-111454-1c81b1ce` и
`20261026-112042-e83d9b48`; первая parser-error попытка terminal failed,
побочных изменений не было, её не считать измерением.

## Причина в коде и исправление

`Worker::schedule` вызывается каждые25ms и ранее читал `Store::all()` и
`imports(false)`: полная десериализация завершённой истории, включая большие
PowerShell encoded arguments,40 раз/секунду. То же чтение было в проверке
старшинства queued workflow. В installed d052c6c этот код такой же.

Добавлены pending-only queries с `INDEXED BY queue_pending_tasks/imports`:
существующие с schema13 partial indexes, без миграции. Простого WHERE недостаточно:
EXPLAIN regression сначала показала `SCAN tasks` из-за ORDER BY rowid; явный
индекс исключает такой план. Порядок rowid сохранён, queued/running видны,
terminal history остаётся в журнале. Empty queue не вызывает fair/capacity scans.
Workflow advancement, recovery и проверка authority/freeze по-прежнему выполняются.

Каждый30s при wall occupancy>=5% или tick>=100ms worker пишет агрегат времени:
maintenance/recovery/admission_wait/workflows/queue_read/dispatch, tick count/max
и pending maxima. Время включает lock waits, это **не CPU**. Для вывода и при
малой нагрузке перед запуском нового worker:
`$env:WINBRIDGE_SCHEDULER_DIAGNOSTICS='1'`. Никаких commands/payloads в агрегате.

## Проверка

Portable regression:256 завершённых записей с32768 bytes arguments, сохранность
record/revision, исполнение новой задачи, отсутствие duplicate process при
повторном idempotency key. Store regression проверяет все7 states, порядок и
фактические partial index query plans для tasks/imports.

Подготовлен Windows probe `scripts/test-windows-worker-idle.ps1` через
`scripts/run-windows-perf-probe.py --probe worker-idle --source-ready ...`.
Чистый ZIP проверяется до/после, retained compiler cache под exclusive lease.
Baseline берётся по hash из предыдущего passed report, копируется в новую
test directory; installed runtime не используется для запуска child.
Изолированные child имеют собственные state/instance/IPC и одинаковую synthetic
history, controller URL отсутствует. CPU читается через GetProcessTimes за33s
после readiness+2s warmup. Only owned idle children завершаются через harness.
Проверяются scheduler/queue/workflow/resource/lease/FSM regressions.

Windows native результат **ещё не получен**. Наличие найденного hot path не
исключает другие причины в работающем экземпляре. Для installed окончательная
проверка потребует запуска исправленного executable; его замена здесь не делается.

Перед Linux compile: Debian24,547,356,672 bytes, Windows28,973,785,088 bytes свободно.
Windows probe дополнительно измеряет свободное место перед каждой Cargo командой.
