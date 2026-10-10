# Общие профили рецептов и runner

Первый сценарий этапа 7: два зарегистрированных проекта используют один профиль
из локального worker config. Не требуются новый RPC, отдельная очередь или копия
task service. Это **профиль рецептов**, отдельно от editable desktop workflow
profiles: workflow по-прежнему сохраняет собственные import/build/start параметры.

```json
{
  "recipeProfiles": [{
    "id": "rust-local",
    "label": "Локальная Rust сборка",
    "description": "Locked сборка; cache каждого проекта хранится вне snapshots.",
    "recipes": [{
      "id": "build",
      "executable": "cargo.exe",
      "arguments": ["build", "--locked", "--offline", "--target-dir", "${workspaceRoot}/build-cache"],
      "workingDirectory": ".",
      "timeoutMs": 180000
    }]
  }]
}
```

Это добавление к полному worker config, а не самостоятельный файл. В каждой
выбранной записи `projects` добавить `"recipeProfile": "rust-local"`; локальные
`recipes` оставить массивом, при необходимости пустым. Остальные параметры
проекта, пути и `allowDirectCommands` задаются как прежде. Профиль не включает
разрешение direct и не меняет workspace/project lease.

В executable/arguments рецептов **профиля** разрешены только `${workspaceRoot}` и
`${projectId}`. Подстановка однократная, без shell/environment evaluation. Каждая
строка остаётся одним argv; Unicode/кавычки не требуют ручного shell escaping.
`${workspaceRoot}` всегда означает зарегистрированный корень проекта: при
snapshot build cache остаётся вне immutable поколения. `workingDirectory`
остаётся фиксированным относительным путём; фактический cwd для snapshot — внутри
проверенного поколения. Локальные recipes сохраняют прежние literal strings.

Профиль один на проект, без inheritance/includes. Общие и локальные recipes
объединяются; совпадающий ID отклоняется, скрытых overrides нет. Неизвестный
profile/binding, traversal, invalid timeout/argv и превышение limits отклоняются
до сохранения/start. До 32 profiles, 128 effective recipes/project; раскрытый
project read model ограничен 512 KiB с учётом JSON escaping, чтобы `Info` не
превышал transport frame. Старый config без profiles по-прежнему работает.

Settings позволяет выбрать профиль и отредактировать определения в JSON. Save
проходит прежние validation/conflict/backup/readback checks; применять изменения
нужно ручным restart worker. Running config не изменяется. Это не разрешение
автоматически перезапускать неизвестно завершившиеся команды.

Command form показывает профиль/описание, источник рецепта и раскрытые параметры.
В одной транзакции accepted task сохраняются resolved command и `recipeOrigin`:
recipe ID, profile ID/label либо источник local configuration. После config edit
история остаётся прежней. Старые records без поля читаются без выдуманной provenance.
Очередь revalidates frozen command перед launch; изменённая команда даёт failed
с объяснением и требует новой явной задачи. Same key возвращает прежнюю task,
даже если профиль уже изменён. Workflow steps используют тот же resolver/store. Определения recipe build/start
сохраняются в recipePins до ACK workflow. Queued workflow после restart сравнивает
их с действующими recipes; изменение даёт failed до import/build/start. Label-only
edit не меняет команду, историческая provenance сохраняется. Legacy workflow без
pins читается; queued recipe workflow требует нового явного запроса, поскольку
прежнее определение восстановить нельзя. История не получает выдуманной provenance.

Статический `CommandHandler` contract находится в core; direct/recipe реализации
и общий filesystem resolver — в `modules/runner`. Handler раскрывает определение,
не владеет процессом/ACK/SQLite. Worker остаётся execution authority. Handler
metadata добавлена в read-only Info, transport/Hello/RPC не изменены. Общий
`JournalTimeline` используется tasks/imports/workflows; snapshot phases подписаны
понятно, остальные summaries сохраняются без изменения смысла.

Проверка на Windows: `validate-workflow-native.py --profiles-only`, вызывающий
`test-windows-workflow.ps1 -ProfilesOnly`. Рабочий worker только запускает сценарий;
отдельный новый worker проверяет два проекта, argv/cwd, dedup/conflict, запрет
direct, cancel, owned crash/restart и provenance. Проверяются также прежние
snapshot/workflow/tree/timeout tests. Desktop не требуется, окон не появляется.
Report/ready/proof пишутся только в новый shared run и проверяются из Debian.

[Automatic Windows gate](windows-profiles-gate-2026-10-10.md) прошёл: 68 native
tests и пять verified artifacts. 152 workspace tests и full browser integration
также прошли. Первый сценарий этапа 7 завершён; работающий runtime не обновлялся.
