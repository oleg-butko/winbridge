# ADR 0004 — доступ к проектам и прозрачность действий

Accepted: 2026-10-04, прямой ответ пользователя.

Разрешены recipes и произвольные команды внутри зарегистрированной папки проекта. Каждая задача проходит один command service, policy, persistence и journal. Обычные разрешённые действия не требуют новой confirmation на каждую команду.

Direct command содержит executable, argv, относительный cwd и timeout. Выбор project root ограничивает scope WinBridge, но не является OS sandbox для кода, выполняемого с правами пользователя.

UI/UX — обязательная часть этапа: пользователь видит инициатора, source snapshot, command/argv/cwd, progress, stdout/stderr, exit code и действия при сбое. Task status и freshness соединения показываются отдельно. Popup/звук — уместный attention event с настройкой пользователя.
