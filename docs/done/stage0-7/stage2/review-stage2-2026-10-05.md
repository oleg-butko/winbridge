# Ревью этапа 2 — 2026-10-05

Проверены controller listener/IPC, обе стороны handshake, RPC correlation/deadlines, reconnect и сохранение ответственности worker за задачи; также controller config, native UI bridge и сценарии проверки. Изменения относятся к локальной сессии этапа 2. Native Windows gate этого этапа остаётся открытым.

## Найдено и исправлено

| Приоритет | Проблема до исправления | Исправление и проверка |
| --- | --- | --- |
| P2 | Прерывание future обработчика после регистрации сессии обходило обычный cleanup. Controller мог сохранить connected/active и отклонять reconnect. | ConnectionGuard очищает только свою session при return/abort/panic. Реальный TCP тест прерывает actor с pending RPC, получает unknown outcome без retry, подключает следующий worker. |
| P2 | Запрос, помещающийся в IPC, мог превышать 1 MiB после добавления WebSocket envelope. Ошибка encode завершала всю рабочую сессию и оставляла посторонние RPC без ответа. | Проверка полного envelope до pending registration и записи. Тест подтверждает отказ до передачи, прежнюю сессию и доставку следующего Info. |
| P2 | Текст serde ошибки неизвестного поля/варианта мог включать входящий payload почти до 1 MiB и попадать в status/UI/stderr. | Ошибка содержит только категорию, line и column. Настоящий WebSocket peer с 60 KiB неизвестным type закрывается; lastError короче 200 символов и не содержит payload. |
| P2 | Worker принимал Welcome без appVersion или обязательных task_submit/task_cancel capabilities, хотя UI уже показывал connected. | Обе стороны проверяют handshake metadata/capabilities. Реальный TCP тест отправляет два некорректных Welcome, подтверждает disconnected без session ID и сохранение локального command handler. |
| P3 | Пятисекундный controller handshake deadline покрывал HTTP/Hello, но Welcome имел отдельный двухсекундный budget. | Один абсолютный deadline используется до завершения Welcome; cleanup защищён тем же guard. Общие session checks и Windows compile проходят. |

Найденные пункты закрыты кодом; незакрытых findings в этом ревью нет. Отмена IP-фильтра — отдельное явное решение пользователя, а не дефект handshake. Проверка instance/node/version/capabilities остаётся.

## Журнал активности

`stateDirectory/activity.jsonl`, текущий файл до 1 MiB и пять архивов `.1`–`.5`. Записи содержат Unix timestamp ms, исходный IP/port, connect/disconnect/refusal, принятые session/node/generation и метод/исход RPC. Origin локальных вызовов — local_ipc; workerAddress обозначает получателя. Сырые команды/argv/output и входящие payload не пишутся.

Отдельный writer использует ограниченную очередь: 256 записей по максимуму 8 KiB. Переполнение/ошибка не останавливает execution authority; status.activityLog показывает droppedRecords/lastError, stderr сообщает о проблеме. Нормальный shutdown дожидается записи очереди не более двух секунд. Журнал диагностический; он не заменяет SQLite accepted task до ACK.

Ротация удерживает exclusive writer lock, проверяет private regular files и сохраняет неожиданные ссылки/чужие файлы. Полная строка сохраняется при restart; незавершённый crash fragment удаляется с учётом droppedRecords. При ошибке частичной записи выполняется откат к последней полной строке. Rust filesystem tests проверяют ротацию/retention/reopen, symlink preservation, запись на shutdown, видимую ошибку и восстановление, незавершённую строку.

Старое поле allowedWorkerAddresses загружается и игнорируется, поэтому существующие local configs менять не требуется. Новые examples не содержат этого поля. Будущий allowlist потребует отдельного явного включения.

## Доказательства и пределы

- 27 Rust tests, workspace Clippy с `-D warnings`, fmt и Windows MSVC all-targets compile.
- 14 групп `validate-session.py` с настоящими controller/worker/fixture процессами: другой исходный IP принят и записан, version/pair/capabilities отказаны, dedup/lost ACK/tree cancel/reconnect/watch/crash/timeout/correlation/frame/RPC/heartbeat проверены.
- Итоговый session report: `.winbridge/session/20261005-125239-6e4527f3/validation-report.json`, platform linux. Девять групп `validate-local.py` также проходят; отчёты `.winbridge/validation/20261005-125300-50fdabaa`.
- TypeScript strict, generated contracts, production UI build, browser integration с настоящим worker/controller и standalone Linux Tauri check.
- Результаты этих прогонов относятся к Debian development adapter. Реальное Debian→Windows исполнение и native UI с новой сессией проверяются по [инструкции этапа 2](../../../validation/windows-session-validation.md); этап не закрывается compile check.
