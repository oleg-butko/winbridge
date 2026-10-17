# U1: свежий SourceControl и проверка файлов без общего mutex

2026-10-17. Продолжение [worker launcher/control](worker-control.md).
Добавлен readonly IPC adapter для существующей [source transaction](source-transaction.md).
Он пока не подключён к автоматически выполняющему apply supervisor: public update,
compiler driver, shutdown и runtime install остаются открытыми.

## Свежий статус

`WorkerSourceControl` получает frozen binding из trusted supervisor spec и путь
worker config из installation policy. Он проверяет точный SHA config, строит обычный
worker endpoint из проверенного config и запрашивает новый Info при каждом вызове.
Перед отправкой kernel peer PID сверяется с binding; PID/birth/host/session проверяются
до и после запроса. Generation, полный BuildIdentity и journal context сверяются
с ответом. Inspection/recovery/maintenance error запрещают source effects.

Адрес не принимается из JSON доказательства; отдельный lease-control endpoint
не используется для больших Info. Обычный frame остаётся ограничен 1 MiB, весь
запрос — 10 секундами. Нет повторной отправки или автоматического перехода к новому
worker. Adapter вызывается на blocking thread и использует переданный Tokio Handle.
Он не создаёт source lease: существующий driver дополнительно проверяет SourceHeld,
owner/epoch, manifest pins и revision перед каждым файловым действием.

## Чтение файлов и разрешение записи

До долгого чтения baseline worker выполняет короткую transaction: exact scope/revision,
SourcePending и отсутствие accepted work/owners в пересекающихся проектах.
SourcePending уже запрещает новые roots этих проектов. При пустом accepted set
нет workflow child, который мог бы начать пользоваться исходниками во время scan.
Read-only scan выполняется без `Store` и `active_projects` mutex.

Это не grant и не резервирование work slot. Другие проекты могут выполняться;
если они заняли budget, итоговый grant отклоняется. После scan worker снова берёт
обычные locks, проверяет startup guard, текущую authority/config/phase, revision,
логические/физические blockers и в одной transaction резервирует slot + SourceHeld
receipt. Ошибка scan или устаревшая authority оставляет SourcePending; к исходникам
нельзя применять изменения. Новый исходный command не пересылается автоматически.

Verified release аналогично читает файлы без общего mutex, затем снова проверяет
scope/revision/authority и снимает только свою lease. Terminal/unsafe parent phases
по-прежнему не разрешают release. Между updater и worker нет общей transaction:
свежая внешняя проверка не заменяет fencing обоих journals и последующую live-lease
проверку source driver. Named-resource policy пока явно откладывает source/build.

## Проверки

- Реальная source transaction через worker IPC применяет тестовые Add/Replace/Remove,
  достигает Verified/Building и отклоняет другой generation/build/incarnation/birth
  и изменение config bytes.
- Управляемая пауза verifier: настоящий Info IPC завершается до продолжения scan;
  lease остаётся Pending, occupied slots — 0. Изменённая authority после scan
  отклоняет grant. Свежий запрос после этого проходит обычные проверки.
- Прежние source/crash и worker coordination tests проверяются повторно.

10 worker update/control, 13 source/crash и 3 coordination regressions и Clippy passed.
Windows/tests cross-check и fmt/script syntax passed;
[native gate clean 3e01e56](windows-fresh-source-gate-2026-10-17.md): 82 tests /
13 groups и полный candidate build прошли. Следом — compiler attempt/OS Job,
cleanup proof, автоматическое выполнение source driver и exact shutdown; установка
рабочего Windows runtime до U1–U3 gates не разрешена.
