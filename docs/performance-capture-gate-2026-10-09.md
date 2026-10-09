# Git batch capture: измерения и Windows gate

Первая оптимизация этапа 6 завершена. Clean source `b4d355a` заменяет два Git
процесса на файл одним sequential `cat-file --batch`, не меняя manifest/ZIP/ready
или Windows RPC. Baseline и выбор описаны в [предыдущем отчёте](performance-baseline-2026-10-09.md).

## Сопоставимое сравнение

Run `20261009T095122Z-095becc1`: три исходных many-files commits × три pairs;
old/new order чередуется. Original binary source `1e01b8e`, new `b4d355a`, оба clean
debug builds. Оба заново пакуют те же Git objects, затем проверяют существующий
immutable shared package. Mutable HEAD в fixture уже содержит one-file change,
но request явно выбирает исходный commit. Все 18 ready совпали с baseline,
включая payload и manifest SHA-256, размеры и snapshot ID.

| Capture+pack, 518 files | Median | p95 / max | Samples |
| --- | ---: | ---: | ---: |
| Original | 7004.6 ms | 7822.8 ms | 9 |
| Git batch | 515.6 ms | 532.8 ms | 9 |

Снижение 92.64%, ускорение 13.58× именно capture+pack. Shared reuse не считается
новым transfer; нет заявления о таком же ускорении всего Windows pipeline.
Report: `/mnt/hgfs/shared/WinBridge/performance/20261009T095122Z-095becc1/capture-comparison.json`,
18643 bytes, SHA-256 `59043c75788028b202f4dc3fe722bac3201f24e7c1b35e86fe098d2832ffe7c2`.
Readback/ready подтверждены, passed=true, error=null.

## Новый Windows repeat

Run `20261009T095310Z-d5a9d432`: 4 fresh samples (small/many-files/one-file-change/
large-blob), 4 imports + 17 tasks = 21 actual Windows operations. Publisher clean
`b4d355a`, installed worker clean `58f3c14`; runtime не заменён. Каждый import
подтвердил files/bytes, offline builds и запуски вывели delivered Unicode marker,
tree probes проверили hashes; stdout/stderr по 1 MiB проверены cursor/offset/hash.

Fresh many-files publication: capture 466.5 ms, total 1283.4 ms. Observed
time-to-first-build 20311 ms. Один repeat с OS/HGFS caches и изменчивым Windows I/O
не доказывает статистического выигрыша всего pipeline: большой blob достиг
41809 ms против baseline median 35043 ms, а import/launch verification остаются
существенными затратами. Доказанный выигрыш относится к matched capture выше.

Info: 755 samples, median 19.4 ms, p95 36.2 ms, max 481.1 ms; RPC/worker errors нет.
Explicit cancel собственной task с observed child PID: cancelled за 49.2 ms.
Здесь проверяется backend доступность/control/cancel; popup latency не измерялась,
desktop закрыт. Attention regression tests прошли вместе с workspace.

Report: `/mnt/hgfs/shared/WinBridge/performance/20261009T095310Z-d5a9d432/performance-report.json`,
266843 bytes, SHA-256 `1b8d7e9d0ca0a7f95430a9cb31988b8cfe16f4113e5d69da4333228317d5ea2c`.
Readback/ready совпали, passed=true, error=null. Final installed generation
`41623e38-adb1-4a34-a2c4-3a43d9c1cf2c` прежняя; activeTaskCount=0,
journalRecoveryRequired/maintenanceError=null. TEST окон не было. Desktop можно
завершить, рабочий worker оставить запущенным. Manual gate не нужен.

## Regression и дальнейшие границы

123 workspace tests, Clippy all-targets `-D warnings`, Windows all-targets compile,
fmt, Python compile и PowerShell parser прошли. Новые tests проверяют bounded
headers, wrong object/type/size, truncated/separator frames, binary/newline/NUL/
empty/repeated objects через настоящий Git и import. Existing scope/link/dirty
tree/mount loss/cancel/corrupt payload/immutable publication проверки сохранены.
При раннем отказе batch закрывает stdin и kill/wait; partial staging не публикуется.

UI ревью отдельно исправило crash JSON редактора на неверных nested types;
actual browser regression/typecheck проходят, новое manual/native UI подтверждение
не назначалось. Исходники для следующего обновления включают этот fix.

Дальше — отдельный измеренный сценарий delta/content cache. Он должен сохранять
verified immutable generations, exact commit/result provenance, source checks,
cancel/recovery и историю. Resume/output batching/virtualization/SocketDelivery
ещё не реализованы; этап 6 полностью не закрыт. Старые данные не очищались.
