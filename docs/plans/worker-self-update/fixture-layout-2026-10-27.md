# Одна раскладка runtime для полного activation fixture

2026-10-27. Продолжение [reservation](activation-runtime-reservation-2026-10-26.md).
Source journal20/worker17/actor25/view17 прежние. Изменяется только private
`update-validation` fixture и platform helper для проверки числа hard links.
Production install/capabilities не включаются, установленный worker не меняется.

## Изменение

`FixtureLayout` один раз связывает case root и runtime. Все helpers Main shutdown/
replacement/activation используют этот объект, включая cold/post-exit reads.
Никакой ambient переменной или повторного чтения location JSON для перенаправления
после admission нет. Legacy fixture сохраняет `root/runtime` и input schema1.

Private input schema2 требует explicit runtime directory + SHA marker. Проверяются
source commit/manifest, exact marker bytes, run parent, отдельный production parent,
initial worker SHA, отсутствие traversal/reparse/hard links и настоящий busy lease.
Marker pin сохраняется на lifetime объекта. Saved JSON/наблюдение busy lease не
являются native process/Job custody и не разрешают install/replay/новый epoch.
В setup внешние пути проверяются до перечисления их содержимого; неизвестные
файлы не перезаписываются. Runtime объявляется до config/manifest/baseline.

Schema1 по-прежнему требует controller=None. Schema2 может явно задать controller
private/loopback ws://IPv4:1144. Main сверяет это точное значение с frozen config,
actual loaded exe, source/state/runtime paths, initial image SHA и runtime guard.
Production controller1143 не становится fixture controller. Scheduler/controller
поведение нормального Main не подменяется тестовыми flags.

## Review

Обычный `require_safe_path(lease)` на Windows открывает файл для data read, что
ожидаемо запрещено `FileShare::None`. Теперь проверяется безопасный parent,
regular/non-reparse metadata и single-link через attributes-only handle. Попытка
data open должна дать именно sharing violation32; missing/ACL/прочие IO ошибки
не считаются busy. Доступ к данным lease и его эксклюзивность сохраняются.

`require_single_link_handle` использует тот же kernel link-count check, что path
API; существующая Windows path API делегирует ему. Unix path API не меняется.
Проверяются настоящий hard link и Windows attributes handle при exclusive lease.
В unit fixture lease закрывается раньше TempDir; Unix test явно снимает свой flock
перед проверкой cold no-adoption, чтобы fork/exec другого parallel test не удержал
унаследованный open description.

## Проверки и воспроизводимость

Linux workspace с `winbridge-worker/update-validation`: **843 passed /0 failed /
17 ignored**, helper updater предварительно собран свежим. Focused layout5 и
file-links1 прошли повтор после review. Linux/MSVC all-targets Clippy прошли;
MSVC повтор и fmt прошли после изменения overlap helper. Native результаты
фиксируются отдельным component gate. Логи:
`.winbridge/update-preparation/layout-{unit-final,links,helper-build,workspace,
clippy-linux-repeat,clippy-windows-final,unit-post-review,links-post-review,fmt}.log`.

`scripts/test-update-fixture-layout.ps1` — автоматический Windows component gate:
fresh clean source archive/manifest, проверка всех исходников, два фиксированных
Cargo argv, actual compiled source markers, exact counts/no ignored, logs и ready
SHA. Проверяет layout и file-link/lease mechanics; **не actual Main25**. Synthetic
declaration fixtures не считаются original process guards. Сборка/применение
production runtime, desktop и новая controller session здесь не выполняются.

Cache — только compiler cache original owner20261017-061527-f4639971,
report SHA22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5.
У этого исторического report `passed=false`, но все4 recorded native groups
завершились exit0. Как в существующем runner, разрешено переиспользовать только
воспроизводимые compiler artifacts под exclusive cache lock; failed workflow не
становится положительным proof/reference. Новый gate проверяет свою clean build
identity и результаты независимо. Старые задачи не повторяются.

Первый native attempt20261027-061445-e82d94b7/taskbb9978fb-3166-4b2f-8ce9-821177745426
на sourcef0dd146 failed/1 до Cargo/cache lease: ASCII-only manifest path whitelist
ошибочно отклонил существующий fixtures/snapshot-project/Привет.txt. Структурная
проверка теперь допускает Unicode, сохраняя запрет absolute/empty/traversal/colon/
backslash/control-character paths. Failed source/task/output сохранены; рабочий
worker не менялся. Повтор использует новый source pin и новый run, не replay.

Второй attempt20261027-061828-eb0a2a77/taskfa70254f-9ada-42fa-b4db-5e1d0f84873f
на source403a4fd дошёл до настоящих Windows tests:4 passed/1 failed. Успешный bind
прошёл, но positive assertion сравнивал canonical root/state с исходным путём без
\\?\ prefix. Исправлена expected path через canonicalize(root); проверка canonical
runtime не ослаблялась. Cold no-lease assertion после этого и platform group ещё
требуют повтора. Исходные source/task/report/ready/logs сохранены, не являются
положительным gate; установленный worker healthy и неизменён.

## Disk

Preflight taskdfc4d89f-a200-4eab-bba3-3db4c87f9f6b succeeded/0:
Debian16,097,132,544 bytes, Windows33,404,575,744 bytes free; worker SHA536d8637…
прежний. Evidence `layout-preflight-20261026T211712-c28840`.

После full feature workspace Debian остался10,429,886,464 bytes free. При отсутствии
Rust builds удалены только184 старых воспроизводимых ELF test executables из
`target/debug/deps`, до2026-10-26; оставлены две новейшие версии каждого stem,
все более новые файлы, root binaries и inode всех запущенных процессов.
Удалено12,332,348,240 bytes по размерам файлов; available стало22,743,068,672.
Libraries/configs/journals/reports/Windows directories не затрагивались.
План/факт с exact paths/inodes/timestamps сохранены в
`.winbridge/update-preparation/test-binary-cache-cleanup-{plan,applied}.json`.
Работающий controller сохранён. Cache cleanup не является изменением программы.

## Следующее

1. Подтвердить layout/link mechanics нативным component gate и сохранить exact
   source/command/task/report pins. Это ещё не запуск normal Main по внешнему пути.
2. Добавить сохранение собственного inactive slot в archive после actual child/
   Job exit, с process/lock/marker/file checks и crash-safe plan. Старые state,
   configs и source paths сохраняются. Reservation helper пока cold reuse запрещает.
3. Подключить stable slot к Main25, отдельный controller1144 и firewall preflight900s
   до original startup window240s. Не переносить live exe или менять manifest paths
   после admission. Перед reservation собрать точный worker с feature
   update-validation, который затем использует CARGO_BIN_EXE_winbridge-worker:
   ordinary payload worker имеет другой SHA. Setup проверяет этот SHA до создания
   state/config/journal. Рабочий runtime/основной controller не заменять.
4. Main25 сразу после writer confirmation, до тяжёлых audits: actual same child/
   lock/generation, epoch+1 ровно один, сохранность history/output/pause/shutdown,
   configured URL без session у local service, root AwaitingController, lost ACK/
   cold no-replay, actual Job exit. Independent proof/negative mutations и actual
   retained19→20/refusal. Только затем controller/lifetime/commit/release и U2–U4.
