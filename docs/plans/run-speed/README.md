# Оптимизация скорости прогонов

2026-10-25, America/New_York. Пользователь сначала приостановил updater и поручил
подготовить эксперимент. Затем поручил **продолжать до существенных результатов
в отдельной ветке**, чтобы другой агент мог продолжать updater в `main`.
Здесь только оптимизация; последний исходный checkpoint updater —
[writer24 + service data foundation](../worker-self-update/resume-2026-10-25-writer-verified.md),
source58e6109. Schema19/17/private3 и public install off сохраняются.

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

## Статус

План и эксперимент подготавливаются. Реального E1 A/B результата ещё нет;
ускорение не заявлено. Эта ветка не открывает schema20/private4/actor25;
updater развивается независимо в `main`.

Основания инструментов: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html),
[CLI config overrides](https://doc.rust-lang.org/cargo/reference/config.html#command-line-overrides),
[Cargo JSON artifacts](https://doc.rust-lang.org/cargo/reference/external-tools.html#json-messages),
[build timings](https://doc.rust-lang.org/cargo/reference/timings.html).
