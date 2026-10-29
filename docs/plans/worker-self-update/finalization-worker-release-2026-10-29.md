# Same-Store release и промежуточный режим worker

2026-10-29. Продолжение [ledger23](finalization-ledger-2026-10-29.md) после
[actual retained22→23 migration](finalization-migration-2026-10-29.md).
Source worker19/updater23. Это внутренний runtime-код: mutable V8 decoder/marker,
physical decision factory, Active, keeper conversion и public install ещё выключены.
Installed worker0.2.0/source48 остаётся прежним; Windows-команд в этой части нет,
рабочий worker оставить запущенным, desktop не нужен.

## Что реализовано

`apps/worker/src/store/activation_finalization.rs` выполняет отдельную
транзакцию на уже открытом Store. Она не открывает новую базу, не получает
новую authority и не повторяет startup/recovery. Проверяются исходные service,
writer activation, shutdown history, schema/header/authority и точный
Quiesced lease; target manifest и release key выводятся из frozen finalization
command. Транзакция повторно проверяет отсутствие accepted work/logical owners.
Ordinary post-shutdown Release по-прежнему отвергается.

Lease Released и существующий immutable worker command receipt записываются
в одной IMMEDIATE transaction с проверкой числа строк и readback. Формат schema19
не меняется. Только подтверждённый SQLite COMMIT возвращает non-Serde/non-Clone
`ActivatedReleaseCommit`. Existing receipt возвращается как данные, а не новый
permit. Ошибка COMMIT означает необходимость observation; автоматическая повторная
доставка эффекта не разрешается. Три checkpoint позволяют повторить внешние
OS/committed-updater-graph проверки до и после SQL writes без повторного Store lock.

Worker journal хранит derived Release key/payload. Полный V8 command остаётся
в immutable updater23 intent и в original runtime Pending owner: перед вызовом
Store owning coordinator обязан сверить оба. Один SQL receipt не доказывает
весь V8 causal graph и никогда не означает Active.

Native checkpoints также требуют пустые process/readiness/recovery/maintenance
registries, нулевой active_jobs и actual Empty execution domain. Registry locks
берутся через try_lock: некоторые существующие пути идут registry → Store,
поэтому ожидание registry под Store создало бы инверсию порядка. Busy/poisoned
означает отказ, а не предположение о пустом реестре. Сейчас checkpoints повторяют
полную проверку runtime/graph; их длительность нужно измерить в следующем native
gate, не выдавая cross-check за доказательство допустимой Windows latency.

`ServiceLifetime` дополнен `ReleasePending`. Этот режим удерживает исходный
ResidentLifetime/keeper и полный command. Вход возможен только из того же
Supervised Arc owner до SQL release; replay, foreign owner и saved release
не создают новый mode. Internal Windows coordinator дополнительно требует
sealed OS peer исходного keeper, живой loaded child в original Job, runtime/config
pins, instance lock и exact committed pending ledger23 graph. Decoder/mailbox
пока отсутствует: эти методы не достижимы через CLI/MCP/IPC.

Watchdog в ReleasePending допускает только исходный Quiesced lease либо exact
same-key Released receipt. Другой active update и foreign release отвергаются.
Он не переводит режим в Active; V6/V7 queries также этого не делают. Dispatch,
maintenance и mutations остаются закрытыми, config pins сохраняются.

## Ревью и реальные риски

Исправлена гонка Store/mode: прежде watchdog копировал Supervised до получения
Store mutex. После ожидания он мог увидеть уже Released lease и завершить
исправный worker. Теперь порядок watchdog/seal/release одинаковый: Store → mode;
mode snapshot и проверка lease сериализованы. Нельзя вернуть старый порядок,
удерживать mode mutex во время ожидания Store либо проверять pending через старый
exact-Quiesced reader.

SQL tests проверяют first commit/duplicate, shutdown history и user pause,
original authority/instance lock, отказ при ignored UPDATE/INSERT, rollback на
каждом checkpoint,15 scope mutations, foreign keys/lease, initialization,
stale authority, readonly connection, unexpected accepted work и future command.
Runtime tests проверяют SQL/mode gap после исходного deadline, сохранение
readonly gates и запрет Active через старые queries, foreign Arc/replay/dead keeper.
Это настоящая SQLite/IPC/locked-state работа с synthetic runtime/OS metadata;
она не заменяет actual Windows V8 gate.

Исходные неуспешные test logs сохранены. Одна фикстура сначала пыталась поменять
epoch без нового authority ID и была остановлена SQL trigger. Две runtime
фикстуры удерживали свой дополнительный Worker Arc при close и получили честный
«handlers have not all stopped»; тесты исправлены освобождением собственного Arc.
Production close/authority guards ради тестов не ослаблялись.
Фикстура unexpected queued task также сначала нарушила revision trigger;
исправлена записью следующей ревизии, без удаления trigger.

Проверки: `cargo test -p winbridge-worker --lib --locked --features update-validation`
—144 passed,3 ignored,9 новых tests. Linux worker all-target Clippy с той же
feature и `-D warnings` passed, Windows MSVC all-target Clippy также passed;
это компиляция, не Windows execution. Первоначальный MSVC test-target pass
обнаружил unused native methods без V8 route: scope dead-code allowance добавлен
только для этих ещё не подключённых native test paths. Fmt и scaffold passed.
Logs/measurement/readonly installed Info:
`.winbridge/review-2026-10-29/finalization-worker-release-*`; финальный test log
`finalization-worker-release-audited-tests.log`. До первой сборки Debian15,978,655,744
и shared17,935,618,048 bytes available; повторное измерение15,926,366,208/
17,916,817,408. Очистка не требовалась. Readonly installed Info подтвердил прежние
source48/build1793126009319459400/gen946/context5a8e и connected/error=null.

## Продолжение без потери контекста

1. Подключить bounded coordinator mailbox к original RunningLocalService;
   mutable V8 request должен нести sealed LocalPeer от normal IPC. Старый private
   V6 channel с unread Ready body для этого не использовать.
2. Добавить consuming original-guard factory: fresh V7 + original custody/config
   checkpoint → FinalizationDecisionPermit → atomic root Committed/ReleasePending.
   Partial/lost ACK не даёт повторного launch или fresh delivery permit.
3. Перед Active освободить родительские verification/config pins и последний
   LocalService config pin. Реализовать known-release seal и отдельную explicit
   scoped reconciliation при неизвестном COMMIT, только на том же Pending owner.
   Readonly observation не меняет mode и не восстанавливает Store/keeper из JSON.
4. Заменить только operational readonly gates на typed Active permission;
   startup origin/controller ownership/Ctrl+C ветки не менять механически по
   `local_activation.is_some()`. Не запускать второе network controller manager.
   Active watchdog опирается на original OS keeper/Store и исторический release,
   допускает desired config save и следующую updater authority.
5. Actor/view, permanent physical keeper, закрытие updater writer и всех authority
   Arc, next-generation delegation и U2–U4 остаются обязательными. Следующий
   actual gate должен проверить config save, task execution, unknown release/ACK,
   partial reads и две последовательные генерации, включая FileShare assertions
   из предыдущего config-pin handoff. Использовать только stable runtime-test-204.
