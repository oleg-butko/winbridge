# A2.3.1 — journal authority и попытки исполнения

Дата: 2026-10-13. База: завершённый A2.2, `6b1959e`.
Статус: **завершён**, implementation `ad9b1f9`;
[141 native Windows tests passed](windows-ownership-gate-2026-10-13.md).
Это первая часть A2.3. Постоянные project/resource leases остаются в A2.3.2,
physical reconciliation и восстановление backup — в A2.4. Весь A2 открыт.

## Решение и границы

SQLite schema **9** добавляет `journal_authority` и `execution_attempts`.
Wire v1, fingerprints, keys, output cursors и пользовательские IDs не меняются.
Новый broker, actor runtime и параллелизм проектов не вводятся.

| Контекст | Что означает | Когда меняется |
| --- | --- | --- |
| Journal incarnation | Идентичность этой истории SQLite | Создаётся при миграции; explicit restore policy ещё не реализована |
| Owner epoch + authority UUID | Право текущего worker записывать эту историю | Каждая активация worker под прежней exclusive instance lock |
| OperationRef | Incarnation + kind + operation ID + project | Сохраняется с первой попыткой task/import |
| Attempt ID | Единственная разрешённая попытка внешнего исполнения операции | Создаётся при durable claim; повторный claim запрещён |
| Record revision | Версия конкретной записи | Каждый сохранённый переход/metadata update, включая output/cancel |

Controller session и worker generation не являются authority tokens.
Output/cancel изменяют revision, но действующая попытка продолжает записывать
реальный результат: callback проверяет attempt/owner, а CAS — свежую revision.
История попыток не удаляется; completed attempt нельзя снова открыть или изменить.
Даже ошибочный возврат task в queued не разрешает второй claim той же операции.
Явный повтор работы требует нового idempotency key и новой операции.

## Активация и единый guard writers

`Worker::open` сначала получает прежнюю OS instance lock, открывает/мигрирует БД,
одной immediate transaction увеличивает owner epoch и создаёт новый authority UUID,
затем выполняет прежнее recovery. Простое `Store::open` не активирует новый owner.
Интерфейс активации предназначен для доверенного runtime после OS lock; это
контракт владения, а не механизм авторизации против владельца SQLite-файла.

Все production transactions task/import/workflow/application/attention/retention
проверяют привязанный Store context внутри той же transaction, что и запись.
Старое соединение не может просто прочитать новый context и продолжить писать.
TaskSnapshot/JournalSnapshot читают record, revision и authority **одним SELECT**;
adapter отвергает snapshot прежнего owner даже при совпадении revision/raw bytes.
Migration и активация имеют свои bootstrap transactions, проверенные отдельно.

## Commit перед эффектом и callback scope

```text
queued task/import
→ immediate transaction: next state + immutable execution attempt
→ commit
→ прежний исполнитель получает ExecutionToken
→ PID/progress/output/exit: проверка token и текущего owner в write transaction
→ terminal record + attempt completion (+ task outcome attention) одной transaction
```

Ignored/failed INSERT attempt откатывает launch intent; процесс не создаётся.
Ignored completion откатывает terminal result и attention. Два конкурирующих
claims дают только одну попытку. Scope callback запрещает другую операцию,
смену owner и administrative writes; scope восстанавливается после error/panic.
Production Mutex после panic продолжает требовать restart, как прежде.

Process owner передаёт token в оба output readers, PID/result и error fallback.
Import owner — в progress/checkpoint/result и fallback. Потеря owner прекращает
обслуживание прежним worker, а RAII managed child завершает принадлежащее ему
дерево. Прежний callback не записывает ложный terminal outcome; новая authority
при recovery сохраняет `indeterminate`, без автоматического повторного запуска.

Workflow/readiness/application/attention/retention сейчас принадлежат одному
worker и защищены общим epoch. У них пока нет отдельных actor ownership scopes.
Application execution использует попытку её start task.

## Миграция и совместимость

Для существующей schema 8 сначала создаётся consistent SQLite backup
`journal-v8-<timestamp>-<uuid>.sqlite` через `VACUUM INTO`, затем metadata tables,
triggers, seed и user_version=9 сохраняются одной transaction. Старые схемы
сохраняют прежний backup исходной версии перед последовательными миграциями.
Повторное открытие не создаёт backup/новую incarnation. Legacy JSON не переписывается.

Историческим running tasks/imports не приписываются выдуманные attempts или PIDs.
Они проходят прежнее recovery; отсутствие нового attempt не доказывает отсутствие
внешнего эффекта. Native gate запускает реальный прежний schema-6 binary с
**новым тестовым state** и проверяет отказ schema 9 без изменения файла БД.
Установленный runtime и его schema-6 journal продолжают работать отдельно.

## Автоматическая проверка

Local gate: **229 workspace tests passed**, один explicit reference test ignored;
Windows-target workspace/worker tests, Clippy/fmt, PowerShell AST, Python syntax/
pre-IPC argument guards и scaffold passed. Финальные ownership/runtime tests повторены
после проверки, что дочерний fixture на Windows тоже использует CREATE_NO_WINDOW.

16 новых SQLite/runtime tests: competing claims; stale connections/snapshots всех
writers; callback retarget/nesting; late PID/output/import progress/outcome; cancel
между revisions; commit fault до real spawn; completion/attention rollback;
attempt history; owner overflow; incarnation change; additive migration rollback,
raw bytes/backup/dedup; потеря owner с **реальным управляемым деревом процессов**.
Физический fault вводится только в изолированной test DB.

`--task-cas-only` / `-TaskCasOnly` включает новый `ownership` target вместе с
FSM, legacy baseline, domain CAS, output/retention, workflows, resume и profiles.
На Windows нужен только установленный worker; desktop/test UI windows не нужны.
Результат подтверждается accepted task, exit, native logs и checksum-ready receipt.

## Что ещё требуется до actor migration

1. A2.3.2: durable resource/project leases и atomic association с operation;
   сейчас reservation остаётся прежней in-memory, общий writer owner уже защищён.
2. A2.4: проверить физического владельца перед новым admission/release. Epoch
   защищает SQLite, но сам не блокирует OS/filesystem effects между двумя проверками.
   Нельзя разрешать takeover живого executor только по изменённому epoch.
3. Explicit restore recovery mode: копия БД сохраняет старую incarnation/keys и
   может потерять новые admissions. UUID новой активации предотвращает повтор старого
   token после reboot, но не обнаруживает утраченную историю. Обычный runtime rollback
   поверх новой схемы и восстановление backup после новых задач не разрешены.
4. A3: общий recovery/status read model и discovery; внутренние tokens пока не
   добавлены в wire/UI/MCP. A4/A5 не включаются по результатам одного этого gate.
