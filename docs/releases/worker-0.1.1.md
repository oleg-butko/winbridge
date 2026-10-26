# Локальный Windows worker0.1.1

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
с сохранением прежнего exe для отката и прежнего config/state/journal. Для
runtime с installed-set manifest заменять один exe нельзя: требуется согласованный
набор и штатная проверка manifest. Эти guards не обходятся и не ослабляются.

После успешной сборки: дождаться отсутствия работ, Ctrl+C в окне старого worker,
сохранить его exe, скопировать новый worker, запустить с тем же выбранным
`K:\_proj-26\winbridge\runtime\winbridge-worker.snapshot.local_204.json`.
Замена работающего exe/остановка через его же managed build task не выполняется.
В этом сценарии финальный перезапуск требует действия в Windows консоли.
