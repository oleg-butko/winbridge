# Windows gate этапа 3: shared snapshot → import → build

Windows runtime gate пройден 2026-10-06: 19 checks, actual import/build/run/cancel/NTFS junction. [Доказательства и точные IDs](windows-snapshot-gate-2026-10-06.md). Остаётся подтвердить исправления формы доставки после обновления binaries. Этот документ сохраняет инструкцию для нового прогона; уже пройденные gates этапов 1–3 повторять не нужно.

## Подготовка Windows

Закрыть desktop и остановить прежний worker через Ctrl+C. Распаковать новый архив исходников из `K:\VMs\shared\WinBridge\sources` в отдельную локальную папку Windows. Не собирать в shared. В PowerShell из папки `winbridge`:

```powershell
.\scripts\prepare-windows-snapshot.ps1
```

Нужны установленный Rust/Cargo 1.88+ и Node/npm для уже используемой сборки desktop. Первый запуск собирает Rust workspace с bundled SQLite и desktop, затем обновляет **те же** exe в `K:\_proj-26\winbridge\runtime`. Для уже собранных новых исходников можно добавить `-SkipBuild`; `-PrepareOnly` оставляет runtime подготовленным без запуска.

Настройки берутся из `target\debug\winbridge-snapshot.local.json`, первоначальные значения — [пример](../examples/windows-snapshot.config.json). Можно передать `-SettingsPath` с существующим локальным JSON. Shared root, namespace, Windows workspace, peer path Debian и report directory находятся в этом файле; exe получает actual delivery/project настройки из дополнительного `winbridge-worker.snapshot.local.json` рядом с exe.

Новый конфиг сохраняет instance/state прежнего worker и его зарегистрированные проекты, добавляет `snapshot-fixture` и рецепт `snapshot-build`. Исходный `winbridge-worker.local.json`, desktop config и форма остаются сохранены. Desktop использует прежний IPC instance/state и получает actual registry выбранного worker через Info. Существующий snapshot config не переписывается; его значения определяют работу worker. Запускается один worker с явным `--config ...snapshot.local.json`.

По умолчанию:

```text
Windows shared: K:\VMs\shared\WinBridge\deliveries\snapshot-fixture\<snapshotId>
Debian shared:  /mnt/hgfs/shared/WinBridge/deliveries/snapshot-fixture/<snapshotId>
Local sources:  K:\_proj-26\winbridge\workspaces\snapshot-fixture\snapshots\<snapshotId>
Build cache:    K:\_proj-26\winbridge\workspaces\snapshot-fixture\build-cache
Journal:        прежний runtime\state\journal.sqlite (локальный диск)
Reports:        K:\VMs\shared\WinBridge\snapshot-validation\<runId>
```

Путь worker.exe не меняется от прогона к прогону. Новые пути firewall, сетевые настройки, mount и services скрипт не создаёт. При обнаружении запущенных exe обновление отклоняется до записи binaries. Первая миграция journal schema 1→2 делает consistent backup `journal-v1-<timestamp>-<uuid>.sqlite` рядом с базой через SQLite `VACUUM INTO`, затем добавляет imports table. Подробности восстановления — [формат](snapshot-format.md).

Оставить окно PowerShell с worker открытым. Передать имя shared каталога `setup-report.json` и подтвердить появление «Controller Debian подключён» и блока «Доставка исходников». JSON подготовки не означает успешный импорт или сборку.

## Что выполняет агент Debian

Fixture не запускает Netwatch. Controller читает только committed Git objects; dirty working tree не включается. Публикация и gate:

```bash
python3 scripts/prepare-snapshot-source.py
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
# В другом терминале, когда Windows worker подключён:
python3 scripts/validate-snapshot.py --controller-config .winbridge/controller-snapshot.local.json
```

Если controller уже работает, использовать один listener: агент останавливает только принадлежащий ему старый процесс и запускает новый с дополнительным конфигом. Пользователь не должен запускать второй controller на том же порту. Обычный config не перезаписывается.

Автоматический gate проверяет actual platform windows, preview и Unicode, четыре concurrent request с одним key, payload conflict, verified import, реальную `cargo.exe build --locked --offline` в snapshot с cache вне исходников, запуск собранного exe и Unicode output, отсутствующий ready, повреждённый ZIP, недоступный пакет, отказ сборки из каждого неполного поколения, отмену активного импорта 64 MiB, отсутствие buildable generation после отмены и настоящий NTFS junction отказ с сохранённым canary. Junction/canary создаются только под уникальными именами внутри зарегистрированного fixture workspace и сохраняются как доказательства; оригинальный готовый package не меняется.

Отчёт `validation-report.json` содержит platform, passed, каждый check, import/task IDs, actual команды и worker generation. Отдельные TaskReport содержат output. `report.ready.json` фиксирует экспортированные filenames/sizes/SHA-256. Local runtime configs, source Git repository и payload staging в экспорт не входят. Агент прочитает готовый отчёт из shared.

Недоступность целого mount покрыта filesystem checks в Debian; native gate проверяет отказ чтения недоступного package. Он не отключает VMware mapping. Crash recovery дополнительно проверен настоящим падением отдельного owned Unix worker и Rust reconciliation tests. Эти проверки не выдают себя за Windows crash execution.

## Наблюдения в desktop

Основной маленький тестовый пакет: **`s-9c3799cf4a9a4c658b222158dc319b2fb0718279`**, проект **`snapshot-fixture`**, 4 файла, ZIP 885 байт. В shared также остаются большие пакеты с `padding.dat` и `gate-...` для отрицательных проверок; наличие папки само по себе не означает успешный импорт. Не выбирать произвольный пакет для основного build/run сценария.

«Snapshot ID опубликованного пакета» теперь searchable select. Worker читает папки `delivery.root\namespace\deliveries\projectId`, возвращает по 100 пакетов, ID/commit/count/ZIP bytes или ошибку метаданных. «Обновить список пакетов» перечитывает shared; «Следующие 100 пакетов» загружает следующую страницу. Поиск действует на загруженные страницы. `.part` и имена вне snapshot policy не предлагаются. ZIP ещё не проверен: повреждённый payload может иметь корректные manifest/ready, а импорт обнаружит ошибку содержимого.

1. В «Последние 10 импортов» видны операции Debian, project/snapshot/commit, shared и локальный пути, phases и причина завершения. Готовый import показывает «Готово для сборки».
2. Выбрать проект `snapshot-fixture`, обновить shared-список и выбрать точный Snapshot ID выше. Нажать «Посмотреть состав snapshot»: видны `Привет.txt`, Rust sources/lockfile, sizes/hashes и обе стороны mapping. Preview подписан как предварительный, без утверждения о payload verification.
3. В задачах выбрать build `0f9646e9-86b2-4786-8993-b58caaddabb2`: snapshot ID, actual cwd под `snapshots\...`, argv с отдельным `build-cache`, stderr Cargo и Exit code 0. Затем выбрать **другую** задачу `102d9bb3-434a-4df7-9bd5-3952d38e3772` и вкладку **«stdout / stderr»**: здесь находится «Проверенный commit → Windows build ✓». Рецепт `snapshot-build` только компилирует программу; её текста в build stdout не ожидается.
4. Failed imports показывают конкретный отказ; отменённый import не предлагается как готовые исходники. Состояние controller и статус операции отображаются отдельно.
5. Закрыть и открыть desktop: прежние imports/tasks читаются из journal. Поля «Проект доставки» и «Snapshot ID опубликованного пакета» восстанавливаются независимо от command form; для каждого worker/project/root сохраняется свой выбор. «Snapshot ID для сборки», как остальные command fields, также сохраняется. Восстановление формы не запускает импорт или команду. Сохранённый пакет, которого больше нет в загруженном shared-списке, остаётся виден с пояснением.

Для ручного запуска после успешной сборки выбрать зарегистрированный проект `snapshot-fixture`, Snapshot ID для сборки тот же, способ «Прямая команда», Executable `K:\_proj-26\winbridge\workspaces\snapshot-fixture\build-cache\debug\snapshot-fixture.exe`, аргументы `[]`, cwd `.`, timeout 60 секунд. Передать задачу и открыть «stdout / stderr». При попытке сборки snapshot без успешного импорта worker объясняет, что сначала нужен импорт, сохраняет исходную причину и не создаёт процесс.

Для новых исправлений достаточно трёх наблюдений: пакеты доступны из select; проект/snapshot сохраняются после открытия окна без нового импорта; непроверенный snapshot даёт понятный отказ сборки. В готовом отчёте не менять результаты старого runtime gate.

Передать результат этих пяти наблюдений. Для закрытия этапа нужны и настоящий Windows JSON gate, и UI подтверждение. При ошибке передать её текст и имя каталога отчёта: новый request/key без сверки существующей операции не нужен.
