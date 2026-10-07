# Windows companion gate — 2026-10-07

После [первого прерванного прогона](windows-companion-attempt-2026-10-07.md)
пользователь разрешил диагностику, исправление и автоматический повтор. Новый gate
**пройден на настоящей Windows**, без ручных UI шагов и обновления рабочего runtime.

- Clean source commit: `f7c4878fa5800bec5d8f4ae2781ee9b5108f0b36`.
- ZIP: `winbridge-20261007T103234Z-f7c4878-0ac18efc.zip`, SHA-256
  `b52d35a1b78fc7bc9db2357ac1f9ed3336ad8e70f671e1fdc5175aaea73e7e44`.
  В manifest включён собранный frontend; sourceDirty=false.
- Outer task: `f4d7db75-db65-4e15-8a75-7d05bd2e21d1`, succeeded, exit 0.
- Shared report: `WinBridge/workflow-validation/20261007-063240-8e63081e`.
  Windows: `K:\VMs\shared\WinBridge\workflow-validation\20261007-063240-8e63081e`.
- `report.json` SHA-256:
  `5b2fa9985438efcab6006ce73c13a8f2bb96a6e2b2cccf988060e54b2af93c86`.
  Все **16** файлов сверены с bytes/SHA из `report.ready.json`.
- Время: 10:32:44–10:36:14 UTC. 21 core и 9 attention native tests passed;
  отдельные worker и Tauri desktop с custom-protocol UI собраны offline, exit 0.

## Фактические native наблюдения

Initial desktop PID 13288 и restart PID 8572 завершились через реальный UI save/quit
с exit 0. Оба phase JSON содержат passed=true, platform=windows; checkpoints
сохраняют args, PID, время и шаги. Isolated instance:
`tray-gate-d0481620d0c54fe591bbb9249d3a84a0`.

Проверены настоящий tray и скрытие окна после сохранения форм; случайный крестик
во время теста оставляет окно открытым с понятным объяснением. Повторный запуск
того же exe завершает secondary process и возвращает прежнее окно. При скрытом
main window background poll показывает настоящий popup без захвата foreground focus
и успешно вызывает встроенный Windows sound API. Слышимость человеком не проверялась.

«Открыть задачу» из popup переводит от исходной task page
`42e14f1b-a9cd-4d50-a548-23821623b03e` к подтверждённой worker странице
`3db6274e-673b-40e1-85f0-421afde761e0`; переход не подтверждает attention.
Отложенное событие после ACK не сигнализирует; burst объединён в последний сигнал.
Restart сохраняет настройки и pending count 3, не повторяя уже показанные события.
После обоих UI quit отдельный тестовый worker остаётся доступен и имеет прежний instance.

## Очистка и границы доказательства

Harness завершил только собственный тестовый worker PID 19180 (exit -1 от явного
cleanup), дождался обоих потоков и сохранил immutable local logs перед копированием
в shared. Cleanup errors отсутствуют. Read-only process query task
`9c935c4e-81e9-408d-bab5-cf29b1256b4c`, exit 0, дополнительно подтвердил отсутствие
тестовых worker/desktop; остался только рабочий worker PID 12272 в постоянном runtime.
Его read-only info: source `523fc5b`, generation
`bff7f8da-af71-432b-83af-739a020d9e37`, соединение доступно. Рабочие config/state/exe
не заменялись. Прежние shared reports не переписывались.

Локально прошли 20 core tests, standalone desktop Clippy с -D warnings,
contracts/typecheck/UI build, PowerShell parser и smoke реального Process capture
(131072 stderr bytes параллельно с Unicode stdout и точным exit 7). Browser integration
с настоящим Unix worker прошла, включая живой validation banner и явный abort action.
Native guard close проверен отдельно выше; пользовательский abort на Windows специально
не нажимался, чтобы не прерывать успешный сценарий.

Правило общения закреплено в AGENTS.md: перед Windows прогоном объяснять, какие
worker/desktop нужны и какие тестовые окна появятся; после явно разрешать закрытие
окон и сообщать, какой worker оставить запущенным. Тестовые окна заметно обозначены,
показывают шаги и имеют явную кнопку прерывания. Рабочий desktop необязателен для gate.

Tray/popup/settings сценарий этапа 5 завершён. Workflow/app attention,
history/audit/diagnostic export и MCP остаются следующими задачами; весь этап 5 не закрыт.
