# Автоматический Windows regression — 2026-10-06

Основной workflow/process/log цикл этапа 4 проверен на настоящей Windows. Результат получен автоматически через ранее разрешённый worker; ручных действий пользователя в этом прогоне не потребовалось. Полный этап 4 остаётся открытым до отдельного Netwatch profile/probe и оставшихся UI критериев. Новый native desktop визуально не проверялся: пользователь отложил подробные наблюдения.

## Итоговый прогон

| Поле | Значение |
| --- | --- |
| Run | `20261006-132034-a57beb0d` |
| Task ID рабочего worker | `fc247059-8df0-4005-9253-377d328a0fac` |
| Task status/exit | succeeded / 0 |
| Actual platform / report.passed | windows / true |
| Shared report | `K:\VMs\shared\WinBridge\workflow-validation\20261006-132034-a57beb0d` |
| Report SHA-256 | `914c7e54e90ddac912079d6a543c4cc0cc5c0a257f7aaf3c19eea79cba8ba46a` |
| Проверенный source ZIP | `winbridge-20261006T171912Z-700ddd5-7742eae9.zip` |
| ZIP SHA-256 | `fb529a2e4de34cbaf1def09cc1b9cdd65583c5683496e274870064769edd1aa1` |
| Source metadata | base commit `700ddd5b2862e52a35ea2ea54a7b075cd29d006f`, dirty=true, per-file manifest |
| Test groups | core 16 + log-reader 1 + worker runtime 7 + snapshots 5 + workflows 8 = **37 passed** |

Обе Cargo группы завершились exit 0, без ignored/failed tests. Report и два logs сверены с report.ready.json. Исходники извлечены/проверены в `K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\<run>`, cache/TEMP/journals — там же на локальном диске. Рабочий fixed worker оставался generation `69c79c25-93d2-475b-93ce-53e3af67a92e`; его exe/config/state не обновлялись.

Workflow checks исполняли реальный Cargo build и Windows exe из verified immutable generation. Подтверждены duplicate key без повторного процесса, lease ownership, запрет запуска stale exe после build failure, readiness timeout и общий startup timeout, stop до готовности, отделение позднего app crash от успешного startup, migration v2→3 с сохранением task/output и consistent backup. Crash test завершает только созданный harness worker, проверяет закрытие Windows Job и отсутствие owned app PID, затем journal recovery и прежние workflow/step IDs без retry. NTFS guard test проверяет junction/hardlink canaries без записи в targets.

Ранний automatic run `20261006-131133-517de66d` прошёл 34 tests (до двух startup tests и Windows journal guard). Первый bootstrap `20261006-131042-9392c407` не начал tests: Windows PowerShell не нашёл Get-FileHash. Driver переведён на доступный pwsh.exe; последующие два native runs прошли. Старые evidence не удалены.

## Остальные проверки

- Rust workspace: 59 tests на Unix adapter, включая реальные процессы/imports/workflows.
- Windows all-targets compile, Clippy с `-D warnings`, fmt.
- 15 WebSocket/session groups, включая отказ нового RPC старому worker без разрыва его сессии.
- TypeScript/contracts/build и реальная browser integration с Unix worker/WebSocket: workflow lost ACK/dedup, пять steps, Unicode log, linked build task, reconnect с прежним app PID, profile restore без команды, stop с сохранением startup history. [Скриншот](../../../screenshots/workflow-worker-linux.png).
- Standalone Linux Tauri check. Эти проверки не объявляются новым Windows визуальным gate.

## Проверка обновлённого рабочего runtime

Пользователь распаковал выпуск `652635b`, выполнил `prepare-windows-snapshot.ps1` и оставил worker работающим. Новый actual Windows worker generation `210bd4b6-e35f-48e1-85a6-81c973c79605` подключён к controller generation `ca6e5e43-3739-4f2a-819c-11e8e8ce63e0`; Info подтверждает workflowAvailable=true. Общий update состоялся; повторной установки или ручного runtime gate не требуется.

Агент автоматически передал ранее подготовленный frozen профиль по WebSocket и проверил пять групп:

1. Повтор запроса с прежним key и новым requestId вернул тот же Workflow ID.
2. Все пять steps import/build/start/readiness/log succeeded; import привязан к manifest hash, Cargo build завершился exit 0.
3. Registry показывал ready, PID 15476 и readyAtMs; PID совпал с живой start task. Persisted stdout содержал `READY Привет ✓`.
4. Remote LogRead страницами 3/5/65536 bytes сохранил Unicode и строку текущего PID; последующее чтение получило appended heartbeat с большим offset без reset.
5. Явный stop завершил приложение: Application stopped, start Task cancelled; успешный Workflow и все operation IDs сохранились. Worker продолжает работать с прежней generation.

| Поле | Значение |
| --- | --- |
| Report run | `installed-20261006-155439-827cbbac` |
| Shared report | `K:\VMs\shared\WinBridge\workflow-validation\installed-20261006-155439-827cbbac` |
| Report SHA-256 | `466334303cfa8a2e28483d79a3b6e87c85341f293ad661647d246b6fd4babb8b` |
| Workflow ID | `be3004e6-8056-4e50-a5e5-58a41fde2c75` |
| Application ID | `e6b41cb9-0b88-46dc-84ec-7bc6e5b1baf7` |
| Build task | `33871f80-cd30-4f4b-b538-97611b9f1334` |
| Start task | `fa7aba78-80ae-461f-b78a-4ffba5e9d551` |

Report passed=true, platform=windows; report.ready.json содержит сверенный hash/размер. Report сохраняет workflow/import/tasks/output/log pages, ready/stopped app и stop intent. Это проверка установленного runtime, дополняющая 37 isolated native tests, а не новое визуальное UI подтверждение. Подробные ручные UI наблюдения остаются отложенными.

Перепроверка выполняется [автоматическим сценарием](../../../validation/windows-workflow-validation.md). Профиль опубликован: `s-e0ee0e50ee6c6de17a487199ca09689dec8f6abd`, manifest `474f1188ec53f7b0492e6a20ec8e5c17295c7c281cdc148706e904547cfb0cb1`; request в Debian `.winbridge/workflow-fixture/20261006-131804-d59a5488/workflow.request.json`. Повтор этого файла возвращает уже завершённый workflow; новый самостоятельный цикл требует нового явно подготовленного профиля/key.

## Завершение этапа 4: Netwatch, profiles, artifacts

Автоматический прогон 2026-10-06 завершился успешно. Текущий fixed worker выполнил
одну разрешённую задачу проверки новой версии в отдельном каталоге; его exe/config/state
не обновлялись. Test worker использовал свой user-scoped pipe, SQLite и workspace,
без WebSocket. Production Netwatch не запускался и его репозиторий не менялся.

| Поле | Значение |
| --- | --- |
| Shared run | `K:\VMs\shared\WinBridge\workflow-validation\20261006-163840-a97da6e3` |
| Outer task | `98c4a0eb-00f8-4f20-8747-77ecddc8af12`, succeeded, exit 0 |
| Report SHA-256 | `f1ccabe0039fadeb3daa7820f1e347896cddb4d70691632579fa9c5ed0ff0195` |
| Test source commit | `612b3b229e37cb7d25cd99eb05314d96a3c15d43`, dirty=false |
| Source ZIP | `winbridge-20261006T203827Z-612b3b2-0f006336.zip` |
| ZIP SHA-256 | `f85b755a24503ba64901e99361de0b2951aa2dfaa58f5471348e028c79855185` |
| Source manifest SHA-256 | `d33f36cd8a58b810b1d100ca3eb6395ad18816bde906b41f8629a1906579c425` |
| Netwatch original commit | `39409e56a26ef438adab3488afea504ef40f1363` |
| Netwatch validation commit | `e295806456a5715510918dc178ff1e7975e8626b` |
| Snapshot manifest | `b9a2cb5d1740057566452ad5acfe5f6c19be45390182a58ceaa470a1b06700ed` |
| Netwatch workflow | `6f647cac-c662-4761-a836-a383757809d1`, succeeded |
| Netwatch application | `7cd82e14-5337-49ef-8062-67272ad51128`, PID 21416, ready → stopped |
| Build task | `c4d9a5ae-d813-4d35-81f1-909a1fd8f2f1`, exit 0 |
| Start task | `44dfb02a-d49e-46c7-abb9-2253030ad6fd`, cancelled после stop |
| Netwatch executable | 6 085 120 bytes; SHA-256 `60f0a2b8f54376ccba041dde0ebb949bbe3abbeaada4ebe9685b457634e268f7` |
| Netwatch report SHA-256 | `0d0db91fbdd1d2e283fad48a84c543e4b6b7231e18bc8de035fd8d8d9dba594a` |

Четыре группы cargo завершились exit 0: core/log reader (18 tests), readiness IPC
(2), worker runtime/snapshots/workflows (21), native desktop check. Всего 41 native
Windows test; это включает настоящие NTFS/process/Job/crash/recovery checks и новые
profiles/artifact/IPC PID cases. Desktop проверен Windows toolchain, включая новый IPC
сохранения профилей и build identity; запуск нового визуального окна не заявляется.

Отдельный Netwatch сценарий подтвердил import/build/artifacts/start/IPC/log/dedup/stop.
Health сверил server PID средствами ОС и pid/service/status из Netwatch ответа.
Лог содержал config изолированного node и ready IPC listener. SHA executable был
пересчитан из реальных Windows bytes. После stop PID исчез, успешная startup history
сохранилась. Test worker остановлен; текущий fixed worker остаётся подключённым.

63 Rust tests и browser integration прошли в Debian: сохранение/восстановление named
profile и draft не запускает команды, artifacts видны, фильтр log действует, lost ACK
не дублирует workflow. Новые ручные Windows UI наблюдения пользователь отложил.

В первоначальном JSON строка build reason искажена OEM-декодированием PowerShell.
Journal и UTF-8 CLI содержат правильную строку; отдельная native проверка
`encoding-20261006-612b3b2.json` (SHA
`f530848f81367eb6740faf15ee6a0e14fa7a21166271413182f1f95fac75555a`)
подтвердила «Команда завершилась с кодом 0», task
`ea16150d-452a-4996-b385-c41628428f10` exit 0. Исправлен только report reader:
stdout/stderr декодируются как UTF-8 через ProcessStartInfo без изменения console code
page. Исходные checksum-verified отчёты сохранены. Этот дефект не находился в worker;
его обновление для исправления кодировки не требовалось.

Этап 4 завершён. Новый installed worker/desktop обновляются одним общим пакетом;
проверенный Rust/UI код тот же, что в 612b3b2. Повтор ручного gate не требуется.
