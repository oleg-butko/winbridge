# A2.2 — версии imports, workflows, applications и attention

Дата: 2026-10-13. База: A2.1, `5f3c007`.
Статус: local gate passed; автоматический native gate pending, весь A2 открыт.

## Что меняется

Schema **8** добавляет внутренние integer revisions в imports, workflows,
applications и attention. Они не входят в wire v1, MCP или UI JSON. Task revision
из schema 7 сохраняется. Shared metadata, frozen payloads, request keys, IDs,
output cursors и attention sequence не пересериализуются миграцией.

[JournalSnapshot](../../../apps/worker/src/store/domain_records.rs) хранит
наблюдаемую revision, исходные bytes record и immutable identity. Изменённая
копия может быть сохранена только с прежней revision/identity; успех требует
ровно одной изменённой row. После commit обновляется observation. Bare record
больше не разрешает import/workflow/application UPDATE. SQLite triggers требуют
следующей revision и сохраняют ключи/identity columns. Терминальный domain record
нельзя вернуть в running/ready через adapter. Production progress, cancel,
recovery, readiness и exit writers используют этот guard.

Workflow step state остаётся частью versioned parent record. Заморожены submission,
recipe pins, step IDs/names и accepted time; для applications — ownership IDs,
project, log path и created time, для imports — submission и source/local paths.
Чистая FSM пока есть только для tasks: этот пакет не объявляет остальные imperative
reducers типизированными FSM и не добавляет actor runtime.

Import/workflow admission делает dedup lookup и INSERT в одной immediate
transaction. Explicit attention сохраняет прежнюю глобальную область agent key,
сериализует dedup/queue limit/INSERT и проверяет row count. Повтор key с иным
payload отклоняется; новый requestId разрешён и не создаёт второй effect.

## Связанные решения сохраняются вместе

- Child import и operation link/step result workflow — одна transaction.
- Build/start task, step link и при start application registry — одна transaction.
  Сбой parent CAS или registry INSERT откатывает child admission; scheduler не
  видит orphan queued task и не получает разрешение spawn. Caller observation
  меняется только после commit.
- Application readiness и успешный readiness step — одна transaction.
- Stop application и task cancellation intent — одна transaction; ACK не обещает
  уже завершённый process tree. Deadline readiness сохраняет task cancel,
  application failure intent и step reason вместе.
- Workflow/application outcome и generated attention — одна transaction.
  Ошибка или ignored INSERT не публикуют успешный terminal outcome.
- Explicit attention ACK проверяет revision и row count. Первый ACK фиксирует
  timestamp/revision; повтор возвращает прежний timestamp без UPDATE. Legacy
  task-only ACK не меняет workflow/application targets. Старый observed revision
  отвергается даже после уже выполненного ACK.

In-memory project reservation появляется после сохранённого workflow decision.
Новые durable leases ещё не введены. Раздельное распространение workflow cancel
к нескольким child operations остаётся намеренным: durable intent, затем
идемпотентная отмена каждого child и ожидание реального результата.

Старый journal мог сохранить application registry до parent link. При таком
recovery повторное association использует прежнюю application с теми же ownership
IDs и log path, сохраняет PID/timestamps/terminal outcome и не сбрасывает её в
Starting. Принятая задача `indeterminate` не запускается заново. Manifest mismatch
сохраняет понятный Failed workflow и не допускает новый child.

## Миграция и границы

Schema 7 → 8 сначала создаёт consistent `VACUUM INTO` backup
`journal-v7-<timestamp>-<uuid>.sqlite` рядом с журналом. ALTER всех четырёх таблиц,
triggers и user_version выполняются одной transaction. Ошибка на любом trigger
откатывает все columns/version. Исторические схемы 1–6 сохраняют backup исходной
версии до последовательной миграции. Reopen не делает повторную migration.

Старый binary должен отказать schema 8 до recovery/admission. Downgrade с новыми
accepted operations через восстановление старого backup не поддерживается:
можно потерять dedup keys. Native reference открывает только новый test journal;
рабочие runtime/config/DB не заменяются. Synthetic legacy fixtures снимают новые
columns/triggers перед установкой исторической версии; frozen A0 fixtures неизменны.
Отдельные synthetic crash fixtures сознательно используют guarded SQL, чтобы
воссоздать старое промежуточное состояние; production terminal guard не ослаблен.

CAS защищает записи, но не определяет владельца процесса или файлового эффекта.
Асинхронный completion читает свежую revision и сохраняет актуальную отмену:
launch revision не является owner token. Attempt/epoch/incarnation и stale owner
fencing относятся к **A2.3**, physical-owner reconciliation до нового admission —
к **A2.4**. SQLite authority и прежний Worker Mutex пока сохраняются.

## Проверки и ревью

[16 SQLite regression tests](../../../apps/worker/tests/domain_cas.rs) покрывают
stale progress/cancel/terminal, workflow ABA, stop против поздней readiness,
rollback child link/registry, stale parent admission, atomic stop/readiness/attention,
ignored accepts/ACK, concurrent dedup трёх доменов, immutable identities, revision
exhaustion, exact schema-7 backup/raw payload/attention high-water, rollback migration
и legacy orphan application без replay. Fake commands/PIDs в этом target никогда
не исполняются. Existing lifecycle, snapshot/resume/workflow regression tests
проверяют реальные процессы отдельно.

Self-review исправило blind application upsert, раздельные child/parent commits,
отдельную запись readiness/step, успех ignored ACK/accept и потерю legacy registry
outcome при восстановлении parent link. Новые guard errors имеют `local journal:`
prefix: scheduler при storage fault останавливает admission и не публикует новый
обычный outcome вместо несохранённого решения.

Local gate: **213 workspace tests passed**, один reference test ignored; Windows-target
workspace/worker tests check, worker Clippy `-D warnings`, fmt, Python syntax/CLI
argument guard, PowerShell AST и scaffold/local links passed.

Сценарий `--task-cas-only` / `-TaskCasOnly` расширен до всех writers, включая
native snapshot/resume/profile/workflow regressions и actual отказ старого worker
schema 8 с побайтной проверкой test journal. Историческое имя флага сохранено.
Worker нужен, desktop необязателен, UI-окон не будет. Один пакет заменяет отдельные
ручные проверки; завершение A2.2 требует проверенного native receipt.
