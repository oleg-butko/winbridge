# Delta-доставка исходников

Сценарий этапа 6 реализует согласованный [план](snapshot-delta-plan.md): ранее
импортированное поколение служит локальным кэшем. Независимого object store,
автоматической очистки snapshots и resume здесь нет.

## Публикация и импорт

```bash
# Первый commit доставляется полностью; его ready.snapshotId нужен как база.
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json publish snapshot-fixture HEAD

# После нового commit: полный состав дерева, ZIP только изменённых файлов.
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json publish-delta snapshot-fixture HEAD BASE_SNAPSHOT_ID

# REQUEST.json имеет прежний ImportSubmission: projectId, snapshotId,
# manifestSha256, protocolVersion, requestId, idempotencyKey, timeoutMs.
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json import-delta REQUEST.json
```

`publish-delta` возвращает ready и monotonic stage timings, читает только Git
commit и hashes actual blobs, включая переиспользуемые. Неизменённые файлы могут
переименовываться или менять executable mode. Новый snapshot имеет отдельный
`d-<SHA-256>` ID от target commit, project и точной базы/hash. Полный пакет
того же commit остаётся доступен как `s-<commit>`.

MCP: `winbridge_publish_delta(projectId, revision, baseSnapshotId)` и
`winbridge_import_delta_snapshot(request)`. CLI/MCP вызывают те же handlers.
Controller проверяет `workspace_delta` перед отправкой нового запроса.
При обновлении сначала запустите новый Debian controller, затем обновите Windows
обычным `prepare-windows-snapshot.ps1`. Прежний worker обслуживает полный формат.

База должна быть импортирована в workspace этого проекта. Недоступная база,
неверный manifest pin, links или повреждённые используемые файлы блокируют
публикацию нового локального поколения. Ошибка не запускает сборку. Для явного
fallback опубликуйте полный пакет выбранного commit и импортируйте его с новым
ключом; это отдельный запрос пользователя/агента, а не скрытый retry.

## Что видит пользователь

Список shared-пакетов отмечает delta. Preview показывает полный состав целевого
commit, ID/hash базы, число файлов из базы, ZIP bytes и полный объём исходников.
Во время импорта видна фаза копирования/проверки базы; её сведения сохраняются
в journal и читаются после закрытия desktop. Lost ACK сохраняет прежний метод,
параметры и ключ. После проверки новое поколение готово для отдельной сборки.

Удалённый из Git файл отсутствует в новом поколении; прежнее поколение остаётся.
Windows копирует actual bytes в новый staging, без hard links, затем проверяет
всё дерево. Уже готовое поколение не зависит от доступности базы/shared при
повторной проверке или сборке. Build проверяет принятый manifest pin и исходники
непосредственно перед process launch.

## Автоматическая проверка

Агент запускает `validate-workflow-native.py --delta-only` с clean source archive
через установленный worker. Desktop и тестовые окна не нужны. Сценарий собирает
offline новые Rust tests; отдельные TempDir workspace/journal используют реальные
HGFS пакеты в новом shared report. Installed runtime/config/journal не заменяются.
Проверяются full/delta equivalence, rename/delete/mode/empty ZIP/chain, неверная и
отсутствующая база, cancellation, durable dedup, restart recovery, offline build,
Unicode output и отказ запуска после tamper. Отчёт содержит hashes и timings.

Проверки Linux, browser UI и Windows cross-check проходят. Actual Windows gate
для этой реализации ещё выполняется; baseline предыдущих этапов не заменяет его.
