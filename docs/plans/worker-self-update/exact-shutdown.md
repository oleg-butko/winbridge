# Следующий блок U1: подтверждённая остановка worker

2026-10-20. План реализации после combined drain gate. Код остановки обновлением
пока отсутствует; public update и replacement остаются отключёнными.

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
