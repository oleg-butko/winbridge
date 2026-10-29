# Команда V8 на исходном владельце сервиса

2026-10-29, .204/main, после [readonly observation](finalization-observation-2026-10-29.md).

## Изменения и ревью

Strict `finalizationVersion: 8` normal-IPC decoder принимает только exact command
от настоящего OS peer исходного живого keeper. Cold/Verification, чужой keeper,
смешанные envelope и новые поля force/adopt/permit отвергаются без legacy fallback.
Worker передаёт запрос в bounded mailbox исходного RunningLocalService; queued1,
executing1 максимум. Нет второго Store, instance lock, Job, network manager или
worker. Native marker V8 сохраняет только реализованный Windows receiver.

RunningLocalService последовательно выполняет native pending checks, staging
update-control, SQL release на том же Store и Active seal. Readiness сама не
открывает команды. Если COMMIT известен, но последний seal не завершился,
координатор сохраняет non-clone commit/readiness; тот же exact authenticated key
может закончить именно этот seal без повторного SQL. Другая команда отвергается.

Если SQL вернул ошибку или только Observed receipt, readiness остаётся у
координатора, режим Pending сохраняется; JSON/сохранённая строка не создаёт COMMIT
permit и SQL не повторяется. Ошибка до SQL также сохраняет закрытый Pending;
автоматический repair такого этапа пока не подключён. После Active повтор exact
key возвращает проверенный текущий факт через readonly observation, без release.

Отмена/обрыв IPC-клиента уничтожает только ожидание ответа. Принятое сообщение
остаётся у владельца. Переполнение/закрытый mailbox отвергает новую доставку;
потеря владельца уже принятого сообщения возвращает Indeterminate. Close сначала
закрывает admission, затем join-ит также retained/staged control callbacks; нельзя
объявить instance lock свободным, пока они удерживают Worker Arc.

При ревью post-queue revalidation failure также переведён в Indeterminate:
остановка после принятия не должна выглядеть как отказ новой доставки и стирать
возможный исход прежнего exact key. Тест проверяет как потерю receiver, так и stop
между enqueue и исполнением.

Structured diagnostics содержат actual PID/generation/decision key, queueWaitMs,
executionMs/outcome. Native checkpoints показывают pending/runtime, readiness,
same-Store COMMIT и seal, время/объём хеширования. Trace end не означает успех.

## Проверки и границы

Четыре новых mailbox tests используют настоящий OS peer и IPC: cancellation
не убирает accepted message, capacity1 отвергает новую доставку, закрытие owner
возвращает unknown вместо успеха, cold/foreign/mixed запросы не попадают в очередь.
Original owner loop реально принимает сообщение, но synthetic runtime не проходит
native checks. Отдельный wire test требует exact Active receipt и version/key,
отвергает Indeterminate/Rejected, другой lease и mutating flags.

Логи и результаты полного набора: `.winbridge/finalization-mailbox-review-204/`.
Первый полный прогон выявил прежнюю гонку в workflow fault injection: CREATE
TRIGGER параллельно output writer мог дать SQLITE_BUSY вместо нужного Ready
отказа. Trigger теперь устанавливается до запуска задач; тест требует именно
исходную injected error и проверяет stale/recovery обеих записей. Focused repeat
и повторный full workspace/update-validation прошли: **1027 passed,17 ignored**,
128 result groups. Linux/MSVC workspace/all-target Clippy с `-D warnings`,
generated contracts/typecheck/fmt/diff/scaffold338 passed. Initial failure log
сохранён отдельно, исправление не подменено увеличением timeout ожидания Ready.
Включённая теперь фаза build использует общий fixture budget30s вместо прежних
двух последовательных ожиданий30s+10s.
Перед сборкой Debian19,439,357,952 / Windows(shared)19,062,255,616 bytes free.
Новый `-MailboxGate` готовит отдельный native unit26 (6 SQL+10 lifetime+4 Active+
2 observation+4 mailbox). Default16 и прежний ActiveGate20 сохранены. Report kind
отдельный, `physicalV8Proved=false`, `authenticatedActiveProved=false`: successful
physical delivery/Active observation этот unit gate не доказывает.

## Windows unit26: проверенный результат

Run `20261029-234239-b0cb4fc3`, task
`197dceb6-5ef6-435b-aabe-10a416c36aa3`, producer6e8dbca, succeeded/exit0.
Source ZIP `winbridge-20261029T234224Z-6e8dbca-0ba7fc9a.zip`, SHA
`1ab4523f15370f7693b283648409aac9f5786e7a24b11c74458ab45b3c99677b`.
Все26 tests passed (6/10/4/2/4);8 ready files проверены дважды по SHA/размеру.
Report SHA `24049dbc22920a518a34ebf30866b2e70a879e203e98eca6ba39e194988e9c6f`,
artifact SHA `08aedbb968d1333fb25f80d7d403f5077e1a4be42aa56bfe01a9c1393de7d132`.
Build98,488ms; test commands10,457/20,159/12,412/3,310/7,919ms. Windows
free19,318,575,104 before /19,307,634,688 after. No cleanup/runtime changes/windows;
before/after installed0.2.0/source48/generation946 неизменны, active0/errors=null.
Audit `.winbridge/finalization-mailbox-review-204/native-audit.json`; requests,
terminal/before/after/output находятся в подкаталоге run. Новый native unit gate
не повторять и не переименовывать в положительный physical V8 proof.
Рабочий worker оставить, desktop не нужен; окон не открывалось.

Production physical factory/sender ещё не подключены; parent config/verification
pins пока не отпускаются реальным updater. Нужны fresh V7 под исходной custody,
RootCommitted/ReleasePending, consuming retirement parent pins с сохранением Job,
одна V8 доставка и независимое чтение Active, затем запись подтверждения ledger.
Далее permanent keeper без updater authority, max2 keepers/two generations/U2–U4.
Public installation выключена; рабочий установленный worker не меняется.
