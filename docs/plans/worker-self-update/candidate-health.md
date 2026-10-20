# Проверка кандидата с журналом и отменой

2026-10-20. Внутренняя реализация U1: updater schema 10, supervisor v10,
preparation view schema 4. Public update, drain/shutdown, замена exe и bootstrap
этим блоком не включаются. Полный сквозной native gate health-driver ещё требуется;
отдельные DTO-тесты и standalone rehearsal не заменяют этот gate.

Сквозной run20261020-103249-4030e35e выявил реальный дефект: rehearsal
использовал обычный console readiness banner в stderr, поэтому strict health
правильно отклонял даже exit0. Теперь пробный server сообщает readiness через
IPC и typed stdout report, обычный server сохраняет banner. Ошибки не подавляются.
Standalone inspection также отказывает при stderr, а regression запускает настоящий
exe и проверяет чистый stdout/stderr. Native повтор с новым clean candidate
ещё требуется; прежний green standalone test не доказывал strict actor approval.

## Последовательность

1. После verified source, compiler report, artifact receipt, baseline и пяти
   preimages сохраняется `CandidateHealthAdmitted`. Intent привязан к их digest,
   supervisor launch, owner/epoch, root revision и новому attempt ID. Только свежий
   вызов получает непередаваемый execution permit. Дубликат возвращает историю;
   pending после сбоя никогда не создаёт второй permit.
2. Supervisor возвращает сохранённый статус до исполнения и запускает один
   blocking driver. Status и readonly runtime queries продолжают обслуживаться.
   Compatibility поступает через process-authenticated worker control v3;
   вызывающая сторона не передаёт версии схем и готовые health receipts.
3. Driver удерживает четыре candidate image pins и pin точных bytes worker config.
   Сверяет текущую worker identity с baseline, реальные schemas/config/context,
   source lease, manifest, report и завершение компилятора.
4. В фиксированном новом `runtime/update/<id>/health` запускается candidate worker:
   сначала `--check-update-config`, затем `--rehearsal .../health/worker`.
   Перед каждым CreateProcess сохраняется probe intent, перед Resume — exact
   PID/birth/session/host и authorization. Записанные context/PID/cleanup
   неизменяемы; второй helper разрешён только после cleanup первого.
5. Config helper не открывает worker journal. Rehearsal создаёт новый пустой
   journal/instance/IPC без controller/direct commands, проверяет readiness и
   останавливает сервер. Живой worker journal не копируется и не запускается.
   Вывод bounded; в health row сохраняются типизированные отчёты и cleanup,
   а не сырые stdout/stderr или значения конфигурации.
6. После обоих cleanup снова проверяются source/config/artifacts/live compatibility.
   Typed reports должны совпасть с exact process, descriptor, config File ID/SHA,
   новыми instance/generation/journal incarnation и текущими схемами. Итоговый
   receipt и `CandidateAccepted` фиксируются в одной SQLite transaction. Raw
   lifecycle `CandidateAccepted` и `CandidateHealthAdmitted` запрещены.

## Сбой и отмена

Ненулевой exit, несовместимость или неверный отчёт дают Rejected, а подтверждённая
отмена — Cancelled. Неизвестное создание/завершение/чтение output сохраняет
Indeterminate либо pending при ошибке записи; повторного запуска нет.
`exit 0` без tree/capture/resume proof не является успехом.

При активном health-driver actor сохраняет Aborting через отдельное соединение
с той же физической authority/epoch. Poll останавливает только свой Job; cleanup
разрешён в Aborting, принятие кандидата запрещено. Abort/release выполняется после
возврата blocking driver и дополнительно проверяет terminal health metadata,
exact process birth и пустой Windows Job. При неизвестном исходе barrier остаётся.
Stop и другие изменения не обходят активный driver.

Health view показывает pending/verified/rejected/cancelled/indeterminate, attempt,
revision, оба PID, cleanup и причину. Поля baseline/preimages/health читаются в
одном SQLite snapshot вместе с root/source/build. Старые view 1–3 не получают
нового JSON-поля; worker control v1–v3 не изменены.

## Миграция и следующие проверки

Schema 9 → 10 использует прежние owner lock, consistent SQLite backup и epoch
fencing. Старые операции сохраняются без выдуманного health receipt. Backup не
считается разрешением запускать старый worker на новом журнале.

Добавлены проверки монотонности PID/cleanup, запрета raw acceptance, pending и
unknown cleanup, полей schema/process/isolation, старых JSON projections и миграции
9 → 10. Синтетические DTO-тесты проверяют контракт, а не реальное здоровье exe.

Следующий обязательный сценарий — isolated native v10 actor с полным candidate
bundle: реальные config/rehearsal helpers, быстрый ACK/status, потерянный ACK,
дубликаты, отказ записи PID, смена owner/root и отмена при живом helper. Для этого
можно использовать уже собранный native bundle, явно отметив fixture compiler
receipt; нельзя выдавать такой fixture за production install. До прохождения
сценария не подключать health к публичному обновлению и не объявлять U1 готовым.

Локальная проверка этого блока: 541 workspace test/doctest, Linux и Windows
all-target Clippy, generated TypeScript/check, fmt, UI typecheck/build и scaffold
прошли. Это не утверждение о прохождении сквозного native v10 health gate.

## Сквозной native сценарий

`scripts/validate-update-health-actor-native.py` запускает через уже одобренный
worker `test-update-health-actor.ps1` из нового чистого source archive A. Target
archive C передаётся отдельно; его commit/manifest отличаются от A. Подготовленный
C от `2aac553` включает исправление retained-transition validator. Это реальные
Git-снимки: binary metadata не переименовываются и source SHA не подменяются.

Тестовая копия worker A обслуживает private IPC, supervisor A проходит
baseline/preimages/source/настоящую сборку C/artifacts/health. Private build adapter
добавляет существующий параметр `ValidationCacheDirectory` к реальному PowerShell
build script. Cargo caches переиспользуются; product policy и Main не получают
специальных тестовых переключателей. Старый worker в этом fixture — скопированный
test harness с явно синтетическими дескрипторами остальных старых ролей. Candidate
C и оба его helper entry points настоящие. Активация/установка не выполняется.

Четыре отдельные операции проверяют: отказ записи PID без resume, отмену при
заблокированном v3 callback до helper, запрос отмены при наблюдаемом живом rehearsal
и успешный config/rehearsal с atomic CandidateAccepted. После каждой операции
coordinated abort освобождает только свой source lease и завершает supervisor;
записи/ключи сохраняются, следующий operation ID новый. Старый runtime manifest
сравнивается по bytes. При гонке отмены helper может завершиться самостоятельно;
cleanup сохраняет фактический исход, успех установки из этого не выводится.

Runner проверяет source ZIP до remote extraction (manifest hashes, extra entries,
traversal, case collisions, symlinks), копирует/pin-ит ZIP локально, ждёт именно
принятый Task ID и проверяет ready bytes/SHA и структуру native proof. Тайм-аут
наблюдения не означает остановку task и не разрешает повтор submit. GUI не нужен;
достаточно оставить установленный worker. В output каждые пять секунд появляется
текущая фаза из настоящего updater journal. Пока native результат не получен,
этот сценарий считается подготовленным, а не пройденным.
