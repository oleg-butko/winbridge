# A2 — durable переходы: первая поставка и оставшаяся работа

Дата: 2026-10-13. База: завершённый A1, `60c841f`.
Статус: **A2.1 task revisions/CAS завершён**, implementation `8f631dd`;
[107 native Windows tests passed](windows-task-cas-gate-2026-10-13.md).
A2.2: [domain revisions и atomic links](domain-transitions.md) завершены,
[125 native tests passed](windows-domain-cas-gate-2026-10-13.md).
Весь A2 пока не завершён. [A2.3.1 authority/attempt context](ownership-context.md)
завершён, [141 native tests passed](windows-ownership-gate-2026-10-13.md); [A2.3.2 durable leases](project-resource-leases.md) реализованы и ожидают native gate;
A2.4 reconciliation ещё впереди.

## Зачем нужна первая поставка

FSM выбирает допустимый переход, но не защищает запись от другого writer.
Tasks меняются также при чтении stdout/stderr и удалении старого output.
Старая копия record не должна стирать новый cancel flag, output или terminal outcome.
Один production Mutex пока сериализует доступ; durable guard нужен до разделения
owners на акторов. SQLite остаётся единственным authority, новых брокеров/БД нет.

## Инвентаризация writers

| Область | Текущие точки записи | Реализовано A2.1 / A2.2 | Осталось в A2 |
| --- | --- | --- | --- |
| Task admission | Store::accept_with_origin, UNIQUE(project,key), frozen payload | Dedup/read/insert в одной immediate transaction; INSERT обязан сохранить одну row. | Journal incarnation, durable operation/attempt context. |
| Task lifecycle | transition, PID, cancel, finish, recover | Pure FSM + revision CAS + terminal attention в одной transaction. | Owner/attempt/epoch guards и recovery admission. |
| Task output | append_output, sequence, byte counter/quota event | Output INSERT и task CAS атомарны; игнорирование записи откатывает всё. | Process-owner fencing и согласованный общий read snapshot. |
| Output retention | preview/apply_output_cleanup, cleanup receipt | Revision входит в preview hash; delete/task CAS/receipt одной transaction. | Общие guards связанных workflow/app/attention и ownership maintenance. |
| Snapshot imports | imports.rs: accept_import/save_import/finish_import/cancel_import, progress/recovery | Versioned import CAS, atomic dedup/admission, progress/cancel/recovery guards. | Transfer attempt, durable workspace ownership/reconciliation. |
| Workflow | workflows.rs: accept/save/finish/cancel, scheduler steps | Versioned flow/step CAS, atomic child association и outcome/attention; reservation после commit. | Durable lease/epoch и восстановление physical resources. |
| Application | workflows.rs: save_application, stop/readiness/exit/recovery | Versioned registry, atomic task stop и readiness/step; legacy orphan association без reset/replay. | Lifetime owner/epoch, unresolved physical owner. |
| Attention | attention.rs: explicit request, outcome insertion, ACK | Generated outcome и explicit accept проверяют durable row; ACK revision CAS/монотонность. | Общий consistent read model и связанный owner context. |
| Runtime settings | settings.rs и core file storage, manual restart | SQLite не используется; прежний checked file/atomic save. | Связь config generation с admission/resource policy; без hot reload. |
| Controller/desktop state | Dispatch/connection freshness и локальные формы | Исполнительного authority не получают. | OperationRef/discovery A3; connection UUID не становится epoch. |

Все domain UPDATE guards реализованы в A2.2; native gate passed.
Исходные application/process callbacks и файловые эффекты пока у прежних owners.

## Контракт A2.1

Schema **7** добавляет только внутренний `tasks.revision`: integer, начальное
значение 0, проверка nonnegative/integer. `TaskRecord`, wire v1, IDs и fingerprints
не меняются; поле revision не добавляется в JSON, TypeScript или MCP.

[TaskSnapshot](../../../apps/worker/src/store/task_records.rs) связывает record
с immutable identity и observed revision. `Store::save` больше не принимает
голый TaskRecord; legacy fixtures получают snapshot и редактируют его record.
Все production task UPDATE используют единственный `write_task_snapshot`:

```text
read(record, revision)
→ pure decision или metadata mutation
→ UPDATE WHERE id AND expected revision AND original project/key/payload
  SET record, revision = expected + 1
→ связанные rows/events/attention в той же transaction
→ commit
→ разрешённый внешний effect
```

Zero changed rows, stale snapshot, другой Task ID/payload или integer exhaustion
дают ошибку без разрешения effect. SQLite trigger запрещает UPDATE без следующей
revision и изменение stored identity/fingerprint. Это дополнительная защита от
забытого SQL writer, а не sandbox для владельца БД, способного удалить trigger.
Task revision увеличивается и при output/retention. Output sequence продолжает
определять порядок только output chunks; journal event sequence — свою область.

`transition_at(id, revision, event)` явно отвергает stale observation, включая
no-op observation. Legacy `transition`/finish/cancel читают current state внутри
transaction. Completion **не сравнивается с launch revision**: свежий output или
cancel меняют revision, но не отменяют право действующего process owner сохранить
реальный exit. Epoch/attempt owner token будет отдельным guard следующей поставки.
Повтор terminal completion не меняет record, revision или attention.

Output INSERT, counter/quota event и task CAS либо сохраняются вместе, либо
откатываются. Cleanup preview включает revision, так что ABA с прежними bytes
не принимает старый план. Cleanup проверяет отсутствие оставшихся chunks и
запись receipt; partial/ignored DELETE или INSERT не подтверждают очистку.
Confirmed duplicate receipt по прежнему key возвращает прежний результат.

## Миграция и откат

Schema 6 → 7 сначала делает SQLite consistent `VACUUM INTO` backup
`journal-v6-<timestamp>-<uuid>.sqlite` рядом с journal, затем additive ALTER/trigger/
user_version одной transaction. Старые версии 1–5 используют прежний consistent
backup исходной схемы; новая пустая БД не требует копии. Migration не пересериализует
payload/record, не сбрасывает dedup, output cursors или attention sequence.
Reopen не повторяет migration. Сбой после ALTER откатывает новую column и version.

Schema-6 worker должен отвергнуть schema 7 до recovery/admission. Native gate
проверяет реальный прежний executable с новым тестовым config/state; установленная
БД не открывается. Откат runtime на старый binary поверх schema 7 не поддерживается.
Работу с принятой историей продолжает совместимый binary; восстанавливать backup
после новых admissions нельзя как обычный downgrade: он может утратить новые keys.
Journal incarnation/recovery mode при restore ещё относятся к остальной части A2.

## Ревью и проверки

Ревью исправило missing-row/ignored INSERT, output counter без сохранённого chunk,
terminal outcome без attention, partial cleanup DELETE и ignored cleanup receipt.
Продолжает действовать A1: commit перед spawn, неизвестный outcome без replay.
В полном Unix прогоне synthetic snapshot restart однажды увидел ещё занятую
instance lock после drop. Fixture теперь ограниченно ждёт её настоящего освобождения,
retry делает только для ownership error; production lock не ослаблялся.

Local gate: **197 workspace tests passed**, один explicit reference test ignored;
Windows-target workspace и protocol/core/worker tests check, worker Clippy, fmt,
Python syntax/argument guards, PowerShell AST и scaffold/local links passed.

[task_cas tests](../../../apps/worker/tests/task_cas.rs): 15 обычных checks через
реальную SQLite + отдельный native reference check. Stale connections/cancel/exit,
stale launch/no-op, reopen/ABA, immutable identity/payload, атомарный output,
attention/retention rollback, concurrent dedup, exact schema-6 backup, rollback
migration и exhausted revision. Reference check явно ignored в общем прогоне;
native scenario запускает его отдельно, не считает отсутствие old exe проверкой.

Добавлен `--task-cas-only` / `-TaskCasOnly` с обязательным schema-6 reference exe.
В одной партии: frozen baseline, pure FSM, task CAS/faults, runtime/output/history/
maintenance/snapshots/workflows/attention и actual older-binary refusal. Driver
проверяет report hashes и candidate/reference build identities.
Worker должен быть запущен, desktop необязателен, тестовых UI-окон не будет.
Candidate и reference используют изолированные config/state/cache/workspaces;
установленные exe/configs не заменяются. Native receipt подтверждает task exit 0,
107 tests и шесть verified artifacts, включая реальный отказ old binary новой схеме.

## Последовательность оставшегося A2

1. A2.2 (завершён, native passed): versioned import, workflow/step/application и explicit attention writers;
   DB transaction объединяет child association, event и resource decision.
   До изменения каждого домена расширить исторические fixtures и fault matrix.
2. A2.3.1: [journal incarnation, OperationRef/attempt IDs и owner epochs](ownership-context.md);
   task/import claim и attempt одной transaction, token checks callbacks, все writers
   под общим owner guard. Завершён, 141 native tests passed.
   A2.3.2: durable project/resource leases и atomic operation association.
   Connection generation не заменяет durable epoch.
3. A2.4: startup reconciliation до нового admission, unresolved physical owners,
   понятный recovery read model и explicit restore recovery mode; paired native
   crash/takeover gate. Epoch не прекращает физическое исполнение: workspace
   освобождается после доказанной остановки либо заменяется изолированным root.

A2 закрывается только после всей этой работы. Actor migration A5 и global
parallelism A4 не включаются на основании одного task CAS gate.
