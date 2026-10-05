# Snapshot format — начало этапа 3

Реализованы DTO, проверка metadata и CLI preflight. Публикация payload/ready, импорт, проверка bytes/links и UI import ещё не реализованы. `workspace_import` остаётся planned capability. [Windows gate этапа 2](windows-session-gate-2026-10-05.md) завершён отдельно и не доказывает доставку файлов.

Контракт определяется [protocol/snapshot.rs](../crates/protocol/src/snapshot.rs); TypeScript генерируется из него. Проверки без filesystem находятся в [core/snapshot.rs](../crates/core/src/snapshot.rs). Core не содержит HGFS путей. Producer и importer будут использовать одинаковые правила.

## Раскладка и контракт

Shared mapping остаётся `/mnt/hgfs/shared` ↔ `K:\VMs\shared`. Namespace задаётся локальным config; при подключении текущей пары следует использовать существующий `WinBridge` с той же капитализацией на Debian. Delivery ID состоит из projectId/snapshotId, оба — 1–80 букв/цифр/dash/underscore без зарезервированных Windows имён. Абсолютный Windows путь в request не передаётся.

```text
<configured-shared-root>/<namespace>/deliveries/<projectId>/<snapshotId>/
  payload.zip.part       незавершённая запись producer
  payload.zip            файлы с относительными путями, без внешнего root prefix
  manifest.json          schemaVersion/projectId/snapshotId/source/files/sizes/SHA-256
  ready.json             IDs, SHA-256 manifest/payload и размер payload; публикуется последним
```

Первая source mode — git_commit с полным lowercase Git object hash (40 или 64 hex). Capture должен читать фиксированный commit, не меняющийся working tree. Immutable dirty snapshot — последующее расширение. Snapshot ID непрозрачный, для повторного импорта передаётся тот же ID; источник и manifest должны совпадать с сохранённой проверенной generation.

Manifest задаёт размер/hash payload.zip и список regular files: canonical относительный UTF-8 path с `/`, bytes, SHA-256 и executable flag. Полный uncompressed byte count должен совпадать с суммой files. Тип ссылок отсутствует: producer обязан отклонить Git symlinks/submodules, importer — link/reparse/archive entries вместо regular files. Executable flag относится к исходному mode, не разрешает автоматический запуск файла.

Ready marker повторяет IDs, размер/hash payload и отдельно SHA-256 exact manifest bytes. Проверка agreement метаданных не вычисляет hashes: importer обязан проверить actual bytes перед публикацией локальной generation. Ready marker сам по себе не доказывает корректность пакета.

## Metadata limits и отказы

- Manifest JSON — до 1 MiB, ready JSON — до 8 KiB; неизвестные поля/schema/modes отклоняются.
- 1–10000 files; ZIP до 512 MiB, суммарный uncompressed content до 1 GiB. Arithmetic overflow отклоняется. Граница JSON размера дополнительно ограничивает реально достижимое число entries.
- Relative path — до 1024 UTF-16 units, component до 255, depth до 64. Реальные filesystem/API ограничения дополнительно проверит importer.
- Traversal, absolute/drive/UNC/device/ADS paths, backslash и dot components, trailing dots/spaces, NTFS reserved names/control characters запрещены.
- Повтор file, case collisions файлов/каталогов и file/directory conflicts отклоняются в любом порядке. Unicode uppercase comparison консервативный; проверка actual Windows destination остаётся обязательной.
- Git metadata, .winbridge, node_modules/target, environment files и .local.json исключены из source payload. Никаких runtime configs/SQLite/build caches в shared.

Изменение лимитов либо source selection должно быть явным и одинаковым у producer/importer. Metadata preflight не читает ZIP, не распаковывает files и не проверяет источник по filesystem.

## Уже доступная проверка

```bash
cargo build -p winbridge-controller --locked
./target/debug/winbridge-controller --check-snapshot-manifest examples/snapshot-manifest.json
```

[Пример](../examples/snapshot-manifest.json) — только metadata fixture с условными hashes, не настоящий payload. CLI возвращает `validation: metadata_only`, `payloadVerified: false`, `imported: false`. Для этой команды controller listener/config и Windows worker не нужны; она читает не более 1 MiB + 1 byte входного файла.

Следующий вертикальный сценарий: capture commit → pack/validate → shared staging/ready → worker verified local import с project mutation lease → UI preview/progress/result → build из проверенной generation. Уже работающие command task, cancel, durable acceptance и reconnect должны сохраниться.
