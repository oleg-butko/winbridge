# Lifetime V6: same-child quarantine и независимый normal IPC

**Обновление2026-10-29:** [Main27 Windows gate](main27-native-2026-10-29.md)
пройден на reviewed merge5cbc7b0:208 native tests/60 files twice, independent
reread identical. Фикс WIP observation подтверждён; private Ready loss/permanent
keeper/commit/release ещё впереди. Ниже — исходный source checkpoint.

2026-10-28, Debian .204, ветка `feat/updater-lifetime-runtime-204`, base main94629b2.
После pull изучены22 новых main commits, в том числе original lifetime admission,
Main26/OS/retained21→22 evidence. Предыдущий review f069697 уже включён в main.
Продолжен [original admission checkpoint](lifetime-admission-2026-10-28.md).

## Ревью и исправление

В раннем private receiver V1–V3 вычислялся original monotonic deadline, но connect,
challenge/Ready/handoff/writer IO повторно получали **полный** startup_timeout_ms.
Это не позволяло родителю законно подтвердить просроченную activation, но могло
удерживать child и ждать канал дольше исходного бюджета; поздний Ready тоже мог
быть записан после дорогой runtime-проверки. Теперь каждое IO получает остаток
того же frozen deadline, с UTC rollback/expiry и overflow checks. Persisted120s
не превращаются в240s. Новый budget на приём очередного frame не создаётся.
Pure clock regression не спит: проверяет10→7s, обе expiry границы, rollback,
overflow и сохранение исторического120s окна.

Включённый main950-test checkpoint проверяет предыдущий source, не этот новый
runtime. Main26/OS/migration gates terminal; ради ревью они не replay-ились.

## Реализовано в source

- Private V6 decoder продолжает V5 на **том же authenticated stream/parent** и
  RunningLocalService. Initial delivery требует original Issued lifetime,
  неизменные physical/writer/service/local/controller команды и current schema22.
  Snapshot и оба original clocks проверяются перед entry.
- `ServiceLifetime::Verification → Supervised(ResidentLifetime)` — отдельный
  resident state. Deadline остаётся исходным Instant; `None`, новый Store open,
  begin_authority, instance-lock acquisition и новый PID не используются.
  Transition проверяет same loaded identity/config/context/generation/authority,
  current controller binding, original keeper PID/birth/host/session и Job.
  Atomic seal происходит внутри original monotonic/UTC entry window.
- `PinnedProcess::require_job_membership` открывает только **существующую** Job
  с query правами и сразу закрывает временный handle. Он не создаёт Job, не
  меняет limits и не оставляет child постоянным дополнительным владельцем Job.
  Это сохраняет last-handle kill-on-close keeper. Права проверены по
  [IsProcessInJob](https://learn.microsoft.com/en-us/windows/win32/api/jobapi/nf-jobapi-isprocessinjob),
  связь последнего handle с kill-on-close — по
  [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
  Дополнительные membership/refusal checks встроены в существующий Windows
  platform test, не создавая нового тестового process scenario.
- После seal entry receipt immutable. Ошибка записи private Ready не отменяет
  принятую quarantine decision. Child остаётся под original keeper/Job до его
  cleanup/loss; normal endpoint доступен для отдельного наблюдения. Watchdog
  каждые250ms проверяет pinned parent и ту же authority/lease, без idle image hash
  scans. Закрытые roots/dispatch/admin остаются закрыты после verification expiry.
- Normal IPC `lifetimeObservationVersion:1` — отдельный strict readonly envelope,
  не новый LocalRequest/RPC action. Версии/unknown fields/normal mode/чужой command
  отвергаются. Только уже resident mode может вернуть receipt; cold query не
  создаёт keeper или lifetime. Отдельный observedAt относится к текущему
  наблюдению; entry receipt по-прежнему относится к исходному окну.
- Continuation snapshot допускает тот же causal Issued/Indeterminate/Confirmed:
  потеря ACK или запись факта не убивает уже принятый mode и не требует повторного
  admission. Полный graph/root/owner/command всё равно проверяется. Этот reader
  возвращает только данные; initial receiver по-прежнему принимает только Issued.
- `ActivationLifetimePreparation::enter_lifetime_blocking` потребляет original
  permit один раз на original IO runtime. После private Ready он независимо
  опрашивает **actual normal endpoint/PID**, проверяет immutable receipt и Info с
  closed dispatch, затем сохраняет lifetime fact. Ошибка сохраняет тот же owner
  и Indeterminate по прежнему key, без send/replay из cold/Existing.
- В worker сохраняется verbatim V6 marker только вместе с настоящим decoder.
  Dynamic sender pattern остаётся отдельным. Source worker19/updater22,
  **private6 receiver/sender; actor26/view18 прежние**. Новых schema нет.

## Границы и продолжение

Это source runtime/trusted API, **не выполненный Windows V6 gate**. Actor26 ещё
не отправляет новую lifetime операцию из worker mailbox; normal observation не
является root commit, keeper-role transfer или maintenance release.

Следующий вертикальный шаг:

1. Actor27/view19: fresh lifetime operation из того же original controller owner,
   отдельные saved/current mode/custody diagnostics; старые views1–18 frozen.
   Observe не создаёт fresh permission, повтор ключа не повторяет send.
2. Новый Main27 на clean source19/22 с настоящим V6 target, stable runtime204,
   original240s и существующим отдельным controller/preflight. Проверить private
   Ready loss, normal IPC/PID, original window/refusal, quarantine после expiry,
   keeper loss, duplicate/cold no replay и closed roots. Старые Main26 и migration
   archives использовать только как retained evidence, не как живой owner.
3. Consuming keeper role должен после проверенного commit удержать original Job/
   process/channel/output cleanup, но освободить file/config pins и updater
   journal authority. Обычный ActivationProcessGuard Drop всё ещё закрывает Job;
   библиотечный lifetime entry не является готовым permanent keeper.
4. Final root commit/scoped release, next-update delegation, max2 keepers и два
   поколения, remaining U2/U3/UI/U4 actual update. Public activation/install off.

Установленный worker0.2.0/source48c834c и production controller не заменяются.
Новых Windows tasks/UI, изменений firewall/служб/slots в этом source шаге нет.
Рабочий worker оставить запущенным; desktop не нужен.

## Проверки

Focused synthetic worker tests проверяют actual Store/IPC/fences, original window
и no query-based adoption, повторный entry, quarantine после verification expiry,
неизменную authority и keeper-loss watchdog. Runtime/keeper metadata в этих Linux
fixtures synthetic; это не native authentication/membership evidence. Новые
updater tests различают immutable entry receipt/current observation и lost-ACK
continuation, private Ready без normal IPC не подтверждает факт.

Полный `cargo test --workspace --features winbridge-worker/update-validation
--locked`: **958 passed,17 ignored**,125 result groups, ошибок нет. Полный лог:
`.winbridge/lifetime-runtime-review.C20DBH/workspace-verified.log`.
45 Python regressions проверили service/controller/lifetime/migration proof
readers и multi-controller inputs. TypeScript strict, UI build и Linux workspace
all-target Clippy с update-validation и `-D warnings` прошли. MSVC workspace
all-target Clippy с теми же features/`-D warnings` тоже прошёл; это cross-check,
не выполнение Windows tests. Generated Rust/UI contracts, `cargo fmt --all
-- --check`, `git diff --check` и scaffold321 прошли. Clippy logs лежат рядом с
workspace log: `clippy-linux.log`, `clippy-windows.log`.

Первый дополнительный authority fault-injection менял только epoch и был
правильно отвергнут существующим SQL trigger. Исправлен сам тест: одновременно
меняются epoch и authority UUID. Проверка теперь подтверждает отказ и до entry,
и после resident seal; production trigger не ослаблялся.

Перед тяжёлыми проверками свободно Debian26,005,069,824 bytes,
Windows/shared24,253,890,560 bytes; перед финальным Clippy Debian25,604,317,184 и
Windows/shared24,873,254,912 bytes. Очистка не требовалась. Readonly запрос к
установленному worker0.2.0/source48c834c подтвердил прежний gen946f1128 и обе
connected связи .203/.204 с текущим `error:null`. Рабочий worker не заменялся.

Итог: source receiver/sender готовы к следующему actor/native gate; готовность
всего self-update и native V6 этим checkpoint не объявляется.
