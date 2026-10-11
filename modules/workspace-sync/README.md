# Workspace sync

Rust adapter для точного Git commit capture, deterministic Stored ZIP, shared `.part`/readback/ready publication и verified local import. [Контракт](../../docs/reference/snapshot-format.md), [native gate](../../docs/validation/windows-snapshot-validation.md).

Core содержит pure metadata policy. Worker владеет SQLite acceptance/recovery/progress и общей project lease; module не выполняет build и не открывает shared SQLite. Source generations immutable, caches/output находятся отдельно. Full runtime прошёл Windows gate этапа 3. [Delta](../../docs/guides/snapshot-delta.md) добавляет schema 2, pinned base reuse, отдельное immutable generation и measured publication; [автоматические Windows gates](../../docs/done/stage0-7/stage6/windows-delta-gate-2026-10-09.md) прошли: native tests и actual Debian-produced packages с build/output/cancel/attention.
