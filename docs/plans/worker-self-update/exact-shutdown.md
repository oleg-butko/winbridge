# Следующий блок U1: подтверждённая остановка worker

2026-10-20. Реализация после пройденного combined drain gate. Добавлены типизированные
данные и два журнала; доставка команды остановки и native gate ещё впереди. Добавлен trusted OS
exit/lock adapter, который пока не проверен на настоящей Windows. Public update и replacement остаются отключёнными.

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
