# Candidate support actor v18 / view10

2026-10-23. Продолжение [schema16 foundation](candidate-support.md).
Добавлены authenticated CaptureCandidateSupport/ObserveCandidateSupport и client
метод SupervisorHandle::candidate_support. Journal schema16 и worker16 прежние;
старые supervisor contracts1–17 и view schemas1–9 не получают новые поля.

## Исполнение и наблюдение

Supervisor использует свой единственный blocking workslot для admission preflight
и query. Worker IPC и hashing executor image выполняются вне mailbox. Пока
admission готовится, новая view показывает Admitting; прочие observe остаются
доступны. Fresh intent сохраняется до первого ACK и до process creation.
После возврата ownership actor запускает только полученный non-Serde permit.
Ошибка доставки ACK не отзывает уже принятый запрос. Existing record возвращается
до fresh-only checks и никогда не создаёт новый процесс.

View10 читает parent/source/build/предшествующие receipts/support в одном SQLite
срезе. Она показывает command, actual PID, progress, cleanup, report, assessment
и причину. Checked сохраняет blockers и не означает разрешение установки.
Cold pending без живого driver показывается Indeterminate, сохраняя attempt;
finished Checked остаётся историческим Checked. Data cleanup не является
physical custody. В view нет canInstall/canDrain/permit.

Support-active входит в coordinated abort: Root Aborting останавливает свежие
действия driver через CAS/current checks, callback сохраняет cleanup; source
освобождается после data/physical barriers. При неизвестном исходе barrier
сохраняется. Generic v1 lifecycle method не исполняет новые actions.

## Код и проверки

- Protocol: update/candidate_support.rs, actions в update.rs; generated TS.
- Projection: journal/supervisor.rs и candidate_support::view.
- Actor/client/workslot: apps/update/src/supervisor.rs.
- Blocking worker bridge: apps/update/src/preparation.rs.
- Два projection regressions, два strict protocol tests, frozen views1–10
  regression и actual IPC worker mailbox regression.
- Private validation Main path: update_control/validation/candidate_support.rs.
  Он выполняет source/build/artifact/health → v18 support, проверяет lost ACK,
  duplicate, blockers и живой SourceHeld worker. Cleanup делает явный abort
  fixture, не updater shutdown/install.
- Native integration: update_shutdown/candidate_support.rs. Parent дополнительно
  опрашивает настоящий normal worker IPC и сверяет cold record/old file identities.
- Approved runner: test-update-support-actor.ps1 и
  validate-update-support-actor-native.py. Один retained cache lease, immutable
  producer/target source pins, ready files twice и independent mutation audit.

[Actual Main Windows23/0 gate](windows-candidate-support-actor-gate-2026-10-23.md)
пройден: retry run20261023-233438-99674bc1 / task
adf48103-c02b-40e6-9442-6d089882c48c, producerdb0105a.
21 ready files twice/28 mutations, normal IPC/owned abort/history проверены.
Первый run failed из-за harness endpoint; source/failed reports сохранены.
Controller-only verifier shadowing исправлен и readonly recheck прошёл.
Local workspace679/0, обе Clippy, jsonschema/TS/UI/Python passed.

## Следующий блок: обязательный production drain

Эта интеграция **не меняет legacy DrainCandidate v13** и ещё не добавляет
обязательный support→drain binding. Private Main gate намеренно не вызывает
drain и явно сообщает productionDrainEnforced=false. Отсутствие BeginDrain
в таком сценарии не доказывает enforcement production entry.

Следом новый versioned production drain entry и атомарная durable связь с
exact support attempt/digest/health/root/owner; frozen v1 DrainIntent сохраняется.
Driver обязан заново проверить current evidence перед worker effect. Затем
actual Main gate должен **запросить** этот entry и получить durable blocker
до BeginDrain при живом old worker. Потребуется schema migration/refusal gate,
если для этой связи добавляется новая таблица.

После этого authorized activation из resident replacement guard, local IPC и
controller confirmation, remaining U2 faults, U3 desktop и U4 public/bootstrap.
Public installation остаётся выключена; installed worker пока не обновляется.

## Уточнение следующей реализации: ещё не выполнено

Рекомендуемая форма обязательного допуска:

1. Новый supervisor19/view11 и separate production-drain record. Следующий
   schema17 нужен для новой таблицы/связи; старые16 обязаны отказать до effects.
   Query1 и DrainIntent1 не расширять молча.
2. Immutable gate record связывает support attempt/revision/digest, installation/
   owner/epoch/root, source/health/config/executor. Терминальный результат:
   Blocked с сохранёнными typed reasons либо Admitted с exact drain attempt.
   Pending support не превращать в terminal blocked: сначала дождаться query.
3. Blocked сохраняется без BeginDrain/root transition/worker command; existing
   запрос возвращает ту же запись. Saved blocker не становится authority после
   расширения поддержки будущим executable; нужен новый update attempt.
4. Для Admitted использовать тот же transaction callback advance_with_driver,
   который создаёт frozen DrainIntent1/command outbox и root edge. Link и root
   либо сохраняются вместе с readback, либо полностью откатываются.
5. Fresh DrainPermit может удерживать sealed in-memory production binding;
   сохранённая JSON-запись не восстанавливает permit. Перед каждым worker effect
   consuming driver сверяет link/current evidence и удерживает exact image/config
   pins. Legacy внутренние tests остаются на прежнем контракте.
6. Production entry сначала сверяет owner/root/support context. Даже empty
   blockers не заменяют revalidation actual executor/source/health/config/worker.
   Старый schema16 support record после миграции — история, не fresh admission.
7. Native actual17 Main должен запросить новый entry, получить durable blocker
   при missing activation/controller/desktop и сохранить SourceHeld revision1/
   zero BeginDrain. Это отличается от нынешнего теста, который drain не вызывает.
8. Actual16→17/refusal делать на consistent копии retained successful support16
   history. Проверить backup, epoch fence, старые rows и отсутствие нового permit.
