# Compatibility до production drain: policy и дальнейшая интеграция

2026-10-23. [Native discovery gate](windows-updater-support-gate-2026-10-23.md)
завершён8/0. Следующий блок начат с pure typed policy; journal15/worker16,
supervisor17/view9 не меняются. **Durable admission, новый actor и обязательная
проверка перед production drain ещё не подключены.** Public install off.

## Реализованная модель

`crates/protocol/src/update/updater_compatibility.rs` задаёт strict DTO:
`UpdaterCompatibilityPolicy::SameSchemaInstallV1`, шесть execution contracts,
typed blockers и passive assessment. Нет canInstall/permit/custody; JSON этой
оценки нельзя использовать как разрешение. Неизвестная policy, blocker, contract,
лишние поля и boolean вместо counter отвергаются. TypeScript генерируется из Rust.

`crates/core/src/update/updater_compatibility.rs` — чистая функция без I/O.
Она собирает причины целиком и в стабильном порядке:

- Неподдерживаемый query или artifact descriptor/target/role.
- Незафиксированная/dirty source identity; SHA-1 и SHA-256 commit IDs допускаются
  согласно существующему source manifest, manifest SHA обязателен.
- Validation build: diagnostics не может стать production candidate.
- Несовпадение updater journal schema; нулевой expected schema тоже blocker.
- Отсутствующие или несовпадающие версии backup15/preparation16/replacement17.
- Отсутствующие candidate activation/controller/desktop contracts и отсутствие
  их реальных drivers у текущего updater. Заявление кандидата о будущей версии
  не включает отсутствующий локальный driver.

Policy означает полный same-schema binary set с desktop coordination. Условие
«desktop сейчас закрыт» не отменяет необходимость правильно обновить/восстановить
companion. Отдельную облегченную установку без этой гарантии молча не вводить.
Текущий updater остаётся blocked даже при совпадении15/16; activation/U3 не готовы.
Поддержанные execution versions добавлять лишь вместе с реальным driver и gate.

При review исправлена несогласованность SHA-1-only с уже существующей политикой
manifest40/64. Regression покрывает обе длины и отсутствие source identity.
Пять core tests проверяют blockers при matching schema, ложных future contracts,
dirty/validation/wrong target/schema и unknown versions. Protocol test проверяет
strict roundtrip и отказ authority/unknown fields.

Проверки: core/protocol целиком **107/0** без optional features и **108/0** с
jsonschema, Linux/Windows all-target Clippy с jsonschema/typescript, generated
TypeScript check и TS typecheck прошли. В старом projection test исправлен
unguarded вызов schema-hash API: hash assertion выполняется при jsonschema,
сама projection проверка — в обоих режимах. Frozen fenced-RPC hash совпал.
Native gate8 выше относится к f1e2f10 и **не является native проверкой этой policy**.

## Следующий вертикальный блок

1. Добавить **новый** durable support contract, сохраняя семантику frozen health
   и прежних internal actions. Health CandidateVerified доказывает worker health;
   новая compatibility — отдельная prerequisite перед production BeginDrain.
2. Admission связывает installation/config/root revision, update/launch/owner/
   epoch, exact artifact set, source и завершённый health record. Intent/attempt
   сохраняются до ACK и до каждого процесса; existing/reopen не дают fresh permit.
3. Query запускать через journal-bound `ProbeObserver` с intent/PID-before-resume/
   Job cleanup, как health probes. **Недостаточно просто вызвать unjournaled
   `updater_support::inspect` и сохранить его JSON как approval.** Общий decoder
   и image/process binding переиспользовать; expected descriptor уже связан с
   frozen artifacts. После crash неизвестный probe не повторять автоматически.
4. Сохранять immutable support observation и пересчитанную policy вместе с
   binding. Cached JSON, empty blockers или MatchingSchema не являются authority.
   Перед новым drain revalidate owner/root/config/source/images/health; stale
   build/epoch/revision блокируют его. Validation build отвергается всегда.
5. Новые action/view/capability согласовать versioned способом. Старые native
   fixtures не переобъявлять production-ready и не менять frozen contracts молча.
   Unsupported/missing marker должен быть видим до остановки old worker.
6. Если добавляется schema16, обновить synthetic downgrade fixtures и выполнить
   actual retained15→16 + old15 refusal на consistent копии. Original reference
   не менять; migration не выдаёт permission для прежних pending attempts.
7. Native Main gate: valid query + missing future contracts дают durable blocker,
   old worker продолжает обслуживать задачи, BeginDrain не отправляется. Проверить
   stale owner/image/health, duplicate/lost ACK и crash query/receipt boundaries.

Далее activation из того же resident replacement guard: suspended PID-before-resume,
точный lock/file-handle handoff, local health/controller receipts. Затем remaining
U2 faults, U3 desktop/migration/recovery и один согласованный U4 bootstrap.
Installed worker пока обновлять не требуется.
