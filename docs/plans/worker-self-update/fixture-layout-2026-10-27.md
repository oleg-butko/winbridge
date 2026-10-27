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

## Native component подтверждён

Run **20261027-063128-2873e42d**, task **a9650312-9a20-40a5-b34c-76946f71b090**,
**succeeded/0,5 layout +2 file-link/lease Windows tests**. Producer
**c4b7aea23d15bda4827d853e4c273354702eb596**. Source ZIP:
`winbridge-20261027T102836Z-c4b7aea-a660f5df.zip`,896 files, clean/no built UI.

- ZIP SHA: `8e58395361b4b03a26d03ca15f17e3b73438935d64ef8107888fd05d73ccb95e`.
- Manifest SHA: `593351788459d3abdfe8da264f55fa64222c55f89ce657d64db8216b2693b3d3`.
- Report SHA: `0248788097cb0d9476e034af6c27927b807e27900aa554fbb6a577d6c32fc371`.
- Layout log SHA: `6495a6a48993e2614a50f77fe0e154bc5925361b2865bafd0f907215d008a363`.
- Links log SHA: `c8ff4ef8fd875d8798c37a9b803e0b34852a93f43fd2bf8a5d4b202265933fee`.

Independent Debian verifier проверил exact argv/group/count/typed flags,
compiled commit/manifest markers и3 ready files дважды;12 in-memory negative
mutations отвергнуты (wrong source/argv/flags/count/marker/missing log и попытка
назвать component полным Main25). Cold no-lease assertion и оба настоящих kernel
checks пройдены. На Linux последний canonical assertion также проверен5/0.
Это **не** actual normal Main по внешнему пути, actor25 same-child proof,
controller session или actual19→20 migration. Root/lease data fixtures не выдаются
за original process custody. Full Main25 остаётся обязательным.

Evidence: `.winbridge/update-preparation/layout-native-20261027-063128-2873e42d/`
(`pin`, `request`, `accepted`, `terminal`, `chunks`, `info-before/after`, `verified.json`),
`layout-native-final-observer.log`, `layout-native-proof.log`.
Readonly recheck без новых задач/сборки:
`python3 .winbridge/update-preparation/verify-layout-native.py`.
Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261027-063128-2873e42d`.
Каталоги новых source/report и верхний tmp-каталог сохранены; внутренние
TempDir fixtures освобождены штатными тестами. Старые Windows validation
directories не очищались и не удалялись.

Windows free33,654,177,792 →33,494,745,088 bytes; commands75,362ms +31,621ms.
Debian перед native22,743,166,976 bytes. Installed SHA536d8637… совпал до/после;
Info build59249e6/generation936909c6/incarnation5a8eb173 прежние, active0,
maintenance/recovery errors=null. Controller5103 по-прежнему работает. Test
processes закончились; рабочий worker оставить, desktop не нужен.

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

1. Native layout/link component подтверждён выше. Начать с archive и включения
   stable slot в full Main25; не повторять component вместо этого сценария.
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

При проектировании следующего gate сохранить original script slot lease на всё
время native Cargo/case и архивирования. Closing script lease/receipt JSON не
превращаются в updater process custody. Archive сохраняет все собственные файлы
в прежнем run с locator/identity/hash, не переписывает configs/serialized paths,
не удаляет validation directories и не принимает unknown/live scope.

Для controller1144 выбрать fixture instance ID заранее в GateInput и использовать
его в обоих configs: запуск controller не должен ждать случайный UUID из середины
Windows case. First firewall permission wait900s можно выполнить в реальном old
Main driver после normal serve readiness, **до launch_supervisor/accept/baseline**.
Так используются тот же stable exe и frozen config без дополнительного открытия
Store/смены epoch только ради preflight. Original startup grant240s начинается
позже и не продлевается. Увеличивать внешний case/task timeout отдельно от этого
startup grant. Отдельный controller оставить доступным при local service probe,
чтобы configured-but-disconnected не объяснялось намеренно выключенным сервером.
