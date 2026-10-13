# Документация WinBridge

Начните с [краткого руководства пользователя](user-friendly-guide-v1/README.md):
готовые возможности, интерфейс и состояния, подключение проектов, инструменты MCP.

[Практический сценарий WinBridge v1](user-friendly-guide-v1/winbridge-v1/README.md): один Windows `src`, постоянный `runtime`, MCP, звук/UI и ручная проверка обновления.

[Аналитические отчёты](analysis/README.md):
[консистентность, очередь и расширяемость](analysis/consistency-extensibility/README.md)
и [Netwatch через WinBridge](analysis/netwatch-agent-pipeline/README.md).
Готовые возможности отделены от предложений по развитию.

[Долгосрочный переход A0–A9](plans/architecture-transition/README.md): FSM,
consistency guards, очередь нескольких проектов, actors и versioned executors.
[Ревью плана](plans/architecture-transition/review.md) объясняет порядок и риски.

## Основные документы

| Документ | Для чего читать |
| --- | --- |
| [Архитектура](architecture.md) | Компоненты, владение задачами и текущие решения. |
| [UI/UX](ux.md) | Прозрачность действий, состояния и поведение интерфейса. |
| [Структура кода](structure.md) | Границы packages/modules и правила расширения. |
| [Roadmap](roadmap.md) | Результаты этапов 0–7 и новый цикл архитектурного перехода A0–A9. |
| [Handoff](handoff.md) | Готовность проекта, последние проверки и сведения для следующей сессии. |

## Руководства — `guides/`

| Документ | Содержание |
| --- | --- |
| [Подключение проекта](guides/project-integration.md) | Регистрация, рецепты и полный цикл нового приложения. |
| [Общие профили рецептов](guides/project-profiles.md) | Повторное использование команд несколькими проектами. |
| [Workflow и приложения](guides/workflows.md) | Импорт, сборка, запуск, готовность, логи и остановка. |
| [Delta-доставка](guides/snapshot-delta.md) | Передача изменений относительно проверенного snapshot. |
| [Возобновление доставки](guides/snapshot-resume.md) | Проверенные блоки и новый явный импорт после обрыва. |
| [MCP](guides/mcp.md) | Инструменты агента, запуск адаптера и правила повторных запросов. |
| [Внимание](guides/attention.md) | Уведомления о задачах, workflow и приложениях. |
| [Tray и уведомления](guides/companion.md) | Фоновый интерфейс, popup, звук и тихие часы. |
| [Диагностика desktop](guides/desktop-diagnostics.md) | Кнопки закрытия, тайм-ауты и передача журналов из Windows. |
| [История и диагностика](guides/history-diagnostics.md) | Поиск операций и подготовка отчёта. |
| [Настройки и хранение вывода](guides/settings-retention.md) | Конфигурация, ручной restart и очистка stdout/stderr. |
| [Диагностика журнала](guides/journal-recovery.md) | Candidate: чтение восстановленного журнала с сохраняющимся запретом исполнения. |

## Форматы и среда — `reference/`

- [Shared Folders](reference/shared-folders.md): соответствие Debian/Windows путей и границы shared/local данных.
- [Формат snapshot](reference/snapshot-format.md): manifest, ready marker и проверка файлов.

## Повторяемые проверки — `validation/`

Это инструкции запуска сценариев, а не отчёты о конкретных прошлых прогонах.

| Инструкция | Что проверяет |
| --- | --- |
| [Windows worker](validation/windows-validation.md) | Команды, output, отмена дерева, IPC и восстановление. |
| [Сессия Debian ↔ Windows](validation/windows-session-validation.md) | Подготовка пары и связь через controller. |
| [Snapshot](validation/windows-snapshot-validation.md) | Доставка через shared, импорт и сборка. |
| [Автоматические workflow проверки](validation/windows-workflow-validation.md) | Назначение сценариев, параметры и отдельные режимы native gate. |
| [Изолированный Netwatch](validation/netwatch-validation.md) | Проверка реального приложения без изменения рабочего экземпляра. |
| [Измерения производительности](validation/performance-baseline.md) | Воспроизводимые измерения доставки, сборки и output. |

## История и решения

- [Завершённые этапы 0–7](done/stage0-7/README.md): промежуточные планы, ревью, измерения и отчёты Windows gates, распределённые по этапам.
- [Принятые решения](decisions/README.md): ADR с действующими архитектурными решениями.
- [Исторический контекст](context/README.md): исходное предложение и записи восстановления.
- `screenshots/`: изображения интерфейса, на которые ссылаются руководства и отчёты.

При расхождении со старым планом ориентируйтесь на текущие руководства,
архитектуру и актуальное состояние в handoff. Состояния и IDs прошлых запусков
в архиве относятся к описанным сессиям.
