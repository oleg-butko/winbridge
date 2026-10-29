# Чтение Active после потери ответа

2026-10-29, .204/main, продолжение [Active core](finalization-active-2026-10-29.md).

## Назначение и границы

Отдельный readonly normal-IPC запрос `finalizationObservationVersion: 1`
читает уже существующий in-memory факт Active. Это не mutable V8 и не повтор
release. Новый request ID отличается от execution keys и прежней V7 correlation.
Обе стороны сверяют OS peer PID: receiver — исходного keeper, sender — исходного
child. PID из JSON не является доказательством.

Worker требует исходный typed lifetime, живой pinned keeper, фактически
загруженный RuntimeGuard/installation/scope и Windows Job membership. Ответ Active
возможен только из сохранённого typed Active owner, с тем же command digest,
instance lock и текущей authority того же Store. Последняя проверка под
Store→mode lock закрывает гонку с потерей authority/остановкой процесса.

Оригинальный Active receipt остаётся неизменным; отдельный observedAtMs обозначает
текущее чтение. Parent validator проверяет исходную shutdown/released lease,
receipt digest, correlation и часы. Pending/Unavailable/Rejected не являются
успешным подтверждением. Sender имеет ограниченный бюджет чтения, не продлевает
startup grant и не читает старый V6 stream с возможным частичным ответом.

После Active не проверяются старый config SHA, Quiesced lease, updater epoch
или пустота execution domain: сохранение настроек, обычные задачи и следующий
update допустимы. Однако текущая Store authority и живой keeper обязательны.
Ни запрос, ни сохранённый receipt не создают режим, SQL commit, Job или permit.
Malformed envelope/смешанные версии не уходят в legacy fallback.

Receiver имеет отдельный readonly marker; sender создаёт pattern динамически.
Mutable V8 marker/decoder, physical factory и public installation пока отсутствуют.
Положительный native Active observation требует будущего полного V8 сценария;
прежний native20 gate73063db относится только к Active core и не переименовывается.

## Проверки

Пять новых tests: два pure-model контракта, parent probe без живого сервера/
с истекшим бюджетом/чужим endpoint и два real IPC negative tests. Последние
проверяют чужой настоящий peer и семь вариантов malformed/mixed envelope,
сохраняя Pending/config pin/authority; synthetic Active также не выдаёт
authenticated proof. Synthetic metadata не заменяется разрешающим test hook.

Полный workspace/update-validation: **1022 passed,17 ignored**,128 result groups.
Linux/MSVC workspace/all-target Clippy с `-D warnings`, generated Rust/TS
contracts, typecheck, fmt/diff и scaffold337 прошли; результаты находятся
в `.winbridge/finalization-observation-review-204/summary.json`.
Перед проверками свободно Debian24,863,117,312 / Windows(shared)18,916,372,480 bytes.
Новых Windows задач/окон этот этап не запускает, установленный worker не меняет.

## Следующий этап

Bounded mailbox должен передавать mutable V8 только исходному
RunningLocalService, который последовательно удерживает Pending/readiness/known
commit. Обрыв ответа не отменяет принятое решение; после неопределённого исхода
caller использует этот readonly observation, не повторяет release. Далее нужны
consuming physical factory с отпусканием parent config pins, actual V8 native
gate, permanent keeper без updater authority и две последовательные генерации.
