# ADR 0001 — отдельный проект и стек

Accepted: 2026-10-04.

Пользователь выбрал имя WinBridge, отдельный каталог `~/winbridge` и Git repository. Netwatch сохраняет собственный runtime и код мониторинга; его исправления закоммичены отдельно.

Стек: Rust для controller/worker и общих libraries; React/Mantine для UI; Tauri для будущего Windows shell. Node.js нужен при сборке frontend. Общие DTO/core не зависят от Tauri и соседнего репозитория.

Scaffold начинается с protocol/core и controller/worker. Статические modules подключаются постепенно, а interfaces стабилизируются после реальных handlers. Предыдущая идея одного workspace с Netwatch заменена этим решением.
