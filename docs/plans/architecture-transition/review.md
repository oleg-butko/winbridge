# Повторное ревью объединённого плана

Дата: 2026-10-13. Это второй проход автора по [плану](README.md),
не независимое ревью другим агентом. Проверены фактические DTO/Store и три
исходных отчёта. Ниже перечислены найденные проблемы порядка и принятые исправления.

| Риск исходного объединения | Исправление в итоговом плане | Проверка при реализации |
| --- | --- | --- |
| Полная Actor Model/Run/report система задерживает простую очередь нескольких проектов. | A4 перед A5–A6; старые task/workflow используют новый scheduler через adapter. | Три проекта и общий лимит работают до появления внешнего executor. |
| Actor mailbox принимается за durable очередь; restart повторяет spawn. | A0–A2 раньше actor pilot, commit до ACK и grant/recovery до dispatch. | Lost ACK, actor crash, intent без PID, поздний completion. |
| Новую capability добавляют в Hello и ломают старый reader до negotiation. | Frozen v1; optional catalog сначала через WorkerInfo, full-route matrix и legacy fallback. | Old/new по всем hop, включая старый controller и TS/MCP. |
| Task FSM защищена, но workflow/import/retention продолжают писать в обход CAS/leases. | A2 учитывает всех writers; A4 не включается до общей защиты ресурсов. | Inventory writers и проверка обхода guard; app/build/import contention. |
| A1 обещает persistent attempt guards раньше введения их в A2. | A1 защищает переходы и terminal; durable attempt/epoch guards входят в A2 до новых owners. | Отдельные reducer tests и затем persisted stale-owner tests. |
| Launch revision используется для любого completion: cancel делает реальный exit устаревшим. | Event допускается по attempt/epoch/FSM, CAS применяется к текущему состоянию. | Cancel → настоящий exit; event старого attempt → отказ. |
| Повышение epoch считается остановкой старого процесса. | Fencing DB отдельно от physical isolation; остановка доказана или новый workspace. | Неизвестный live owner не освобождает root/IPC по TTL. |
| Workflow родитель и child занимают два слота; приложение удерживает все build slots. | Resource lifetime по шагу; parent/child accounting и отдельный app lease. | Cap=1, build→start→live app и очередь другого проекта. |
| Каждому читателю сразу вводят outbox, inbox и вторую authoritative БД. | A6 начинает с сохранённого queryable report; A7 условна. Durable receiver требует обе стороны одновременно. | Lost receipt/ACK, reopen обоих stores и business dedup. |
| Ключ разделяют по contract version и обходят прежнюю защиту от повторного эффекта. | Namespace по stable operation family; contract version внутри fingerprint; legacy payload сохранён. | Одинаковый ключ и новый contract/payload дают conflict, прежний retry даёт старый ID. |
| Rollback трактуется как запуск старого binary поверх новой БД. | Disable admission и поддержка принятых runs; compatible schema reader; backup restore — отдельный recovery. | Older binary отказывает новой схеме; restore с потерей ключей не обещает dedup. |
| Общая архитектура создаёт одного перегруженного актора. | Coordinator только маршрутизирует; Store, process и connection owners разделены, handlers короткие. | Status/cancel latency при build, mailbox overload и shutdown. |
| Новая фича приложения требует полного protocol gate каждый раз. | Stable executor adapter/contract отдельно от application source; gate по изменённым рискам. | Consumer tests на каждом source change, mixed pair при изменении контракта. |

## Зависимости проверены повторно

1. Сначала воспроизводим legacy, затем меняем reducer: иначе нельзя отличить
   исправление от случайного изменения ACK/dedup/recovery.
2. Store guard и ownership предшествуют нескольким state owners; сериализация
   акторов не гарантирует консистентность после аварии или внешнего процесса.
3. Новый public scheduler/Run API следует за discovery; один лишь additive Rust
   enum не обеспечивает совместимость wire или TypeScript.
4. Queue/runtime policy может появиться до полного actor перехода; outbox может
   появиться после queryable reports. Это снижает размер первой полезной поставки.
5. Consumer specification/test pack предшествует implementation контракта;
   enable следует за paired native gate, а не за одной удачной сборкой.

## Границы и отложенные решения

Kameo остаётся кандидатом: выбор конкретной версии проверяется в A5; сейчас не
меняется dependency graph. Значения production slots/backlog и policy profile
нужно согласовать при A4; план не даёт молчаливого разрешения на произвольную
параллельную нагрузку. Kafka, remote actor addressing, general DAG, hot config
reload и schema downgrade не входят в этот переход.

Baseline A0 сознательно ограничен текущими task/session гарантиями. Он не доказывает
будущие CAS/fencing/outbox и не заменяет Windows recovery test. Import/workflow/app
fixtures и fault matrix расширяются перед изменением соответствующего domain.
Все восемь техник имеют место в плане; outbox/inbox вводятся при доказанной
потребности в durable delivery, а не в обязательном первом schema migration.

Итог ревью: порядок пригоден для начала A0; дальнейшие этапы имеют отдельные
criteria/gates/rollback и не требуют одномоментной замены рабочего продукта.
Проверки до commit плана: `npm run check:scaffold` (111 Markdown документов,
локальные ссылки и examples) и `git diff --check` passed. Runtime tests относятся
к следующему пакету; эти проверки не обозначают готовность новой архитектуры.
