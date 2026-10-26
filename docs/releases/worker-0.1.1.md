# Локальный Windows worker0.1.1

Ветка слита в main: merge50ceb0d, review fixes2b3734c.
[Результаты ревью](../plans/run-speed/review-merge-2026-10-26.md).

## Постоянный сетевой test runtime — 2026-10-26

Пользователь подтвердил блокировку exe из каталога с run ID. Для сетевой проверки
теперь используется **`K:\_proj-26\winbridge\runtime-test\winbridge-worker.exe`**.
Это отдельный слот, не установленный `runtime\winbridge-worker.exe`.
Разрешение файервола понадобится для этого постоянного пути; правила автоматически
не меняются. Fresh config/state/instance/report каждого теста по-прежнему изолированы.
Старые каталоги не переименовываются и не удаляются.

- Test controller: **ws://192.168.44.203:1144**, working controller остаётся1143.
- Ожидание разрешения/соединения: **900s (15min)** по умолчанию. CLI
  `--firewall-wait-seconds 1800` даёт30min (допустимо60..3600).
- RPC получает отдельные120s после connection; общий task budget увеличен на
  connection allowance. **Startup240s самого updater и сохранённые120s attempts
  не меняются.** Статус ожидания печатается каждые15s.
- `--test-runtime-directory` позволяет выбрать другой постоянный локальный путь;
  default получается как sibling runtime-test от фактического installed runtime.
  Относительные/UNC/traversal и пересечение с production runtime запрещены.
- Ownership marker, exclusive lease и read pin защищают слот. Неизвестный каталог
  с файлами не присваивается; live test worker не заменяется/не запускается вторично.
  В слоте только binary/marker/lease, без journal/config; пользовательские данные
  и рабочие executable не переносятся сюда. После подтверждённого exit слот остаётся
  для следующей версии с тем же executable path.
- Native policy test проверяет6 свойств: защита production, чужих файлов, lease,
  write pin, hash mismatch и live process; также проверяет reuse того же пути.
  Independent publication verifier связывает эти проверки, stable path/hash/budget
  с actual worker payload и новым RPC. Python4 passed; native check ещё впереди.

Старый run20261026-133349-ec2d99c5/taska6a111f7-8350-4d95-b28d-959ef697881d
теперь **terminal cancelled** (`Managed process tree cancelled`), не passed.
Рабочий d052/generation301be6ea снова connected/active0/errors=null17:38Z.
Старый observer завершился; его namespace не replay. Новый gate допускается только
после fresh Info/space/source pins. ZIP0.1.1 пока не опубликован.

## История предыдущего прогона

Source50ceb0d был чистым. Metadata
`.winbridge/releases/20261026-133349-ec2d99c5/submission.json`,
log `.winbridge/releases/review-0.1.1/native-build.log`. Он прошёл стадии regressions/
CPU, но network не подтвердился из-за блокировки временного exe. Cleanup долго
оставался unconfirmed, затем cancel подтверждён. Не выдавать это за успешный
release proof; исходные файлы и отчёты сохраняются. Source74ae95e ограничил waits
и добавил connection diagnostic; реальное выполнение исправления — в новом gate.
Старые записи о perf-ветке ниже — история подготовки, не инструкция запрещать merge.

Подготовка2026-10-26 в `perf/run-speed-2026-10-25`. Собирается worker-only пакет;
main worktree и установленная Windows пара не заменяются. Статус native build
**ещё не подтверждён**. Метаданные workspace/npm/desktop синхронизированы0.1.1;
это не означает выпуск новых desktop/controller/updater executable.

## Изменения

- Исправлено чтение всей task/import истории каждые25ms: только pending records
  через existing partial indexes, ранний выход при пустой очереди. Workflow FIFO
  checks тоже используют pending queries. История/дедупликация/authority сохранены.
- Scheduler diagnostics каждые30s: sum/max tick, maintenance/recovery/admission/
  workflows/queue/dispatch, pending maxima. Slow windows выводятся автоматически;
  `WINBRIDGE_SCHEDULER_DIAGNOSTICS=1` включает и быстрые окна. Это wall time,
  включая ожидания, а не CPU. Отдельный native sampler измеряет process/thread CPU.
- Startup показывает PID, application version, source commit/dirty и generation.
  `--version`/`-V` без config; существующий `--artifact-info` остаётся чистым JSON.
- Инструменты storage inventory и tmp-marking доступны из исходников; это не
  функция автоматического удаления каталогов внутри worker.

Сравнение с installed d052c6c показывает также дополнительные private updater
FSM/journal/control проверки и latency fixes. Они не превращают незавершённый
normal local service/controller/lifetime transfer в рабочую публичную установку.
Public install остаётся выключенным, validation feature в пакет не включается.
Новые Cargo test/desktop профили — инструменты прогонов; private desktop opt0
не переносится на пользовательский UI. Worker local dev profile сохраняет
debug assertions/overflow checks. [Два controller](../plans/multiple-debian-controllers.md)
— план, не capability0.1.1.

## Проверка и применение

Linux worker lib и выбранные runtime/scheduler/lease/workflow/FSM regressions
прошли с0.1.1, включая no-config version/clean artifact JSON test.
Перед компиляцией Debian22,969,991,168 / Windows28,255,289,344 bytes free.

`scripts/build-worker-release-native.py` берёт clean exported ZIP, проверенный
original compiler cache и retained updater verifier/baseline worker. Он не
компилирует или заменяет installed controller. Из `/proc/5312/exe` сохранён именно
фактически работающий controller0.1.0 (его source identity dirty/2aac553, это
существующий binary, а не утверждение clean source). Его копия работает с новой
отдельной config/state/instance/port. Windows кандидат проходит regressions/CPU
и реальный RPC этого controller: submit, same-key duplicate, stale scope refusal,
terminal success и Unicode output. Все команды и оригинальные requests сохраняются.

Native builder собирает без `update-validation`, копирует worker из compiler
cache в независимый frozen файл, проверяет роль/build/source/hash и тестирует
этот frozen exe в отдельном Windows state. Старый config только валидируется,
новый CLI читает Info работающего старого worker. Новый isolated child завершён
перед successful publication; payload, logs и ready hashes проверяются дважды.

Текущий installed runtime **не имеет** `winbridge-installed-set.json`.
Поэтому применим ручной worker-only patch при остановленном старом процессе,
с сохранением прежнего exe для диагностики и прежнего config/state/journal.
Новый worker мигрирует inactive journal16→17 с consistent automatic backup. Старый
worker16 не открывает17: после запуска новой версии нельзя откатить только exe;
после новых задач нельзя автоматически восстанавливать прежнюю БД. Для
runtime с installed-set manifest заменять один exe нельзя: требуется согласованный
набор и штатная проверка manifest. Эти guards не обходятся и не ослабляются.

После успешной сборки: дождаться отсутствия работ, Ctrl+C в окне старого worker,
сохранить его exe, скопировать новый worker, запустить с тем же выбранным
`K:\_proj-26\winbridge\runtime\winbridge-worker.snapshot.local_203.json`.
Замена работающего exe/остановка через его же managed build task не выполняется.
В этом сценарии финальный перезапуск требует действия в Windows консоли.

Первый Windows run20261026-123551-7d4c9904/task1bd56ac8-3d7e-43c2-884c-b916681b5c0b
terminal failed:79 library tests passed,2 network timing tests failed при параллельном
прогоне. Не является релизным proof. На **том же** test executable (SHA256
03d72a938c47d9952e496aaa2459cf4bc09e76823edef501a56c4692df2a08b3)
все3 network tests serial passed12.85s; taskfb4d0d35-198e-41b7-aaf3-940439acecc9.
Release harness переведён на `--test-threads=1`, как основной Windows harness.
Production deadlines не увеличиваются, проверки не исключаются. Новый run имеет
новый source/key/state; старый failed task не replay и не становится passed.

Повторный старый run20261026-124153-18ee88bb также terminal failed: worker
regressions прошли, CPU fixture попыталась открыть baseline рядом с retained
update guards и получила missing updater config. Исправлено отдельной копией
baseline; этот run не становится successful и не replay.
