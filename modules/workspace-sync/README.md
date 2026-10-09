# Workspace sync

Rust adapter для точного Git commit capture, deterministic Stored ZIP, shared `.part`/readback/ready publication и verified local import. [Контракт](../../docs/snapshot-format.md), [native gate](../../docs/windows-snapshot-validation.md).

Core содержит pure metadata policy. Worker владеет SQLite acceptance/recovery/progress и общей project lease; module не выполняет build и не открывает shared SQLite. Source generations immutable, caches/output находятся отдельно. Full runtime прошёл Windows gate этапа 3. [Delta](../../docs/snapshot-delta.md) добавляет schema 2, pinned base reuse, отдельное immutable generation и measured publication; новый автоматический Windows gate выполняется.
