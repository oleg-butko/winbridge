# Проверка локальной сессии Debian ↔ Windows — этап 2

Этап 1 закрыт и закоммичен как `9ae8153`. Код этапа 2 и реальные Debian проверки готовы; native Windows gate этой новой сессии ещё не выполнен. Первый gate повторять не требуется.

## Выбранная пара

- Debian VM: `192.168.44.203:1143`, существующий VMnet8.
- Windows host: `192.168.44.1`, адрес подтверждён пользователем.
- Worker сам подключается к `ws://192.168.44.203:1143`. Controller не фильтрует источник по IP; handshake требует node `windows-host`, instance `local-development` и совместимый протокол/capabilities.
- Данные fixture, SQLite и exe — на локальном Windows диске; отчёты — `K:\VMs\shared\WinBridge\session-validation`.

Настройки VMware, mount, firewall и services менять не требуется. Если изменится адрес Debian, обновить listenAddress/controllerUrl; смена IP Windows не требует правки allowlist. Handshake/version/timeout ошибки показываются в desktop.

## Один запуск в Windows

Подготовленный source archive находится в `K:\VMs\shared\WinBridge\sources`. Его точное имя и SHA-256 агент сообщает вместе с результатом подготовки. Архив содержит исходники и закреплённые lockfiles; сборка выполняется в отдельной папке Windows.

Распаковать актуальный архив в новую локальную папку, например `K:\_proj-26\winbridge\session-src-20261005`. Внутри будет каталог `winbridge`. Затем в PowerShell:

```powershell
Set-Location 'K:\_proj-26\winbridge\session-src-20261005\winbridge'
.\scripts\prepare-windows-session.ps1
```

Сценарий собирает worker с bundled SQLite, UI и desktop. Он создаёт `target\debug\winbridge-session.local.json` из [seed](../examples/windows-session.config.json), если локального файла ещё нет. Существующий settings JSON сохраняется. В нём задаются controllerUrl, instanceId/nodeId, reportDirectory и fixtureDirectory; переменные окружения не нужны.

Каждый прогон получает отдельный `.winbridge\windows-session\TIMESTAMP-ID` с `project`, `state` и `runtime`. В runtime лежат копии готовых exe, `winbridge-worker.local.json` и `winbridge-desktop.local.json` с workerConfigPath. Форма desktop тоже сохраняется рядом с этим exe. Старые configs, exe и SQLite предыдущего gate не перезаписываются.

Сценарий открывает новый desktop и оставляет worker работать в текущем PowerShell. Оставить это окно открытым. При необходимости повторного запуска использовать напечатанную команду `worker --config ... serve`; Ctrl+C останавливает этот worker. Если старое desktop окно ещё открыто, закрыть его для удобства и смотреть новое окно текущего прогона.

В shared появляется `session-validation\TIMESTAMP-ID\setup-report.json`; при недоступном shared остаётся локальная копия. Передать агенту имя `TIMESTAMP-ID`. Это отчёт подготовки: удалённое исполнение проверяется следующим шагом.

`-SkipBuild` допустим после сборки актуальных исходников. `-PrepareOnly` создаёт configs/report и печатает команды запуска без открытия UI и worker. `-SettingsPath PATH` позволяет выбрать другой локальный settings файл. Скрипт не меняет существующие desktop configs в target debug/release.

## Действия в Debian

Локальный controller config в этой сессии: `.winbridge/controller-session.local.json`; он исключён из Git. Запуск:

```bash
./target/debug/winbridge-controller --config .winbridge/controller-session.local.json serve
```

Для вручную созданного config использовать [пример](../examples/controller.config.json), свои stateDirectory и адреса. stateDirectory — private directory 0700, local socket — 0600. Открытие UI и закрытие CLI не управляют lifetime принятой задачи.

IP-блокировки отключены, включая поле allowedWorkerAddresses в прежних локальных configs. Источники TCP активности, подключения/отказы/разрывы и методы RPC записываются в `stateDirectory/activity.jsonl`. Ротация: текущий файл до 1 MiB плюс пять архивов `activity.jsonl.1`–`.5` (0600, `.1` — самый свежий). Команды/аргументы/output в диагностический журнал не попадают. `status.activityLog` показывает путь, лимиты, droppedRecords и lastError; stderr сообщает об отказе записи. Например, в текущей VM:

```bash
tail -n 30 .winbridge/controller-session/activity.jsonl
```

Неверная версия/пара по-прежнему отклоняется; запись содержит реальный исходный IP и port. Список разрешённых IP в конфигурации сейчас не действует и может быть добавлен отдельным расширением.

Проверить подключение и реальные проекты:

```bash
./target/debug/winbridge-controller --config .winbridge/controller-session.local.json status
./target/debug/winbridge-controller --config .winbridge/controller-session.local.json info
```

`info.platform` должен быть `windows`, controllerConnection — connected; status показывает новый sessionId и обе generation. `submit REQUEST.json`, `get TASK_ID`, `list`, `output TASK_ID [AFTER_SEQUENCE]`, `cancel TASK_ID` передаются фактическому worker. `watch TASK_ID [AFTER_EVENT_SEQUENCE] [AFTER_OUTPUT_SEQUENCE]` выводит JSON events/output и восстанавливает чтение того же ID после reconnect. `outputComplete` подтверждается только после terminal task и дочитанной последней страницы.

Автоматический gate выполняет агент из Debian:

```bash
python3 scripts/validate-remote.py --config .winbridge/controller-session.local.json --managed-controller
```

Перед `--managed-controller` остановить ранее запущенный listener: harness сам создаёт controller subprocess, перезапускает и завершает только его. После проверки снова запустить controller командой выше, чтобы UI оставался подключённым. Режим без этого флага использует текущий listener и проверяет команды, но оставляет `reconnectVerified: false`. Скрипт требует platform windows и отдельный fixture registry; `--allow-unix-fixture` предназначен только для development проверки и сохраняет фактическую platform linux.

Отчёты и TaskReport сохраняются локально в `.winbridge/remote-validation/TIMESTAMP-ID`; при настоящем Windows worker копии публикуются в `shared/WinBridge/session-validation/debian-TIMESTAMP-ID`. Summary отдельно указывает platform, passed и reconnectVerified; ручное UI подтверждение фиксируется в документах.

Агент проверяет fixture success/recipe, failure, literal argv/Unicode, timeout, cancel, потерю ACK и duplicate key. Затем временно останавливает только свой controller при выполняющейся fixture: Windows worker и PID продолжают работу, после restart возвращается прежний Task ID/output. Падение worker не должно вызывать повтор неизвестно завершившейся команды.

## Что увидеть в Windows UI

| Действие | Ожидаемый результат |
| --- | --- |
| Подключение worker к Debian | Отдельные «Worker подключён» и «Controller Debian подключён», реальный URL/session и время heartbeat. |
| Агент отправляет fixture из Debian | Та же задача появляется в recent tasks; видны command/argv/cwd, stdout/stderr, exit/status. |
| Controller выключен при running task | «Нет свежего соединения с controller Debian», задача остаётся running; local worker доступен и локальная отмена работает. |
| Controller запущен снова | Новый sessionId, прежние task ID/PID/output; автоматического нового запуска нет. |
| Worker выключен | Свежесть controller помечена как неизвестная/последняя известная, task не становится successful/failed только из-за offline. |

Ожидаемое поведение нового UI подтвердить текстом; `dev:ui` этапа 1 уже проверен и повторять его не нужно. Закрыть этап 2 можно после реального Debian→Windows выполнения и reconnect проверки, а затем перейти к snapshot import.
