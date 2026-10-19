# U1: authenticated live runtime identity

2026-10-18. Trusted internal API. Worker control v2 добавляет только
`ObserveRuntime` → `RuntimeIdentity`; v1 сохраняет lease commands и Snapshot/Error
JSON. Перепутанные versions/results отвергаются `into_lease`/`into_runtime_identity`;
нет downgrade к effect command. Legacy RPC/Info/MCP и schemas worker/updater 15/7
не меняются. Normal Main пока не включает update control автоматически.

Worker берёт identity из прикреплённого guard, а не из caller JSON. Проверяет
exact active supervisor PID/birth, worker context/generation/build, owner/epoch,
installation/scope и frozen config. Manifest должен принадлежать той же installation.
Guard process/build совпадают с immutable `SupervisorWorkerBinding`. Пять file pins
удерживаются worker; короткий mutex освобождается до hashing, Store не удерживается.
После I/O повторяются authority/config/context checks. Pending config restart
отклоняет fresh observation. Resource reservation для readonly observation не нужна;
ограничения выдачи source/build grants сохранены отдельно.

Supervisor internal v6 `ObserveWorkerRuntime` вызывает этот канал своим настоящим
PID. `SupervisorHandle::runtime_identity` возвращает DTO; `observe` позволяет
параллельно читать прежний v1 status. Один async readonly query обслуживается вне
actor mailbox; duplicate получает Busy для readonly повтора. JoinSet отменяет
ожидающий IO task при выходе supervisor. Updater image SHA в Registration сохраняет
прежнее значение; worker image SHA возвращается отдельно.

Runtime identity не является сохранённым baseline или разрешением менять файлы.
Пути из DTO не назначают executable/root policy. Следующий driver закрепит local
known manifest/четыре images и live binding до SourceApply с отдельной durable
receipt и проверкой текущей фазы. Прежние операции без такой receipt не допускаются
к installation задним числом. Новая worker activation не перепривязывает старый
`SupervisorSpec.worker` к другому generation/PID/context.

## Проверки и границы

Локальные IPC tests проверяют несовместимые lease/runtime versions, forged peer/
birth/context/build/epoch, config changes, отсутствие guard, нулевые lease/slots и
неизменный updater snapshot. С удержанным worker reply supervisor отвечает на status
и отклоняет duplicate. Protocol tests фиксируют прежний JSON и запрет interpretation
runtime observation как lease. 490 workspace tests, Linux/Windows Clippy all-targets, fmt/contracts/typecheck,
scaffold, Python export/runner tests и PowerShell syntax passed. Initial full run
нашёл transient lock contention между независимыми recovery fixtures; cases
сериализованы, production lock policy не менялась. Все девять recovery cases и
полный повтор passed. Windows cross-check включает positive fixture; выполнение
этого native fixture ещё впереди.

Положительный explicit Windows test запускает копию **test harness** в отдельном
управляемом Job, без breakaway. Сам процесс содержит настоящий Worker actor,
использует startup guard с собственными PID/File ID/build и настоящий supervisor
IPC. Другие role descriptors fixture синтетические: это не health четырёх WinBridge
exe и не production bootstrap. Тест сохраняет `workerEntryPointIsTestHarness` и
`syntheticOtherRoleDescriptors`, проверяет смену worker/updater config после hashing,
доступность Store и status, отсутствие lease/slot/effects. Source A и runtime C
различаются намеренно. Parent ограничивает время и чистит только своё дерево.

Native combined gate добавляет эти tests, три protocol cases и сохраняет отдельный
`runtime-identity-control-proof.json`. Положительный native run ещё не выполнен.
Actual product Main/pins проверены предыдущим [gate](windows-runtime-startup-gate-2026-10-18.md);
новый combined run повторит их на полном candidate. Public install остаётся отключён.

Review для следующего runtime transaction: durable startup marker должен пережить
отсутствующий/mixed manifest после начала install. Нельзя включать legacy serve по
отсутствию manifest посреди cutover. Один `journal init` не объявляет installation,
поэтому writer metadata сам по себе не должен блокировать обычный legacy/dev serve.
Этот guard относится к будущему install transaction; текущий блок exe не заменяет.

Native 5b3d591 run `20261018-201934-bbddb98b` failed: 164 tests passed, positive
fixture получил первую identity и достиг post-hash checkpoint, но single try_lock
соревновался с обычным scheduler. Проверка исправлена на bounded 2 s acquisition,
пока сам runtime query остаётся held. Permanent lock по-прежнему fail до 10 s gate.
Windows Clippy passed, native повтор впереди. 35 ready files сохранены/проверены,
installed worker healthy; full candidate build в failed run не выполнялся.

Native db2e565 repeat `20261018-203256-077d1a67` reached the next query but
failed on a false Busy: response publication preceded JoinSet retirement.
Supervisor now retires the completed task before sending its reply. The mailbox
regression immediately repeats three observations without sleep or intervening
status calls. Busy remains valid while the first query is genuinely active.
The working worker was healthy; only the isolated fixture failed. New native
execution remains required; schema-8 baseline WIP is excluded from this retry.

Combined native retry clean 256b182 passed: [165 tests / 19 groups, full build,
actual Main и pin lifetime](windows-runtime-control-gate-2026-10-18.md).
43 ready files verified; both earlier isolated fixture failures are resolved.
This gate covers schema 7/live identity; schema-8 baseline still needs its own gate.
