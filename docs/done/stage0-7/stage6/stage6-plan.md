# Этап 6: измерения до оптимизации

Первый сценарий — воспроизводимый baseline существующего commit → verified ZIP →
HGFS → Windows import → offline Cargo build. Установленный worker используется
через прежние RPC; desktop и новые тестовые окна не нужны. Настройки, exe и журнал
установленного runtime не заменяются. Все записи — новые fixture snapshots,
task/import records и каталоги `WinBridge/performance/<run>`.

## Что измеряем

- Git enumeration, capture/ZIP packing, local payload verification, shared write,
  shared readback и ready publication отдельно. Capture и packing сейчас
  interleaved, поэтому их нельзя честно разделить без изменения алгоритма.
- Маленький buildable проект, множество маленьких files и большой blob;
  параметры/seed/commit/hashes сохраняются. Повторы имеют новый commit и snapshot,
  чтобы не выдавать reuse готового пакета за новый transfer.
- Actual Windows import с ID/status/events/bytes, первая сборка в новом cache и
  повторная сборка в том же cache, запуск полученного fixture.
- Отдельный HGFS tree copy/readback microbenchmark на тех же bytes. Это сравнение
  файлового доступа, не альтернативный production delivery adapter.
- Повтор полного пакета после изменения одного файла: baseline для будущего delta,
  без реализации удаления sources в этом сценарии.
- Read-only Info latency во время import и build, explicit cancellation отдельной
  long fixture, объёмы task output/journal/WAL до и после прогона. Background
  activity и cache state указываются; файл SQLite может не уменьшаться после cleanup.

Monotonic wall clock используется только внутри одного хоста. Worker timestamps
используются для queue/execution duration; разность Windows и Debian wall clock
не выдаётся за latency. Polling completion включает погрешность периода опроса.
Results содержат raw samples, не только средние, и source/build identity обеих сторон.

## Проверка и продолжение

Сценарий сохраняет submission до RPC и не повторяет неизвестный результат мутации.
Любая неожиданная ошибка worker останавливает прогон с retained IDs/report.
Report публикуется create-only с bytes/SHA-256 readback и ready marker последним.
Изменять или удалять старые snapshots/reports/history нельзя.

После baseline выбирается одна оптимизация с измеримой целью и той же regression
проверкой. Baseline/Git batch и delta/local source cache теперь завершены;
[Pinned chunk/resume](windows-resume-gate-2026-10-10.md) теперь также завершён; SocketDelivery по измерениям не требуется. [Output batching/virtualization](output-batching-plan.md)
выбраны 2026-10-10 и [прошли automatic gate](windows-output-gate-2026-10-10.md).
Этап 6 завершён; следующий — interfaces/profiles этапа 7. Новый ручной Windows gate не нужен; unexpected
worker behaviour требует остановки и уточнения по инструкции пользователя.

## Следующий сценарий — delta

2026-10-09 пользователь выбрал delta от проверенного snapshot как локального
кэша. [Отдельный контракт](snapshot-delta-plan.md) зафиксирован до реализации;
[инструкции](../../../guides/snapshot-delta.md). Baseline/Git batch завершены; новый schema 2
[прошёл отдельные tests и automatic Windows gates](windows-delta-gate-2026-10-09.md).
Output batching/virtualization также завершены; [resume также завершён](windows-resume-gate-2026-10-10.md); SocketDelivery остаётся условным расширением по новым измерениям.
