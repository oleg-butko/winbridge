# Время жизни activated worker и повторные обновления

2026-10-28. Следующий блок после Main26. Это проект решения, **не реализованный
контракт и не разрешение менять работающий Windows runtime**. Public install off.
Main26 доказывает session того же resident child, но оставляет root
AwaitingController/local7 и автоматическое завершение child вместе с updater.

## Подготовлен test-only OS spike

`crates/platform/src/lifetime_spike.rs` добавлен только под `cfg(windows,test)`.
MSVC Clippy и Linux check прошли; **настоящего Windows execution ещё нет**.
Production platform/process APIs, updater schemas/private contracts и limits не
менялись. Это lower-level process experiment, не работающие keeper actors.

Два explicit ignored tests:

- `lifetime_spike::resident_keeper_launches_next_updater_outside_old_worker_job_without_breakaway`:
  original keeper удерживает worker Job, worker инициирует следующий запрос,
  keeper запускает следующий independent process вне старой worker Job/domain;
  следующий worker и keeper живы после штатного выхода старого keeper. Две
  worker Job остаются соседними, transport Job охватывает всех helpers до cleanup.
- `lifetime_spike::keeper_process_loss_kills_waiting_worker_without_recovery_or_limit_changes`:
  original keeper вызывает exit73 **без Rust destructors**; last-handle closure
  исходной Job завершает worker, автоматического восстановления нет.

До смены поколения выполняется отдельная реальная отмена task+grandchild в
execution domain. Original keeper/worker и next keeper/worker связаны actual
PID/birth/host/session; следующий keeper query-ит отсутствие членства в старой
Job/domain. Сохраняются flags KILL_ON_JOB_CLOSE, чтение обоих output потоков,
bounded local waits, positive empty/exit observations. Никакого breakaway, changes
limits, network, installed worker или удаления Windows validation directories.

PS runner `scripts/test-update-lifetime-spike.ps1` и
`scripts/run-update-lifetime-spike-native.py` подготовлены. Они проверяют source
archive/manifest,≥6GiB, берут exclusive retained build-cache lease и выполняют два
exact native tests. Source сначала копируется локально и повторно хешируется до
распаковки; существующий case/report сохраняется. Build/test output читается,
ограниченные helper logs и опубликованные JSON/ready копируются в отдельный report.

`verify-update-lifetime-spike-native.py` не делает Windows RPC: saved fenced2
request/ACK/terminal, source pin, namespace, native selection, обе OS-цепочки,
PID/birth/host/session/Job/domain/exit/unchanged limits и health installed worker
проверяются отдельно.36 negative data mutations и8 synthetic observer tests
passed; historical PID reuse допустим при другой birth, concurrent PID alias
отклоняется. Эти Python tests не являются OS proof. Native execution ещё впереди.

Перед запуском — clean source export с `--include-built-ui` для существующего
строгого archive verifier (UI не собирается/не запускается), pinned PS parser и
storage check. Installed worker оставить, desktop не нужен/окон нет. Hashed
platform helper не подключается к сети, networking slot204 не нужен.

```bash
python3 scripts/run-update-lifetime-spike-native.py \
  --source-archive <clean-reviewed-source-with-built-ui.zip> \
  --cache-run 20261017-061527-f4639971 \
  --controller-config .winbridge/controller-snapshot.local.json
```

Сохраняется run-info в `.winbridge/update-preparation/lifetime-spike-<run>/`;
unknown acceptance/timeout наблюдается по **тому же** context/key/task, без resubmit.
При verifier-only ошибке исправлять reader и перечитывать artifacts; Windows
команды не повторять. Финальный report — `WinBridge/workflow-validation/<run>/result`.

`WINBRIDGE_LIFETIME_SPIKE_INPUT` — test-only JSON
`{"schemaVersion":1,"root":"<fresh existing local test base>"}`. Каждый test
создаёт create-only child directory `two-generation`/`keeper-loss`; существующий
case отказывает. Helper root canonicalize-ится и сверяется с metadata, original
Domain живёт до выхода old worker. Next keeper detach происходит **до** отправки
одноразового start marker, чтобы потерянное readiness evidence не убило accepted
child. Keeper-loss worker собственным timer не завершится ранее300s, больше
45s+45s окон наблюдения parent; его timeout не подменяет last-handle closure.
Source/target runtime, updater journal/schema и installed worker не меняются.

Пример exact selection (только внутри подготовленного native runner):

```powershell
cargo test -p winbridge-platform --lib --locked --offline -- --exact lifetime_spike::resident_keeper_launches_next_updater_outside_old_worker_job_without_breakaway --ignored --test-threads=1 --nocapture
cargo test -p winbridge-platform --lib --locked --offline -- --exact lifetime_spike::keeper_process_loss_kills_waiting_worker_without_recovery_or_limit_changes --ignored --test-threads=1 --nocapture
```

На ревью до native run исправлены две ошибки самого spike: query уже уничтоженной
empty old Job трактуется как absence только при ERROR_FILE_NOT_FOUND, а transport
pipe readers join-ятся лишь после завершения всех helpers. Original execution
Domain handle удерживается в old worker до его выхода, чтобы следующая проверка
не могла случайно открыть заново пустую Job под прежним именем.

## Проблема, которую надо решить до нового DTO

`ActivationProcessGuard::drop` вызывает `ManagedChild::close_unconfirmed`:
он явно вызывает TerminateJobObject и закрывает kill-on-close Job. Одного
закрытия private pipe или сохранения controller receipt недостаточно для жизни
worker после выхода supervisor. Флаг Drop и OS Job — две разные причины завершения.

Простое копирование Job handle в target решает только первый выход updater.
Новый worker остаётся участником activation Job; его следующий updater наследует
эту Job. При закрытии последнего handle старого worker следующий updater также
может погибнуть. Нужна проверка **двух последовательных обновлений**, а не только
survive-parent-exit. Постоянное накопление ancestor handles не выбираем как
долгосрочное решение. Это вывод из текущего способа запуска
и правил [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
и [Nested Jobs](https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs).

Три разных объекта не смешивать:

- Temporary activation Job содержит новый trusted worker на время проверки.
- Worker execution domain и task Jobs владеют выполняемыми пользовательскими
  процессами; их kill-on-close и запрет breakaway сохраняются.
- Внешняя Job транспорта/пользователя не принадлежит updater; её limits и handles
  нельзя изменять или использовать как основание для production readiness.

## Выбор для первого native spike

Сначала проверить **resident lifetime supervisor**: уже запущенный independent
updater остаётся владельцем original activation Job после final commit. Это
обычный user process, не Windows service, не отдельная установка и не новый
сетевой listener. Он спит на событиях; его PID/состояние и причина присутствия
видны в диагностике. Process Job/task limits остаются прежними.

Для следующего обновления новый supervisor должен запускаться **из keeper**,
который находится вне activation Job, а не из worker — участника этой Job.
Тогда поколения workers получают соседние temporary Jobs, не бесконечную цепочку
ancestor Jobs. Это меняет старое внутреннее ожидание «updater обязательно может
полностью выйти после update»: заканчивается update operation, но её supervisor
остаётся контролировать lifetime worker. Цена — один дополнительный процесс и
новый authenticated local delegation path при следующем update. Desktop и MCP
не должны выдавать такой процесс за незавершённое обновление или скрывать его.

Передавать **тот же** старый Job handle следующему supervisor сейчас не требуется:
старый keeper держит старое поколение до проверенного shutdown, новый keeper
владеет только новым child. После подтверждённого empty старой Job прежний keeper
может завершиться. Максимум два keeper во время перехода; lost ACK не создаёт
второго spawn, холодный JSON не восстанавливает owner. Этот предел должен быть
проверяемым свойством ledger/actor, а не надеждой на normal happy path.

Resident role нельзя реализовать просто сохранением полного ActivationProcessGuard
навсегда. После final commit нужен отдельный non-clone runtime owner:
Job/process/output cleanup и authenticated keeper endpoint остаются живы, а
runtime/config file pins и exclusive updater journal authority освобождаются по
проверенному переходу. Иначе они заблокируют замену EXE/config или открытие
updater journal в следующем поколении. Cold status такого keeper не создаёт.

Новое поколение получает следующую authority/epoch через явный handoff, старый
keeper после передачи не сохраняет права изменять updater journal. Его право
закрыть именно старый original Job — OS custody, отдельно от journal authority;
оно не даёт закрыть новый child, открыть старый Store или снять новую lease.
Next-update delegation требует нового admission/current context, не повторного
использования истёкшего activation permit. При гонке двух controller оба запроса
идут через обычную worker queue/idempotency и только один durable launch intent.

Альтернатива — retire собственной temporary activation Job, снимая
KILL_ON_JOB_CLOSE после нового durable intent, и позволить updater выйти.
Она требует изменения принятого cleanup механизма, отдельного анализа
crash windows и всё равно не удаляет association с Job: последующие дочерние
процессы наследуют ancestry. Поэтому этот путь сейчас **не выбираем и не
исполняем**; generic setters limits/breakaway не добавляются.
[Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
объясняет last-handle lifetime; [Nested Jobs](https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs)
объясняет наследование ancestry. Это основания для оценки, а не native proof.

## Порядок реализации и проверки

1. **Изолированный platform spike без изменения limits.** Original keeper
   остаётся вне scoped activation Job, child сохраняет те же PID/Job. Доказать,
   что keeper удерживает Job, его закрытие завершает child, а task execution
   domain остаётся отдельным. Helper не подключается к сети; новые firewall
   rules не нужны. Production ManagedChild и Main25/26 cleanup пока не менять.
2. **Повторное обновление.** Worker запрашивает следующий update у keeper;
   именно keeper запускает другой independent process вне старой activation Job.
   Старый worker штатно закрывается, а новый supervisor продолжает работать и
   создаёт следующий child в соседней Job. Доказать отсутствие второго supervisor
   в старой Job, unchanged external Job, отмену nested task tree и empty cleanup
   обоих поколений. Не включать breakaway и не менять внешние Jobs.
3. **Модель/контракты.** После выбора native механизма добавить typed lifetime
   intent/receipt/FSM и отдельный private contract. Привязать confirmed controller
   record + local7 + original process/Job + source owner/epoch + original deadline.
   В новом DTO использовать flat digests и validated retained records вместо
   повторного вложения полного service/controller graph. Сохранённый readiness
   receipt не является standalone process ownership.
4. **Durable ledger/admission.** Lifetime intent до смены роли owner. Fresh permit
   только из original resident guard; Existing/cold JSON не восстанавливают effect.
   Номера новых schema/private/view выбирать при реализации. При изменении БД —
   actual retained21 migration/refusal отдельно. Worker-to-keeper next-update
   delegation — отдельный typed contract с persist-before-spawn, owner/epoch/
   process/context fences и двумя одновременно живыми keeper максимум.
5. **Receiver и runtime.** Original receiver подтверждает готовность перейти
   из bounded verification в supervised runtime. До durable decision сохраняются
   parent/deadline guards. После принятого decision появляется новый режим
   supervised quarantine, который требует живого original keeper, той же Job и
   same Store/lock/generation/authority. Никакого нового startup timeout, отключения
   проверок через `None`, re-open Store или восстановления owner из JSON.
6. **Final commit/release.** Независимо наблюдать typed lifetime readiness по
   normal IPC перед root ControllerConfirmed. Lease и dispatch остаются закрыты
   до separate scoped release для exact authority/incarnation. Late historical
   receipt не создаёт root commit. Потеря keeper — потеря текущей физической
   custody, а не повод автоматически повторять команды или запускать копию worker.
7. **Actor/view и full native.** View разделяет завершённую update operation,
   живой lifetime supervisor, saved history и current child. Новый full Main gate
   на stable runtime-test-204: lost ACK, keeper exit/crash на границах, cold no
   replay, reconnect и два поколения. U2 faults, U3 attention UI и U4 actual
   installed self-update остаются обязательными до public readiness.

## Окна неопределённости, которые нельзя скрыть

| Граница | Требуемое поведение |
| --- | --- |
| До lifetime intent | Старый kill-on-close/cleanup; никакого detach. |
| Intent есть, receiver не armed | Нет смены lifetime mode или повторного send из cold. |
| Receiver armed, mode ещё не принят | Parent loss остаётся verification failure; original window не обновляется. |
| Lifetime decision неизвестен | Indeterminate, current Job/PID наблюдаются; запрет root commit/replay. |
| Supervised, root ещё не committed | Quarantine под original keeper; tasks закрыты, нужна диагностика/reconciliation. |
| Root committed, scoped release не подтверждён | Не показывать готовность dispatch; release наблюдается отдельно. |
| Второй updater во время выхода предыдущего worker | Updater остаётся жив, не владеет task domain и не меняет внешний Job. |

Этот план начинается с OS spike: заранее реализовать большой SQL/protocol layer
под непроверенный lifetime механизм было бы дороже переделывать. Actual Main26
и retained20→21 evidence остаются отдельными проверками и не переигрываются ради
документации или Python observer fixes.
