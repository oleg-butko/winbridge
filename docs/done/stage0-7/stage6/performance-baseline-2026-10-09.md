# Baseline этапа 6 — 2026-10-09

Автоматический Windows прогон `20261009T093357Z-0dd53063` завершён:
12 samples (3 повтора small/many-files/one-file-change/large-blob), 12 imports и
47 tasks, всего 59 операций. Publisher clean source `1e01b8e`; installed worker
clean `58f3c14`, generation `41623e38-adb1-4a34-a2c4-3a43d9c1cf2c` сохранилась.
Report: `/mnt/hgfs/shared/WinBridge/performance/20261009T093357Z-0dd53063/performance-report.json`,
719618 bytes, SHA-256 `a6d886b28d8c4ecf50ba0ca80b8c14ecb78722d663d11570f1fd006be56b16cd`.
`ready.json`/readback совпадают; passed=true, error=null.

## Измерения

Медианы трёх samples, миллисекунды; debug Rust builds, без очистки OS/HGFS cache.
Acceptance→PID включает полную проверку immutable snapshot перед командой.

| Fixture | Files / source size | Capture+ZIP | Shared write | Publication total | Windows import | First build | Warm build | Observed time-to-first-build |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| small | 6 / 6168 B | 95 | 19 | 212 | 219 | 2172 | 472 | 2888 |
| many-files | 518 / 2103325 B | 5636 | 220 | 6377 | 9657 | 5240 | 2886 | 22161 |
| one-file-change | 518 / 2103325 B | 5702 | 249 | 6556 | 8821 | 4915 | 3281 | 21179 |
| large-blob | 7 / 33560605 B | 3770 | 3188 | 13061 | 14689 | 7157 | 5150 | 35043 |

Many-files tree probe: Debian guarded copy/write 1758 ms + readback 1244 ms;
Windows guarded copy 16057 ms + local verification 377 ms. Это microbenchmark,
включающий directory creation/path checks/fsync; его нельзя выдавать за полноценный
alternative adapter. Большой blob: copy/readback tree 477/276 ms, Windows 468/377 ms;
production archive дополнительно тратит время на capture, SHA-256, verification,
journal и generation publication. Прямое сравнение разных слоёв не доказывает
выигрыш SocketDelivery или отказ от archive.

Info latency: 1983 samples, median 14.4 ms, p95 28.2 ms, max 203.2 ms.
Управляемая tree task действительно вывела child PID; explicit cancel достиг
cancelled за 221.6 ms (observed RPC/polling time). Ровно 1048576 bytes на каждом из
stdout/stderr: SHA-256 `8f990ba0b577b51cf009ea049368c16bbda1b21e1b93be07a824758bb253c39b`.
Cursor/offset/byte count проверены; чтение 2 MiB по JSON страницам заняло 4161 ms.

Stored output вырос с 518684 до 2627096 bytes; task count 35→82, import count 9→21.
Journal 1458176→3678208 bytes, WAL 16512→4157112 bytes. Эти global наблюдения
включают metadata, не являются per-task disk usage; shrink/retention не выполнялись.
Windows: 24 logical CPUs, Windows 10 build 19045, PowerShell 7.6.5,
Cargo/rustc 1.97.1 MSVC. Toolchain/build identities и raw samples есть в report.

Final worker здоров, activeTaskCount=0, generation прежняя, journal/maintenance
errors отсутствуют. Тестовых окон не было; desktop можно завершить, worker оставить.
Новые snapshots/caches/reports/history сохранены. Manual gate не требуется.

## Первая оптимизация

Измерения показали 5.6 s capture+packing для 518 files при 0.22 s shared write.
Нынешний capture запускает `git cat-file -s` и `git cat-file blob` на каждый файл.
Первый bounded шаг — один `git cat-file --batch` process: запрос/ответ одного blob
за раз, bounded header/size/streaming SHA-256, exact newline framing и kill/wait
при ошибке. Snapshot manifest/ZIP/ready и Windows RPC остаются прежними.

Проверка: empty/binary/newline-containing blobs, отказ malformed/truncated/oversized
frames, Git source scope/dirty tree/link/immutable publication regressions. Затем
alternating matched old/new capture на тех же Git commits и проверка точного ready
совпадения с baseline packages; после — один полный автоматический Windows repeat
с новым publisher. Не обходить launch-time source hashes ради ускорения.

Delta/content cache/resume/output batching/UI virtualization остаются последующими
задачами; новые adapters пока не обоснованы. Эта запись не закрывает весь этап 6.
