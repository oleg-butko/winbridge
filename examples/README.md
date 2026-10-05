# Образцы контрактов

`task.direct.json` и `task.recipe.json` соответствуют Rust TaskSubmission. Worker принимает их через `--config PATH submit REQUEST.json`, если зарегистрирован соответствующий проект и запущен `serve`. Controller пока scaffold. Это образцы для Netwatch, а не команда проверки fixture.

`worker.config.json` поддерживается loader schemaVersion 1. Зарегистрированные workspaceRoot и recipe cwd должны существовать; stateDirectory должен указывать на отдельную локальную папку вне shared. `controller.config.json` остаётся образцом для этапа 2. Все IP и Windows workspace paths — примеры, их нужно выбирать в setup. Пара `192.168.56.2`/`.1` не измерена в этой VM. Примерный port WinBridge — 1143, не Netwatch 1113. Поля controllerUrl/delivery читаются, но connection/import ещё не выполняются.

Mapping shared folder задан пользователем. Пути в JSON не используют подстановку переменных окружения. `netwatch` — пример первого реального проекта; sourceRoot относится к соседнему самостоятельному repository. Не создавать runtime dependency на его Rust crate.

Настройка project даёт обычные recipes/direct в выбранном workspace. Она не обещает OS sandbox. Snapshot/generation binding добавится в task contract на этапе 3 до реальной доставки.

На этапе 1 `maxConcurrentMutations` равен 1, recipe arguments фиксированы (request.arguments для recipe должен быть пустым), effective timeout ограничен request и recipe. `maxOutputBytesPerTask` по умолчанию 16 MiB; превышение помечает gap и продолжает drain. Browser recent list содержит последние 10 summary; полный record читается по Task ID, output — страницами по 16 chunks с afterSequence.

`desktop.config.json` — образец файла `winbridge-desktop.local.json` рядом с desktop exe. `workerConfigPath` задаётся явно; относительный путь отсчитывается от папки desktop config.

`windows-validation.config.json` задаёт defaults для `target/debug/winbridge-validation.local.json` рядом с worker exe. Отчёты по умолчанию находятся в `K:\VMs\shared\WinBridge\validation`, fixture/state — в локальной `.winbridge/windows-validation` папке исходников. Сценарий сохраняет существующие настройки и desktop configs, для каждого прогона создаёт собственный каталог. См. [инструкцию](../docs/windows-validation.md).
