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

Native gate: task fe719061-08e8-4f5a-a0a7-6c6b225dd3a5,
run20261023-231817-d6b1374c, producer ede7318, terminal failed/1; не resubmit.
22 prerequisites и actual support Checked5 прошли; следующий test parent
использовал control pipe для normal Info и получил EOF. Harness исправлен на
основной pipe из instance context, сохранённые effects не повторяются.
Следом новый isolated run, подробные failed pins в handoff. После terminal нужны ready/proof audit и
readonly --verify-run текущим verifier. В controller-only review устранены
cross-run proof reuse и принятие лишних authority полей в passive projection;
проверяются exact runtime paths, final worker/config/schema/lease/image scope.
Новые checks не меняют immutable producer/Windows execution. Код теста сам по
себе не доказывает actual Main исполнение.

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
