# Main23: автоматическая проверка физического StateOwned

2026-10-25 America/New_York. Продолжение [source actor23](activation-state-handoff.md)
и [закрывающего checkpoint](resume-2026-10-25-state-handoff.md). **Harness и
independent verifier добавлены; actual Windows state gate пока не отправлен.**
Source updater18/worker16, supervisor23/view15/private handoff2 прежние.
Public install и advertised activation/controller/desktop drivers выключены.

## Что добавлено

- `apps/worker/tests/update_shutdown/activation_state.rs`: отдельный actual Main
  case через обычный shutdown/backup/preparation/replacement. После Prepare20/
  Bind21/Launch22/Ready немедленно отправляет Handoff23 с потерей ACK, без file/
  history audits в оставшемся original120s. Проверяет same child/process/Job,
  generation/state IDs и реальные seven ledger prefixes0–6.
- Validation-only `stateTraceOnly` input: пропускает прежние blocking Creating/
  Created holds **только в новом explicit режиме**. Default false не меняет
  сериализованный old input; прежний Main22 и его barriers сохранены.
- Неблокирующие state probes в original updater: до своего unlock exact lock
  действительно busy; после подтверждённого release, до private command,
  тот же lock доступен. Успешный пробный handle закрывается до команды child.
  Ошибка доступа/пути не считается busy. Markers содержат build/input hashes,
  HandoffIssued5, separate barrier/private-command record digests, observed
  identities/time и результат acquisition. Не содержат nonce/token/permission.
- После receipt6 сценарий проверяет реально занятый child lock, five-file
  identities/write/delete denial, unchanged worker DB/WAL и duplicates/legacy
  projections. После owned Job stop проверяет retained actual child handle,
  empty Job, повторную доступность exact lock и cold StateOwned без ресурсов.
  Не выдаёт state6 за прежний Ready4/cold process proof.
- `-ActivationStateGate` в PowerShell и mutually-exclusive `--activation-state`
  в observer. Новый mode сохраняется в scoped metadata до ACK; namespace
  `native-activation-state-`, тот же `--verify-run`. Standalone validation
  updater замораживается до остальных builds. Старые режимы сохранены.
- `scripts/update-activation-state-proof.py`: independent scope/types/privacy,
  причинная история0–6, original grants/deadline, обе OS observations, same
  process/generation/FileIDs, warm/legacy/cold и no-writer evidence. Mutation
  audit включает согласованную подмену нового generation прежним во всех
  копиях receipt, чтобы проверять intrinsic rule, а не только несовпадение копий.

Дополнительные native groups: boundary probes3, state contract2, mailbox1,
actor owner1, readonly journal2, selective release5 и old pins2. Минимумы не
объявляются фактически выполненными counts; observer читает их из actual logs.
Прежние process/replacement/backup regressions и actual17→18/refusal сохраняются.

## Проверки source

Portable3/0 на настоящих Unix locks, Python state5/0 + process3/0 + preparation5/0
прошли. Linux updater validation Clippy и MSVC updater/worker validation
all-target Clippy прошли. Финальные проверки после review — в raw receipts
`.winbridge/update-preparation/state23-{probes,main,native}-*`.
MSVC compilation не доказывает Windows execution.

Перед builds измерены Debian25,670,488,064 bytes и Windows12,406,075,392 bytes
(task `91cb8830-891e-478d-843e-625ce2e24cbd`, succeeded/0; только disk query).
Диски выше6GiB, cleanup не было. Перед actual gate Windows preflight измеряет
их снова. Installed d052 подключён/active0, runtime не менялся.

## Следующий запуск

После final checks/review и clean commit экспортировать distinct **producer**.
Готовый target receiver2 — commit1370d6b, ZIP
`winbridge-20261025T092432Z-1370d6b-257867fc.zip`; SHA и environment pins в
[checkpoint](resume-2026-10-25-state-handoff.md#5-готовые-inputs-следующего-native-gate).
1c23d48 содержит Ready1 и **не подходит** для state handoff; observer проверяет
contract2 в обоих новых archives до submit.

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/PRODUCER.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T092432Z-1370d6b-257867fc.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-state
~~~

Перед submit сообщить: нужен только installed worker, desktop не нужен,
UI-окон не будет. Сохранить task/run и наблюдать тот же запуск; при потере
observer добавить `--verify-run RUN` к той же команде. Unknown outcome не replay.
Native succeeded/audit failed означает сначала readonly исправление checker/
того же run. Не менять installed runtime и не чистить Windows directories.
После terminal сообщить: desktop можно закрыть, installed worker оставить.

## Review и ограничения

Пробы проверяют реальные handles, но сами не выдают authority. Fencing/live
child/original monotonic window перепроверяются вокруг probes и непосредственно
перед release/send; trace input не продлевает deadline. Before/unconfirmed/
after release failures сохраняют прежнюю residual custody, не relock/retry.
В target fixture validation hooks не нужны: её receiver2 уже реализован.

Положительный state gate докажет **физический handoff**, а не writable Store,
JournalAuthority/epoch, local IPC/controller service или эксплуатационный Job
lifetime. Далее отдельный writable contract/transaction, local/controller/
lifetime/commit, U2 fault/management matrix и U3/U4. Updater целиком не готов.
