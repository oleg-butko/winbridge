# Activation после проверенного replacement

2026-10-24. **План следующего driver; execution/ledger ещё не реализованы.**
Текущий source updater17/supervisor19/view11. Сначала закрыть actual production
drain17 gate, затем реализовать этот блок. [Общий U0–U4 план](README.md),
[original custody](runtime-replacement-custody.md),
[resident replacement actor](runtime-replacement-actor.md).

## Результат, который нужно получить

После FilesMoved22/VerifyingLocal запустить **новый установленный** worker из
того же постоянного runtime. Новый worker должен проверить loaded set/config,
получить прежний stateDirectory, пройти миграцию/reconciliation и соединиться
с controller, сохраняя запрет новых задач до подтверждённого commit. Supervisor
после успеха может завершиться: работающий новый worker должен сохраниться.
Это отличается от прежнего rehearsal на новом пустом journal.

Не выдавать наличие FSM или нового DTO за реализованный activation contract.
`UpdaterDriverContracts::implemented_windows()` пока сохраняет activation=None.

## Первый элемент модели реализован отдельно

`apps/update/src/activation.rs` содержит strict `ActivationSource`: плоскую
привязку к полному FilesMoved record через domain-separated canonical SHA256.
Она сохраняет installation/set, owner/epoch/launch, ожидаемый parent revision,
config/shutdown/backup digests и **installed** worker/manifest evidence.
Candidate или displaced paths не подставляются. Нет копии всей цепочки
preparation/history, live observation, permits, файловых или процессных эффектов.

`validate` сравнивает сохранённые данные с exact retained replacement. Это
проверка исторической связи; она не доказывает текущий VerifyingLocal root,
владение original guard или готовность capabilities. Всё это обязан проверить
будущий admission в собственной транзакции и physical bridge.

Пять новых model regressions прошли вместе с одним прежним projection test:
неполный/unknown prefix, изменённые owner/generation/hash/image, digest всего
наборa включая retained fixture, exhausted revision и неизвестные authority
fields. Linux/MSVC all-target Clippy update/worker passed; логи
`.winbridge/update-preparation/activation-source-*.log`.
Эта модель добавлена **после** export628c0f6 и не входит в текущий native17 run.
Новый actual activation/ledger proof отсутствует; updater schema остаётся17.

## Intent и FSM запуска: модель реализована

`apps/update/src/activation/lifecycle.rs` добавляет frozen `ActivationContext`,
`ActivationIntent`, typed receipts и `ActivationRecord`. Все проверки/переходы
получают retained RuntimeReplacementRecord и сверяют плоский source binding.
Context сохраняет предыдущий worker, supervisor, logical journal scope,
config/file identities, ожидаемые schemas/protocol, timeout и **только SHA256**
будущего channel nonce. Actual nonce не сериализуется в эти данные.

| State | Подтверждённые данные |
| --- | --- |
| Prepared | Намерение подготовлено, OS launch permission отсутствует. |
| Creating | Intent создания; исход OS call ещё не подтверждён. |
| Created | Сохранены actual process stamp и собственный scoped Job. |
| ResumeGranted | Разрешение записано до resume; это ещё не Ready. |
| ReadyForState | Тот же PID сообщил exact loaded set/manifest/image/config. |
| HandoffIssued | Intent передачи instance/state; новый owner ещё не подтверждён. |
| StateOwned | Подтверждены тот же PID, прежние journal/lock IDs и новый generation. |
| LocalVerified | Совпали frozen schemas/protocol, loaded identity и normal local compatibility. Это не whole update commit. |
| CreationFailed | Подтверждённый no-process outcome; сохранять его может только будущий trusted creation observer. |
| Indeterminate | Последний подтверждённый prefix сохранён, новых внешних действий нет. |

Revision идёт по одному причинному шагу. Duplicate create/resume/handoff,
перезапись nonce/context/receipts, чужой process/Job и nested uncertainty
отклоняются. Схемы сравниваются с frozen context, а не с будущими глобальными
константами executable.

Deadline запрещает **новые внешние grants**, но поздний PID/Ready/fact не
теряется: его можно сохранить и остановиться без resume/handoff. Тайм-аут
не превращается в «процесс не создан». При будущей реализации ledger/root
transition поздний local fact также не должен автоматически объявлять весь
update успешным: это отдельное решение live driver с его monotonic deadline.

Только metadata: нет launch/state-open/commit callbacks, RPC, fresh permit или
новой capability. Будущий schema18 reader обязан загрузить связанные shutdown,
health и supervisor rows и сверить **весь Context**, включая config/state/schemas,
с этими authoritative records; одного `record.validate(replacement)` недостаточно
для fresh admission. JSON нельзя использовать как источник текущих полномочий.

Проверки:10 новых lifecycle regressions (всего16 в activation filter, включая
5 Source tests и прежний projection), workspace703/0 (16 ignored), Linux/MSVC
all-target Clippy update/worker passed. Логи `activation-lifecycle-*.log` в
`.winbridge/update-preparation/`. Actual native activation ещё не выполнялась.
Следом ledger/schema18 и original-custody admission, затем physical launcher,
private worker receiver и handoff. Controller/lifetime/global commit остаются
отдельными следующими блоками; support activation пока=None.

## Ограничения фактического кода

- `RuntimeReplacementGuard` удерживает original exited process/state/instance
  custody, anchors и OwnedRuntimeFile handles. Новый cold reader их не получает.
  `FilesMoved` — исторический факт, а не permission на повторный запуск.
- `LockedShutdownState` удерживает exclusive worker.lock. Обычный Worker::open
  не сможет получить тот же lock до явной передачи. `Worker::open` также дважды
  проверяет maintenance marker перед Store::open; эту защиту сохраняем.
- `OwnedRuntimeFile` запрашивает DELETE и запрещает чужих writers/deleters;
  обычные runtime pins открываются с другим sharing mode. Перед стартом нужно
  превратить move custody в обычные read pins, сравнив exact file IDs/hash/set.
  Простое `drop(guard)` слишком рано теряет остальные права и наблюдения.
- `InfrastructureChild::spawn` сегодня не suspended и не является готовым
  activation launcher. `ManagedChild` обеспечивает suspended→Job→resume,
  но его обычная lifetime/cleanup семантика не подходит постоянному worker.
  Нельзя изменить поведение всех task/probe children ради нового сценария.

По [документации Microsoft о Job limits](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_limit_information)
KILL_ON_JOB_CLOSE завершает связанные процессы при закрытии последнего Job
handle. Значит, «parent завершился успешно» и «child продолжает работать»
требуют отдельного проверяемого lifetime handoff. Это вывод для нашего launcher,
а не свойство существующего `InfrastructureChild::detached()`.

## Последовательность реализации

1. **Модель и binding.** Отдельный typed activation intent/record/result,
   distinct attempt ID, root VerifyingLocal revision, owner/epoch/launch,
   replacement attempt/revision/hash, exact target manifest/set/image/config,
   old worker state identity и ожидаемые journal/protocol schemas. Ссылки/digests
   связывать с immutable rows; не вкладывать ещё одну полную копию всей цепочки
   source→build→health→replacement в каждый новый message.
2. **Ledger и fresh admission.** Updater schema18, backup/fencing migration17→18,
   current/history/outbox с CAS/readback. Admission только из живого original
   guard после проверенного FilesMoved, не из JSON. Один non-Clone/non-Serde
   permit. Новые wire versions/views вводятся отдельно; старые формы frozen.
3. **Файловая передача.** Под guard проверить все пять текущих файлов и anchors,
   подготовить совместимые промежуточные read handles, затем убрать DELETE
   access move handles и получить final installed-set read pins. Любое окно
   открытия закрывается повторной exact identity/hash проверкой до launch.
   Ошибка удерживает сохранившиеся ресурсы/barrier и запрещает продолжение;
   произвольный чужой файл по прежнему пути не принимается.
4. **Один suspended launch.** Отдельный activation launcher вне старого worker
   execution domain, fresh owned Job и ограниченный capture. Durable Creating
   до OS call; actual PID/birth/session/host + Job receipt до resume grant.
   Failed creation и unknown creation — разные состояния, неизвестный исход
   не запускает второй процесс. Не передавать nonce в command line/stdout.
5. **Ready до state writer.** Новый private activation entry worker проверяет
   собственный loaded runtime, точный config и actual supervisor process.
   Из отдельного аутентифицированного канала он подтверждает готовность;
   обычный `serve` по-прежнему получает maintenance refusal. До передачи
   instance lock не открывать journal writer и не подключать scheduler.
6. **State handoff.** Durable HandoffIssued → проверенный release только своего
   instance lock → acquisition того же file ID новым процессом. Старые state
   directory/journal/runtime anchors и original exited process сохраняются
   столько, сколько допускают sharing rules. Новый in-process startup permit
   создаётся только этим handshake, не десериализуется и не доступен обычному
   CLI флагу «ignore guard». На worker стороне сохранить отдельную durable
   квитанцию нового владельца; при добавлении таблицы — worker schema17 и
   actual16→17/refusal gate, не side channel в случайной metadata.
7. **Local verification.** Exact new build/process/instance/incarnation/schema,
   ownership/recovery/resource reconciliation, normal IPC, сохранённые user
   pause и настройки. Новые roots всё ещё блокируются maintenance. Только
   trusted receipt переводит root в AwaitingController; raw lifecycle запрещён.
8. **Controller confirmation и lifetime.** Подтвердить настоящую новую session
   и scoped generation/build, не один доступный TCP порт. До финального commit
   сделать отдельный durable handoff постоянного child lifetime. Рекомендуемый
   адаптер изменяет только fresh owned activation Job после intent и проверяет
   результат через QueryInformationJobObject; не трогает task/domain Jobs,
   DACL, breakaway policy или системные настройки. По
   [SetInformationJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-setinformationjobobject)
   текущие limits сначала читаются, затем меняется нужная часть и выполняется
   readback. Такой адаптер ещё нужно реализовать и проверить, это не готовая API.
9. **Commit и maintenance release.** Commit допускается после всех receipts;
   worker очищает только свой marker/lease, сохраняя независимую user pause.
   Потерянный ACK сверяется тем же command ID. Раздельно показывать install
   outcome, текущую доступность процесса и доставку уведомления/отчёта.

Предложенные schema18/worker17 — следующий свободный шаг на момент документа,
не объявленная capability. Уточнять номера при имплементации по фактическому HEAD.
Supervisor остаётся владельцем одного blocking slot. Heap boundary после
stack overflow сохраняется; тяжёлые physical preflights не переносить в mailbox.

## FSM и восстановление

Минимальная причинная цепочка: Planned → Creating → Created → ResumeGranted →
ReadyForState → HandoffIssued → StateOwned → LocalVerified → ControllerVerified →
LifetimeHandedOff → Committed. Это проект состояний, не текущий wire enum.
Каждая external delivery имеет собственный Operation/Command ID и saved intent.
Unknown creation/resume/handoff/detach/commit удерживает Indeterminate с точным
последним известным prefix. Saved receipt не воссоздаёт live custody/child.

Cold reader только наблюдает и объясняет manual recovery. Не открывать old worker
на мигрированном журнале и не делать автоматический downgrade из backup.
После ошибки updater не удаляет guard или частичные данные ради повторного запуска.

## Автоматические gates

- Same-key/different-payload, stale root/owner/epoch/process, changed config/set,
  busy instance lock, ignored SQL write, atomic history/readback rollback.
- Actual schema17→18 updater и, если добавлен ledger worker,16→17 worker на
  consistent копиях настоящих данных, с backup/history/old-exe refusal.
- Actual Main positive: original guard → launch → state handoff → reconciliation
  → local IPC → controller confirmation → release; данные и user pause сохранены.
- Child не открывает DB writer до разрешения и не принимает задачи до commit;
  чужой CLI/process/replayed nonce не создаёт startup permit.
- Crash до/после каждого OS effect и receipt; no duplicate spawn/resume,
  unknown handoff не освобождает чужой lease, старый worker не запускается.
- Parent выходит после commit, новый worker сохраняется и выполняет fixture;
  crash до lifetime handoff убирает принадлежащий тесту child/Job tree.
- Ready files дважды, независимый verifier и negative proof mutations;
  disk measurements обоих хостов и exclusive cache lease. Не создавать UI окон.

После этого остаются полный U2 fault coverage, desktop U3 и public/bootstrap U4.
Только после их настоящих drivers/gates можно убрать production blockers и
проверить положительный новый support→drain маршрут end-to-end.
