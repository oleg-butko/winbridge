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
- TypeScript/contracts/build и реальная browser integration с Unix worker/WebSocket: workflow lost ACK/dedup, пять steps, Unicode log, linked build task, reconnect с прежним app PID, profile restore без команды, stop с сохранением startup history. [Скриншот](screenshots/workflow-worker-linux.png).
- Standalone Linux Tauri check. Эти проверки не объявляются новым Windows визуальным gate.

Перепроверка выполняется [автоматическим сценарием](windows-workflow-validation.md). Профиль для общего обновлённого runtime уже опубликован: `s-e0ee0e50ee6c6de17a487199ca09689dec8f6abd`, manifest `474f1188ec53f7b0492e6a20ec8e5c17295c7c281cdc148706e904547cfb0cb1`; request в Debian `.winbridge/workflow-fixture/20261006-131804-d59a5488/workflow.request.json`. Старый рабочий worker ещё не поддерживает workflow RPC; для UI цикла нужен общий выпуск worker/desktop в прежний fixed runtime.
