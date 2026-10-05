# VMware Shared Folders: доставка исходников

## Что известно

Пользователь сообщил mapping `/mnt/hgfs/shared` ↔ `K:\VMs\shared`. Проверка в Debian 2026-10-04 показала, что `/mnt/hgfs` смонтирован как `fuse.vmhgfs-fuse`, а каталог shared существует. На этапах 1/2 mapping подтверждён настоящими Windows отчётами; на этапе 3 actual publisher записал manifest/ZIP/ready через HGFS. Native import/build gate ещё ожидается. Mount, права и настройки VMware не изменялись.

HGFS даёт гостю представление каталога хоста. Файл, записанный Debian через shared путь, становится доступен Windows через native путь; отдельный сетевой uploader для этой доставки не требуется. Использование user-mode `vmhgfs-fuse` описано [Broadcom](https://knowledge.broadcom.com/external/article?legacyId=60262) и [open-vm-tools](https://github.com/vmware/open-vm-tools).

Пользователь выбрал shared папку для доставки готовых снимков и отдельную Windows папку для сборки. Direct build/edit в общем каталоге не входит в первый вариант.

## Раскладка данных

```text
/mnt/hgfs/shared/WinBridge/                    K:\VMs\shared\WinBridge\
  deliveries/<projectId>/<snapshotId>/
    payload.zip.part                         Незавершённая запись
    payload.zip                              Готовый пакет
    manifest.json                            Состав и hashes
    ready.json                               Публикуется последним

K:\_proj-26\winbridge\workspaces\<projectId>\           Отдельная локальная Windows папка
  snapshots/<snapshotId>/                     Проверенные исходники
  cache/                                     Windows build/dependency caches
  artifacts/<taskId>/                         Результаты конкретной задачи

K:\_proj-26\winbridge\runtime\state\                          Локальное состояние worker из config
  journal.sqlite                            SQLite tasks/imports/output

/mnt/hgfs/shared/WinBridge/validation/         K:\VMs\shared\WinBridge\validation\
  <timestamp>-<id>/                          Отчёты проверки Windows, JSON и stderr
```

Корни задаются локальными JSON; на этапе 3 UI показывает обе стороны mapping и состав snapshot. Автоматический setup editor/probe ещё не реализован. Controller выбирает relative delivery ID, а не передаёт произвольный Windows absolute path.

На этапе 1 [Windows проверка](windows-validation.md) берёт reportDirectory из локального файла рядом с worker exe и сохраняет отчёты в shared. Тестовый project и live SQLite остаются в локальной папке исходников; desktop читает отдельный локальный JSON рядом со своим exe. Это экспорт диагностических файлов, не реализация snapshot import.

## Публикация и импорт

1. Controller фиксирует точный commit SHA из Git objects в локальном staging Debian; dirty snapshot — последующее расширение.
2. Собирает manifest и payload с allowlist проекта: исходники, lockfiles, build scripts, assets и templates.
3. Копирует payload в отдельный snapshot namespace с `.part` именем. После закрытия записи завершает manifest и публикует ready marker последним.
4. Через WebSocket отправляет worker request на импорт relative delivery ID и manifest hash. Ready marker — условие доступности, но не доказательство корректности bytes.
5. Worker читает manifest, проверяет ограничение объёма/числа файлов, traversal, дубли и case collisions, reserved names, links/reparse points, размеры и hashes.
6. Worker распаковывает в staging на локальном Windows диске, проверяет итог и публикует immutable поколение. Сборка начинается только после verified import.
7. ACK и события import передаются по control session. При обрыве mount задача показывает причину и допускает повтор импорта с тем же snapshot ID; команда сборки не дублируется.

Не считать rename через HGFS и native Windows, directory notifications, mtime и flush универсальной транзакцией. Ready marker + validation используются совместно. Polling и явный RPC должны работать без обязательного inotify; оптимизацию watchers добавлять после проверки на этой VM.

## Скорость

HGFS может убрать необходимость отдельной передачи payload по socket, но результат зависит от размера файлов, числа операций metadata, compression и записи на диск. Нельзя заранее обещать, что live дерево мелких файлов быстрее одного пакета или TCP. Linux запись в HGFS всё равно проходит через файловый adapter.

MVP: один пакет точного snapshot, простой manifest, проверенный import. Следующий шаг: передавать изменившиеся файлы относительно manifest worker, сохраняя локальные caches Windows. Для больших объектов — chunks, hashes и resume. SharedFolderDelivery и будущий SocketDelivery реализуют одинаковый интерфейс; выбор backend виден в UI и не меняет task semantics.

Сравнить: полное дерево мелких файлов, один archive, incremental пакет, один большой файл. Измерять capture/pack, shared write, Windows import/hash, общий time-to-build и задержку cancel во время доставки. Для экспериментов использовать только отдельный разрешённый namespace; существующие shared файлы не трогать.

## Что не хранить в общей папке

Live SQLite/WAL, process handles, runtime configs, credentials и постоянные caches. Общая папка не заменяет WebSocket control session, журнал и локальную execution authority. Retention/cleanup удаляет только зарегистрированные delivery IDs WinBridge по явной политике и не очищает корень shared.

На настоящем HGFS rename непустого staging directory дал ENOENT. Publisher использует per-file `.part`/readback/rename и ready last; local Windows source generation публикуется отдельным local rename после проверки. [Runtime contract и ограничения](snapshot-format.md).
