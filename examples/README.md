# Образцы контрактов

`task.direct.json` и `task.recipe.json` соответствуют Rust TaskSubmission. Worker принимает их через `--config PATH submit REQUEST.json`, если зарегистрирован соответствующий проект и запущен `serve`. Controller передаёт тот же submit через локальную WebSocket сессию к выбранному worker. Это образцы для Netwatch, а не команда проверки fixture.

`worker.config.json` поддерживается loader schemaVersion 1. Зарегистрированные workspaceRoot и recipe cwd должны существовать; stateDirectory должен указывать на отдельную локальную папку вне shared. `controller.config.json` поддерживается controller loader schemaVersion 1. Все IP и Windows workspace paths — примеры, их нужно выбирать в setup. Выбранная пара VMnet8 `192.168.44.203`/`192.168.44.1` подтверждена пользователем; смена адреса Debian требует обновить listenAddress/controllerUrl. IP источника не фильтруется и записывается в activity.jsonl с ротацией; старое allowedWorkerAddresses игнорируется. Примерный port WinBridge — 1143, не Netwatch 1113. controllerUrl устанавливает реальную сессию; delivery/import ещё не выполняются.

Mapping shared folder задан пользователем. Пути в JSON не используют подстановку переменных окружения. `netwatch` — пример первого реального проекта; sourceRoot относится к соседнему самостоятельному repository. Не создавать runtime dependency на его Rust crate.

Настройка project даёт обычные recipes/direct в выбранном workspace. Она не обещает OS sandbox. Snapshot/generation binding добавится в task contract на этапе 3 до реальной доставки.

На этапе 1 `maxConcurrentMutations` равен 1, recipe arguments фиксированы (request.arguments для recipe должен быть пустым), effective timeout ограничен request и recipe. `maxOutputBytesPerTask` по умолчанию 16 MiB; превышение помечает gap и продолжает drain. Browser recent list содержит последние 10 summary; полный record читается по Task ID, output — страницами по 16 chunks с afterSequence.

`desktop.config.json` — образец файла `winbridge-desktop.local.json` рядом с desktop exe. `workerConfigPath` задаётся явно; относительный путь отсчитывается от папки desktop config.

`windows-validation.config.json` задаёт defaults для `target/debug/winbridge-validation.local.json` рядом с worker exe. Отчёты по умолчанию находятся в `K:\VMs\shared\WinBridge\validation`, fixture/state — в локальной `.winbridge/windows-validation` папке исходников. Сценарий сохраняет существующие настройки и desktop configs, для каждого прогона создаёт собственный каталог. См. [инструкцию](../docs/windows-validation.md).

`windows-session.config.json` задаёт seed настроек Windows session подготовки. Скрипт создаёт `target/debug/winbridge-session.local.json`, существующий файл сохраняет. Новые worker/desktop configs и exe размещаются в отдельном локальном runtime; setup report публикуется в shared. [Инструкция](../docs/windows-session-validation.md).
