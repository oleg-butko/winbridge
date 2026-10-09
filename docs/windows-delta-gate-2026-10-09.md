# Delta этапа 6: автоматические Windows gates, 2026-10-09

Согласованный [план](snapshot-delta-plan.md) зафиксирован до реализации. Delta
использует прежнее verified generation как локальный кэш и создаёт новый полный
target. Исходники, hashes и immutable generation проверяются до каждой сборки.
Независимого object store, cleanup snapshots, resume или SocketDelivery здесь нет.

## Изолированный native gate

Clean source `efeaf652d6df380b58ff258f05992f72f30193a5`, archive
`winbridge-20261009T104701Z-efeaf65-3d5c87ab.zip`, SHA-256
`597002a0ae58ed8ce2aded8f77346a75e1e7b942883d0b333dfb57a8be333fbd`.
Сценарий `validate-workflow-native.py --delta-only` выполнен через installed worker.
Task `f25d23d0-7970-45f1-9b56-77546d3433f7`: succeeded, exit 0.

Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261009-064716-ffce5995`.
`report.json` SHA-256:
`b34285db8c87586ff7655afb513ceea3c03fbc46bcd441d0b8043e25e1a660a8`.
Шесть artifacts проверены по `report.ready.json`: три logs, две proof JSON и report.
40 tests: Windows core 24, Git batch 2, прежняя delivery 8, delta 5, worker delta 1.

Проверены rename/delete/mode/empty files, пустой ZIP, цепочка delta, full/delta
совпадение каждого target file record, actual copied SHA-256, неизменность базы,
неверный pin, отсутствующая/повреждённая база, Windows junction, cancellation,
ZIP path overlap, forged metadata, повтор готового target без базы/shared,
durable acceptance/dedup/restart, offline build/Unicode и tamper до process launch.
Crash между publication и journal result восстанавливает также phase `verified`,
состав, commit и summary базы; build не повторяется.

Windows-generated fixture: 67 target files, full ZIP 2 362 087 bytes, delta 269.
Это correctness case с rename/delete и двумя изменёнными файлами. Его Windows
publisher timings не выдаются за Debian publication performance.

## Debian → shared → Windows: три свежих one-file commit

`validate-delta-delivery.py` переиспользовал уже проверенный test worker из native
run в новом isolated runtime. Controller publisher clean `dcd25d8`; test worker
clean `efeaf65`, manifest SHA-256
`62ad1f89196b42927834cf8e771eeddd9f4481681fcf65c2fba30df6ce86786a`.
Network у тестового worker выключен, desktop/новых окон не было.

Shared: `K:\VMs\shared\WinBridge\delta-validation\20261009T105909Z-c59b2060`.
Outer task `8b5186a7-9bc4-4905-ab2c-2173f266afef`: succeeded, exit 0.
`report.json` SHA-256:
`9cdab83a6c5420f99184fda0039393b684f419a536e3b381d5a0520ad837008b`.
`windows-report.json` SHA-256:
`cac9e0c4e92778c17ccb40d98280f35f193b205678c866686a58aee7ec40679c`.
Profile SHA-256:
`ceabb778dc8a7eea62fdc3ee536508f94b78ec17cecfc99ce13cfb40c6d2b393`.
Шесть top-level artifacts проверены по ready.json; package hashes проверяются
publisher readback и worker import. Windows helper сначала проверен по hash и
скопирован на локальный диск. Submissions сохранены до RPC; неизвестная мутация
не повторяется автоматически.

Каждый commit: 69 target files, 68 файлов / 2 360 889 bytes переиспользуются из
первого full generation, один Unicode text меняется. Все packages новые; full и
delta publication/import order чередуются. OS/HGFS caches не сбрасывались.

| Метрика | Полный пакет | Delta |
| --- | ---: | ---: |
| ZIP bytes, каждый повтор | 2 367 994 | 193 |
| Debian publication, ms | 1150.73 / 1139.33 / 1218.78 | 412.81 / 438.29 / 414.14 |
| Publication median, ms | 1150.73 | 414.14 |
| Capture/packing, ms | 337.64 / 288.24 / 306.11 | 276.66 / 274.21 / 263.20 |
| Observed Windows import, ms | 1887.01 / 1834.00 / 1794.54 | 1563.15 / 1730.07 / 1675.17 |
| Import median, ms | 1834.00 | 1675.17 |

Observed import включает startup локального CLI и polling 80 ms. Разность clocks
Windows/Debian не используется. Это три небольших paired fixtures; уменьшение
ZIP bytes не означает такое же ускорение всего pipeline. Target materialization
и hashing всех files остаются; локальные копии занимают прежний полный объём.
Сравнения больших проектов, build p95 и flushed cold cache здесь нет.

Три actual Windows builds сохраняют точный Debian commit/manifest pin и exit 0;
полученные executable через worker вывели соответствующий `Debian delta 0/1/2 ·
Привет ✓`. Повтор delta с тем же key возвращает прежний Import ID.
Итого 7 imports (base + 3 full + 3 delta) и три dedup повторения без новой
операции; 7 tasks (3 builds, 3 executable runs, 1 tree probe). Шесть source generations
новых commits и base не изменяются или не очищаются.

Installed worker Info: 48 probes, median 19.48 ms, p95 37.47, max 45.19.
Test worker IPC Info: 111 probes, median 36.08 ms, p95 48.07, max 62.12;
включает startup CLI. Actual tree child PID 2484 наблюдался до cancel; parent и
child завершились, cancelled подтвердился за 102.88 ms. Explicit attention
принят за 52.75 ms. Эти probes подтверждают отклик данного прогона, а не общий
статистический предел latency. Новых уведомляющих окон не создавалось.

Installed worker source `58f3c14`, generation
`41623e38-adb1-4a34-a2c4-3a43d9c1cf2c` до/после одинаковы; healthy и
activeTaskCount=0. Installed exe/config/journal не заменялись. Test runtime завершён;
desktop можно оставить закрытым, установленный worker оставить запущенным.

## Linux/UI/review

131 workspace tests, Clippy all-targets `-D warnings`, Rust format, generated
contracts, TypeScript strict, scaffold, UI build и Windows all-targets compile
проходят. Browser с реальными Unix worker/controller проверил delta discovery,
base/hash summary, actual import и потерянный ACK: два одинаковых delta RPC,
один import, новое содержимое и неизменная база. Native desktop заново не открывался;
этот UI проверен browser integration, backend/IPC — настоящей Windows.

Ревью ограничило discovery/history ответ компактной summary вместо полного reuse
map, сохранило формат-1 hashes и прежний import dedup payload, сверило schemaVersion
ready/manifest, проверило hashes actual Git blobs при reuse и восстановление journal.
Новое capability нельзя отправлять в Welcome старому worker; controller Welcome
сохраняет прежние values. Перед обновлением Windows запускается новый controller.

Delta/cache vertical завершён. Этап 6 продолжается: resume больших объектов,
output batching и virtualization ещё не реализованы; другой adapter добавляется
только при подтверждённой необходимости.
