# WinBridge v1: проверять собственные изменения на Windows

Актуально на 2026-10-10. Это введение в конкретный рабочий сценарий и инструкция его ручной проверки. [Общее руководство](../README.md) объясняет остальные возможности и состояния интерфейса.

**Сценарий возможен в текущей версии через MCP и обычные Git-команды.** Агент меняет проект в Debian, передаёт сохранённый commit, обновляет один Windows checkout и собирает в нём. Пользователь устанавливает собранные exe в постоянный runtime и перезапускает worker. Для звука и popup агент явно отправляет запрос внимания.

Два действия требуют участия пользователя: первоначальная регистрация проекта и замена работающего runtime. Автоматическое обновление worker через него самого пока не реализовано. Уведомление также не возникает автоматически на каждый MCP-запрос.

## 1. Где что находится

| Назначение | Постоянный путь |
| --- | --- |
| Исходники, которые редактирует агент | Debian: `~/winbridge` |
| Windows проект | `K:\_proj-26\winbridge` |
| Единственный Windows checkout и место сборки | `K:\_proj-26\winbridge\src` |
| Exe, из которых запускаются worker и desktop | `K:\_proj-26\winbridge\runtime` |
| Передаваемые Git-пакеты | `/mnt/hgfs/shared/WinBridge/git-bundles` ↔ `K:\VMs\shared\WinBridge\git-bundles` |

Cargo сохраняет промежуточные файлы и результаты в `src\target`; Tauri — в `src\apps\desktop\src-tauri\target`. Устанавливаемые `winbridge-worker.exe`, `winbridge-desktop.exe` и `winbridge-fixture.exe` копируются в `runtime`. Повторные сборки используют прежние каталоги и кэш.

`runtimeDirectory` уже поддерживается локальными настройками подготовки; его значение по умолчанию — `K:\_proj-26\winbridge\runtime`. Настройки находятся в `src\target\debug\winbridge-session.local.json` и `winbridge-snapshot.local.json`. Конфигурация, журнал и состояние desktop сохраняются в постоянном runtime. Подготовка сохраняет существующие настройки, поэтому проверьте их фактическое значение.

Правило firewall для worker относится к `runtime\winbridge-worker.exe`. Постоянный путь устраняет причину повторных запросов из-за нового каталога. Конкретный firewall может дополнительно проверять изменение содержимого exe; WinBridge не управляет его правилами.

Встроенная доставка snapshot сохраняет каждую версию в `snapshots\<ID>`. Для требования «один `src`» здесь используется **Git bundle** — файл с Git commit и его историей, читаемый обычным Git. Это сценарий поверх существующих произвольных команд, а не новый инструмент MCP. `winbridge_publish` и блок «Доставка исходников» для него не используются.

## 2. Что подготовить один раз

На Windows нужны Git, Rust с MSVC build tools, Node.js/npm, WebView2 и PowerShell 7.4 или новее (`pwsh.exe`). Worker должен запускаться из `runtime`, тогда сборка новых exe в `src` не затрагивает работающие exe. В Debian нужны собранный controller, MCP-клиент агента и доступ к общей папке.

### Зарегистрировать Windows проект

Откройте desktop → **«Настройки» → «Проекты, соединение и хранение» → «Добавить проект»**:

- ID: `winbridge`;
- локальный Windows корень: `K:\_proj-26\winbridge\src`;
- включить **«Разрешить произвольные команды агента в этом проекте»**;
- оставить одну одновременно выполняемую команду; рецепты для этой проверки можно оставить пустыми.

Корень — именно `src`: регистрация всего `K:\_proj-26\winbridge` пересеклась бы с существующими проектами в `runtime\project` и `workspaces`. Каталоги зарегистрированных проектов не должны пересекаться. Разрешённые команды выполняются с правами пользователя Windows; `cwd` задаёт рабочий каталог, а не изоляцию файловой системы.

Сохраните конфигурацию. Она начнёт действовать после **ручного перезапуска worker**. Дождитесь завершения его работы; в окне PowerShell с worker нажмите Ctrl+C и запустите его прежней командой, например:

```powershell
& 'K:\_proj-26\winbridge\runtime\winbridge-worker.exe' --config 'K:\_proj-26\winbridge\runtime\winbridge-worker.snapshot.local.json' serve
```

Используйте фактический путь конфигурации из «Настроек»: обычный и snapshot-конфиг — разные файлы. Desktop показывает конфигурацию работающего worker. Для выполнения команд через MCP отдельная запись проекта в Debian `controller.projects` не требуется; она нужна для встроенной публикации snapshot.

### Настроить видимый сигнал

В desktop → **«Tray и уведомления»** включите «Показывать popup» и «Встроенный звук Windows», задайте ненулевую громкость, минимальную важность «Предупреждение», отключите тихие часы на время проверки. Для раздельной проверки сигналов удобно временно выставить интервал 1 секунду. Обычные defaults: звук/popup включены, громкость 25%, интервал 30 секунд.

**Оставьте worker и desktop запущенными.** Desktop можно скрыть в tray крестиком. «Завершить интерфейс» отключает звук и popup до следующего запуска, хотя события продолжают сохраняться в журнале.

В Debian агент запускает controller, если он ещё не работает:

```bash
cd ~/winbridge
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
```

MCP-клиент отдельно запускает адаптер с абсолютными путями к executable и конфигурации:

```bash
/home/user1/winbridge/target/debug/winbridge-controller --config /home/user1/winbridge/.winbridge/controller-snapshot.local.json mcp
```

Это настройка запуска дочернего процесса в MCP-клиенте. Один `serve` обеспечивает связь, другой процесс `mcp` предоставляет инструменты агенту. [Контракт MCP](../../guides/mcp.md).

## 3. Подготовить передачу исходников

Эту часть выполняет агент в Debian. Сначала сохранить проверяемые изменения в commit. Несохранённые изменения не входят в Git bundle. Следующий пример создаёт полный пакет в новом каталоге; `.part` переименовывается после проверки. Shared служит для передачи, сборка будет на локальном Windows диске.

```bash
(
  set -eu
  cd ~/winbridge
  if [ -n "$(git status --porcelain)" ]; then
    printf '%s\n' 'Сначала сохраните проверяемые изменения в commit.' >&2
    exit 1
  fi
  source_commit=$(git rev-parse HEAD)
  bundle_root=/mnt/hgfs/shared/WinBridge/git-bundles
  bundle_run_directory="$bundle_root/$(date -u +%Y%m%dT%H%M%SZ)-$source_commit"
  mkdir -p "$bundle_root"
  mkdir "$bundle_run_directory"
  git bundle create "$bundle_run_directory/winbridge.bundle.part" HEAD
  git bundle verify "$bundle_run_directory/winbridge.bundle.part"
  test "$(git bundle list-heads "$bundle_run_directory/winbridge.bundle.part" HEAD)" = "$source_commit HEAD"
  mv "$bundle_run_directory/winbridge.bundle.part" "$bundle_run_directory/winbridge.bundle"
  printf 'Commit: %s\n' "$source_commit"
  sha256sum "$bundle_run_directory/winbridge.bundle"
)
```

Запишите три значения: каталог пакета, полный commit SHA и SHA-256 файла. Агент передаёт их пользователю. Bundle содержит историю до выбранного commit; следующие изменения должны продолжать эту историю.

### Если Windows `src` сейчас распакован из ZIP

Это разовый переход к Git **в том же каталоге**, с сохранением исходников, настроек и кэша. Он рассчитан на неизменённый официальный ZIP с `dirty: false` в `winbridge-source-manifest.json`. Если Windows исходники менялись, сначала сохраните свои изменения и согласуйте их перенос; инструкция не разрешает их стирать.

Подставьте реальный путь bundle и SHA-256 из предыдущего шага вместо `<каталог>` и `<SHA-256>`. Выполните в Windows PowerShell 7.4+:

```powershell
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location 'K:\_proj-26\winbridge\src'
$bundle = 'K:\VMs\shared\WinBridge\git-bundles\<каталог>\winbridge.bundle'
$expectedHash = '<SHA-256>'
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $bundle).Hash -ne $expectedHash) { throw 'SHA-256 bundle не совпал' }
if (Test-Path -LiteralPath '.git') { throw 'Git уже инициализирован: этот шаг повторять не нужно' }
$manifest = Get-Content -Raw -LiteralPath 'winbridge-source-manifest.json' | ConvertFrom-Json
if ($manifest.dirty -or -not $manifest.sourceCommit) { throw 'Нужен чистый ZIP с известным commit' }
git init
git bundle verify $bundle
git fetch $bundle HEAD
git reset --mixed $manifest.sourceCommit
Move-Item -LiteralPath 'winbridge-source-manifest.json' -Destination '.git\winbridge-bootstrap-source-manifest.json'
git status --short
git rev-parse HEAD
```

`reset --mixed` здесь первоначально связывает уже распакованные файлы с **их прежним commit**, не заменяя их содержимое. Старый манифест ZIP сохраняется в `.git`: оставленный в корне, он заставлял бы build identity использовать прежнюю версию вместо Git. Ошибка любой команды останавливает сценарий. Если `git status --short` вывел изменения, остановитесь и покажите агенту результат; он не должен исправлять это через `reset --hard` или `git clean`.

Если `src` уже собственный чистый Git checkout без ZIP-манифеста в корне, этот переход не нужен. Один Windows checkout будет дальше обновляться обычным fast-forward: новая история добавляется без переписывания прежней.

## 4. Ручная проверка: агент обновляет и собирает

Перед прогоном агент сообщает: **«Нужны запущенные worker и desktop из постоянного runtime. Будут обновление исходников и сборка в `src`. Не редактируйте там файлы и не перезапускайте worker до завершения. Список проверок: …»** Пользователь сохраняет открытые файлы. Состав проверок и возможные тестовые окна агент объявляет заранее.

С этим checkout работает один агент, команды идут последовательно. Обновление на месте не имеет гарантии неизменяемого snapshot на весь цикл: от параллельного редактирования исходников здесь защищает согласованный порядок работы.

### A. Проверить связь, проект и уведомление

Агент вызывает `winbridge_info` и `winbridge_controller_status`: связь свежая, у worker есть проект `winbridge` с нужным Windows root. На момент написания этого гайда установленный worker подключён, но `winbridge` ещё не зарегистрирован: этот шаг нужно выполнить пользователю.

Для первой безопасной пробы вызвать `winbridge_submit` с такими аргументами:

```json
{
  "request": {
    "protocolVersion": 1,
    "requestId": "<run>-status",
    "projectId": "winbridge",
    "idempotencyKey": "<run>-status",
    "timeoutMs": 30000,
    "command": {
      "kind": "direct",
      "executable": "git.exe",
      "arguments": ["status", "--porcelain"],
      "workingDirectory": "."
    }
  }
}
```

`<run>` — уникальное имя этого прогона. Ответ содержит настоящий Task ID. Для той же задачи агент сразу вызывает `winbridge_request_attention`:

```json
{
  "request": {
    "requestId": "<run>-status-notice",
    "idempotencyKey": "<run>-status-notice",
    "taskId": "<Task ID из ответа submit>",
    "severity": "warning",
    "message": "Агент проверяет Windows исходники WinBridge в постоянном src. Далее планируются обновление commit и сборка."
  }
}
```

Ожидается короткий звук, popup и событие в **«Внимание»**. «Открыть задачу» показывает настоящую `git status`; «Прочитано» отдельно подтверждает событие. Если сигнал не появился, сначала проверить desktop, настройки и журнал внимания, затем продолжать изменяющие действия.

Это предупреждение об активности агента; результат самой задачи остаётся отдельным статусом и не становится ошибкой из-за уведомления.

Агент читает эту задачу через `winbridge_get_task` и `winbridge_read_output`. Для чистого checkout ожидаются «Завершено», exit code 0 и пустой stdout. Любые изменения в `src` требуют остановки и объяснения пользователю.

### B. Обновить этот же `src`

Последовательно через `winbridge_submit`, каждый раз с новым ключом операции, агент выполняет:

| Шаг | Команда в Windows `src` | Что подтвердить |
| --- | --- | --- |
| Проверить пакет | PowerShell: `Get-FileHash -Algorithm SHA256 -LiteralPath '<полный путь bundle>'` | SHA-256 равен записанному в Debian; при несовпадении не выполнять следующие шаги. |
| Проверить Git-пакет | `git.exe bundle verify <полный путь bundle>` | Exit code 0; пакет пригоден для этого репозитория. |
| Загрузить commit | `git.exe fetch <полный путь bundle> HEAD` | Exit code 0. |
| Обновить файлы | `git.exe merge --ff-only <полный commit SHA из Debian>` | Exit code 0; отсутствует конфликт или расхождение истории. |
| Подтвердить версию | `git.exe rev-parse HEAD`, затем `git.exe status --porcelain` | Точный ожидаемый commit; чистые исходники. |

В RPC executable и аргументы передаются отдельно, как в примере A; путь с пробелами — один аргумент. Для PowerShell используйте `pwsh.exe` с `-NoLogo`, `-NoProfile`, `-Command` и читаемым текстом команды. Поле `snapshotId` оставьте пустым: cwd должен быть зарегистрированный `src`.

После принятия каждого действия агент явно запрашивает внимание с его Task ID и понятным описанием. События остаются в журнале, даже если popup объединяет несколько из них за установленный интервал. Это уведомление о работе, **не запрос разрешения**. Текущий API не гарантирует сигнал до запуска процесса: attention привязан к уже принятой задаче. Неизменяющие чтения статуса/output можно считать частью уже объявленной проверки.

Fast-forward обновляет добавленные, изменённые и удалённые Git-файлы, сохраняя игнорируемые настройки и build cache. Ошибка `--ff-only` означает остановку и уточнение, а не принудительную перезапись Windows исходников.

### C. Собрать и проверить

В том же проекте и cwd `.` агент последовательно выполняет согласованные команды:

| Назначение | Команда | Результат |
| --- | --- | --- |
| Rust проверки | `cargo.exe test --workspace --locked --features winbridge-worker/bundled-sqlite` | Все выбранные тесты проходят, exit code 0. |
| Worker и fixture | `cargo.exe build --workspace --locked --features winbridge-worker/bundled-sqlite` | Exe в `src\target\debug`. |
| Зависимости UI | PowerShell: `npm.cmd ci` | Установка по lockfile завершена. |
| Типы UI | PowerShell: `npm.cmd run typecheck` | Exit code 0. |
| Native desktop | PowerShell: `npm.cmd run build:desktop` | Exe в `src\apps\desktop\src-tauri\target\release`. |

Для npm команда MCP имеет executable `pwsh.exe`, аргументы `-NoLogo`, `-NoProfile`, `-Command`, например `npm.cmd run build:desktop; exit $LASTEXITCODE`. `.cmd` не передаётся напрямую как executable. Для длинной сборки задайте достаточный timeout, например 30 минут (`1800000` ms).

Пользователь видит задачи в **«История»**, настоящие команды, stdout/stderr и переход «В очереди → Выполняется → Завершено» либо конкретную ошибку. Агент получает те же данные через MCP. Принятие команды ещё не означает успешную сборку. Обновление runtime допускается после проверки успешных результатов всех согласованных шагов; ошибка любого шага останавливает этот цикл.

После сборки агент повторно сверяет Windows HEAD и чистоту исходников. Если версия изменилась во время проверки, результат нельзя приписывать выбранному commit и устанавливать как проверенную сборку.

При необходимости отдельно проверить отмену длительной команды кнопкой «Отменить задачу»: ожидается «Отменено». Это прерывает управляемое дерево выбранной задачи. При потере ответа агент сверяет прежний Task ID/ключ запроса, сохраняя его; повтор с новым ключом мог бы создать второй процесс.

## 5. Установить сборку в постоянный runtime

Агент сообщает результаты и просит пользователя выполнить **ручное переключение runtime**. Worker, исполняющий сборку, всё ещё работает из прежнего exe: окончание Cargo не обновляет его версию.

1. Убедиться, что задачи завершены и управляемые приложения остановлены.
2. В desktop выбрать **«Завершить интерфейс»**; крестик лишь скрывает окно.
3. В окне PowerShell с worker нажать Ctrl+C.
4. В Windows выполнить:

```powershell
Set-Location 'K:\_proj-26\winbridge\src'
.\scripts\prepare-windows-snapshot.ps1 -SkipBuild
```

Сценарий копирует уже собранные exe в прежний runtime, проверяет их SHA-256, сохраняет существующие конфиги/журнал, запускает desktop и worker. `-SkipBuild` означает «сборка уже выполнена агентом»; если её не было, используйте команду без этого флага. При активных runtime-процессах подготовка отказывает в замене exe.

В desktop должны появиться подключённый worker и **«Controller Debian подключён»**. Агент через `winbridge_info` сверяет `build.sourceCommit` с проверенным commit, `sourceDirty: false` и новую generation. Старые задачи остаются в истории; новый worker не переисполняет их.

Пути запущенных процессов можно проверить в PowerShell:

```powershell
Get-Process -Name winbridge-worker,winbridge-desktop | Select-Object ProcessName,Path
```

Ожидаются exe из `K:\_proj-26\winbridge\runtime`. Отчёт подготовки записывается в shared: `K:\VMs\shared\WinBridge\snapshot-validation\<run>\setup-report.json`; агент может прочитать его из Debian.

Desktop можно запускать без параметров: `K:\_proj-26\winbridge\runtime\winbridge-desktop.exe`. Если скрытие или завершение не сработало, после обновления на исправления от 2026-10-10 передайте `runtime\logs\desktop*.jsonl` и время сбоя. [Инструкция диагностики](../../guides/desktop-diagnostics.md) объясняет сообщения, ротацию и копирование журналов в shared.

## 6. Что считать успешной ручной проверкой

- Windows HEAD совпадает с выбранным Debian commit; повторный цикл обновляет прежний `src`, включая удаления файлов.
- Сборка использует прежние каталоги, а запускаемые worker/desktop находятся в постоянном runtime.
- MCP и desktop показывают одинаковые Task ID, команды, output и финальный результат.
- Явный запрос внимания вызывает звук/UI при работающем desktop; событие требует отдельного «Прочитано».
- После ручного переключения подтверждена новая версия worker; история сохранена.

Агент сохраняет итог с commit, hash пакета, Task IDs и результатами шагов и явно сообщает: **«Проверка завершена. Worker оставить запущенным. Desktop можно оставить в tray для дальнейших звуков и popup; полное завершение интерфейса отключит эти сигналы»**.

Для этого гайда проверены контракт MCP, настройки/пути подготовки, механизм уведомлений и Git bundle-переход на временных Debian репозиториях, включая удаление файлов и сохранение build cache. Подключение существующего Windows worker подтверждено только чтением состояния. **Описанный полный сценарий в постоянном Windows `src` ещё требует ручной проверки; его настройка и обновление runtime при написании гайда не выполнялись.**
