# Windows gate этапа 2 — пройден 2026-10-05

Автоматическая проверка настоящего Windows worker пройдена; пользователь подтвердил подключение и все три новых native UI наблюдения. Этап 2 завершён. Этап 1 повторно не проверялся.

Подготовка пользователя: `K:\VMs\shared\WinBridge\session-validation\20261005-235244-97785d52\setup-report.json`. Runtime — постоянный `K:\_proj-26\winbridge\runtime`; worker config, fixture project и SQLite находятся на локальном Windows диске. Пользователь подтвердил «Controller Debian подключён», URL `ws://192.168.44.203:1143`, node `debian-vm`, session `42cdb08d-41bb-40c7-a34a-99b2bd3a871b`.

Агент проверил `controller info`: platform windows, node windows-host, instance local-development, generation `584724a7-d7c4-4fc9-9c71-8688dd5ee53e`, registry только fixture. Прогон выполнялся через настоящий WebSocket и Windows процессы, без `--allow-unix-fixture`.

```bash
python3 scripts/validate-remote.py --config .winbridge/controller-session.local.json --managed-controller --reconnect-pause-seconds 15
```

Отчёты: `K:\VMs\shared\WinBridge\session-validation\debian-20261005-170829-53b92c6d`; в Debian — `/mnt/hgfs/shared/WinBridge/session-validation/debian-20261005-170829-53b92c6d`. Итоговый JSON: `platform: windows`, `passed: true`, `reconnectVerified: true`, шесть checks, без error/cleanupErrors. SHA-256 `validation-report.json`: `da5af9bfc1f3347b5be59161a2bf6951fdb4d259d10308a14efe016851765c7b`. Shared reports прочитаны без изменения; проверены последовательности chunks, offsets обоих streams и точное outputBytes.

## Runtime checks

| Проверка | Результат |
| --- | --- |
| Platform/pair/registry | Настоящий Windows worker в согласованной паре; только изолированный fixture project. |
| Success/lost ACK/dedup/Unicode | Потеря ответа клиенту controller, повтор того же key до/после завершения возвращает прежний Task ID; exit 0, Unicode stdout, diagnostic stderr. |
| Literal argv | Пробелы, shell metacharacters, кавычка, завершающий backslash и Unicode переданы отдельными аргументами. |
| Failure/recipe | Direct failure возвращает exit 7; fixture-success recipe завершается успешно. |
| Timeout/tree cancel | Timeout и отмена управляемого Windows дерева через Debian controller завершают задачу с реальным состоянием. |
| Reconnect/watch | Остановка только принадлежащего harness controller на 15 секунд сохраняет running task/PID/generation worker; после restart watch дочитывает события/output без повторов. |

## TaskReports

| Файл | Task ID | Результат |
| --- | --- | --- |
| success.task-report.json | 1f749520-2e9b-472b-a556-5359c83586b7 | succeeded, exit 0, 83 bytes |
| arguments.task-report.json | 78c9788e-0867-419d-9f9d-a9c3f0605729 | succeeded, exit 0, 113 bytes |
| failure.task-report.json | 27e7b460-c2a8-4401-a0f3-0059b5edcf68 | failed, exit 7, 58 bytes |
| recipe.task-report.json | 18720b0b-5aba-4ff4-afa8-5b217480da64 | succeeded, exit 0, 82 bytes |
| timeout.task-report.json | 245c0045-a58f-4532-9390-eb3e2a832bd4 | failed, timeout, 128 bytes |
| cancelled.task-report.json | 1413b076-08f4-4274-89d1-2add8628da7f | cancelled, 43 bytes |
| reconnect.task-report.json | a5f391a8-dee8-4eeb-8151-efd2e383ccec | running после reconnect, затем cancelled, 4328 bytes |

## Доказательство reconnect

- Task ID: `a5f391a8-dee8-4eeb-8151-efd2e383ccec`; Windows PID до/после offline: `17620`.
- Worker generation до/после: `584724a7-d7c4-4fc9-9c71-8688dd5ee53e`.
- Controller generation: `fe19af54-edb0-4259-ba7d-ecc82781cfff` → `2898b77b-a6e8-4544-995e-adeadb799b00`.
- Session: `3e069125-ac7c-4416-a829-c9c2e70b47f6` → `e213e559-7923-4d95-ac99-939e729a3dc6`.
- После завершения harness постоянный controller вновь запущен; worker подключился с прежней generation. Команда повторно не отправлялась.

## Native UI

Подключение controller подтверждено пользователем. После автоматического прогона пользователь ответил «Все три наблюдения подтвердились»:

1. Задачи из Debian появляются в desktop с command/output/exit.
2. Offline controller отображается отдельно от продолжающейся long task.
3. После reconnect появляется новый session, сохраняется прежний Task ID; затем видна отмена.

Firewall, VMware, mounts и services агент не менял. Windows worker не останавливался; выполнялись только задачи fixture текущего прогона. Следующий этап — verified snapshot import через shared folder, его готовность пока не повышается.
