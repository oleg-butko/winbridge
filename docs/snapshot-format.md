# Snapshot format и runtime этапа 3

Реализованы capture commit → Stored ZIP → verified shared publication → durable worker import → explicit build из проверенного локального поколения. [Windows gate](windows-snapshot-validation.md) ещё требуется. Unix/HGFS/WebSocket проверки не закрывают Windows этап автоматически.

Контракт: [protocol/snapshot.rs](../crates/protocol/src/snapshot.rs), pure metadata policy: [core/snapshot.rs](../crates/core/src/snapshot.rs), Git/filesystem adapter: [workspace-sync](../modules/workspace-sync/src/lib.rs). Core не содержит HGFS путей. TypeScript генерируется из Rust.

## Раскладка и capture

Shared mapping `/mnt/hgfs/shared` ↔ `K:\VMs\shared`; `delivery.root`, `namespace` и optional `peerRoot` заданы локальными JSON. Namespace текущей пары — **WinBridge** с той же капитализацией на Debian. Source registry controller задаёт unique project ID, Git repository root и `sourceMode: commit`. Worker регистрирует отдельный local workspace.

```text
<shared-root>/<namespace>/deliveries/<projectId>/<snapshotId>/
  payload.zip.part         незавершённый payload
  payload.zip              файлы с относительными путями, без root prefix
  manifest.json.part       незавершённый manifest
  manifest.json            exact JSON bytes, размеры и SHA-256
  ready.json               IDs, SHA-256 manifest/payload и размер payload; последняя запись

<local-workspace>/.winbridge/imports/<importId>/payload.zip
<local-workspace>/.winbridge/imports/<importId>/source/       unbuildable staging
<local-workspace>/snapshots/<snapshotId>/                    verified immutable source
  .winbridge/manifest.json
  .winbridge/verified.json
<local-workspace>/build-cache/                              отдельно от исходников
```

Publisher resolves полный lowercase commit SHA (40/64 hex) и читает `ls-tree`/`cat-file` только этого commit с отключёнными Git replacement objects. Source root должен совпадать с repository root. Незакоммиченные файлы не читаются. Git symlinks/submodules и nonregular modes отклоняются. Snapshot ID publisher — `s-<full-SHA>`; ZIP Stored, timestamp 1980, regular/executable modes фиксированы. Повтор того же capture возвращает тот же verified package после сверки manifest и actual payload.

Публикация CreateNew не переписывает existing ready packages. В этой VM HGFS возвращает ENOENT при rename непустого каталога, поэтому используются отдельные `.part` files, readback hashes и per-file rename; `ready.json` остаётся барьером доступности. Частичная запись JSON ready отклоняется parser/hash checks. Interrupted publication остаётся для диагностики и не импортируется; automatic repair/cleanup не реализован. Не удалять чужие пакеты или содержимое shared root.

Manifest задаёт source git_commit, IDs, размер/hash payload.zip и список canonical UTF-8 files: path, bytes, SHA-256, executable. Payload paths не могут включать внешний ZIP prefix. Ready повторяет IDs, размер/hash payload и SHA-256 **исходных manifest bytes**, без повторной сериализации. Ready не является доказательством корректности ZIP.

## Валидация и import

- Manifest до 1 MiB, ready до 8 KiB, strict schema 1/known fields. 1–10000 files, ZIP до 512 MiB, content до 1 GiB, checked size sums.
- Path до 1024 UTF-16 units, component до 255, depth до 64. Traversal, absolute/drive/UNC/device/ADS, backslash/dot components, trailing dots/spaces, reserved names/controls, duplicate/case/file-directory collisions отклоняются.
- Git data, `.winbridge`, `node_modules`, `target`, `.env*` и `*.local.json` запрещены в payload. Snapshot не переносит runtime настройки, SQLite или caches.
- Перед ZIP parser ограничены EOCD entry count и central directory bytes; ZIP64/multipart/compressed archives не поддерживаются в schema 1. ZIP должен содержать ровно files manifest, без дополнительных directory/link/special entries.
- Worker копирует payload на локальный диск и проверяет bytes/hash, распаковывает только manifest paths с CreateNew, ограничивает декодируемые bytes, проверяет file hashes и итоговое дерево. Symlink, NTFS junction/reparse, special files и hard links отклоняются на actual filesystem.
- Final source rename выполняется только на локальном fixed disk после verified marker. Interrupted staging не используется как cwd. Неожиданные дополнительные файлы в поколении тоже блокируют build; outputs/cache должны записываться отдельно.

Импорт имеет собственный Import ID, submission и journal events. `import_snapshot` сохраняет accepted до ACK. Dedup scope — import/project/key, requestId может меняться; изменённый payload с тем же key отклоняется. Command keys и import keys имеют отдельные namespaces. Close UI/reconnect не запускает повторный import/build. UI preview возвращает до 100 files/page и честное `payloadVerified: false`.

Import и command используют одну project mutation lease; queued порядок определяется accepted timestamps, при равенстве command принимается первым. Два поколения одного project тоже не импортируются/не собираются параллельно. Cancel/timeout проверяются на checkpoints каждого 64 KiB и перед публикацией. ACK отмены означает запрос; итог может быть succeeded, если generation уже опубликована. Отмена после publication не удаляет готовые источники.

После crash running import сверяется с final local marker и actual source hashes. Полностью verified generation восстанавливает succeeded; incomplete import становится interrupted и требует нового explicit key. Queued сохраняется. Unknown external command остаётся indeterminate, без автоматического retry. Existing verified generation можно повторно проверить и использовать без shared/mount; другой manifest hash при том же ID отклоняется.

`TaskSubmission.snapshotId` optional: без него сохраняется исходный command service проекта, с ним worker разрешает cwd/relative exe внутри verified generation и сохраняет snapshot/commit/manifest hash в TaskRecord при acceptance. Перед process launch под lease проверяются исходный accepted manifest hash и всё дерево, включая попытку заменить local marker вместе с файлами. `cwd` не является sandbox: доверенная команда выполняется с правами пользователя. Source verification защищает стабильность исходников, не изолирует arbitrary shell/arguments от ОС.

## CLI и UI

```bash
./target/debug/winbridge-controller --config CONTROLLER.local.json publish PROJECT HEAD
./target/debug/winbridge-controller --config CONTROLLER.local.json preview PROJECT s-<full-SHA>
./target/debug/winbridge-controller --config CONTROLLER.local.json import IMPORT.json
./target/debug/winbridge-controller --config CONTROLLER.local.json imports
./target/debug/winbridge-controller --config CONTROLLER.local.json get-import IMPORT_ID
./target/debug/winbridge-controller --config CONTROLLER.local.json cancel-import IMPORT_ID
```

```json
{
  "protocolVersion": 1,
  "requestId": "correlation-id",
  "projectId": "snapshot-fixture",
  "snapshotId": "s-full-commit-sha",
  "manifestSha256": "64 lowercase hex characters from published ready",
  "idempotencyKey": "one-logical-import",
  "timeoutMs": 180000
}
```

Worker и controller CLI поддерживают `snapshots PROJECT [OFFSET]`: read-only discovery по 100 entries в настроенном shared namespace, максимум 10000 directory entries за запрос. Разрешены только зарегистрированные проекты и IDs из policy, linked/reparse parent отклоняется, linked candidates показывают metadata error. Список проверяет bounded manifest/ready и их hashes, не читает payload; отсутствие пакета и недоступность root различаются. Порядок лексикографический по ID; после изменения каталога обновить список. Worker CLI поддерживает те же import/preview/query/cancel операции через IPC. Handshake advertises workspace_import только при delivery config; controller проверяет capability до отправки import RPC. Info `snapshotImportAvailable` управляет native UI. Desktop показывает commit, manifest files/hashes, обе mapping стороны, local source path, phases, bytes/files, history/reasons и cancel; command form сохраняет optional snapshot ID. Сборка остаётся явной отдельной командой. Shared package select показывает ID/commit/count/ZIP size; неполные metadata обозначаются причиной ошибки. Delivery project/snapshot сохраняются независимо от command form, по worker/project/workspaceRoot, в `winbridge-desktop-state.local.json` рядом с exe. Schema 1 дополнена optional-on-read `deliveryDrafts`; старые файлы принимаются, существующие command drafts сохраняются. Новый desktop следует обновлять вместе с worker; обратный запуск старого desktop с расширенным state не поддерживается его strict reader. Close ждёт последние saves для каждой изменённой формы/scope; ошибка не скрывается успешным сохранением другой формы.

`--check-snapshot-manifest PATH` остаётся metadata-only preflight для [условного примера](../examples/snapshot-manifest.json), не проверяет ZIP/hash/import. Browser demo не выполняет команды.

## Journal migration и восстановление

Schema 2 добавляет imports table к journal schema 1. Перед migration существующего versioned SQLite используется consistent `VACUUM INTO journal-v1-<timestamp>-<uuid>.sqlite`; FULL/WAL сохраняют accepted state. Новая пустая база не требует backup. При неудаче backup migration не начинается; future/unknown schema отклоняется. Backup filename печатается при запуске, повторная migration не создаёт новую копию.

Для восстановления **остановить worker**, выбрать отдельный пустой локальный state directory, скопировать туда consistent backup как `journal.sqlite` и указать этот directory в локальном worker config. Не копировать открытые SQLite/WAL и не заменять active database; original state и backup сохранить. Для старого desktop Windows IPC использует instance ID; не запускать одновременно два worker одного instance. Новые imports отсутствуют в version 1 backup; команды сохраняют состояние на момент consistent snapshot. Автоматический возврат schema не выполняется.

## Проверенные границы

48 Rust tests, actual Unix builds, WebSocket/HGFS gate, active 64 MiB cancel, actual owned process crash/recovery и UI checks проходят. [Windows runtime gate](windows-snapshot-gate-2026-10-06.md) прошёл 19 actual checks. Windows all-targets compile и standalone desktop также проходят; native UI новых delivery fixes принята пользователем; подробная ручная проверка отложена. [Инструкция](windows-snapshot-validation.md).

Dirty snapshots, chunk resume, incremental sync, cleanup/retention и benchmarks остаются следующими расширениями. Импорт не удаляет пользовательские файлы и не обновляет runtime/source generation in-place.
