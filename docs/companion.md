# Tray и фоновые уведомления

Сценарий этапа 5, начатый 2026-10-07 после [ревью](review-attention-2026-10-07.md)
и commit плана. Реализация готова к автоматическому native gate; завершение проверки
фиксируется отдельно, compilation/browser shim не доказывают Windows popup или звук.

## Ежедневное использование

Крестик окна дожидается save ACK всех изменённых форм и настроек, затем скрывает окно.
Нажатие на tray или повторный запуск того же exe возвращает существующее окно.
Для завершения desktop выберите **«Завершить интерфейс»** в окне или tray.
При ошибке сохранения окно остаётся доступным и объясняет отказ; пользователь может
явно закрыть без сохранения. При отсутствии tray скрытие запрещено.

Перед обновлением exe нужно именно завершить интерфейс: крестик теперь его скрывает.
Worker продолжает работу независимо от скрытия/завершения desktop. При закрытом
companion Windows popup/звук не воспроизводятся, но worker сохраняет pending attention.
Следующий запуск companion прочитает ещё не представленные события.

Rust poll работает независимо от WebView. Tooltip tray показывает свежесть worker,
общее непрочитанное внимание и queued/running command count по всему journal.
Оранжевый badge обозначает pending attention, серый — отсутствие worker.
Недоступные счётчики обозначаются `?`, в окне — «неизвестно».
Для старого worker без attention capability требуется совместное обновление;
неизвестные attention RPC ему не отправляются.

Popup содержит проект, важность, причину и общее число непрочитанных событий.
Он появляется у края текущего экрана без активации окна. «Открыть задачу» показывает
main window с точной task page; «Скрыть» закрывает только popup. Ни одно действие
не подтверждает attention — это отдельно делает «Прочитано» в journal UI.

## Настройки и доставка сигнала

`winbridge-desktop-state.local.json` рядом с exe сохраняет формы,
`notificationPreferences` и scoped `notificationCursors`. Legacy schemaVersion 1
без этих полей получает defaults. Запись сериализована общим mutex и атомарной заменой;
повреждённый или неизвестный state не перезаписывается.

По умолчанию popup и встроенный короткий Windows звук включены для warning/error,
громкость 25%, интервал 30 секунд. Ползунок системной громкости не меняется:
масштабируются PCM samples встроенного WAV, Windows sound API вызывается в фоне.
Quiet hours выключены; часы задаются местным временем Windows, допускают переход
через полночь. Начало входит в интервал, конец не входит; одинаковые времена отвергаются.

Desktop читает до 200 pending records за poll и объединяет подходящие события в один
сигнал с последним событием этой порции. Все records остаются у worker. Rate/quiet
откладывают cursor: перед следующим сигналом pending перечитываются, поэтому
acknowledgement во время ожидания подавляет отложенный сигнал. Severity threshold
применяется к новым порциям; изменение порога не воспроизводит ранее пропущенные
по важности события. Отключённые popup и звук удерживают подходящие pending до включения.
Restart не повторяет уже представленные cursor events. Интервал измеряется monotonic
clock текущего запуска; restart сбрасывает elapsed timer для ещё не показанной порции.

Cursor сохраняется после presentation. При ошибке popup/sound или cursor save UI
показывает причину; событие может повториться после интервала. Cursor не является
ACK пользователя. Это best-effort presentation: crash между показом и cursor write
может повторить сигнал, а acknowledgement остаётся долговечным и idempotent у worker.
Cursor scope — instanceId/nodeId; смена journal с теми же идентификаторами требует
отдельного runtime/state, автоматический reset базы этим сценарием не предусмотрен.

## Автоматическая Windows проверка

Агент собирает UI в Debian и экспортирует `scripts/export-source.py --include-built-ui`.
Manifest фиксирует также SHA/bytes встроенного frontend. Затем
`validate-workflow-native.py --companion-only` через установленный worker запускает
`test-windows-workflow.ps1 -CompanionOnly` в новой локальной папке проекта.
Offline Cargo проверяет core/attention и собирает отдельные worker/desktop.
Рабочие exe/config/state не заменяются; тестовый instance не соединяется по WebSocket.

`test-companion-native.ps1` создаёт отдельный runtime и дважды запускает desktop.
Явный validation mode разрешён только для instance `tray-gate-*`, node `tray-validation`
и собственного project root. Он использует реальные Tauri окна и фиксированные
React actions, проверяет close/hide, single instance/reopen, фоновый popup без focus,
успех Windows sound API, task page, отсутствие implicit ACK, rate aggregation,
ACK отложенного события, restart cursor и сохранённые настройки.
Наличие слышимого звука человеком не заявляется как автоматическая проверка.
Тестовый quit выполняется через UI; PowerShell отдельно проверяет exit и живой worker.
Нет debugger port, внешнего control endpoint или изменения глобальных настроек.
При failure удерживаются runtime/logs/reports; только собственные тестовые PIDs очищаются.
Отчёт в shared завершается `report.ready.json` с SHA-256 всех файлов.

Workflow/app attention, расширенный history/audit/export и MCP остаются следующими
сценариями этапа 5; tray сам по себе не закрывает весь этап.
