# Продолжение: writer24 подтверждён на Windows

2026-10-25, America/New_York. Этот checkpoint заменяет старые NEXT/ACTIVE и
[предыдущий checkpoint](resume-2026-10-25-journal-activation.md).
Пользователь поручил review, исправления, диагностику задержек и продолжение updater.
Он явно разрешил увеличить окно новых попыток120→240s; старые intents не меняются.

## Фактическая готовность

Source updater19 / worker17 / supervisor24 / view16 / physical2 + writer3.
U0/internal U1, final backup, physical replacement/state handoff и **journal writer**
проверены. Новый child атомарно получает epoch+1 с immutable receipt, сохраняет
incarnation/generation/тот же lock, fenced old owner и Quiesced maintenance.

**Normal local service/reconciliation, новая controller session, Job lifetime
transfer, commit/scoped release, remaining U2, U3/U4 ещё не реализованы.**
Public install и advertised activation/controller_confirmation/desktop_update
выключены. Writer receipt не равна whole-update success. Рабочий runtime не обновлялся.

## Последний actual gate

Run `20261025-232746-26bad6d7`, task `a49ade1f-4e35-4eff-84ac-3b8d21c0a9ea`:
**terminal succeeded/0, 193 tests/39 groups/42 commands, все exit0**.
56 ready files дважды сверены. Independent audits:44 replacement +16 writer +18
migration mutations rejected. Actual updater18→19/old18 refusal и worker16→17/
active16 refusal прошли; actual child/Job exit и cold non-replay подтверждены.

Первый observer остановился из-за двух ошибок verifier, а не Windows сценария:
он ожидал reason=None вместо существующих explanatory read-model strings, затем
считал замену1→true пустой мутацией из-за Python equality. Исправлены точные warm/
cold ожидания и typed JSON mutation comparison; regression добавлены. После этого
**тот же retained run проверен readonly --verify-run**, без resubmit/replay,
с полным отрицательным аудитом. Reports/журналы/исходные архивы не менялись.

| Pin | Значение |
| --- | --- |
| Producer | `acf712313405c0e6be04b91893f8ed927772f1ed` |
| Producer ZIP | `winbridge-20261025T232727Z-acf7123-7d95795d.zip` |
| ZIP SHA | `7e8ccc681c030f72194c9359cf646111dd7720bce58652d105c263fbdcc26008` |
| Manifest SHA | `78ed0388b1303d87b8468fec4e8eb6854fd570de821ccd61aa6cd6972e6e1ce6` |
| Receiver | `24136539e0b1c542c48c90bace4bf975bed6c69c` |
| Receiver ZIP | `winbridge-20261025T232657Z-2413653-bf3f5db3.zip` |
| Receiver ZIP SHA | `0ed054c007a0c2b6ee035a71471c97176b15cee0043192ef09b701fcbb694c9f` |
| Receiver manifest SHA | `563efa9a7380da00c8d89a3bf15faac9d45ea18958ee63029fb5cb9222e97613` |
| Report SHA | `f5ed30fb6bfcac8df37d9584aef22d4f2dc644ba66774bb1e5d8cd08c28eecde` |
| Main24 proof SHA | `b71a98c2d6cf41d1c4895d223fe12eb1a34eaa3c4c44ab7157bf1eaeabda34df` |
| Updater migration SHA | `8fddec265ee431f03a2b218ef50a7c87f47a1853f936bcb9d71355126c2d100d` |
| Worker migration SHA | `08892839c47e66f86c095e1f67ea5dde8e01302444f206b19b809b66e2372f0d` |

Shared report: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261025-232746-26bad6d7/`.
Local pin: `.winbridge/workflow-validation/20261025-232746-26bad6d7/activation-preparation-verified.json`.
Observer logs `.winbridge/update-preparation/review-writer24-{native,verify3}.log`.

Readonly recheck, **не запуск нового Windows gate**:

```bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T232727Z-acf7123-7d95795d.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261025T232657Z-2413653-bf3f5db3.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261025-100933-d8a50807/activation-preparation-verified.json \
  --activation-writer --verify-run 20261025-232746-26bad6d7
```

Old physical Main23 pin восстановлен readonly из retained ready files/actual
terminal task:45 files twice/40 state mutations. Он остаётся18/16 reference.
Cache owner report `22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`,
owner taskac5724f1 terminal failed, четыре compiler commands exit0: cache, не proof.

## Что известно о задержках

[Review и изменения](review-2026-10-25-latency.md). Время новых commands суммарно
1445.78s; reported Cargo build517.42s и test bodies858.29s включают вложенные
действия и не являются двумя независимыми stopwatch интервалами.
Main fixture730.28s включает target compiler216.16s (durable wall-clock interval).

| Стадия Main | Время |
| --- | ---: |
| Setup | 6.93s |
| Main source/build/health/drain/shutdown до actual old exit | 374.01s |
| Final backup | 12.81s |
| Runtime preparation | 39.11s |
| Дополнительные shutdown audits | 3.27s |
| Replacement path | 131.90s |
| Activation, writer, postflight и cleanup | 162.26s |

Внутри activation: prepare5.50s, binding44.22s, launch21.96s, handoff23.62s,
writer admission/delivery20.03s. Это вложенные/частично разнесённые observations,
не сумма всей строки Main. Confirmed writer оставил **108.3s original window**:
actual startup age около131.7s. Поэтому прежних120s было недостаточно.

На20 custody checkpoints binding повторно захешировано около2.23GB за14.49s;
physical checks заняли20.55s,23 graph checks17.07s. Причины — повторная полная
проверка файлов и глубокого causal graph, плюс ожидания между фазами. Наивные marker
scans заменены memchr в production supports и native observer, но полные SHA и
fencing/custody не сокращались. Overall speedup не заявлен: прежний Main failed
и не выполнял весь successful postflight; это не matched comparison.

`python3 scripts/summarize-update-timings.py SHARED_REPORT_DIRECTORY` проверяет
ready bytes дважды, выводит команды, nested compiler interval и slow timing edges.
Null у старых hash counters означает отсутствие измерения. Счётчики artifact::pin
не покрывают platform OwnedRuntimeFile hashing. Следующая диагностика может добавить
CPU/wall разделение и подробный replacement trace; нельзя считать warm filesystem
pins разрешением кешировать mutable owner/root/epoch вне одной read transaction.

## Следующий вертикальный сценарий

[Проект normal local service](local-service-activation.md): отдельный typed
contract/intent/history; планируемые updater20/private4/actor25/view17, worker17.
Эти версии **ещё не реализованы**. Реализовать normal IPC/reconciliation в том же
новом child, передавая ActivationJournal + Fresh permit + полный LockedShutdownState
consuming factory, без normal Store::open/begin_authority/нового epoch/второго lock.
Maintenance/user pause/settings сохраняются; roots/dispatch закрыты.
Actual normal IPC должен показать прежние tasks/output, exact loaded process/build/
manifest/config/context/generation/schema. Private Ready не заменяет этот proof.

Затем новая реальная controller session → durable owned activation Job lifetime
transfer → commit/scoped release → remaining U2 faults/management → U3 desktop →
U4 actual stable-runtime self-update → A4.3. Cold data не дают fresh grant. Новый
сценарий создаёт **новый isolated run**: worker прошлого Main24 уже завершён.

## Окружение и проверки

Controller PID5312, `ws://192.168.44.204:1143`, socket
`.winbridge/controller-session/controller-local-development.sock`, config
`.winbridge/controller-snapshot.local.json`. Сверять после reboot; не запускать второй.
Installed source d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2, generation
`66c20413-63a9-4725-b4c1-f5d97cbfac51`, connected/active0/errors=null,
recoveryCoverage.complete=true при финальном readonly Info.
Windows config пользователь выбрал `winbridge-worker.snapshot.local_204.json`.
**Desktop можно закрыть, installed worker оставить работать.** UI окон не было.

Source checks: workspace783/0 (16 ignored) до финального marker regression;
focused activation55/0 и marker3/0 после него; Linux/MSVC all-target validation
Clippy, fmt passed; closing scaffold281/local links passed. После verifier fixes Python27/0 и readonly actual
Main24 audits; новый Rust build для Python-only изменений не нужен.
Windows disk before13540618240 / after37176528896 bytes; initial admission
Debian24930435072 bytes. Cleanup агентом не выполнялся; validation directories,
runtime/config/journal/reports сохранены. Перед следующей heavy build измерить
оба диска≥6GiB, использовать CARGO_INCREMENTAL=0 и один Cargo/cache lease.
System/services/firewall/mount/global config не менять.
