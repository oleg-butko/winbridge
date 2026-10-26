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

## Native результат: passed

Run `20261026-113022-1c313991`, task
`018d081d-7b69-45c2-b938-549ddd183681`, source
`d683199046353ef23f15260e836596d23e62d29f`: succeeded/0.
**52 Windows tests passed**;2 ранее существовавших opt-in cases не запускались.
CPU test сначала ignored в обычном наборе, затем явно выполнен отдельной командой.
Все4 Cargo команды exit0.
5 ready files проверены дважды; report SHA256
`9ca24747b6824b576b7fa41d953f0d2d735b3aa328ca08dd8d3de0ab195f9998`.
Локальный pin `.winbridge/run-speed/20261026-113022-1c313991/verified.json`.

| Изолированный idle worker | CPU за33s | Доля одного ядра |
| --- | ---: | ---: |
| Старый scheduler |32734.375ms|99.1866%|
| Исправленный scheduler |500ms|1.5148%|

Падение CPU98.47%, примерно65 раз. Это одна пара на synthetic256-row history,
а не измерение installed worker после обновления. Controller URL отсутствует,
поэтому гипотеза network/heartbeat не требуется для воспроизведения проблемы.
Новые diagnostic logs включены только у candidate; их overhead входит в500ms.
Обычные dev/test profiles совпадают: root Cargo.toml SHA256
`37ac706c7d4cac1a001c3bf6b6bf8666a2fda73815796b88b6844c9b736cdd6f`,
включая receiver source предыдущего gate. Baseline — retained worker из того
gate, не старый installed executable с другими профилями.

Candidate log:1198 ticks/30.025s, scheduler wall829.6ms(2.76%), max tick50.4ms;
admission_wait334.8ms, workflows243.6ms, queue_read226.1ms, recovery14.8ms,
maintenance5.8ms, dispatch4.5ms. admission_wait включает получение locks **и**
authority/freeze checks; это не отдельный счётчик чистого ожидания mutex.
Фоновые threads и network handlers не входят в scheduler wall counters.

Baseline worker SHA256
`055ea8da0a50c1ae4f9f17532355d32e9b38866c9b50e9d6cb3517d2eb9bbb29`;
candidate `19cb080b832abcfc635d22985b0cc98c38f3e302fa9dde03504f28deaa293f65`.
Candidate в compiler cache является build artifact, не установленным runtime
и не опубликованным immutable delivery package.
ZIP `winbridge-20261026T112928Z-d683199-6ced72c4.zip`, SHA256
`2a2f3c470909809e83033416738f21deb53122816525048c513f6e1ccf6279ff`,
manifest SHA256
`1884450bf8f8844d3d3c7d04acc4931bc9f257919050ed68dc8c833f8be43310`.

После прогона installed worker остаётся d052c6c/generation66c20413, active0,
maintenance/recovery errors отсутствуют. Readonly StorageUsage:306 tasks,
25 imports,1 workflow,1 application; journal20525056/WAL4231272 bytes.
Тестовые child завершены, UI не запускался; working worker оставить работающим.
Его окончательная проверка потребует запуска исправленного executable, замена
installed runtime/config/journal в этой задаче не делалась.

Linux:83 library tests passed/3 ignored и выбранные integration suites
idle/runtime/leases/queue/resource/work scheduling/FSM/workflows passed.
Fmt/scaffold285/Python syntax passed. Новые Linux native benchmarks не запускали.

Повторное наблюдение **того же terminal task**, без submit/build:

```sh
python3 scripts/run-windows-perf-probe.py --observe .winbridge/run-speed/20261026-113022-1c313991/submission.json
```

Raw report/logs: `/mnt/hgfs/shared/WinBridge/run-speed/20261026-113022-1c313991`.
Для повторной проверки hashes использовать `verified_files` из
`scripts/summarize-update-timings.py`: он проверяет sizes/SHA и bytes дважды,
сравнить report SHA с pin выше. Новый `--probe` создаёт новый эксперимент,
не использовать его для просмотра старого результата.

Перед Linux compile: Debian24,547,356,672 bytes, Windows28,973,785,088 bytes свободно.
Windows probe дополнительно измеряет свободное место перед каждой Cargo командой.
В native report Windows before28,840,718,336/after27,725,246,464 bytes;
Debian после22,965,530,624 bytes. Windows каталоги не удалялись.
