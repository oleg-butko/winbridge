# Время жизни activated worker и повторные обновления

2026-10-28. Следующий блок после Main26. Это проект решения, **не реализованный
контракт и не разрешение менять работающий Windows runtime**. Public install off.
Main26 доказывает session того же resident child, но оставляет root
AwaitingController/local7 и автоматическое завершение child вместе с updater.

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
