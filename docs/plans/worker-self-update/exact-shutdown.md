# Следующий блок U1: подтверждённая остановка worker

2026-10-20. Реализация после пройденного combined drain gate. Добавлены типизированные
данные и два журнала; доставка команды остановки и native gate ещё впереди. Добавлен trusted OS
exit/lock adapter; его process/file примитивы прошли Windows gate ниже, полный
stop/ACK/exit сценарий ещё не проверен. Public update и replacement остаются отключёнными.

## Реализованный слой данных

`apps/update/src/shutdown.rs` хранит immutable binding: update/launch/owner/epoch,
Quiesced root revision, SHA полного drain snapshot, worker generation/config/build,
точную loaded identity и file identities state directory/journal/instance lock.
Команда связана с binding каноническим SHA. Pure FSM разделяет Prepared, Issued,
Uncertain, Acknowledged и Exited. Прочитанный JSON не создаёт разрешения на действие.

Updater schema12 хранит intent, current record и append-only history. Admission
атомарен с root Stopping; Issued сохраняется до выдачи одноразового delivery permit.
Повтор возвращает сохранённые данные. ACK не переводит root в Replacing. Публичный library API принимает Exited/ShutdownVerified только с несериализуемым
`ShutdownExitGuard`: pinned process handle, полученным при жизни процесса, занятой
исходной instance lock и проверенными runtime pins. JSON-флаги не создают этот
guard. Receipt и root Replacing записываются в одной транзакции; runtime files
при этом не меняются. Доставка stop и новый worker ещё не подключены.

Worker schema16 хранит immutable acceptance receipt, связанную с оригинальной
ConfirmQuiesced receipt. Fresh acceptance выдаётся только после commit; duplicate
читается как история и не выдаёт новое разрешение. Последующее изменение lease не
переписывает историю. Новые обычные lease changes/release блокируются после
acceptance; отдельного activation/recovery contract пока нет. Cold inspection
использует Store readonly API без Worker construction и recovery marker.

Миграции создают consistent SQLite backup. Updater migration fences старого owner,
не создавая остановку для прежней операции; worker15 с active update lease
отклоняется до migration writes. Schema15/11 reference binaries сохранены на
Windows для будущей проверки отказа старых executables. Legacy control v1–v5 и
supervisor v1–v13 не получили скрытой команды остановки.

Проверки ledger покрывают CAS, immutable history, reopen/lost ACK, rollback при
ignored write и отсутствие fabricated exit authority. Эти проверки не подтверждают
реальную остановку процесса: она будет проверена отдельным native gate ниже.

## Что требуется доказать

Есть два разных факта: worker принял конкретную команду и именно этот процесс
завершился, освободив владение runtime. Квитанция первого не доказывает второе.
Отсутствие named pipe, timeout или похожий PID не разрешают замену файлов.

В текущем `serve_inner` уже есть подходящая последовательность: завершить network
owner, закрыть control server, дождаться его blocking callbacks и активных jobs.
`Worker::stop()` только устанавливает flag. Нужно подключить к этому существующему
пути authenticated update command, сохранив receipt до ответа. Не создавать второй
независимый shutdown loop и не закрывать control server из его собственного callback.

## Последовательность

1. Проверить свежий Quiesced той же drain-попытки, immutable worker spec,
   owner/epoch, live process/config/runtime identity и завершённые helper/compiler
   процессы. Нулевые counters из caller payload не являются разрешением.
2. В updater journal атомарно сохранить root Stopping и immutable shutdown intent;
   delivery Issued — до RPC. Закрыть raw BeginShutdown/ShutdownVerified за private
   driver guard. Повтор saved intent только наблюдает результат.
3. Добавить отдельный версионированный control contract для остановки. Worker
   проверяет сохранённый intent в updater и собственный Quiesced, сохраняет точную
   immutable receipt в своём журнале до ACK. Новые таблицы требуют явной миграции,
   consistent backup и old-worker refusal; legacy v1–v5 не расширять скрытно.
4. После попытки отправить ответ выйти через существующий stop path. Принятую
   команду не терять при разрыве канала; callback должен завершиться до освобождения
   instance/runtime locks. Независимый supervisor продолжает жить вне worker Jobs.
5. При lost ACK читать только точную receipt. Если worker уже завершён, потребуется
   bounded readonly inspection через pinned worker verifier: без `Store::open`,
   миграции, scheduler, controller и acquisition нового execution owner. Updater
   не должен самостоятельно становиться writer worker DB. Отсутствие receipt не
   даёт права повторить shutdown или автоматически запустить worker.
6. Проверить exact PID/birth/host/session exit и освобождение instance/runtime
   владения; сохранить physical receipt вместе с root-переходом, допускающим
   следующий этап. Новый worker не запускается и файлы пока не заменяются.

После Issued отсутствие ответа остаётся indeterminate до доказанного reconciliation.
До этой границы возможен прежний coordinated abort; после остановки требуется
отдельный recovery/rollback путь. Нельзя пытаться снять lease через уже завершённый
worker или удалять startup guard ради прохождения теста. Обычный startup остаётся
заблокирован активной update lease до реализации проверенного activation contract.

## Автоматическая проверка

Проверять actual worker Main и независимый supervisor в изолированном runtime,
без installed update и GUI. Сохранить schema15 reference до перезаписи build cache.
Нужны: receipt-before-ACK, lost ACK без второго request, stale context/epoch/revision,
PID reuse, задержанный callback/instance lock, отказ записи receipt и чтение после
exit. Supervisor обязан оставаться живым. Удаление marker, force kill чужих процессов
и ранний replacement не являются способами завершить gate.

После green drain gate сначала проверить реальные interfaces stop/control/instance
locks, затем реализовать data FSM, два журнала и driver одним вертикальным пакетом.
Native gate этого блока должен проверять завершение процесса; компиляции и чистой
FSM для этого недостаточно. Полная установка, desktop save/exit и controller
confirmation остаются следующими U2/U3.

## Проверенные interfaces

Instance lock — `stateDirectory/worker.lock`, открываемый в `Worker::open_inner`
и удерживаемый полем `_instance_lock`. Текущий startup проверяет update guard
до его acquisition и повторно под lock. Recovery startup здесь не подходит:
`Worker::open_recovery_file` сохраняет persistent recovery marker, хотя journal
открывается readonly. Для нового cold receipt verifier следует использовать
отдельный bounded reader, опираясь на `Store::open_read_only`/existing admission
header, без Worker construction и без изменения recovery metadata.

## Подтверждение OS exit — реализация, native gate впереди

`PinnedProcess` в platform удерживает Windows handle конкретного process object,
проверяет birth/host/session и читает завершение через WaitForSingleObject. Ошибка
доступа не становится положительным доказательством. Debian не имитирует эту
capability: acquisition явно Unsupported. После падения updater утраченный handle
не восстанавливается из отсутствующего PID; автоматическое продолжение этим не
разрешается.

`ShutdownStatePins` открывает существующие state directory, journal.sqlite и
worker.lock без create, проверяет File IDs/links и удерживает Windows handles без
share-delete. Получение exclusive instance lock — отдельный шаг; оно не доказывает
exit само по себе. Guard проверяет прежний manifest, exe identities и process
identity ещё раз перед commit. Эти handles удерживаются вызывающим кодом до
завершения journal transaction. Сохранённые доказательства читаются как история,
без восстановления physical permit. Schema12/16 при добавлении adapter не меняется.

Промежуточный автоматический gate `test-update-shutdown-primitives.ps1` проверяет
19 тестов в пяти группах: live process pin/exit, data/state/journal, worker receipt,
отказ worker15 от16 и updater11 от12. Запуск из Debian —
`validate-update-shutdown-primitives-native.py`: чистый ZIP с проверенными bytes,
существующий отдельный Cargo cache, свежие fixtures и checksum report в shared.
Он не собирает desktop/candidate, не останавливает installed worker и не заменяет
будущий combined actual Main stop/ACK/cold-verifier gate.

[Primitive gate 2026-10-20 пройден](windows-shutdown-primitives-gate-2026-10-20.md):
19 native tests, пять групп, девять checksum artifacts. Это не подтверждение полной
самоостановки actual Main; новый wire contract/driver и cold verifier ещё впереди.

## Worker control v6 — реализация 2026-10-21, combined gate впереди

Внутренний control v6 разделяет ObserveShutdownState, RequestShutdown и
ObserveShutdownReceipt. Worker сам вычисляет file identities и сверяет instance
lock с удерживаемым handle. Request требует точного сохранённого Issued/Uncertain
intent в updater, root Stopping, owner/epoch, loaded runtime/config, Quiesced lease
и отсутствия логических/физических исполнителей. Duplicate возвращает историю;
сохранённый updater ACK при отсутствующей worker receipt не разрешает новую остановку.

Fresh Store permit переходит в StopAfterReply: он принадлежит результату blocking
callback, затем попытке ответа. После её завершения или отказа доставки guard
устанавливает existing stop flag. Serve loop закрывает network/control и ждёт
callbacks/jobs. Callback не вызывает control.close изнутри себя. Никакая
десериализованная response DTO не создаёт этот guard. Положительный путь должен
пройти combined native Main gate; одних отрицательных проверок недостаточно.

Версия каждой команды задаётся одним required_version(), общим для routing и
worker admission. Старые команды остались в1–5; generic supervisor v1 forwarding
отклоняет новые наблюдения/stop. Нужны новый consuming supervisor contract/driver
и bounded cold verifier. Public auto-update по-прежнему выключен.

## Cold receipt verifier — реализация 2026-10-21

Internal CLI `--inspect-update-shutdown INPUT_JSON SHA256` обрабатывается до
обычного startup. Input связан с immutable intent, worker config evidence и
проверенным старым helper image. Reader сверяет SHA/identities, открывает только
current-schema SQLite readonly и читает оригинальную receipt. Не конструирует
Worker, не приобретает execution owner/instance lock, не мигрирует и не меняет
startup/recovery markers. Отдельный ShutdownStateObservation не может превратиться
в LockedShutdownState. Report содержит helper process/image, config/state evidence
и optional receipt; он не утверждает exit worker.

Actual CLI на Debian проверен с synthetic intent metadata: чтение при занятой
instance lock и после освобождения fixture owner, сохранение main DB/authority и
pending guard, отсутствие recovery marker, rejected foreign state/different attempt/
bad input SHA. Вместе с coordination/rehearsal прошли 11 entry tests. Это не
Windows stop proof: reader client/consuming supervisor driver и combined gate ещё
нужны. Для reader используется no-effect query через pinned image и bounded owned
probe, по аналогии с artifact-info; прочитанная история не восстанавливает owner
или process handle, не отправляет повторную команду и не запускает worker заново.

Updater client `inspect_saved_receipt` удерживает pinned input/config/old image,
проверяет отдельный capability marker до запуска, получает stdout через существующий
bounded atomic probe/Job и сверяет точный helper PID и весь report. Input содержит
исходную journal schema; неизвестный результат не создаёт stop/exit permission.
Проверены actual CLI/client на Debian и подмена report identities/readonly flags.
Новый `--cold-reader-only` режим Windows runner проверяет CLI/client и v6 contracts
в трёх группах (4 теста), без повторения 19 уже пройденных primitive tests.

[Cold reader Windows gate пройден](windows-shutdown-reader-gate-2026-10-21.md):
4 теста/3 группы,7 проверенных artifacts. Дальше — consuming supervisor driver и
combined actual Main stop. Readonly preflight должен сохранять responsiveness
mailbox, затем durable admission предшествует ACK. После выхода ExitGuard нужно
удерживать в ephemeral custody supervisor до U2; stored Exited не восстанавливает
физическое владение после сбоя. View отделяет подтверждённую историю от текущего
владения, неизвестный исход не разрешает resend/startup или удаление guard.


## Supervisor v14: consuming driver — 2026-10-21

Новый внутренний mailbox `ShutdownCandidate` / `ObserveCandidateShutdown`
возвращает view schema6. В нём отдельно видны readonly preflight, durable Prepared,
ожидание acceptance, ожидание process exit/instance lock, исторический Exited
и наличие удерживаемого OS guard в живом supervisor. Старые v1–v13 не получают
поле `candidateShutdown`. JSON `physicalCustodyHeld` сообщает наблюдение и сам
не даёт разрешения на замену файлов.

Preflight выполняется на blocking executor: повторная проверка source/config,
health, candidate pins, прежнего runtime, live compatibility, process handle,
worker-derived state identities и свежего Quiesced. Readonly inspector старого
worker должен поддерживаться **до** admission. Mailbox остаётся доступен для
наблюдений и coordinated abort. ACK исходного запроса выдаётся после атомарного
admission; потеря этого ACK не отменяет уже сохранённую попытку.

Затем Issued сохраняется до единственного RequestShutdown. Потеря ответа вызывает
только точное readonly наблюдение receipt, при необходимости через pinned cold
inspector. Ни повторного stop, ни force kill нет. После acceptance driver ждёт
выхода удерживаемого process object и acquisition исходной instance lock в пределах
60 секунд. Atomic Exited + root Replacing требует несериализуемого guard.
Supervisor сохраняет этот guard после завершения blocking driver; U2 должен
использовать именно его. После падения supervisor исторический Exited остаётся
фактом, но утраченные handles не восстанавливаются из JSON.

Это внутренняя реализация. Положительный сквозной native gate с завершением
нормального worker-процесса ещё требуется. Обычный Main пока не включает update
control и запуск supervisor; validation harness не должен выдавать отдельный
синтетический Worker за полную проверку Main. Public update, replacement,
activation/rollback и финальный consistent worker DB backup перед replacement
ещё не реализованы.


## Подготовка actual Main gate

Feature `winbridge-worker/update-validation` включает только изолированный fixture
startup. Обычные build/install scripts эту feature не используют. Без неё Main
не читает validation env и не включает update control. С ней нужны SHA-проверенный
input рядом с отдельным runtime, свой source/state и отключённый controller.
Main по-прежнему проходит обычные runtime guard → Worker::open_file → serve.
Fixture task запускает независимый supervisor, выполняет source/build/health/drain,
передаёт shutdown и завершается до выхода Main. Он не держит Arc<Worker> при
ожидании stop и не подменяет факт выхода сигналом stop flag.

`update_shutdown` integration запускает четыре случая: обычный ACK, потерянный ACK,
задержанный callback после durable receipt и отказ записи receipt. После выхода
Main parent ждёт atomic Exited, проверяет живой supervisor и занятый instance lock,
читает receipt настоящим cold CLI, проверяет неизменность четырёх прежних exe и
отказ обычного Main restart из-за активной lease. Затем завершает только свой
validation Job: история остаётся, текущий OS guard уже отсутствует. Каталоги
с журналами/guards сохраняются; release через мёртвый worker не выполняется.

Запуск: существующий `validate-update-health-actor-native.py --shutdown-main-gate`
с чистыми source/target ZIP и прежним cache run. Оба ZIP проверяются до remote
execution; shared report и process proof должны входить в checksum ready manifest.
Выбранный target может быть более ранним проверенным commit с теми же schemas:
это проверка shutdown и same-schema health, а не обещание upgrade/migration.
Пока native результат этого сценария не получен, block остаётся открытым.
