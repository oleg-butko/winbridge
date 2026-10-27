# Worker 0.2.0: ручная установка для двух Debian

2026-10-27. Пакет для Windows x64:

```text
K:\VMs\shared\WinBridge\releases\20261027-worker-0.2.0-48c834c\winbridge-worker-0.2.0-48c834c-windows-x64.zip
```

Пересобирать проект на Windows не нужно. Это тот же normal EXE, который прошёл
Windows multi-controller gate с четырьмя controller и отдельную миграцию журнала
17→19. Он собран без `update-validation`, с bundled SQLite, в dev-профиле.
Source commit: **48c834c00c658f11feec066c9ae4650a4e6cff7a**, clean.
Рабочий код этого worker совпадает с main **591b471**: изменения source после
producer относятся к tests и feature-gated validation. Build identity не переписан.

SHA256 `winbridge-worker.exe`:

```text
9dac4060f3abf9a2167ee432b536d9f9ffc9a787730e5d4096ed4a5eda997104
```

Рядом с ZIP находятся его `.sha256`, инструкция и `release.ready.json`.
Внутри: EXE, build identity, `release.json`, пример topology, эта инструкция,
SHA256 всех файлов и сохранённые evidence. `evidence/worker.json` — тестовый
конфиг, **его нельзя использовать для рабочей установки**.

## 1. Подготовить оба Debian

На `.203` controller должен слушать `192.168.44.203:1143`, на `.204` —
`192.168.44.204:1143`. Существующие controller не останавливать для замены worker.
В обоих controller config значения `instanceId` и `controller.workerNodeId`
должны соответствовать **одному прежнему worker**: обычно `local-development`
и `windows-host`. `nodeId` самих Debian переименовывать не требуется.
У каждого Debian свои controller state и listener. Прежние ограничения разрешённых
адресов Windows сохраняются. Эта инструкция не меняет firewall или службы.

Старый controller 0.1.0 может выполнять обычные задачи с worker 0.2.0 — это
проверено в native gate. Новые инструменты creator/cancellation/connections требуют
controller 0.2.0; обновление Debian controller можно выполнить отдельно.

До остановки Windows worker оба агента должны перестать отправлять новые задачи.
Дождаться окончания задач/импортов/workflows и остановки управляемых приложений.
Одного `activeTaskCount=0` недостаточно для проверки сохранённых приложений.

## 2. Распаковать и проверить пакет

Открыть **PowerShell 7 (`pwsh`)**. Оставить рабочий worker пока запущенным.
Новые окна тестов не появятся, desktop для обновления не нужен.

```powershell
$ErrorActionPreference = 'Stop'
$release = 'K:\VMs\shared\WinBridge\releases\20261027-worker-0.2.0-48c834c'
$zip = Join-Path $release 'winbridge-worker-0.2.0-48c834c-windows-x64.zip'
$runtime = 'K:\_proj-26\winbridge\runtime'
$unpacked = Join-Path $runtime 'update\worker-0.2.0-48c834c'
$expectedZip = ([IO.File]::ReadAllText($zip + '.sha256') -split '\s+')[0]
if ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expectedZip) {
  throw 'ZIP не совпал с контрольной суммой'
}
if (Test-Path -LiteralPath $unpacked) { throw 'Папка распаковки уже существует; не перезаписывать её' }
Expand-Archive -LiteralPath $zip -DestinationPath $unpacked
foreach ($line in [IO.File]::ReadAllLines((Join-Path $unpacked 'SHA256SUMS.txt'))) {
  $digest, $name = $line -split '  ', 2
  if ((Get-FileHash -LiteralPath (Join-Path $unpacked $name) -Algorithm SHA256).Hash.ToLowerInvariant() -cne $digest) {
    throw "Не совпала сумма файла: $name"
  }
}
$newWorker = Join-Path $unpacked 'winbridge-worker.exe'
$expectedWorker = '9dac4060f3abf9a2167ee432b536d9f9ffc9a787730e5d4096ed4a5eda997104'
if ((Get-FileHash -LiteralPath $newWorker -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expectedWorker) {
  throw 'EXE не совпал с проверенной Windows сборкой'
}
& $newWorker --version
if ($LASTEXITCODE -ne 0) { throw 'Не удалось прочитать версию' }
```

Ожидается `WinBridge worker 0.2.0`, source `48c834c…`, dirty `false`.

## 3. Создать новый конфиг, сохранив прежние проекты и state

В той же PowerShell-сессии выполнить следующий блок. `$oldConfig` должен указывать
на **реально используемый сейчас** конфиг. Ниже указан прежний конфиг `.203`;
если имя другое, исправить только эту строку. Старый файл не изменяется.
Новый файл создаётся эксклюзивно; существующий файл не перезаписывается.

```powershell
$oldConfig = Join-Path $runtime 'winbridge-worker.snapshot.local_203.json'
$newConfig = Join-Path $runtime 'winbridge-worker.multi-controller.json'
if (Test-Path -LiteralPath (Join-Path $runtime 'winbridge-installed-set.json')) {
  throw 'Runtime имеет installed-set manifest; замена одного EXE для него не подходит'
}
$config = Get-Content -LiteralPath $oldConfig -Raw | ConvertFrom-Json -AsHashtable -Depth 100
if (-not ($config.worker -is [Collections.IDictionary])) { throw 'В конфиге отсутствует worker settings' }
$config.worker.Remove('controllerUrl') | Out-Null
$config.worker.controllerTopology = Get-Content -LiteralPath (Join-Path $unpacked 'controller-topology.example.json') -Raw |
  ConvertFrom-Json -AsHashtable -Depth 100
$bytes = [Text.UTF8Encoding]::new($false).GetBytes(($config | ConvertTo-Json -Depth 100))
$stream = [IO.File]::Open($newConfig, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try { $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
& $newWorker --config $newConfig validate
if ($LASTEXITCODE -ne 0) { throw 'Новый конфиг не прошёл проверку; рабочий EXE не заменять' }
```

`validate` не открывает journal и не запускает worker. Меняется только способ
подключения: `controllerUrl` заменяется на `controllerTopology` с двумя адресами.
`instanceId`, `nodeId`, `stateDirectory`, delivery, проекты и остальные настройки
сохраняются. Не брать пустой конфиг из примеров: это потеряет прежние регистрации.
Оба worker-поля одновременно задавать нельзя.

## 4. Остановить прежний worker и заменить EXE

В окне **рабочего** worker нажать **Ctrl+C**, когда оба агента закончили работу.
Не завершать его из задания, выполняемого самим worker. Desktop, если запущен,
закрыть на время замены, чтобы он не пытался запустить старую версию.

В прежней PowerShell-сессии:

```powershell
$target = Join-Path $runtime 'winbridge-worker.exe'
if (Test-Path -LiteralPath (Join-Path $runtime 'winbridge-installed-set.json')) {
  throw 'Требуется coordinated installed-set update'
}
$workers = @(Get-CimInstance Win32_Process -Filter "Name='winbridge-worker.exe'")
if (@($workers | Where-Object { -not $_.ExecutablePath -or $_.ExecutablePath -ieq $target }).Count) {
  throw 'Рабочий worker ещё запущен либо его путь нельзя проверить'
}
if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -cne '536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712') {
  throw 'Текущий EXE отличается от установленного 0.1.1; сначала выяснить причину'
}
if ((Get-FileHash -LiteralPath $newWorker -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expectedWorker) {
  throw 'Подготовленный новый EXE изменился'
}
$backup = Join-Path $runtime ('winbridge-worker.before-0.2.0-' + [Guid]::NewGuid().ToString('N') + '.exe')
[IO.File]::Copy($target, $backup, $false)
if ((Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash -cne (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash) {
  throw 'Резервная копия EXE не совпала'
}
Copy-Item -LiteralPath $newWorker -Destination $target -Force
if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expectedWorker) {
  throw 'Установленный EXE не совпал; не запускать'
}
Write-Host "Сохранён старый EXE: $backup"
Write-Host "Новый конфиг: $newConfig; прежний конфиг: $oldConfig"
& $target --config $newConfig serve
if ($LASTEXITCODE -ne 0) { throw 'Worker завершился с ошибкой; сохранить текст ошибки и state' }
```

Запускается один worker по прежнему постоянному пути `runtime\winbridge-worker.exe`.
Путь `runtime-test-204` для рабочей установки не используется.
Файервол может запросить разрешение изменившемуся EXE или второму Debian-адресу;
само совпадение пути не гарантирует доступ. Если появляется запрос, проверить
путь и адрес. Автоматически правила не меняются. Не нужно запускать второй worker.

Первый `serve` выполняет cold migration journal **17→19**, предварительно создавая
проверенный consistent SQLite backup. История и дедупликация сохраняются.
Если есть активная незавершённая updater activation, worker откажет в миграции:
**не удалять guards или journal**, сохранить ошибку и разбирать это состояние.

## 5. Проверить результат

Окно `serve` оставить открытым. В другом PowerShell 7 можно проверить локальный IPC:

```powershell
& 'K:\_proj-26\winbridge\runtime\winbridge-worker.exe' `
  --config 'K:\_proj-26\winbridge\runtime\winbridge-worker.multi-controller.json' info
```

На **каждом** Debian в своём checkout и со своим действующим config:

```bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json status
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json info
```

Критерии: обе машины видят worker `0.2.0/source48c834c`, **одни и те же**
worker `generation` и `fencedRequests.context.journalIncarnation`, свой connection
`connected`, прежнюю историю и отсутствие recovery errors. `instanceId` не меняется,
`generation` после перезапуска будет новым. Локальный Info показывает primary `.203`;
для второй связи нужен ответ controller `.204`, не только Windows Info.

Общий список связей доступен новому controller 0.2.0 через MCP
`winbridge_controller_connections` с context из fresh `winbridge_info`.
У старого controller этого диагностического запроса нет; обычные `info`/задачи работают.
Первый одновременный deployment именно двух разных Debian VMs подтверждается
этой проверкой: сохранённый gate проверял четыре controller-процесса на одной VM.

После проверки оставить **один новый рабочий worker** запущенным с новым конфигом.
Desktop не обязателен; при его дальнейшем запуске выбрать тот же новый конфиг.
Переключать worker вручную между `.203` и `.204` больше не нужно.

Задачи одного project ID используют общую очередь. Для независимых сборок разных
веток нужны отдельные зарегистрированные project IDs и непересекающиеся Windows
workspace roots. Пакет не создаёт их автоматически. Третий и четвёртый Debian
добавляются элементами `{id,url}` в topology; максимум четыре.

## Восстановление и границы выпуска

До первого запуска нового `serve` можно вернуть сохранённый EXE при остановленном
worker и использовать неизменённый старый конфиг:

```powershell
# Только ДО первого запуска нового serve / миграции журнала:
Copy-Item -LiteralPath $backup -Destination $target -Force
& $target --config $oldConfig serve
```

После попытки нового `serve` **не откатывать один EXE**: journal мог уже стать19,
а старый worker понимает17. Не копировать открытый SQLite-файл и не подменять БД
автоматически. Сохранить state, путь consistent migration backup и сообщение ошибки;
после новых задач восстановление старой БД потеряет новые результаты.
Чтобы временно вернуться к одному controller после успешной миграции, можно
использовать **новый EXE 0.2.0 со старым single-URL конфигом** после штатной остановки.

Выпуск не включает desktop/controller/updater binaries и не завершает updater.
Проверки normal worker не являются разрешением active updater migration или
автоматического self-update.

## Evidence и воспроизведение упаковки

Native run `20261027-183307-dda61a12`: четыре связи, пять retained tasks,
общая очередь/история/output, cross-controller dedup/cancel, reconnect и независимые
проекты. При упаковке повторно проверяются 15 ready files, 16 отрицательных audit
сценариев и cold17→19 proof. Windows повторно не запускается, рабочий runtime
и конфиги не изменяются. ZIP проверяется по CRC, каждому payload byte и SHA256.
`evidence/report.ready.json` — оригинальный manifest: его worker EXE лежит в корне
ZIP, остальные перечисленные файлы — в `evidence/`.

Из checkout с этими packaging files:

```bash
python3 scripts/package-worker-0.2.0.py \
  --reports /mnt/hgfs/shared/WinBridge/workflow-validation/20261027-183307-dda61a12 \
  --source-archive /mnt/hgfs/shared/WinBridge/source-archives/winbridge-20261027T183112Z-48c834c-d7b3d320.zip \
  --output /mnt/hgfs/shared/WinBridge/releases/20261027-worker-0.2.0-48c834c
```

Выходной каталог должен быть новым; повторная упаковка не перезаписывает выпуск.
