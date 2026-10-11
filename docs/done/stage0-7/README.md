# Завершённые этапы 0–7

Здесь собраны 41 промежуточный документ: планы реализации, ревью, измерения и
отчёты конкретных проверок. Файлы сохраняют прежние имена и историю; ссылки
обновлены под новые пути. Это доказательства и ход разработки, а не текущая
инструкция запуска. Даты, версии, IDs и незавершённые пункты читаются в контексте
соответствующей сессии.

Актуальные документы: [общее оглавление](../../README.md), [руководство пользователя](../../user-friendly-guide-v1/README.md), [handoff](../../handoff.md), [roadmap](../../roadmap.md).

## Этап 0 — подготовка

- [validation.md](stage0/validation.md): исходный накопительный отчёт — scaffold, контракты, UI и последующие дополнения о ранних runtime этапах. Он охватывает несколько этапов, а не только этап 0.

## Этап 1 — Windows worker и одна команда

- [Windows gate](stage1/windows-gate-2026-10-05.md): реальное исполнение, output, отмена дерева, recovery и подтверждённые UI шаги.

## Этап 2 — управление из Debian

- [Ревью](stage2/review-stage2-2026-10-05.md): ошибки соединения и журнал активности controller.
- [Windows session gate](stage2/windows-session-gate-2026-10-05.md): команды из Debian, dedup, cancel и reconnect с прежней задачей/PID.

## Этап 3 — доставка исходников

- [Ревью runtime](stage3/review-stage3-2026-10-05.md): snapshot, path guards и границы импорта.
- [Ревью UI](stage3/review-snapshot-ux-2026-10-06.md): выбор пакета, сохранение формы и объяснение ошибок.
- [Ревью перед закрытием](stage3/review-stage3-close-2026-10-06.md): итоговые исправления и критерии готовности.
- [Windows snapshot gate](stage3/windows-snapshot-gate-2026-10-06.md): shared → проверенный import → Windows build и наблюдения пользователя.

## Этап 4 — workflow и приложение

- [Windows workflow gate](stage4/windows-workflow-gate-2026-10-06.md): автоматические проверки цепочки импорта, сборки, запуска, готовности и лога.
- Проверка реального Netwatch также описана в [повторяемом сценарии](../../validation/netwatch-validation.md).

## Этап 5 — интерфейс, внимание, история, MCP и настройки

| Группа | Что за файлы |
| --- | --- |
| Планы | [Общий этап 5](stage5/stage5-plan.md), [tray](stage5/tray-plan.md), [workflow/application attention](stage5/workflow-attention-plan.md), [история/диагностика](stage5/history-diagnostics-plan.md), [MCP](stage5/mcp-plan.md), [настройки/retention](stage5/settings-retention-plan.md): порядок реализации и критерии. |
| Ревью | [Журнал внимания](stage5/review-attention-2026-10-07.md), [workflow attention](stage5/review-workflow-attention-2026-10-08.md), [перед историей](stage5/review-stage5-history-2026-10-08.md), [итоговое этапа 5](stage5/review-stage5-2026-10-09.md): найденные ошибки, исправления и проверки. |
| Windows gates | [Внимание](stage5/windows-attention-gate-2026-10-07.md), [companion](stage5/windows-companion-gate-2026-10-07.md), [workflow attention](stage5/windows-workflow-attention-gate-2026-10-08.md), [история](stage5/windows-history-gate-2026-10-08.md), [MCP](stage5/windows-mcp-gate-2026-10-08.md), [настройки/retention](stage5/windows-settings-retention-gate-2026-10-09.md): результаты конкретных native прогонов. |
| Незавершённый прогон | [Companion attempt](stage5/windows-companion-attempt-2026-10-07.md): сохранённая неудачная попытка, объяснение и разрешённый повтор; это не успешный gate. |

Дополнение после roadmap: [закрытие desktop и диагностический лог](stage5/windows-desktop-diagnostics-gate-2026-10-10.md) — исправления project undo, save timeout и popup hide, результаты автоматических Windows прогонов.

## Этап 6 — производительность и доставка

| Группа | Что за файлы |
| --- | --- |
| Общий план и ревью | [План этапа](stage6/stage6-plan.md), [ревью](stage6/review-stage6-2026-10-10.md): выбор измеримых улучшений и проверка рисков. |
| Измерения | [Baseline](stage6/performance-baseline-2026-10-09.md), [Git batch capture](stage6/performance-capture-gate-2026-10-09.md): исходные результаты и сопоставимое ускорение capture. |
| Delta | [План](stage6/snapshot-delta-plan.md), [Windows gate](stage6/windows-delta-gate-2026-10-09.md): reuse проверенной базы, повреждения и фактическая экономия передачи. |
| Output | [План](stage6/output-batching-plan.md), [Windows gate](stage6/windows-output-gate-2026-10-10.md): пакетное чтение, ограниченный UI tail, hashes и управление задачами. |
| Resume | [План](stage6/snapshot-resume-plan.md), [Windows gate](stage6/windows-resume-gate-2026-10-10.md): проверенные блоки, cancel/crash и новый явный import после обрыва. |

## Этап 7 — общие рецепты и следующий проект

- [Ревью перед этапом](stage7/review-stage7-2026-10-10.md): закрепление definitions рецептов до приёма workflow.
- [Windows profiles gate](stage7/windows-profiles-gate-2026-10-10.md): общие рецепты двух проектов, argv, dedup, cancel и recovery.
- [Ревью прикладного сценария](stage7/review-projects-2026-10-10.md): следующий потребитель обработчиков и явные validation adapters.
- [Windows projects gate](stage7/windows-projects-gate-2026-10-10.md): изолированные Netwatch и WinBridge, общий профиль, private IPC, логи, artifacts и остановка.
- [Последующее ревью source identity](stage7/review-source-identity-2026-10-10.md): проверка происхождения сборки после закрытия roadmap; исправление false-clean и узкий автоматический Windows gate.

Сохранённые неудачные попытки внутри отчётов остаются частью истории. Их перенос
не меняет результат проверки. Runtime reports, ZIP, базы и output находятся по
указанным в документах путям; эта папка содержит только документацию.
