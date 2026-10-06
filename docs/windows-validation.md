# Проверка этапа 1 на Windows — 2026-10-05

**Gate этапа 1 пройден 2026-10-05:** все 10 Windows runtime checks и пять native UI шагов подтверждены. [Отчёт и доказательства](windows-gate-2026-10-05.md). Ниже сохранена инструкция для повторного прогона и короткая проверка сохранения формы. Пользователь дополнительно подтвердил её и dev:ui на Windows.

С 2026-10-06 новые runtime regression выполнять [автоматическим сценарием workflow](windows-workflow-validation.md) через уже подключённый worker. Подробные ручные UI проверки отложены пользователем; повторять gate этапа 1 без новой причины не требуется.

## Подготовка

Распаковать/скопировать обновлённые исходники WinBridge в отдельную папку на **локальном диске Windows**, например `C:\WinBridge\source`. Не собирать из `K:\VMs\shared`. Для сборки нужны Rust/MSVC с Windows SDK, Node/npm и WebView2 для desktop. Скрипт ничего не устанавливает, не меняет firewall, services, mounts или глобальные настройки.

Открыть PowerShell в этой папке и выполнить:

```powershell
& .\scripts\validate-windows.ps1
```

При первом запуске скрипт создаёт `target\debug\winbridge-validation.local.json`, рядом с `winbridge-worker.exe`. Настройки берутся из [образца](../examples/windows-validation.config.json):

```json
{
  "schemaVersion": 1,
  "reportDirectory": "K:\\VMs\\shared\\WinBridge\\validation",
  "fixtureDirectory": ".winbridge\\windows-validation"
}
```

`reportDirectory` задаёт место для отчётов, `fixtureDirectory` — для тестового проекта и SQLite-журнала. Относительные пути здесь отсчитываются от `RepoRoot`, по умолчанию — от папки исходников. Можно задать абсолютные пути. Менять эти значения нужно в локальном JSON: последующие запуски сохраняют существующие настройки. Для другого файла настроек:

```powershell
& .\scripts\validate-windows.ps1 -SettingsPath 'C:\WinBridge\validation.local.json'
```

Каждый прогон получает отдельный `<timestamp>-<id>`:

| Содержимое | Путь по умолчанию |
| --- | --- |
| `validation-report.json`, `*.task-report.json`, stdout/stderr, requests, диагностический `worker-config.report.json` | `K:\VMs\shared\WinBridge\validation\<timestamp>-<id>` |
| Тестовый `project`, `state` с SQLite, `worker.local.json`, готовый desktop config и резервный диагностический отчёт | `<RepoRoot>\.winbridge\windows-validation\<timestamp>-<id>` |

Из Debian отчёты доступны в `/mnt/hgfs/shared/WinBridge/validation/<timestamp>-<id>`: агент сможет прочитать их сам. Shared должен быть доступен перед запуском. Если он пропадёт во время проверки, сценарий завершится с ошибкой; копия итогового отчёта останется в локальном каталоге прогона. Прежние проверки не удаляются.

Папка выполнения и база остаются на локальном диске Windows, отдельно от shared. `delivery` и controller в этом сценарии не используются. Пути с пробелами поддерживаются; подстановка переменных окружения в JSON не выполняется.

В обычной сборке workspace SQLite берётся из системы; Windows-сценарий добавляет feature `winbridge-worker/bundled-sqlite`, чтобы собрать SQLite из закреплённой зависимости без отдельной установки DLL. Для cross-target **check** из Debian feature не нужен; такой check не выполняет Windows linker или программу. После актуальной Rust сборки повторную проверку можно запускать с `-SkipBuild`.

## Автоматические проверки

- Настоящий user-scoped named pipe с SID + instance ID; DACL содержит только текущего пользователя и не наследуется. Второй worker не получает тот же journal.
- Direct command, recipe, одновременное чтение stdout/stderr, Unicode и точный exit code.
- Windows argv: пробелы, `&`, `$`, кавычка и завершающий `\` остаются отдельными буквальными аргументами.
- Повтор key возвращает прежнюю задачу и не запускает второй процесс; изменённый payload отклоняется.
- Большой stdout/stderr без UI: процесс не зависает; превышение квоты явно записывается.
- Очередь проекта, отмена queued task, timeout и отмена родителя вместе с дочерним процессом.
- Traversal, UNC, зарезервированные имена, выход через настоящую Windows junction и неверная wire version отклоняются.
- Принудительное завершение **созданного этим сценарием worker** закрывает его Job Object. После рестарта running task получает `indeterminate`, повтор не запускает её снова, terminal result сохраняется.

При сбое скрипт завершается с ошибкой и сохраняет `validation-report.json`, stderr и промежуточные requests. Не повторять неизвестный production command ради проверки: сценарий использует собственный тестовый executable.

PowerShell-синтаксис и C# helper для чтения ACL проверены в Debian через portable PowerShell; Win32 helper там не выполнялся.

## Проверка native UI

Desktop всегда читает `winbridge-desktop.local.json` **рядом с запущенным `winbridge-desktop.exe`**, независимо от текущей папки PowerShell. В нём задаётся путь к той же конфигурации, с которой запущен worker:

```json
{
  "schemaVersion": 1,
  "workerConfigPath": "C:\\WinBridge\\source\\.winbridge\\windows-validation\\<timestamp>-<id>\\worker.local.json"
}
```

Абсолютный `workerConfigPath` используется напрямую, относительный отсчитывается от папки этого desktop config. [Образец](../examples/desktop.config.json) предполагает `worker.local.json` рядом с desktop. При переносе exe нужно перенести и его конфиг; для относительного пути — также worker config. После изменения настроек перезапустить desktop. Ошибка чтения или проверки конфигурации отображается в окне приложения с путём файла.

Сценарий проверки заранее создаёт desktop config в обеих папках `apps\desktop\src-tauri\target\release` и `...\debug`, чтобы работали production и dev запуски. **Существующий desktop config сохраняется**: скрипт выводит его путь и рекомендуемое значение `workerConfigPath` для текущего прогона. При повторной проверке обновить это поле, если оно указывает на предыдущий прогон. Готовый JSON также лежит в локальном каталоге текущего прогона.

После автоматического прогона скрипт завершает только свой worker и выводит точную команду его повторного запуска. Выполнить её в первом окне PowerShell:

```powershell
& .\target\debug\winbridge-worker.exe --config 'C:\WinBridge\source\.winbridge\windows-validation\<timestamp>-<id>\worker.local.json' serve
```

Во втором окне из папки исходников:

```powershell
npm ci
npm run typecheck
npm run build:desktop
& .\apps\desktop\src-tauri\target\release\winbridge-desktop.exe
```

Переменная окружения для выбора конфигурации desktop не нужна. Desktop не запускает и не останавливает worker самостоятельно. Выбор project, recipes, executable, JSON argv, cwd и timeout доступен в форме.

Проверить:

1. UI показывает подключённый Windows worker, реальные recent tasks и их command/argv/cwd, timestamps, output/exit code. Открыть success, failure, cancelled и indeterminate из истории; последняя задача не должна называться successful/failed без подтверждения.
2. Выбрать рецепт `fixture-success` и передать задачу. Посмотреть её timeline и обе вкладки output.
3. В direct mode использовать **полный путь** к `<RepoRoot>\target\debug\winbridge-fixture.exe`, аргументы `["tree"]`, cwd `.` и timeout 60 секунд. Дождаться вывода child PID и нажать «Отменить задачу». UI показывает сначала запрос, затем фактический `cancelled`.
4. Запустить `["long"]`, закрыть desktop, затем открыть его вновь. Worker и задача продолжают работать; отменить задачу после открытия.
5. При открытом desktop проверить stop/restart worker по таблице ниже. UI должен сохранять последний известный Task ID и автоматически перечитать его после восстановления связи.

Пункт 5 проверяет отдельно связь с worker и состояние задачи:

| Действие | Что должно быть видно |
| --- | --- |
| Запустить direct `["long"]` с timeout 120 секунд. Скопировать Task ID. | Задача выполняется; desktop оставить открытым. |
| Нажать **Ctrl+C в окне PowerShell, где запущен worker**. | «Worker недоступен» / «Нет свежего подтверждения worker», прежний Task ID и последнее подтверждённое состояние. Потеря связи не выдаётся за success/failure. |
| Снова выполнить ту же команду `worker --config ... serve`, с тем же `worker.local.json`. | UI подключается автоматически, Task ID прежний. После штатной остановки worker результат обычно «Прервано»; после аварийного завершения — «Результат неизвестен». Команда не запускается повторно автоматически. |

Для dev UI сначала запустить `npm run dev:ui` в отдельном окне, затем `npm run dev:desktop`. Dev exe использует собственный `...\target\debug\winbridge-desktop.local.json`. Production command выше не требует Vite server. Проверка dev UI не обязательна для native Windows gate.

## Короткая проверка сохранения формы после обновления

Пересобрать desktop через `npm run build:desktop`; worker runtime для этого изменения пересобирать необязательно. Существующий `winbridge-desktop.local.json` оставить с прежним `workerConfigPath`.

1. Заполнить Executable и JSON аргументы, выбрать cwd и timeout. Дождаться «Поля сохранены». Команду для этой проверки отправлять необязательно.
2. Закрыть desktop и открыть тот же exe снова. Выбранный проект и все поля должны восстановиться. В списке задач новая задача появляться не должна.

Файл `winbridge-desktop-state.local.json` создаётся автоматически **рядом с desktop exe**, отдельно от конфигурации подключения. Для каждого worker instance/project/workspace root сохраняется своя форма, включая текст незавершённой JSON правки. Переключение проекта восстанавливает его поля. При переносе exe перенести также этот файл, если нужно сохранить введённые команды. Release и dev exe имеют отдельные файлы формы.

Закрытие ждёт записи последних правок. Если запись не удалась, ошибка видна в форме; окно остаётся открытым и предлагает явное «Закрыть без сохранения». Восстановление формы не повторяет принятую задачу и не запускает executable автоматически.

## Что передать для завершения gate

Сообщить имя каталога прогона в shared и результат пяти UI шагов: какие прошли, текст ошибки для непройденных. Агент сам прочитает `validation-report.json`, нужные `*.task-report.json` и stderr в `/mnt/hgfs/shared/WinBridge/validation`. Live SQLite передавать не нужно. `uiVerified` в автоматическом отчёте остаётся `false`: ручные шаги оцениваются отдельно.

Browser `npm run dev:ui` умеет открыть один `*.task-report.json` кнопкой «Открыть сохранённый отчёт worker». Отчёт помечен как сохранённый результат выбранной платформы; browser не подключается к worker и не выполняет команды.

Для принятого прогона `20261005-161827-c8deef84` все проверки уже подтверждены; worker объявляет `task_submit`/`task_cancel`. Управление из Debian по `ws://`, HGFS import и tray/attention относятся к следующим этапам. Сохранение формы и dev:ui также подтверждены пользователем на Windows; Rust/UI integration проверки выполнены в Debian.

Следующий gate: [управление из Debian через локальный ws://](windows-session-validation.md). Первый этап повторять не требуется.

В текущей локальной разработке controller не блокирует IP источника. Адреса активности записываются в Debian `stateDirectory/activity.jsonl` с ротацией (1 MiB и пять архивов); старое поле allowedWorkerAddresses игнорируется. Подробности и результаты [ревью этапа 2](review-stage2-2026-10-05.md).

Для этапа 2 `prepare-windows-session.ps1` использует постоянный `K:\_proj-26\winbridge\runtime`, задаваемый runtimeDirectory в локальном settings JSON. Путь exe не меняется от прогона к прогону; configs, форма и state сохраняются. Отдельные каталоги с датой используются для отчётов/staging. Настройки и запуск описаны в [Windows session инструкции](windows-session-validation.md); этап 1 повторять не требуется.
