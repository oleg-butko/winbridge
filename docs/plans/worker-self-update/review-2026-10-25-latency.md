# Review и диагностика задержек — 2026-10-25

Пользователь поручил review последних изменений, исправления и продолжение updater.
Надёжность важнее скорости. Он явно разрешил увеличить startup window вдвое:
**новые admissions используют 240000ms**. Прежнее требование сохранять120s
отменено для новых попыток; immutable intent старого run не переписывается.

## Проверенный исходный результат

Run `20261025-205856-79e1b7c0`: failed/1, 190 tests passed, Main24 failed.
53 ready files дважды сверены по bytes/SHA; report SHA
`d6b7ba81af899bc2de6480f8297b5e13c9c6e9f5a0021f5efdb3c78dcf0b9437`.
Working runtime не изменялся. Детали — в [прежнем checkpoint](resume-2026-10-25-journal-activation.md).

Из сохранённых Cargo logs: примерно512.24s reported build time и832.50s test-body
time. Это сумма объявленных Cargo/test durations, не замер wall clock всего run.
Main24 занимает715.65s; он включает сборку target и весь путь до writer failure.
Сравнение нельзя выдавать за matched benchmark новой реализации.

В supervisor trace bind_files занял43883ms. На20 checkpoints пришлось20412ms
physical custody checks и14871ms graph checks;23 graph reads require_prepared
заняли17166ms. Эти вложенные интервалы перекрываются: складывать их нельзя.
Launch/handoff/writer раньше не имели сопоставимой детализации.

## Изменения review

- Новый startup default240s; никакого renewal при новых фазах/observe, старые120s
  по-прежнему читаются. Portable FSM regression проверяет обе границы.
- Independent writer verifier получает ожидаемое окно из проверенного source ZIP,
  а не из самого proof. Legacy defaults остаются120s, старые reports читаются.
- Test observer больших EXE и production capability scan (`worker_rehearsal::supports`)
  использовали `slice.windows(...).any(...)`, хотя artifact pin уже перешёл на memchr.
  Оба поиска теперь используют memchr; весь SHA-256 и File ID/sharing проверки
  сохранены. Regression проверяет границу64KiB, конец файла, отсутствие и пустой marker.
- Structured timings охватывают Main fixture, launch, custody graph/physical,
  handoff, receiver, writer opening/checkpoints/admission/delivery/confirmation.
  Содержат PID, elapsed/step/remaining и process-wide artifact pin hash calls/bytes/time deltas.
  Счётчики вложенных/параллельных операций нельзя суммировать как уникальную работу.
  Диагностика не содержит nonce/authority ID/config и не даёт grant.
- Windows command report содержит startedAt/elapsedMs/cargoBuildMs/testExecutionMs;
  stdout показывает начало/итог каждой команды. Длительности разбираются также
  при failed Cargo. Решение pass/fail по-прежнему зависит от exit/proofs/counts.

Пока full graph/hash checks сохранены. Их безопасное сокращение требует отдельного
evidence: held File ID и deny-write/delete pins не разрешают кешировать mutable
SQLite authority/root или считать новую связь подтверждённой.

## Проверки и продолжение

Source checks завершены: workspace783/0 (16 ignored) до последнего marker regression;
после него focused activation55/0 и marker3/0. Linux/MSVC workspace all-target
validation Clippy, fmt/diff, Python25/0 и scaffold279 passed. Native PS parse-only
task `c86a2216-41a8-429b-9a8a-676a800684cd` succeeded/0, scriptExecuted=false.
Старый physical132 proof перепроверен readonly:45 files twice/40 state mutations.
Reference pin восстановлен из retained actual report/terminal task, не из нового запуска.
Cache owner report5 files twice, SHA `22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`,
его task failed, четыре compiler commands exit0. Это только compiler cache.
Disk admission task `ec6006f3-5cb5-4cfa-aa25-004b8d0eed19`: Windows13527228416 /
Debian24930435072 bytes, оба≥6GiB. Installed worker connected/idle/errors=null.
Новый native run впереди; positive Main24 не заявлен.

Clean receiver commit `24136539e0b1c542c48c90bace4bf975bed6c69c`:
`winbridge-20261025T232657Z-2413653-bf3f5db3.zip`,825 files, ZIP SHA
`0ed054c007a0c2b6ee035a71471c97176b15cee0043192ef09b701fcbb694c9f`, manifest
`563efa9a7380da00c8d89a3bf15faac9d45ea18958ee63029fb5cb9222e97613`.
Archive-derived startup default240000ms проверен. Следующий docs-only commit
даёт отдельный producer; оба содержат reviewed code19/17/private3.

## Активный native retry

Run `20261025-232746-26bad6d7`, task `a49ade1f-4e35-4eff-84ac-3b8d21c0a9ea`
принят и наблюдается; **не resubmit**. Producer
`acf712313405c0e6be04b91893f8ed927772f1ed`, ZIP
`winbridge-20261025T232727Z-acf7123-7d95795d.zip`, SHA
`7e8ccc681c030f72194c9359cf646111dd7720bce58652d105c263fbdcc26008`.
Target2413653 выше. Cache `20261017-061527-f4639971`; reference old Main23
`.winbridge/workflow-validation/20261025-100933-d8a50807/activation-preparation-verified.json`.
Observer log `.winbridge/update-preparation/review-writer24-native.log`.
Метаданные task/request/cursor сохранены локально; Windows runtime не меняется.

Для retained terminal report доступна readonly сводка:

```bash
python3 scripts/summarize-update-timings.py /mnt/hgfs/shared/WinBridge/workflow-validation/RUN
```

Она дважды проверяет ready bytes/SHA и выводит command durations и самые долгие
timing edges. Для старых logs отсутствующие hash counters равны null, не0.
Разные операции вложены: их суммы не являются временем всего run.
Следом — clean distinct producer/receiver, actual Main24 + обе migrations,
independent audit. После полного success: normal service/reconciliation в том же
child/Store/lock → новая controller session → durable Job lifetime/commit/scoped
release → remaining U2 → U3/U4 → A4.3.

В этой сессии Debian IP изменился на192.168.44.204. Controller работает на
`ws://192.168.44.204:1143`; installed d052 worker generation
`66c20413-63a9-4725-b4c1-f5d97cbfac51` connected/idle/errors=null.
Windows config пользователь выбрал `winbridge-worker.snapshot.local_204.json`.
Runtime/config/journal Windows не заменялись. Изменён только Debian local listen
с verified backup, путь и restore уже сообщены пользователю.
