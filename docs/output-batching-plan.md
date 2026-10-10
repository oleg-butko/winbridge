# Этап 6: пакетное чтение и ограниченный просмотр output

Сценарий выбран 2026-10-10 после ревью delta. В baseline чтение 2 MiB output
занимало 4161 ms. Прежний ReadOutput выдаёт только 16 journal rows, независимо
от их размера. Desktop удерживает все byte arrays и декодирует их заново при
каждом poll; это делает расход памяти зависимым от всей квоты worker.

Новый ReadOutput сохраняет прежние DTO и sequence cursor. Страница ограничена
192 KiB raw bytes, 1024 rows и 900 KiB encoded chunks; сообщение вместе с
обёрткой остаётся меньше transport limit 1 MiB. Большая одиночная историческая
row выдаётся только если помещается в encoded budget. Неотданная row не
продвигает cursor. Запись output остаётся durable, без фоновой очереди потерь.

Desktop декодирует страницы один раз, отдельно для stdout/stderr, удерживает
последние 65 536 Unicode символов каждого потока и точный loaded byte count.
Разрыв offsets/sequence обозначается явно и сбрасывает UTF-8 decoder на границе,
чтобы не склеивать отсутствующие bytes. Конец decoder подтверждается только после
terminal task и пустой страницы. При backlog следующий poll выполняется сразу,
с короткой уступкой event loop; задачи, cancel и свежесть worker остаются доступны.

Output view виртуализирует строки: scroll/keyboard/filter/pause работают над
загруженным хвостом. UI сообщает ограничение и число неотображаемых символов;
worker journal не очищается. Смена задачи и retention сбрасывают локальный буфер.

Проверки: split UTF-8, gaps, cursor reconnect, tiny rows, worst-case JSON bytes,
quota/drain, реальный IPC на Windows, output hashes и сравнение исходного worker
с новым в отдельных state directories. Installed runtime не заменяется. Gates
используют существующий worker; desktop и новые окна не нужны. Unexpected ошибка
останавливает прогон с сохранённым request/report. Resume доставки остаётся следующим
сценарием и не считается готовым по результатам этого изменения.
