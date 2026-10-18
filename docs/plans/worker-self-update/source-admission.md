# U1: одноразовое разрешение применения исходников

2026-10-18. Library разделяет durable admission и исполнение файловых изменений.
Это следующий шаг к отзывчивому supervisor actor; публичного source-write RPC
по-прежнему нет.

`begin_source_apply` проверяет Prepared source plan, preimages, baseline, staged
source, current owner/config и live worker lease. Затем в одной SQLite transaction
сохраняет parent SourceApplying и source Applying. Только свежий commit возвращает
непередаваемый через протокол `SourceApplyPermit`. Его можно переместить в blocking
thread и употребить один раз методом `apply`. Permit не Clone/Serialize и не
восстанавливается из journal.

Повтор возвращает только Observed, включая Applying, Verified и Blocked. Durable
idempotency receipt не создаёт новый permit. При конкурирующих admissions один
fresh transaction получает разрешение; другой видит состояние или CAS conflict.
File intent/receipt, no-clobber moves, fresh guards и manifest-last остаются прежними.
`apply_source` сохраняет существующий синхронный API через этот же механизм.

Если процесс завершился после admission, либо permit потерян до dispatch, Applying
и worker startup guard остаются. Reopen/new owner не продолжает работу; readonly
inspection показывает отсутствие file intents/receipts и необходимость проверки.
ACK actor должен означать принятие работы, а не завершённое применение файлов.

## Проверки и ограничения

26 source/compiler integration tests passed, включая три новые проверки:
commit-before-dispatch, потерю permit при owner restart и два конкурирующих
admissions с единственным свежим разрешением. Windows Clippy/tests cross-check
passed. Следующий native rollup проверит эти cases и дополнительно exact root exit
в atomic CreateProcess owner-crash test, помимо пустоты Job и отсутствия user effects.

Схемы updater/worker прежние: 4/15; новые protocol DTO и persisted fields не нужны.
Supervisor mailbox, автоматическая source/build orchestration, artifact verification,
abort/release и exact shutdown ещё не реализованы. U1 целиком открыт; установленный
worker не обновляется, bootstrap пока не нужен.
