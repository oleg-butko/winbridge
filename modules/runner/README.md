# Runner

Recipes и direct commands используют общий command service. Worker проверяет project/cwd, фиксирует accepted task, читает stdout/stderr одновременно, владеет process tree и результатом. При неизвестном исходе не повторять автоматически. Первый модуль этапа 1.

`src/lib.rs` реализует статические direct/recipe handlers и общий filesystem resolver.
Контракт `CommandHandler` находится в core; reusable definitions — в
`crates/core/src/profiles.rs`. Обычные tasks и workflow steps используют этот
resolver. Snapshot cwd меняется отдельно от bindings зарегистрированного root,
поэтому build cache не переносится в immutable source generation.

Worker по-прежнему владеет schedule/acceptance/process tree, Store —
`apps/worker/src/store.rs`. Модуль не создаёт вторую очередь и не отправляет ACK.
Добавление проекта с общим recipe profile не меняет transport или task service.
[Контракт, UI и автоматическая проверка](../../docs/project-profiles.md).
