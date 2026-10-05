# Проверка локальной сессии Debian ↔ Windows — этап 2

Этап 1 закрыт и закоммичен как `9ae8153`. Автоматический Windows runtime gate этапа 2 [пройден 2026-10-05](windows-session-gate-2026-10-05.md); подключение и три новых ручных UI наблюдения подтверждены пользователем. Этап 2 завершён. Ниже сохранена инструкция для повторной проверки. Первый gate повторять не требуется.

## Выбранная пара

- Debian VM: `192.168.44.203:1143`, существующий VMnet8.
- Windows host: `192.168.44.1`, адрес подтверждён пользователем.
- Worker сам подключается к `ws://192.168.44.203:1143`. Controller не фильтрует источник по IP; handshake требует node `windows-host`, instance `local-development` и совместимый протокол/capabilities.
- Данные fixture, SQLite и exe — на локальном Windows диске; отчёты — `K:\VMs\shared\WinBridge\session-validation`.

Сценарий не меняет VMware, mount, firewall и services. Для файервола с разрешениями по приложению нужен доступ именно запускаемого worker.exe; ниже описана диагностика. Если изменится адрес Debian, обновить listenAddress/controllerUrl; смена IP Windows не требует правки allowlist. Ошибки TCP, WebSocket и обмена Hello/Welcome показываются в desktop.

## Один запуск в Windows

Подготовленный source archive находится в `K:\VMs\shared\WinBridge\sources`. Его точное имя и SHA-256 агент сообщает вместе с результатом подготовки. Архив содержит исходники и закреплённые lockfiles; сборка выполняется в отдельной папке Windows.

Распаковать актуальный архив в новую локальную папку, например `K:\_proj-26\winbridge\session-src-20261005`. Внутри будет каталог `winbridge`. Затем в PowerShell:

```powershell
Set-Location 'K:\_proj-26\winbridge\session-src-20261005\winbridge'
.\scripts\prepare-windows-session.ps1
```

Сценарий собирает worker с bundled SQLite, UI и desktop. Он создаёт `target\debug\winbridge-session.local.json` из [seed](../examples/windows-session.config.json), если локального файла ещё нет. Существующий settings JSON сохраняется. В нём задаются controllerUrl, instanceId/nodeId, reportDirectory, fixtureDirectory и runtimeDirectory; переменные окружения не нужны.

`runtimeDirectory` по умолчанию — `K:\_proj-26\winbridge\runtime`. Этот путь одинаков для всех прогонов и папок исходников. Старый settings JSON без этого поля поддерживается с тем же значением по умолчанию. Можно задать другой локальный абсолютный путь; относительный разрешается от папки исходников и зависит от её расположения. Для постоянного правила файервола использовать абсолютный путь. Runtime, project и SQLite остаются вне shared.

В постоянной папке лежат три exe, `winbridge-worker.local.json`, `winbridge-desktop.local.json`, форма `winbridge-desktop-state.local.json`, `project` и `state`. Configs создаются только при отсутствии; затем worker config определяет controllerUrl, instance/node, stateDirectory и проекты. Правки defaults в settings JSON не заменяют существующий worker config: адрес менять непосредственно в `K:\_proj-26\winbridge\runtime\winbridge-worker.local.json`. Desktop использует соседний worker config; несовпадающий существующий desktop config вызывает понятную ошибку и сохраняется.

Готовые exe сначала копируются в staging и проверяются по SHA-256. Одинаковые runtime exe не переписываются. Перед обновлением изменившихся exe закрыть desktop и остановить worker; обнаруженный работающий runtime блокирует замену. Сценарий не завершает процессы самостоятельно. Поля формы, configs, project и state при обновлении сохраняются.

Каждый прогон получает отдельный `.winbridge\windows-session\TIMESTAMP-ID` для отчёта и staged-binaries. Старые каталоги предыдущих версий, их configs и SQLite остаются на месте; автоматического переноса прежнего journal или формы в новый постоянный runtime нет. Первый запуск постоянной папки создаёт отдельный fixture; последующие используют его же.

Сценарий открывает desktop из постоянной папки и оставляет worker работать в текущем PowerShell. Оставить это окно открытым. При необходимости повторного запуска использовать напечатанную команду `worker --config ... serve`; Ctrl+C останавливает этот worker. Уже запущенный worker с тем же instance обнаруживается через local IPC: второй запуск отклоняется, чтобы новое окно не подключалось к прежнему worker незаметно.

В shared появляется `session-validation\TIMESTAMP-ID\setup-report.json`; при недоступном shared остаётся локальная копия. Передать агенту имя `TIMESTAMP-ID`. Это отчёт подготовки: удалённое исполнение проверяется следующим шагом.

`-SkipBuild` допустим после сборки актуальных исходников: первый запуск обновлённых исходников выполнить без этого флага. `-PrepareOnly` подготавливает runtime/configs/report и печатает команды запуска без открытия UI и worker. `-SettingsPath PATH` позволяет выбрать другой локальный settings файл. Скрипт не меняет существующие desktop configs в target debug/release.

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
python3 scripts/validate-remote.py --config .winbridge/controller-session.local.json --managed-controller --reconnect-pause-seconds 15
```

Перед `--managed-controller` остановить ранее запущенный listener: harness сам создаёт controller subprocess, перезапускает и завершает только его. После проверки снова запустить controller командой выше, чтобы UI оставался подключённым. Режим без этого флага использует текущий listener и проверяет команды, но оставляет `reconnectVerified: false`. Скрипт требует platform windows и отдельный fixture registry; `--allow-unix-fixture` предназначен только для development проверки и сохраняет фактическую platform linux.

`--reconnect-pause-seconds` задаёт 2–60 секунд offline (по умолчанию 2); 15 секунд позволяют увидеть новую свежесть связи в desktop. Harness печатает Task ID/PID до отключения и сохраняет обе generation/session и PID в итоговом report. Он не управляет Windows worker или чужими controller processes.

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

## Если Windows не подключается

Ошибка `10013` (`WSAEACCES`) означает отказ Windows в операции с сокетом; сама по себе она не определяет конкретный блокирующий продукт. [Коды Winsock Microsoft](https://learn.microsoft.com/en-us/windows/win32/winsock/windows-sockets-error-codes-2). Если в activity.jsonl отсутствует tcp_accepted от Windows, worker ещё не дошёл до обмена протоколом с controller. Журнал фиксирует соединения после TCP accept, а не пакеты, заблокированные до него.

Сначала оставить controller работать в Debian и проверить `status`/listener. Из Windows выполнить проверку без изменения настроек:

```powershell
Test-NetConnection 192.168.44.203 -Port 1143 |
  Select-Object ComputerName,RemoteAddress,RemotePort,InterfaceAlias,SourceAddress,TcpTestSucceeded
```

`TcpTestSucceeded: True` подтверждает TCP доступ из PowerShell; это не проверка разрешения для другого exe. Если worker получает 10013, проверить существующее правило в используемом файерволе для **точного пути запущенного worker**, направление outbound, destination `192.168.44.203:1143`, TCP. Worker не слушает входящий TCP-порт на Windows; исходный порт выбирает ОС. Не путать permission для desktop, target/debug worker и runtime worker.

Новая версия prepare всегда запускает `K:\_proj-26\winbridge\runtime\winbridge-worker.exe` по умолчанию. Установить разрешение для этого постоянного пути вместо `.winbridge\windows-session\TIMESTAMP-ID\runtime\winbridge-worker.exe` прежних версий. Смена runtimeDirectory меняет путь для правила файервола. Во время диагностики можно повторно запускать напечатанную команду `worker --config ... serve`, сохраняя прежние exe/config/state. `-SkipBuild` пропускает сборку; обновлённая диагностика появится только после сборки новых исходников.

Общий deadline подключения остаётся 5 секунд, но сообщение указывает этап: TCP connection; WebSocket upgrade после успешного TCP; WinBridge Hello/Welcome после успешного WebSocket. Для отказа 10013 сообщение содержит полный путь фактически запущенного exe. UI больше не сообщает «ожидается handshake», когда TCP ещё не установлен.

Если разрешение выглядит правильным, нужны название файервола, его запись блокировки с exe/destination/direction и результат Test-NetConnection. Windows event 5157 (если аудит уже включён) содержит application path, направление, адреса/порты и filter ID; его отсутствие не доказывает разрешение. [Описание события Microsoft](https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-10/security/threat-protection/auditing/event-5157). Для этой проверки не включать аудит, не отключать файервол и не менять VMware/firewall/services автоматически.
