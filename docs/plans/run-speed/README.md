# Оптимизация скорости прогонов

**Раунд 2 завершён 2026-10-26:** [Cargo matrix и private release desktop](round2-2026-10-26.md).
Fresh desktop build118.5625→41.7945s(64.75% быстрее); полный native201 tests/56 files
twice/78 negative mutations passed. Сумма команд21.409→20.266min — наблюдаемое,
не строгое end-to-end A/B сравнение. Оптимизация включена только при private
ValidationCacheDirectory; production release argv прежний. `main` независим.

2026-10-25, America/New_York. Пользователь сначала приостановил updater и поручил
подготовить эксперимент. Затем поручил **продолжать до существенных результатов
в отдельной ветке**, чтобы другой агент мог продолжать updater в `main`.
Здесь только оптимизация; последний исходный checkpoint updater —
[writer24 + service data foundation](../worker-self-update/resume-2026-10-25-writer-verified.md),
source58e6109. Schema19/17/private3 и public install off сохраняются.

**2026-10-26: E1 подтвердил ускорение activation suite на 42.83% при более дорогой
первой сборке.** [Измерения, причины задержек и точные pins](results-2026-10-26.md).
V2 оставляет updater opt0 и оптимизирует остальные три packages: median
33.7865→18.3635s,45.65% быстрее. Полный E2 passed:201 Windows tests,56 ready files
twice,78 negative audits. Profiles заданы только в Cargo.toml этой ветки.
Defaults `main` не менялись.

## Исходные измерения

Verified Windows run `20261025-232746-26bad6d7`:193 tests/56 ready files twice,
44+16+18 negative audits. Сумма command wall time1445.78s (~24.1min), Main751.03s
снаружи Cargo (~12.5min); отдельная сборка old runtime204.30s. Main включает
target compiler216.16s. Runtime preparation39.11s, replacement131.90s, binding44.22s.
20 custody checkpoints binding повторно хешировали~2.23GB за14.49s;23 graph
checks заняли17.07s. Вложенные интервалы нельзя складывать как независимые.

## Приоритеты

| Этап | Что пробуем | Условие |
| --- | --- | --- |
| E1 — первый | Умеренная оптимизация компиляции logic/JSON на существующих activation tests | Один source snapshot, одинаковые checks/features/порядок внутри suite; никаких runtime изменений. |
| E2 | Та же кандидатная настройка на полном native gate | Только если E1 даёт устойчивый выигрыш; профиль должен дойти до всех Cargo invocations, включая nested target build. |
| E3 | Сгруппировать builds по features и запускать заранее собранные test binaries | Сохранять production/validation варианты, точные test sets и provenance каждого executable. |
| E4 | Pinned old-runtime fixture для подходящих сценариев | Не переиспользовать её при изменении проверяемого old Main или несовместимом контракте. |
| E5 | Один validated graph внутри одной SQLite read transaction | Не кешировать mutable owner/root/epoch между transactions и перед эффектами. Нужны отдельные failure tests. |
| E6 | Параллельность независимых тестов | После остальных измерений; environment/IPC/Job/crash/cache tests остаются serial, пока изоляция не доказана. |

Первым выбран E1: обход графа и сериализация выполняются преимущественно в
неоптимизированном Rust-коде. Это гипотеза о CPU затратах, не доказанный источник
всего времени replacement. Изменение opt-level не удаляет проверки и обратимо.
Перегруппировку сборок оставляем вторым направлением: она затрагивает feature
unification и выбор frozen supervisor/test images, поэтому требует больше изменений.

## E1: эксперимент профиля

Спецификация: `scripts/experiments/update-profile-v1.json`.
Runner: `scripts/test-windows-update-profile.ps1`; без `-Execute` он только
показывает план. Обычный Cargo.toml, global config и installed runtime не меняются.
Ветка `perf/run-speed-2026-10-25`, worktree `/home/user1/winbridge-run-speed`.
Основной `/home/user1/winbridge` оставлен на чистом `main` для реализации updater.

- A: opt-level0 для updater/core/protocol/serde_json.
- B: opt-level1 для тех же четырёх packages. Существующие sha2/memchr opt-level2
  сохраняются. Другие settings одинаковы: debug2, assertions=true,
  overflow-checks=true, CARGO_INCREMENTAL=0, serial tests.
- Один clean manifest-bound export и одна локальная cache lease; immutable source
  проверяется до/после builds и в конце. Cargo JSON подтверждает фактический профиль,
  features и source identity; executable каждой стороны копируется и удерживается
  readonly. Различающиеся binary hashes/build IDs ожидаемы и записываются.
- `cargo test -p winbridge-update --lib --features bundled-sqlite,update-validation`
  с `--no-run --message-format=json --timings`. Запускаются существующие tests по
  filter `activation`; обязательные имена и minimum count закреплены в spec.
  Списки A/B должны совпасть, каждый measured run — весь список,0 failed/0 ignored.
- Отдельный ignored policy test проверяет **реальные** debug assertions и runtime
  overflow panic в собранном бинарнике. Это инструмент эксперимента, не updater feature.
- После каждой первой сборки — повторный Cargo build с теми же аргументами для
  измерения warm/no-op build. Первая сборка в унаследованном cache записывается как
  observed setup cost; её нельзя называть честным cold A/B сравнением.
- Один прогрев A и B; затем ABBA BAAB (4 пары,4 samples на сторону). Тестовые
  binaries запускаются напрямую: измерение execution не содержит скрытого Cargo.
- Сохраняются toolchain, profile evidence, executable bytes/SHA, exact test names,
  command arguments, wall durations, libtest durations, warm-build cost и raw logs.
  Cargo timing HTML копируется отдельно для каждой сборки.

Именованные package overrides дополнительно задают одинаковые debug/assertions/
overflow/incremental settings. Это касается и serde_json как build dependency:
не полагаться на отличающиеся defaults build-override. Поэтому A — контроль
этого эксперимента, не точная копия settings всех прежних native builds.

E1 promising: все checks passed, median test runtime B хотя бы на15% ниже A,
минимум3 из4 пар быстрее и spread каждой стороны≤20% median. Это **только pilot**.
Оценка warm build+test выводится отдельно; цена первой компиляции учитывается как
наблюдаемая стоимость подготовки. E1 не доказывает сокращение полного gate24min.
Для принятия default нужны E2, cold/rebuild сценарий и полный native proof.

## Безопасный запуск

Нужен только installed worker; desktop и тестовые UI окна не нужны. Перед каждым
запуском fresh Info/idle/error check, отсутствие active Cargo и оба диска≥6GiB;
записать измерение Debian вместе с task metadata. Windows runner повторяет local
disk admission и берёт тот же `.winbridge-build-cache.lock`, что обычный native gate.
Отдельный новый work directory — sibling source directory в том же isolated run.
Он не должен существовать. Нет cleanup, force или resume того же work directory.
Unknown task сначала reconcile; новый key не разрешает повтор неизвестных эффектов.

Source сначала export из clean commit через `scripts/export-source.py`. Следующая
сессия создаёт новый unique isolated run под snapshot-fixture/workflow-validation,
копирует checksum-verified ZIP на local Windows disk и распаковывает в `winbridge`.
В параметры runner передаются **SHA manifest этого ZIP**, source и новый sibling
`profile-v1-YYYYMMDD-HHMMSS-xxxxxxxx`. Cache — retained native test-build-cache;
его owner report/ready bytes и terminal task сверяются по прежнему checkpoint.
Запуск через зарегистрированный worker сохраняет request/key/task ID до ожидания.

```powershell
& .\winbridge\scripts\test-windows-update-profile.ps1 `
  -SourceDirectory .\winbridge `
  -ManifestSha256 EXACT_EXPORTED_MANIFEST_SHA256 `
  -WorkDirectory .\profile-v1-YYYYMMDD-HHMMSS-xxxxxxxx `
  -CacheDirectory 'K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261017-061527-f4639971\test-build-cache'
```

Команда выше — **plan only**. `-Execute` запускает сборки/тесты. Результаты только
после terminal/report.ready.json с двойной bytes/SHA проверкой. После прогона
desktop можно закрыть, installed worker оставить; validation directories не удалять.

Debian driver `scripts/run-update-profile-native.py` принимает `--archive`,
`--controller-config`, `--cache-owner-task`, `--cache-report`,
`--cache-report-sha256`. Он проверяет owner terminal/ready bytes, свежие Info и
Debian disk, сохраняет request до submit и task ID до ожидания. Наблюдение через
`--observe PATH_TO_SUBMISSION_JSON` никогда не делает submit. Новый Windows work
directory и shared `WinBridge/run-speed/<run>` создаются один раз; ready переносится
последним. Полный запрос и task state остаются в ignored `.winbridge/run-speed`.

```bash
python3 scripts/run-update-profile-native.py --observe .winbridge/run-speed/RUN/submission.json
python3 scripts/verify-update-profile.py /mnt/hgfs/shared/WinBridge/run-speed/RUN
python3 scripts/test-update-profile-experiment.py
```

## Статус

2026-10-26: Windows syntax/plan-only preflight succeeded/0,
task `2a55798e-0224-4a53-a8d5-144c01e9b37a`; ни Cargo, ни test binaries не запускались.
Portable verifier: 5 tests, включая 16 независимых подмен, CRLF/collation, selective profile и
noisy/insufficient-gain decisions, passed. Rust fmt passed. Реальный E1 succeeded/0:
run `20261026-040152-f3f5757a`, task `07cfe524-cee4-4b1e-b3c1-a525ac2528bc`;
источник `6035edde28d8362003c96cfee36b9a76f820efe9`, ZIP
`winbridge-20261026T040058Z-6035edd-55e7ad58.zip`, SHA
`a08927a933cbfc95e2c1109c74969793e7cbcb5af8fbd4096af7301bfdc3df2d`.
Перед запуском Debian24,566,149,120 bytes, предыдущий fresh Windows preflight
38,828,167,168 bytes; runner сохраняет собственный fresh admission до каждой сборки.
Owner cache terminal failed, четыре compiler commands succeeded; это только cache,
не proof тестов. Task `ac5724f1-c38a-4a50-a446-f9b57ae5473b`, report SHA
`22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`.
E1: 65 tests × 10 suites + 2 policy probes passed;25 ready files twice.
Median36.165→20.6745s, spread<3%,4/4 pairs faster. Exact diagnostic130/130 passed,
131 ready files twice. Default/full-gate speedup не заявлен.
Followup v2 run `20261026-042205-115786e9`, task
`974c9b0c-86c4-42a2-a688-9f9e2fba26d5` succeeded/0:25 files twice,650 suite
cases +2 policy probes. Для анализа передать `--spec scripts/experiments/update-profile-v2.json`.
V2 сохранён в экспериментальном Cargo.toml для E2, не в `main`.
E2 run `20261026-043444-d41c13aa`, task `c184f27a-7c52-499e-8109-e89cfa246580`
**succeeded/0 и independently verified**:201 tests,56 files twice,44+16+18 mutations.
Сумма команд21.41min против прежних24.10min — наблюдение, не строгое end-to-end
A/B из-за различий source/test set/cache history. Nested compiler всё ещё214s.
Все задачи terminal; installed worker healthy/idle. [Итог и точные pins](results-2026-10-26.md).
Эта ветка не открывает schema20/private4/actor25;
updater развивается независимо в `main`.

Основания инструментов: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html),
[CLI config overrides](https://doc.rust-lang.org/cargo/reference/config.html#command-line-overrides),
[Cargo JSON artifacts](https://doc.rust-lang.org/cargo/reference/external-tools.html#json-messages),
[build timings](https://doc.rust-lang.org/cargo/reference/timings.html).
