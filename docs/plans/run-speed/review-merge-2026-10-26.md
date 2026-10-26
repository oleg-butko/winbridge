# Ревью и слияние performance-ветки, 2026-10-26

Задача пользователя: проверить `perf/run-speed-2026-10-25`, слить в main и
выпустить новый Windows worker. Проверен remote tip `87fc22a`, общий предок
`58e6109` — текущая foundation local-service FSM. Она сохранена.

## Вывод по runtime и профилям

Исправление CPU принято: pending queries используют существующие partial indexes
schema13, сохраняют rowid/FIFO и видимость queued/running. История, idempotency,
authority/freeze/recovery, workflow advancement и проверки capacity остаются.
Ранний выход происходит после workflow/recovery checks и только при отсутствии
queued tasks/imports. Schema17 не меняется. Diagnostics агрегированы по30s,
wall time явно отделён от process CPU; команды/вывод не попадают в агрегаты.

Профили core/protocol/serde_json оптимизируют dev/test, сохраняя debug assertions,
overflow checks и debug info. Updater остаётся opt0. Private desktop opt0 включён
только через ValidationCacheDirectory; обычные release/custom-protocol argv
сохранены. Storage helpers маркируют/измеряют каталоги, не удаляют их.
Планы multi-controller и normal local service не объявляются готовыми.

Повторно прочитаны ready bytes старого full run20261026-094754-aa840d26:
56 файлов, pinned report SHA из round2, independent audits44 replacement +16
writer +18 migration mutations. Это существующий native201 proof, не новый run.
CPU report20261026-113022-1c313991:5 файлов и pinned SHA сверены, passed=true.
Прежние source ZIP уже отсутствуют в shared; они не пересоздавались и не выдаются
за повторно проверенные ZIP. Historical source/target pins взяты из Git round2
и связаны с сохранённым report/proof. Native release заново проверит hashes
реально используемых retained executables на Windows.

## Существенные замечания, исправленные до выпуска

1. Release CPU baseline запускался прямо из displaced runtime с соседними update
   guards. Сохранённый второй release run124153 завершился именно из-за отсутствия
   winbridge-update.local.json, а не CPU регрессии. Теперь запускается independent
   copy с тем же SHA и без соседнего installed-set/config; reference не меняется.
2. Путь config был зашит как _204 и зависел от generated preflight другого агента.
   Теперь обязательный --preflight передаёт hash и фактический config запущенного
   worker. В этой сессии это _203. Installed-set manifest проверяется на Windows
   перед работой и после неё; worker-only пакет не обходит coordinated-set policy.
3. Publication доверяла passed/exit флагам без проверки всей selection и frozen
   identity. Новый independent verifier связывает9 suite results + native CPU,
   exact Cargo flags, оба CPU samples, payload/hash/descriptor и build процесса,
   проверенного по RPC. Проверяются strict bool/int и finite CPU values.
4. Network handshake читался по самому появлению HGFS файла. Incomplete read теперь
   повторяет только чтение того же marker; completion/meta пишутся с fsync/rename.
   Original request сохраняется до dispatch; неизвестный исход не replay.
5. Cleanup при неудачном Process.Start мог сам упасть на HasExited до публикации
   диагностики. Отдельный started flag сохраняет правильный cleanup/report path.
6. Инструкция предлагала старый exe как rollback без условия схемы. Теперь явно
   описана inactive16→17 migration с consistent backup: старый16 не откроет17,
   после новых эффектов нельзя автоматически возвращать старую БД или один exe.

## Проверки и выпуск

На review source: workspace **794 passed /0 failed /17 ignored**, Linux all-target
validation Clippy, fmt, TypeScript/UI build, Python profile5/desktop3 и новые
publication3 passed. MSVC all-target validation Clippy также passed.
Новые publication regressions отвергают12 видов подмен, включая подменённый
бинарник/build, пропуск suite/CPU, numeric success и неверный baseline SHA;
отдельно проверены нечисловые/несогласованные CPU measurements.

Windows preflight task6b4a566c-8372-481c-a9d5-f31c2af2c4a3 succeeded/0:
installed d052/version0.1.0, manifest отсутствует, free26731397120 bytes;
Debian до compile24653250560 bytes. Controller снова запущен на .203:1143.
Для cross-version теста сохранена именно копия /proc/5103/exe controller0.1.0,
а не новый binary из target; её source768422d dirty — наблюдаемый старый build,
не утверждение clean source. Hash f69cdcc61d53759e7b3899b2272d877136eb5fd1d4c05d7e4fba7ac94c0b28e1.

Ветка слита в main с review-fix commit2b3734c; история экспериментов сохранена.
Далее выполнить чистый export с built UI, затем build-worker-release-native.py
с новым --preflight. Нужен только установленный worker; desktop/UI не запускаются.
Тестовый worker работает в отдельном state/instance и соединяется с изолированной
копией прежнего controller. Runtime пользователя не заменяется managed task.
Native результат и пакет фиксируются в [release note](../../releases/worker-0.1.1.md).
Автоматический updater и multi-controller этим выпуском не включаются.

## Неожиданное поведение нового native release

Run20261026-133349-ec2d99c5 из merge50ceb0d прошёл стадии regressions/CPU,
не подтвердил old-controller connection и завис на остановке нового worker.
Cancel запрошен; actual task всё ещё running/cancelRequested=true13:52:30Z.
Никакой release ZIP не опубликован; повтор до terminal запрещён. Запрошено
наблюдение пользователя о специальном файерволе, причина не доказана.
Новый source заменяет unbounded WaitForExit и ожидания EOF на ограниченные
ожидания с unconfirmed cleanup/error, выводит last connection перед teardown.
На текущий frozen script исправление не влияет, native recheck ещё не выполнен.
Все exact IDs/paths/handles и следующий порядок — в release note.

## Исправление по ответу пользователя: постоянный executable

Пользователь подтвердил firewall block; taska6a111f7 теперь terminal cancelled.
Сетевой test worker перемещён в отдельный постоянный runtime-test рядом с runtime,
с exclusive lease/owned marker/read pin и запретом заменять live process/production
или чужие файлы. State/report остаются fresh per-run. Default test port1144,
connection allowance900s, RPC120s отдельно, parent task получает этот allowance.
Updater240s/старые120s contracts не меняются. Python publication4 passed;
actual Windows policy6 и новый release ещё не выполнены. Путь и CLI в release note.

Fixed-path run20261026-180118-1321e697 подтвердил140 Rust/6 native policy tests,
actual RPC/dedup/stale-scope/Unicode и cleanup. Финальный guard остановил публикацию:
наш policy fixture без cwd создал runs.log в source. Cwd теперь explicit scratch,
readiness проверяет log там, source final validation сохраняется и независимо
сверяется. Старый failed snapshot не чистится; повтор новым clean producer и
namespace, network executable path тот же.
