# Журнал внимания: первый сценарий этапа 5

Реализован 2026-10-07: worker journal → local IPC/WebSocket/CLI → desktop список →
открытие задачи → явное подтверждение. Native tray/popup/звук и quiet hours пока не
реализованы; весь [этап 5](stage5-plan.md) остаётся незавершённым.

## Что видит пользователь

Блок «Требует внимания» показывает точное число неподтверждённых событий во всём
журнале и до 50 событий на странице. Есть проект, severity, причина, время, источник
и Task ID. «Открыть задачу» переводит к фактическим параметрам/output; «Прочитано»
записывает acknowledgement у worker. Открытие списка или задачи не подтверждает событие.
Подтверждённые события доступны отдельным переключателем.

Потерянный ответ на acknowledgement не означает отказ записи: кнопка «Проверить
подтверждение» повторяет действие для прежнего ID. Время первого acknowledgement
сохраняется. Offline показывает последние известные данные. После смены generation
список перечитывается с первой страницы; закрытый desktop не теряет журнал.

Failed task автоматически создаёт error, interrupted/indeterminate — warning.
Success и пользовательская cancellation тихие. Результат task и автоматическое
событие записываются одной транзакцией. Старые terminal tasks при upgrade не
переносятся в pending список; recovered running task получает warning. Отдельные
ошибки import/workflow/readiness/application и объединение их сигналов — следующий
сценарий, пока они видны в своих существующих карточках.

PowerShell `EncodedCommand` теперь показывается декодированным UTF-16LE сценарием:
Unicode, кавычки и переносы строк сохранены. Видны executable, параметры оболочки
и cwd; исходный массив с Base64 раскрывается отдельно. Копирование сохраняет точные
исходные параметры. Некорректный Base64/UTF-16LE и превышение 256 KiB Base64 дают
видимую ошибку декодирования. Аргументы сценария после `-File`/`-Command` не
декодируются как параметры оболочки. Просмотр никогда не исполняет команду.

## Запросы агента

Worker и controller CLI используют один обработчик:

```text
--config PATH attention REQUEST.json
--config PATH attention-list [AFTER_SEQUENCE]
--config PATH attention-history [AFTER_SEQUENCE]
--config PATH ack-attention ATTENTION_ID
```

Пример REQUEST.json:

```json
{
  "requestId": "correlation-1",
  "idempotencyKey": "review-result-1",
  "taskId": "ID уже принятой задачи",
  "severity": "warning",
  "message": "Проверьте результат перед продолжением"
}
```

Task ID обязателен: проект выводится из настоящей task, произвольные ссылки не
принимаются. Severity: info/warning/error. Message до 1024 UTF-8 bytes, IDs/key до
200 bytes, без control characters. Не отправляйте secrets в текст уведомления.
Source `agent_request` обозначает путь запроса, без заявления криптографической
идентичности агента. Source `task_outcome` — результат worker.

RPC: `request_attention`, `list_attention {afterSequence,pendingOnly}`,
`acknowledge_attention {attentionId}`. Ответ списка содержит records, nextSequence,
hasMore и pendingCount. Cursor sequence стабильный; предел страницы 50. Worker
сохраняет запрос до ответа; key общий для explicit attention в данном journal,
сравнение payload исключает только requestId. Тот же key/payload возвращает прежний
ID даже после acknowledgement. Другой payload отклоняется.

До 1000 pending explicit запросов: новый explicit запрос отклоняется при достижении
лимита; dedup по прежнему key работает. Автоматические task outcomes продолжают
сохраняться. Retention и удаление истории ещё не включены.

## Совместимость и storage

WorkerInfo `attentionAvailable` по умолчанию false для старого worker. Новые RPC
передаются controller только peer с capability `attention_request`; старый runtime
получает понятный отказ до передачи. Desktop не отправляет ему неизвестные методы.
Обновляются worker/controller/desktop вместе. Hello/wire version не меняются.

SQLite schema 4 добавляет таблицу attention. Перед upgrade существующей schema 3
выполняется consistent `VACUUM INTO journal-v3-<time>-<uuid>.sqlite` рядом с journal;
worker печатает точный путь. Уже действующие v1/v2 migrations также сохраняют
предыдущий consistent backup. Базы и backups остаются локальными, вне HGFS.
Installed journal этой разработкой не менялся: Windows tests используют отдельные
временные journals. Более новая неизвестная schema отклоняется.

## Проверки

`npm run test:command-presentation` проверяет Unicode/emoji, newlines, host/script
option boundaries, malformed/missing/oversized payload. `cargo test --workspace
--locked` включает семь attention tests: настоящее исполнение failure/success/cancel,
dedup/conflict/restart acknowledgement, atomic rollback, recovery без replay,
paging/validation/quota и consistent v3 backup.

`npm run test:ui` использует настоящий Unix worker и WebSocket controller: decoded
script/raw argv, attention task link, потеря acceptance ACK и acknowledgement ACK,
explicit retry и сохранение после desktop/worker restart.

Только новые Windows tests, без повторения закрытого gate:

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/ARCHIVE.zip --attention-only
```

Сценарий запускается агентом через текущий worker, собирает новый тестовый source
offline и публикует checksum-verified report в shared. Он не устанавливает runtime.
[Настоящий Windows прогон](windows-attention-gate-2026-10-07.md) выполнен: семь новых
native tests прошли, отчёт прочитан из shared и checksums сверены. Установленный
worker/desktop остаётся `523fc5b`, новые функции попадут в общее обновление этапа 5.
