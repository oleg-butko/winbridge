# Следующий keeper/finalization: ревью границ после lifetime

2026-10-29. Design checkpoint по фактическому source19/22/private6/actor27.
[Main27](main27-native-2026-10-29.md) пройден; [Main28](main28-native-2026-10-29.md)
также пройден: partial body/late reconciliation подтверждены. Этот документ не включает новые runtime capabilities, root
commit, release или public install. Реализацию эффекта продолжать после Main28.

## Найденные ограничения

1. **Нельзя использовать старый private stream для finalization.** V6 receiver
   после Ready вызывает `hold_supervised_lifetime`, который не читает stream.
   При Main28 у parent в нём намеренно остаётся unread body. Отправлять туда V7
   и ожидать новый framed response было бы неверно даже после успешного normal
   reconciliation. Повторно разбирать остаток как новую команду запрещено.
2. **Нельзя навсегда сохранить весь ActivationProcessGuard.** Он держит installed
   set/config/directory pins, старую released-exit custody и output/Job/channel.
   Supervisor отдельно держит UpdateJournal и Arc<UpdateAuthority> с file lock.
   Они заблокируют следующий update после выхода старого worker, если не выделить
   отдельного physical owner и не освободить journal authority.
3. **Нельзя просто вызвать ControllerConfirmed.** Core переводит root сразу в
   Committed; `update_blocks_installation` больше его не блокирует. Generic journal
   driver сейчас правильно отвергает raw ControllerConfirmed. Будущий trusted
   driver должен связать root decision с durable finalization intent, а новый
   admission оставаться закрытым до проверенного scoped release. Committed root
   и подтверждённый рабочий режим — отдельные факты и отдельные UI состояния.
4. **Недостаточно поменять worker lease на Released.** `local_activation.is_some()`
   безусловно закрывает mutations и scheduler. `serve_inner` один раз выбирает
   отсутствие update-control service; позднее само оно не запустится. Нужна
   typed runtime transition на том же Worker/Store, а не второй service/reopen.
5. **V5 receipt не является текущей сетью после verification expiry.** Controller
   observation строит service receipt с исходным window. Сохранённый V5 факт не
   даёт нового connection confirmation. Перед final commit нужен отдельный
   current-link/projection snapshot, связанный с тем же live child, keeper и
   выбранным controller. Старые contracts не расширять неявно.
6. **После смены updater owner old keeper остаётся только OS owner.** Старые V6
   graph readers требуют initial root/owner/epoch. Постоянный active worker не
   должен зависеть от продолжения старой updater authority: следующий updater
   закономерно её сменит. Текущий normal mode и физическая Job наблюдаются по
   отдельному resident contract, а прежние receipts остаются историей.

Пока эти пути выключены, это ограничения следующей реализации, не обнаруженные
ошибки установленного worker. Нельзя обходить их удалением deadline, locks,
Job limits или проверок scope.

## Рекомендуемая последовательность

### 1. Разделить physical custody и verification pins

Выделить private non-Serde/non-Clone owner для ManagedChild/Job, captures/readers
и исходного channel. Его Drop сохраняет kill-on-close и не называется positive
cleanup. Original process guard содержит этот owner и прежние pins/data; до
нового trusted transition поведение не меняется. Consuming conversion должна
перемещать handles, а не дублировать их и не использовать `mem::forget`.

После independently verified lifetime и нового durable decision старый stream
можно явно retire, не повторяя Ready. Это не новое activation window: тот же
child уже находится в Supervised quarantine. Пока нет trusted factory,
прочитанные JSON records не создают resident owner и не освобождают pins.

### 2. Отдельный authenticated resident control

Для следующих действий использовать отдельный typed local control contract.
Предпочтительно переиспользовать normal endpoint с проверенным OS peer PID;
если это требует изменения adapter, сохранить actual PID/peer stamp отдельным
non-caller-supplied значением. Обычные Info/observation requests остаются readonly.
OS client должен совпасть с pinned live keeper, server — с тем же child. Existing
`transport::supervisor::Listener::accept` показывает нужную проверку peer PID.
Одних JSON PID/nonce или диагноза из MCP недостаточно.

Реальный V7 decoder/marker подключать вместе с runtime, не по наличию DTO.
Reject wrong versions/scope/parent/unknown fields; никаких fallback на V6 или
коррекции journal context. Новая bounded control operation не продлевает initial
startup deadline и не даёт launch/Store reacquisition permission.

### 3. Root decision и scoped release как разные durable FSM

Зафиксировать flat bindings к exact local7/controller/lifetime, текущему selected
link/projection, original keeper/Job/child и installation authority. Только
original consuming owner может получить fresh finalization permit. В одной
updater transaction связать root commit с release intent/causal history; raw
ControllerConfirmed по-прежнему запрещён. Пока release не подтверждён, следующий
update admission и сообщение об общей готовности закрыты независимо от root.phase.

Worker может переиспользовать существующие Released lease/WorkerUpdateCommand
таблицы **если** новый trusted transaction сохраняет их совместимый формат и
все исторические receipts. Обычный generic Release после shutdown расширять
нельзя: он верно закрыт. Если понадобится несовместимый worker journal format,
сначала отдельный migration/cutover design, а не скрытое расширение schema19.
Upgrader ledger migration также требует actual retained22 copy/old22 refusal.

Worker runtime: Verification → SupervisedQuarantine → ReleasePending → Active.
SQL release и Active seal должны сериализоваться Store/mode locks; не открывать
requests до обоих. При commit uncertainty разрешены только observation фактов
и отдельная явная scoped reconciliation transition на том же resident owner.
Query сама не создаёт mode. Watchdog ReleasePending должен признавать только
old Quiesced либо точно связанный release receipt, а не падать из-за собственного
успешного SQL commit. Ошибка не запускает новый worker и не отменяет accepted tasks.

### 4. Перевести supervisor в keeper role

После root decision и нужных physical checks consume original guard, отпустить
verification/runtime/config pins, сохранив physical Job owner. Пока finalization
не окончена, journal authority остаётся у coordinating actor. Parent и child
config pins должны быть освобождены до объявления обычных settings/mutations
доступными; иначе «сохранить настройки с ручным restart» получит sharing violation.
После independently observed Active/release fact отпустить updater connection и
**все** Arc<UpdateAuthority>, не сохраняя stale writer в background keeper.

Диагностика разделяет update completed, live keeper, current worker и historical
receipts. Keeper loss по-прежнему закрывает именно его Job. Отдельный manual stop
и failure semantics не подменяются молчаливым перезапуском.

### 5. Native и следующее поколение

Full gate должен продолжить actual same child через root/release/Active, включая
сценарий с прежним unread Ready, ошибки на границах SQL/mode/pin release, idle CPU
и positive cleanup. Проверить запись локального config при живом Active worker
и освобождение updater lock без убийства worker. Старый Main28 не replay вместо
этого нового сценария.

Затем next-update delegation: spawn нового keeper из старого keeper вне worker
Job, durable single admission, actual logical worker vs physical spawner,
максимум два keeper, exit old только после known empty old Job. Нельзя ослаблять
Job limits или запускать новый supervisor из domain старого worker. Потребуется
явная стратегия fresh updater/schema negotiation до U4, чтобы новая сборка не
зависела от способности старого updater интерпретировать будущую схему.
Оставшиеся U2 fault matrix, U3 UI/attention и U4 actual installed update обязательны.
