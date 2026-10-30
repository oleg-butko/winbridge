# Запись независимого подтверждения Active

2026-10-29, .204/main, после [V8 mailbox/native26](finalization-mailbox-2026-10-29.md).
Source worker19/updater23; schema не менялась.

## Контракт

Library-only `confirm_activation_finalization` потребляет non-clone/non-Serde
FinalizationConfirmationPermit. Его будущий original physical owner создаёт
после normal IPC с настоящим peer исходного child/Job. Saved rows/JSON не создают
этот permit. Сама запись не посылает V8, не повторяет release и не превращает
процесс в permanent keeper.

Immediate transaction проверяет original active coordinator record, явные
launch/update IDs, owner/epoch/installation/scope, полный retained ledger/root/
lifetime/shutdown и exact command/receipt/correlation. Fresh capture использует
исходный monotonic read deadline и wall-clock age ≤ IPC_TIMEOUT; startup grant
не обновляется. Clock rollback/max проверяются также для уже записанного success.
Physical checkpoint вызывается до/после записи; внутри него нельзя брать тот же
Store/UpdateJournal lock. Последняя ошибка откатывает current/history вместе.

Confirmed дописывается один раз и снимает finalization admission barrier. Root
Committed, его revision/history и worker lease не переписываются. Повтор exact
independent fact возвращает прежние данные, без новой history row/permission.
Confirmation COMMIT error остаётся unknown: inspect/observe same key, без release.

`mark_activation_finalization_unknown` пишет только diagnostic Indeterminate под
исходным coordinator/authority, в том числе при гибели child. Не перезаписывает
Confirmed, не запускает процесс и не даёт execution grant. Public installation
по-прежнему выключена: factory/одно delivery/parent pin retirement/keeper ещё
не подключены.

## Ревью и проверки

SQL append вынесен из test-only копии в один production helper; прежние retained
history/corruption tests теперь проверяют его. Три новых tests проверяют real
SQLite one-time confirmation, неизменность root, ignored current/history writes,
ошибку последнего checkpoint/stale authority с rollback и семь отказов capture.
Late unknown подтверждается только отдельным fresh observation. Положительного
Windows custody эти synthetic fixtures не доказывают.

12 focused finalization tests passed; весь update package —426 passed/7 ignored,
20 result groups. Linux/MSVC workspace/all-target Clippy/update-validation с
`-D warnings`, contracts/typecheck/fmt/diff/scaffold прошли. UI code не менялся.
Логи `.winbridge/finalization-completion-review-204/`; предыдущий полный workspace
на source6e8dbca —1027 passed/17 ignored. Три новых tests добавлены после него.
Перед сборкой Debian19,419,144,192 / Windows(shared)19,307,233,280 bytes free.
Новых Windows executions этот этап не запускал; native26 source6e8dbca остаётся
отдельным frozen unit результатом, не proof этого writer или physical V8.

## Продолжение

Consuming original guard factory должна сделать fresh V7 с неизменяемым capture
deadline, atomically RootCommitted/ReleasePending, а затем отпустить ВСЕ parent
config/verification pins с сохранением original ProcessCustody. Только fresh
known commit даёт одну V8 delivery; unknown ACK требует normal Active observation.
После факта применяется этот writer. Далее permanent keeper без всех updater
authority references, max2/two generations и оставшиеся U2–U4. Installed worker
оставить запущенным, desktop не нужен.
