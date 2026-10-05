# Образцы контрактов

`task.direct.json` и `task.recipe.json` соответствуют Rust TaskSubmission. Они являются данными, не исполняемыми скриптами. Controller/worker scaffold их пока не принимают.

`controller.config.json` и `worker.config.json` — предлагаемый schemaVersion 1 для этапа 1/2; config loader пока не реализован. Все IP и Windows workspace paths — примеры, их нужно выбирать в setup. Пара `192.168.56.2`/`.1` не измерена в этой VM. Listener WinBridge использует отдельный примерный port 1143, не Netwatch 1113.

Mapping shared folder задан пользователем. `%LOCALAPPDATA%` должен быть раскрыт config loader на Windows, а не shell-конкатенацией. `netwatch` — пример первого реального проекта; sourceRoot относится к соседнему самостоятельному repository. Не создавать runtime dependency на его Rust crate.

Настройка project даёт обычные recipes/direct в выбранном workspace. Она не обещает OS sandbox. Snapshot/generation binding добавится в task contract на этапе 3 до реальной доставки.
