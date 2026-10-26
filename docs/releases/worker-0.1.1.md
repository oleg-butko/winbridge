# Локальный Windows worker0.1.1

Ветка слита в main: merge50ceb0d, review fixes2b3734c.
[Результаты ревью](../plans/run-speed/review-merge-2026-10-26.md).
Новый native release run20261026-133349-ec2d99c5/taska6a111f7-8350-4d95-b28d-959ef697881d
прошёл стадии regressions/CPU, но network handshake не завершён. Сценарий сообщил
ошибку соединения и завис на cleanup. **ZIP выпуска не опубликован.**
13:52:30Z task всё ещё running/cancelRequested=true; отмена запрошена ровно один
раз. Это не terminal failed и не разрешение повторить build. Worker d052 остаётся
connected/maintenanceError=null/recoveryError=null, generation прежняя; active1
относится к этой незавершённой проверке. Controller .203:1143 и observer оставлены.

Readonly process diagnostic task8a85df70-3caa-4ae3-a132-8b82168586d9 succeeded/0:
root pwsh23220 и тестовый worker17496 оставались живыми. Тестовый путь:
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\releases\worker-0.1.1-20261026-133349-ec2d99c5\winbridge-worker.exe`.
Запрошено наблюдение пользователя о специальном файерволе; причина пока
**не подтверждена**. Системные/firewall настройки не менялись. Новых Windows
сборок/прогонов до выяснения и подтверждения terminal состояния не запускать.

Source50ceb0d чистый. Metadata
`.winbridge/releases/20261026-133349-ec2d99c5/submission.json`, observer session72424,
log `.winbridge/releases/review-0.1.1/native-build.log`; pending info/process/cancel
receipts в той же review-0.1.1 папке. После перезапуска сначала get_task по ID.
В новом source исправлены unbounded WaitForExit/EOF waits и добавлен вывод последнего
controller connection до cleanup. Это изменение ещё **не выполнено на Windows**;
оно не меняет уже работающий старый task и не доказывает остановку PID17496.
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
