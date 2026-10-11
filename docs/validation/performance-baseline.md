# Автоматические измерения этапа 6

`scripts/measure-performance.py` запускается в Debian и использует уже подключённый
Windows worker. Desktop можно завершить; новых окон и обновления runtime нет.
Запросы/команды/результаты видны в обычном журнале задач, scripts выполняются из
проверенного snapshot с читаемым `-File`, без EncodedCommand.

```bash
PATH=/home/user1/.local/bin:$PATH cargo build -p winbridge-controller --locked
python3 scripts/measure-performance.py \
  --controller-config .winbridge/controller-snapshot.local.json
```

Для повторяемого сравнения defaults: 3 повтора, 512 файлов по 4096 bytes, один blob
32 MiB. `--repeats 1 --small-files 8 --blob-mib 1` — короткий smoke, а не полноценный
baseline. Максимум 10 повторов/2048 files/64 MiB. Проект по умолчанию
`snapshot-fixture`; запускайте только в зарегистрированном тестовом проекте.
Worker должен быть здоровым, без активных tasks/imports/apps этого fixture.

## Как проходит сценарий

1. Создаёт новые Git fixtures в `.winbridge/performance/<run>`: small, many-files,
   large-blob; каждый repeat имеет свой commit. Старые исходники не меняет.
2. `controller measure-publish PROJECT HEAD` выполняет прежний verified publisher
   и возвращает ready marker с отдельными monotonic timings. Обычный `publish`
   и формат snapshot/RPC сохранены; reuse явно отмечается и исключён из новых samples.
3. Worker импортирует ZIP из shared, проверяет bytes/SHA-256 и строит Cargo offline
   в новом `project/performance/<run>/<sample>/build-cache`. Второй build использует
   тот же cache. Запуск binary проверяет Unicode marker доставленного исходника.
4. На тех же commit bytes отдельно измеряет tree write/readback в HGFS и Windows
   copy/local verification. Это I/O microbenchmark; журнал/lease/verified generation
   production import им не заменяются. Прежние snapshots не изменяются.
5. После изменения одного padding file снова доставляет полный ZIP: baseline для
   будущего delta. Он использует cache предыдущего many-files sample; новый snapshot
   path может заставить Cargo перестроить package, поэтому это не «warm unchanged».
6. Сохраняет ровно 1 MiB stdout + 1 MiB stderr и читает их cursor pages с проверкой
   bytes/hash/offset. Измеряет explicit cancel собственной long task с реальным child,
   Info latency во время работы, размеры журнала/WAL/output до/после.
7. Публикует `WinBridge/performance/<run>/performance-report.json`, проверяет readback
   bytes/SHA-256 и записывает `ready.json` последним. Report содержит raw samples,
   task/import IDs/events, build identity и параметры toolchains/CPU/OS.

Exact mutation request сохраняется локально до RPC. Неизвестный результат или ошибка
останавливает прогон; automatic submit retry отсутствует. Даже failure report содержит
последний подтверждённый ID/key и `passed=false`; перед повтором нужна reconciliation.
Готовые старые reports/snapshots/caches/history не удаляются. Тест не выполняет retention.

## Как читать результаты

Capture и ZIP packing interleaved в нынешнем publisher: `capturePackMs` суммарный.
`localVerifyMs` — SHA-256/manifest, `sharedWriteMs` — HGFS copy/sync,
`sharedReadbackMs` — повторная проверка payload, `publishReadyMs` — metadata/ready barrier.
Их сумма не превышает `totalMs`; CLI wall time дополнительно включает process startup.
`reusedPackage=true` означает verification готового пакета, а не новый transfer.

Worker `acceptedAtMs/startedAtMs/finishedAtMs` используются только в пределах Windows
для acceptance→PID и PID→terminal durations; первый интервал включает launch-time
проверку snapshot. Observed wall time в Debian включает RPC/polling (150 ms), не
отнимает Windows timestamp от Debian clock. Native tree durations используют Stopwatch.
OS/HGFS cache не очищается; «первый build» означает новый Cargo target directory,
не обещает cold OS cache. Median/p95 публикуются с количеством и raw latency samples;
малое число repeats не доказывает статистическую значимость.

Объёмы journal/WAL и output — read-only наблюдение global worker, не размер отдельной
task. Metadata/attention/background могут менять эти числа; SQLite free pages
переиспользуются и физический файл может расти иначе, чем output. История и dedup
сохраняются. Выбор delta/resume/batching/virtualization делается после baseline.

## Сравнение Git capture

`scripts/compare-capture.py --baseline-report <local performance-report.json>
--reference-controller <original binary>` alternates original/new publisher на
точно тех же commits и сверяет весь ready с immutable baseline package. Оба
алгоритма заново capture/pack перед проверкой существующего shared payload; reuse
не выдаётся за новый transfer. Сравнивается только `capturePackMs`, 3 pairs на каждый
из 3 исходных many-files commits. Source repo HEAD может быть уже изменён — запрос
использует сохранённый commit, не mutable HEAD. Comparison report сохраняется с
readback/SHA-256/ready; Windows команды не отправляются.

Git batch читает один bounded header/body за раз, проверяет blob object/type/size,
вычисляет прежний SHA-256 и требует точный newline separator. Empty/binary bytes
не интерпретируются как строки. Размеры файлов/aggregate и ZIP bounds сохранены;
при раннем отказе процесс Git закрывается/kill/wait, partial stage не публикуется.
