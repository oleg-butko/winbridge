# Supervisor28: отдельное текущее наблюдение resident

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

2026-10-29. Продолжение [readonly V7](resident-observation-2026-10-29.md).
В source подключены mailbox/client/view; **новый Windows gate ещё не выполнен**.
Worker19/updater22 и public install off сохраняются. Final root commit, release,
Active и permanent keeper conversion остаются следующими эффектами.

## Операция и состояния

`SupervisorHandle::observe_resident_connection()` посылает Internal28
`ObserveResidentConnection` с прежним token, exact launch/revision и без worker
command. Supervisor требует Active и worker binding; generic lifecycle command
этот action отвергает. Version27/29, другой launch/revision/token или добавленная
worker command не принимаются, fallback на старый контракт отсутствует.

Каждый idle вызов — новое readonly наблюдение, не повтор effect command.
Fresh UUID внутри V7 отделён от сохранённого lifetime key. При уже выполняющемся
driver возвращается None/busy, а не положительный результат. Original guard
перемещается в `ObservingResident`; работа выполняется на исходном IO runtime,
возвращает тот же owner и удерживается supervisor после разрыва caller IPC.
Mailbox остаётся доступным. В текущем client route pending повторный запрос
получает None/busy; phase observing предусмотрен внутренним builder view и не
означает, что первый caller уже получил ответ. Диагностический timeout10s не продлевает startup240s;
client control timeout45s остаётся прежним.

Новый view20 содержит `activationResident`:

- `not_resident`: у actor нет original owner; saved history его не создаёт.
- `observing`: driver держит original handles, результата ещё нет.
- `observed`: actual worker подтвердил current selected link на observedAtMs;
  это датированное наблюдение, не обещание непрерывной связи и не permission.
- `unconfirmed`: чтение не подтвердило связь/scope/владение; причина видна.

View показывает original lifetime key, process/liveness/custody и checked V7
observation. После failed query cached result очищен; причина хранится отдельно
в resident_error. Подтверждённая V6 история не переписывается в Indeterminate
из-за readonly ошибки сети. Transport success с view20/unconfirmed не считается
успешным обновлением. Root/release/новые задачи по-прежнему закрыты.

Guard отдельно хранит original query и checked response, поэтому actor completion
проверяет фактическую correlation, а не восстанавливает expected UUID из echo.
Перед показом проверяются полный retained graph/current owner/local7/root/epoch,
точный lifetime key, историческое independent lifetime observation, query/response,
same actual child/Job/config custody и process liveness. Данные не создают guard.
Второй Store, новый процесс, SQL intent/append и private V6 read не добавлены.

## Размер и совместимость

View20 использует compact preparation base плюс только новый resident block.
Он намеренно не повторяет весь historical V4/V5/V6 prefix: тот доступен через
прежние typed queries. Это сохраняет размер служебного32KiB channel и отделяет
историческое подтверждение от текущего соединения. Полный projection уже свёрнут
в V7 header+SHA; readiness views не пересылаются здесь второй раз.

Во всех старых views1–19 новый optional field полностью отсутствует, даже null
не добавляется. Rust/TypeScript DTO пересозданы, action не добавлен в public
MCP/discovery/UI flow. Само наличие generated type не объявляет installed worker
поддерживающим новую функцию. Старый installed0.2.0 продолжает служить launcher
обычных одобренных isolated validation tasks.

## Ревью и source проверки

Ревью нашло прежний пропуск в service_started(): pending StartingLifetime и
ObservingLifetime не считались уже начатым service. Добавлены оба и новый
ObservingResident. Повторный V4 старт закрыт; старые service/controller/lifetime
queries во время этих операций показывают uncertainty и причину вместо ложного
cold NotResident. Pending metadata по-прежнему не создаёт process owner.

Workspace с update-validation обоих crates:982 passed/17 ignored/126 groups.
Затем добавлена и отдельно выполнена real bound worker mailbox проверка V28:
ещё1 distinct passed, итого983. Полный workspace не выдаётся за включавший этот
поздний test. Linux/MSVC all-target Clippy выполнены после его source изменения;
обе проверки с update-validation/-D warnings прошли. Contract generation/check,
TypeScript typecheck и fmt/diff прошли.

Пять новых portable tests: correlation/full-graph completion (6 mutations),
cold actor с сохранённым key, compact cold view20/unchanged root, actual unbound
supervisor IPC (5 refusals), bound worker→supervisor IPC (7 refusals/repeated
readonly query/no journal changes/frozen19). Общий frozen-view regression теперь
проверяет1–19 и отсутствие activationResident во всех них. Отдельный Windows-only
pending-service test пока только cross-checked; его execution ещё впереди.

Изначально focused test не компилировался из-за missing mut у fixture journal;
исправлено, повтор11 focused tests прошёл. Проверки не ослаблялись. Logs:
`.winbridge/review-2026-10-29/resident-actor-*`. До тяжёлых проверок Debian
20,015,374,336/shared19,513,053,184 bytes, выше6GiB; новой cleanup не было.

## Следующий настоящий эффект и gate

Main29 должен использовать clean producer с Internal28 и distinct clean target
944a39c, уже имеющий actual V7 receiver. Проверенный target archive:
`/mnt/hgfs/shared/WinBridge/sources/winbridge-20261029T143435Z-944a39c-3580f462.zip`,
SHA1095726437ab9118aab7009286d5a5e7fdb4c9e493857f810c60676bd96d2715,
manifest7fe8cc6e2a701d05547a03f714191391119a43eff933725619c6f30280351346,
commit944a39cf0e864fa4ce0f0f6797f57fb0cd2c4b95. Clean export с отдельно
собранным UI,1061 file; временный detached worktree после проверки удалён.
Source pin `.winbridge/update-preparation/resident-v7-target-944a39c.json`.
Первый local UI build потребовал явный PATH Node24; exporter затем правильно
отклонил временный node_modules symlink. После удаления только этого созданного
link clean export прошёл, данные source проверены archive_pin. Ничего из
installed runtime/config не копировалось в экспорт. Исходный installed worker не заменять.
Stable runtime-test-204, test controller.203:1144, preflight900s и initial240s
сохраняются. Desktop не нужен, окон нет. Не replay Main27/Main28 вместо V7 proof.

Проверить actual same child/keeper/Job/Store после initial expiry и lost Ready
body, свежие повторные observations с разными requestId и прежним lifetime key,
отказ wrong actual OS peer и JSON PID spoof, неизменный ledger/worker journal,
closed submit/root/release, positive cleanup/cold NotResident. Reader отдельно
связывает новый kind, exact ignored test, schemas19/22, source/target/files/markers
и негативные мутации. Reconnect/срыв соединения и остальные faults остаются в
дальнейшем native matrix; synthetic reconnect unit test не подменяет их.

После этого durable finalization intent+root decision, scoped same-Store
ReleasePending→Active, consuming permanent physical keeper с освобождением pins
и **всех** updater authority references, full native proof, два поколения/U2–U4.
Границы — в [finalization review](keeper-finalization-review-2026-10-29.md).
Рабочий worker оставить запущенным; desktop можно закрыть. UI окон этого шага нет.
