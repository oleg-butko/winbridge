# Runner

Recipes и direct commands используют общий command service. Worker проверяет project/cwd, фиксирует accepted task, читает stdout/stderr одновременно, владеет process tree и результатом. При неизвестном исходе не повторять автоматически. Первый модуль этапа 1.
