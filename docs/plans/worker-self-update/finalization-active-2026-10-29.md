# Same-process Active после известного SQL COMMIT

2026-10-29, Debian .204, main после pull доddb83ef. Пришли26 commits;
49b6d7b и предыдущая ветка .204 уже входят в main. Продолжение
[same-Store release](finalization-worker-release-2026-10-29.md).

## Проверка результата .203

Прогон `20261029-215609-d36a169c`, taska2642372-b197-4cc6-8683-edad7e30db38,
завершён: source ddb83ef,6 SQL +10 lifetime tests passed. Все5 файлов ready
manifest перечитаны и проверены по SHA/размеру дважды. Report SHA
`4612e9e3c52b27a6fa50421f8941481475a8b5d4e31c1040160d6fc9fcfeadff`.
Это native unit gate, `physicalV8Proved=false`, runtime не заменялся.
Повтор этого старого прогона не выполнялся. Локальный audit:
`.winbridge/active-review-1lfhh_mg/retained-native-audit.json`.

## Ревью и изменения

- Native pending checks дополнительно сверяют installation/scope именно с
  фактически загруженным pinned installed set. JSON command/SQL metadata не
  подменяют эту проверку.
- Добавлен отдельный Active seal на original RunningLocalService. Он потребляет
  non-clone known `ActivatedReleaseCommit` и readiness original Pending Arc;
  повторно проверяет тот же Store/authority, exact release, живого keeper,
  quiet registries и native checkpoint. Saved/Observed SQL result — данные,
  из которых этот permit не получается.
- Все fallible checks идут до отпускания worker config pin. Затем под теми же
  Store→mode locks listener owner перемещается в RunningLocalService и Mode
  становится Active. Store, instance lock, PID, generation, network manager и
  execution domain не открываются заново.
- Ошибка последнего checkpoint возвращает `ActiveSealFailure` с тем же known
  commit и staged listener. Можно завершить именно этот seal после устранения
  причины; новый SQL release и восстановление permit по JSON не нужны.
- Readiness создаёт отдельный update-control listener из фактического runtime
  guard/updater image hash. Регистрация читает metadata без open/migrate/claim
  updater journal. Пока режим не Active, control commands и updater launch
  отвергаются; готовность listener сама не открывает приём задач.
- Разделены immutable startup origin и dynamic permissions. Dispatch, scheduler,
  queue control и Info переходят в рабочий режим только при typed Active.
  Network/control initial startup, Ctrl+C semantics и recovery origin не
  заменялись механически. Новый network manager не запускается.
- Active watchdog проверяет original keeper и текущую authority того же Store,
  не требует прежнего Quiesced lease/updater epoch/config SHA: последующее
  обновление и сохранение config с restart являются допустимыми действиями.
- Close сначала закрывает admission/stop, затем join-ит staged control и
  прежний network owner. Нельзя отбросить control callbacks и объявить lock
  свободным, пока они ещё удерживают Worker Arc.

## Проверки

Full workspace/update-validation: **1017 passed,17 ignored**,128 result groups.
Четыре новых Active tests исполняют real Store/IPC/queue/scheduler/config locks:
одна fenced команда действительно выполняется и duplicate сохраняет task ID;
user pause не снимается автоматически; сохранённый SQL release не включает
Active; пять отказов сохраняют pending/config pin, два разрешают завершить
исходный known seal; после Active config можно сохранить без возврата в quarantine.
Keeper/runtime metadata в этих fixtures synthetic — это не V8 peer/Job proof.

Linux/MSVC workspace/all-target Clippy с update-validation/`-D warnings`,
generated contracts/fmt/diff прошли. Focused repeat4 подтвердил сохранение и
atomic replacement config после Active; новых Windows execution доказательств
этот cross-check не даёт. Логи:
`.winbridge/active-review-1lfhh_mg/`.

Перед сборками свободно Debian25,379,631,104 и Windows/shared17,776,295,936 bytes;
cache cleanup не требовался. Для нового ограниченного native unit gate добавлен
`-ActiveGate` в `scripts/test-update-worker-release.ps1`: прежний default16
сохраняется, новый gate выбирает6+10+4=20 tests и отдельный report kind.
Он не открывает сеть и UI; test EXE из deps допустим только для этого local-only
профиля. Artifact SHA по-прежнему проверяется до/после всех test groups.

## Windows Active core: результат

Run `20261029-225125-0899195d`, task
`1ce2b323-5b43-46af-8007-518f2587892e` succeeded/exit0. Producer73063db,
source ZIP `winbridge-20261029T225010Z-73063db-906288d7.zip`, SHA
`c9c725c9ce7cc8721f56362acb1d36d359077b865f3d0bfeca0f4fc820b017c6`.
Все20 тестов (6 SQL/10 lifetime/4 Active) прошли. Ready manifest6 файлов
перечитан и проверен дважды. Report SHA
`590f4d80fa5d395d586978755ebbe24ef176e20ef179d091ae256bfad2cc3131`;
test artifact SHA `f2b7490de2018070f144add115f1bb8183848e75cc7590cd0b0183a6028ede25`.
Новый Active test реально выполнил cmd.exe после снятия user pause, повторный
fenced request сохранил task ID. Windows config write и atomic replacement
после seal прошли. Tests используют synthetic runtime/keeper/readiness metadata;
`activeCoreProved=true`, **physicalV8Proved=false**.

Build86,437ms; test commands9,944/20,977/11,869ms, bodies8.81/20.93/11.83s.
Это разделяет compile и исполнение; SHA artifact до/после одинаковый. Windows
free18,699,644,928 before /18,919,854,080 after; Debian перед native24,860,557,312.
Никакой cleanup в этой задаче не выполнялся. Installed before/after build/gen
946f1128 прежние. Desktop/окон не было, рабочий worker оставить запущенным.
Audit `.winbridge/active-review-1lfhh_mg/native-audit.json`, saved request/acceptance/
terminal/output в подкаталоге run. Повторять task ради продолжения не нужно.

## Границы и продолжение после native unit gate

Это source Active core/trusted internal API. Mutable V8 decoder/mailbox,
production physical decision factory и normal-IPC Active observation ещё не
подключены, marker/discovery/public install не advertised. Новый native unit
gate подтверждает только перечисленные local runtime/SQL/FileShare границы.

Перед настоящим V8 send parent factory должна отпустить СВОИ verification/
config pins, сохранив original physical owner; отпускание pin внутри worker
не доказывает, что другой процесс больше не удерживает config. Затем нужны
actual V8/Active/unknown ACK/independent observation, keeper conversion с
освобождением всех updater-authority references, max2 keepers/two generations,
next-update bootstrap и оставшиеся U2–U4.

Installed worker0.2.0/source48, production controllers и рабочий runtime не
заменяются. Рабочий worker оставить запущенным, desktop не нужен.
