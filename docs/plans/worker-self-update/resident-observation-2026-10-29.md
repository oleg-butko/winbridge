# V7: текущее соединение исходного resident worker

**Main29 Windows passed,2026-10-29:** run20261029-155738-d847446d,
task2cd973db-9769-4695-b601-9d75344ab0ad, producer7ca206f→target944a39c.
235 native test executions/54 commands/67 files twice; independent audit и
readonly reread совпали. [Результат и pins](main29-native-2026-10-29.md).
Два authenticated V7 чтения после original expiry подтвердили current session
того же child/Job/Store; foreign peer/spoof/version отказали, bytes/history прежние.
Первая попытка772361c4 остаётся failed: test adapter28→20 исправлен и проверен.
Runtime204 archived; runner/test controller завершены0. Installed0.2.0/source48
прежний/active0/errors=null; рабочий worker оставить, desktop можно закрыть.
Далее durable finalization/root/release/Active/permanent keeper/two generations/U2–U4.
Public install off. Ниже — исторические checkpoint, их running статусы superseded.

2026-10-29. Source шаг после [physical custody](process-custody-owner-2026-10-29.md)
и [finalization review](keeper-finalization-review-2026-10-29.md). Это readonly
наблюдение, **не** root commit, release, Active или готовое самообновление.

## Зачем отдельный контракт

V5 подтверждает соединение внутри initial startup window. V6 подтверждает вход
того же child/Store в supervised quarantine; его receipt остаётся историческим.
Для последующего finalization нужно знать текущее соединение, в том числе после
reconnect и истечения startup window. Нельзя менять даты старого receipt или
снова читать original private stream: Main28 оставляет в нём unread Ready body.

Новый strict V7 `ActivationResidentQuery` передаётся по обычному worker endpoint.
В нём fresh canonical UUID requestId и исходный V6 command целиком. RequestId —
корреляция чтения, отдельная от execution/idempotency key. В ответе version7,
тот же requestId и Observed, Unavailable либо Rejected. Ни одна ветка не меняет
журнал, mode или разрешения. Поля release/canCommit/JSON PID/handles отвергаются.
V7 зарезервирован только для этого implemented readonly decoder. Будущий mutable
control потребует отдельного явного контракта; его поддержка здесь не объявляется.

## Аутентификация и scope

`transport::local_peer::LocalPeer` — sealed non-Serde/non-Clone OS observation.
Его нельзя создать из JSON. Unix adapter проверяет SO_PEERCRED UID/PID; Windows
adapter вызывает GetNamedPipeClientProcessId на actual accepted stream, прежние
same-user DACL и reject_remote_clients остаются. Изменение не расширяет старые
IPC/transport полномочия. Existing supervisor listener не переписан.

Actual client PID должен совпасть с исходным keeper из immutable resident seal;
исходный keeper kernel pin обязан оставаться live. Старый PID не может получить
доступ после повторного использования ОС: pinned original process должен быть
жив одновременно с OS peer check. Sender проверяет actual server PID через
supervisor transport и live original child/Job/kernel identity вокруг IPC.

Worker принимает запрос только на уже Supervised экземпляре: full original V6
command, loaded runtime identity, pinned config, instance lock, same Store/
authority, retained local7/controller/lifetime graph и saved Confirmed lifetime
проверяются. Verification/cold/query никогда не создают resident. Это временный
quarantine observation: после будущего Active/смены updater epoch потребуется
новый контракт, независимый от старого updater ledger.

## Текущее соединение и размер

После дорогих scope проверок producer захватывает принятый frame/current Ticket
и link под одним controller lock. Существующий projection cache проверяет
monotonic age <=10s, sequence/current attempt/session/generation и Connected.
Текущий session/controller generation может отличаться от старого V5 — reconnect
допустим. Выбранный endpoint, замороженная topology, provider build/generation/
context, heartbeat timestamps, UUID, sequence, canonical attempt и отсутствие
актуальной ошибки должны совпасть. Историческая lastError не означает текущий
Disconnect. observedAt относится к этому чтению; original entry/window неизменны.

Полный readiness projection с project/workflow views не пересылается: он может
превысить служебный лимит32KiB. `ActivationResidentFrame` содержит bounded header
и canonical SHA-256 полного реально принятого frame. Consumer не восстанавливает
опущенные views и не выводит из digest permission; доверие основано на actual
peer/child и проверенном producer. Hash не является доказательством содержания
сам по себе. Отдельный current snapshot не обещает будущую доступность сети.

Shared readonly Info predicate перенесён без изменения в service::validate_readonly_info.
Legacy V4/V5/V6 probes используют прежний reexport. V7 worker проверяет этот
predicate локально до захвата current frame (здоровье журнала, active0, actual
Windows recovery coverage, readonly/quiesced lease, закрытые mutations). Полный
Info/catalog не передаётся вторым сообщением: иначе тот же размерный риск остался
бы в новой операции. Observed означает, что predicate проверен самим original
authenticated worker; boolean canCommit/permission в wire не добавлен.

## Original-owner sender

`ActivationProcessGuard::observe_resident_blocking` потребляет тот же boxed owner
и возвращает его даже при ошибке. Перед каждым запросом старый cached result
очищается. Полный owned graph, local7/root revision/updater epoch/key/nonce,
original config pins и actual child/Job проверяются до/после normal query.
Worker image обязан содержать implemented readonly receiver marker; dynamic
sender pattern не добавляет raw marker в linked legacy worker случайно.

Запрос использует original IO runtime и отдельный bounded диагностический timeout
10s. Это не startup window и не разрешение на новый запуск. Saved Confirmed
lifetime обязателен; V7 не занимается примирением Indeterminate V6. Ошибка readonly
запроса сохраняет original owner и не переписывает подтверждённую V6 историю.
Ни private stream read/resend/retire, ни SQL append, ни release здесь нет.
Getter возвращает только данные наблюдения, не finalization permit.

## Проверки и продолжение

Workspace с winbridge-update/update-validation:969 passed/17 ignored/126 groups.
Отдельно worker/update-validation:132 passed/3 ignored, из них9 дополнительных
имён относительно workspace worker123. Итого978 distinct passed/17 ignored;
никакого сложения повторно исполненных123 tests. Новых portable tests6: OS Unix
peer payload spoof, три data-validator cases (26 response mutations, correlation/
strict unknown fields), real IPC verification/wrong actual peer/no Store mutation
и probe no-server/no-deadline-renewal. Named-pipe test подготовлен отдельно.
Shared readonly Info predicate перенесён byte-for-byte, кроме visibility/reexport.
Linux/MSVC workspace/all-target Clippy с обоими update-validation features и
-D warnings, fmt/diff прошли. Windows execution V7 этим не подменяется.
Scaffold329 Markdown/local links/config examples passed; UI source не менялся.

Первая full попытка остановилась с14 storage refusals: initial Debian10.2GB
после сборки стали3.6GB, реальные guards сохранили минимум6GiB. Builds остановлены,
237 unused hashed ELF test executables из target/debug/deps удалены; по каждому
stem сохранены2 newest, active process inode исключены, controller retained image
вне cleanup. Free3,627,847,680→20,909,568,000 bytes. Runtime/config/journal/reports,
Windows directories и backup не менялись. Повтор после cleanup прошёл. Первые
failed logs сохранены; порог не ослаблен. Windows Clippy также нашёл unused-mut
в новом named-pipe test; лишняя mutability убрана, не подавлена allow-атрибутом.
Source logs/measurements/cleanup exact paths:
.winbridge/review-2026-10-29/resident-*.

Readonly installed Info после cleanup: source48c834c/manifest310e6f7f,
gen946f1128-2c30-42b6-b369-8acda852a834, active0/errors=null — без изменений.
Новый Windows gate ещё не выполнен. Named-pipe peer test пока только подготовлен;
Linux socket/fixture tests не подменяют native proof. Main27/Main28 относятся к
своим прежним producer/target, не подтверждают V7 или custody refactor.

Worker19/updater22, actor27/view19 остаются прежними; новых public supports,
MCP/UI action и install/activation нет. Следом подключить отдельный actor/view
для свежего наблюдения и actual native gate на distinct V7 target, включая
wrong actual peer и reconnect при unread original Ready. Потом durable root
commit + release intent, consuming permanent keeper с освобождением pins и всех
updater-authority references, scoped same-Store ReleasePending/Active, новое
полное native доказательство, два поколения и U2–U4. Подробные границы — в review.
Working installed worker0.2.0 и production controller не заменяются. Desktop не
нужен, рабочий worker оставить запущенным; UI окон этого source шага нет.
