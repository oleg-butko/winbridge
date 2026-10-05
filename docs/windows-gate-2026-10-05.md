# Windows gate этапа 1 — пройден 2026-10-05

Основание: пользовательский Windows прогон `20261005-161827-c8deef84` и подтверждение всех пяти native UI шагов. Агент прочитал отчёты из shared и проверил их структуру, task IDs, последовательность chunks, stream offsets и outputBytes. Это результаты реального host запуска, а не cross-target check.

Отчёт: `K:\VMs\shared\WinBridge\validation\20261005-161827-c8deef84\validation-report.json`; в Debian — `/mnt/hgfs/shared/WinBridge/validation/20261005-161827-c8deef84/validation-report.json`.

- `occurredAt`: `2026-10-05T13:19:23.4709066Z`, `platform: windows`.
- `ipcAclVerified: true`; все 10 checks имеют `passed: true`.
- SHA-256 итогового JSON: `ba325aa5fa502d9c1d7968683681d08992de87312cc9ef8e9422de8d08429268`.
- `uiVerified: false` в автоматическом JSON сохранён: он не оценивает ручные шаги. Пользовательские подтверждения зафиксированы ниже; отчёт в shared не переписывался.

## Runtime checks

| Проверка | Результат |
| --- | --- |
| local-ipc | Native Windows worker подключён через named pipe. |
| user-sid-pipe-dacl-exclusive-worker | User-only protected DACL; второй worker отвергнут journal lock. |
| success-output-dedup-payload-conflict | Exit code/output сохранены, повтор не запускает второй процесс, конфликт payload отвергнут. |
| literal-windows-argv-unicode | Пробелы, shell metacharacters, кавычка, завершающий backslash и Unicode сохранены. |
| failure-and-recipe | Exit code 7 и recipe проверены. |
| both-pipes-flood-without-ui | Оба stream читаются без UI, квота записана без зависания. |
| timeout-tree | Timeout завершает управляемое дерево. |
| project-queue-and-tree-cancel | Одна команда на project; queued cancellation и дерево завершены. |
| traversal-reserved-unc-junction-wire-version | Неверные пути, junction escape и версия отвергнуты. |
| worker-crash-job-cleanup-recovery-no-retry | Windows Job закрывает дерево; recovery сохраняет indeterminate и не перезапускает команду. |

Прочитаны пять полных TaskReport:

| Файл | Task ID | Результат |
| --- | --- | --- |
| success.task-report.json | b5ed9884-4858-42c7-8fa5-40dcdec5c7aa | succeeded, exit 0, оба stream и «Привет ✓». |
| arguments.task-report.json | 0288c425-1dc2-46c9-b33e-0c142cdb48ad | succeeded, exit 0, буквальные Windows argv. |
| failure.task-report.json | 50dad919-1e27-42e5-9318-518cc7a39f20 | failed, exit 7, deliberate failure в stderr. |
| cancelled.task-report.json | 58dd92d8-f14e-4e06-9678-3dda72953060 | cancelled, управляемое дерево остановлено. |
| indeterminate.task-report.json | f61dc342-6f1e-40be-ab60-a428e8521639 | indeterminate после crash; automatic retry disabled. |

## Native UI — подтверждение пользователя

1. UI показывает подключённый Windows worker и реальные задачи. `f61dc342-6f1e-40be-ab60-a428e8521639` отображается как «Результат неизвестен», что соответствует report.
2. `fixture-success` показывает exit 0, Unicode stdout и diagnostic stderr.
3. Direct `winbridge-fixture.exe` с `["tree"]`, cwd `.`, timeout 60 секунд отменяется кнопкой.
4. `["long"]` продолжает выполняться после закрытия/открытия desktop; cancel работает. Timeout показывает `Command timeout; managed process tree terminated`.
5. После подробной инструкции stop/restart worker при открытом UI пользователь подтвердил: «Уже проверил: всё совпало». Offline/freshness, прежний Task ID, сохранённый результат и отсутствие нового запуска проверены.

После обновления пользователь подтвердил: «все проверил включая dev:ui, все работает». Production native UI, сохранение формы и dev:ui проверены на Windows.

## Итог и последующий fix

Этап 1 завершён. Worker объявляет `task_submit` и `task_cancel`; На момент закрытия этапа 1 controller/WebSocket, snapshot import, log tail и attention ещё не были реализованы. Текущая готовность следующих этапов — в [roadmap](roadmap.md).

В пункте 4 пользователь обнаружил неудобство: executable/argv не восстанавливались при открытии desktop. После gate добавлено локальное сохранение формы, включая cwd/timeout/recipe, в `winbridge-desktop-state.local.json` рядом с exe. Новый fix проверен Rust filesystem tests, TypeScript, production UI build и browser integration с новым WebView и файловым IPC shim. Пользователь дополнительно подтвердил Windows-native проверку этого fix и dev:ui. Короткие шаги для повторного прогона находятся в [Windows инструкции](windows-validation.md). Незавершённых проверок этапа 1 не осталось.
