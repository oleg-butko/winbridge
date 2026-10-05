# Runner

Recipes и direct commands используют общий command service. Worker проверяет project/cwd, фиксирует accepted task, читает stdout/stderr одновременно, владеет process tree и результатом. При неизвестном исходе не повторять автоматически. Первый модуль этапа 1.

Первый service реализован в `apps/worker/src/lib.rs`, Store — в `apps/worker/src/store.rs`, project/recipe policy — в `crates/core/src/config.rs`. Отдельный modules crate пока не требуется: второй domain handler ещё не появился. Реальные Unix и Windows adapters проверены; [Windows runtime/native UI gate пройден](../../docs/windows-gate-2026-10-05.md).
