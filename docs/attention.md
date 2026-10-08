# Журнал внимания: задачи, workflow и приложения

Статус 2026-10-08: worker journal → local IPC/WebSocket/CLI → desktop → точная
страница → явное подтверждение. [Tray/popup/звук и quiet hours](companion.md)
прошли Windows gate. Workflow/application targets также
[прошли автоматический native gate](windows-workflow-attention-gate-2026-10-08.md):
19 backend tests и actual popup → workflow page → UI ACK. История, diagnostic export и MCP остаются в [этапе 5](stage5-plan.md).

## Что видит пользователь

Блок «Требует внимания» показывает точное число неподтверждённых событий во всём
журнале и до 50 событий на странице. Есть проект, severity, причина, время, источник
и настоящая ссылка на task, workflow или application. Соответствующая кнопка открывает
фактические параметры/output/шаги/состояние приложения, даже за пределами последних
десяти workflow. «Прочитано»
записывает acknowledgement у worker. Открытие списка или задачи не подтверждает событие.
Подтверждённые события доступны отдельным переключателем.

Потерянный ответ на acknowledgement не означает отказ записи: кнопка «Проверить
подтверждение» повторяет действие для прежнего ID. Время первого acknowledgement
сохраняется. Offline показывает последние известные данные. После смены generation
список перечитывается с первой страницы; закрытый desktop не теряет журнал.

Failed task автоматически создаёт error, interrupted/indeterminate — warning.
Success и пользовательская cancellation тихие. Результат task и автоматическое
событие записываются одной транзакцией. Старые terminal tasks при upgrade не
переносятся в pending список; recovered running task получает warning.

Workflow failed получает error, interrupted/indeterminate — warning. Указаны имя
цепочки, неуспешный шаг и причина. Ошибка import/очереди имеет workflow target без
выдуманного Task ID. Ошибка owned build/start task не дублируется отдельным task event.
Standalone imports пока показывают ошибку в delivery card; автоматический сигнал
добавлен для imports внутри workflow.

После подтверждённого успешного startup приложение получает error при failure,
warning при indeterminate или самостоятельном выходе, включая exit 0: управляемый
сервис остановился. Исторический startup остаётся succeeded. Явный stop тихий.
Readiness/log failure до завершения startup даёт один workflow event. Terminal
workflow/application и событие фиксируются одной транзакцией; повтор reconcile не
меняет event ID или ACK. При ошибке записи stale результат отклоняется, новые
запуски ждут восстановления журнала и restart worker.

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
--config PATH attention-events [AFTER_SEQUENCE]
--config PATH attention-events-history [AFTER_SEQUENCE]
--config PATH ack-attention-event ATTENTION_ID
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
идентичности агента. `task_outcome`, `workflow_outcome`, `application_outcome` —
автоматические результаты worker. Explicit запрос по-прежнему привязан к task.

RPC: `request_attention`, `list_attention {afterSequence,pendingOnly}`,
`acknowledge_attention {attentionId}`. Ответ списка содержит records, nextSequence,
hasMore и pendingCount. Cursor sequence стабильный; предел страницы 50. Worker
сохраняет запрос до ответа; key общий для explicit attention в данном journal,
сравнение payload исключает только requestId. Тот же key/payload возвращает прежний
ID даже после acknowledgement. Другой payload отклоняется.

Новый RPC `list_attention_events {afterSequence,pendingOnly}` возвращает тот же
paging envelope с `AttentionEvent`; вместо taskId у него `target`:

```json
{"kind":"application","applicationId":"actual-app-id","workflowId":"actual-workflow-id"}
```

Другие варианты: `{"kind":"task","taskId":"…"}` и
`{"kind":"workflow","workflowId":"…"}`. ACK:
`acknowledge_attention_event {attentionId}`. Legacy список/ACK обслуживают только
task events, включая task-only pendingCount; generic список охватывает все targets.
Оба используют стабильный глобальный sequence и один journal. Legacy ACK для
workflow/application ID отклоняется до изменения записи.

До 1000 pending explicit запросов: новый explicit запрос отклоняется при достижении
лимита; dedup по прежнему key работает. Автоматические task outcomes продолжают
сохраняться, включая workflow/application outcomes. Retention и удаление истории ещё не включены.

## Совместимость и storage

WorkerInfo `attentionAvailable` по умолчанию false для старого worker. Новые RPC
task attention передаются controller только peer с capability `attention_request`.
Generic RPC требуют отдельной `attention_targets`; WorkerInfo
`attentionTargetsAvailable` по умолчанию false. Desktop выбирает generic feed либо
legacy task feed по этому флагу; старый runtime получает понятный отказ до передачи.
Обновляются worker/controller/desktop вместе. Hello/wire version не меняются.

SQLite schema 5 добавляет typed target и nullable task FK. Перед upgrade schema 4
выполняется consistent `VACUUM INTO journal-v4-<time>-<uuid>.sqlite` рядом с journal;
worker печатает точный путь. Сохраняются IDs, sequence high-water, ACK и explicit
dedup. Уже действующие v1/v2/v3 migrations сохраняют прежние consistent backups.
Базы и backups остаются локальными, вне HGFS. Installed journal этой разработкой
не менялся: Windows tests используют отдельные временные journals. Более новая
неизвестная schema отклоняется.

## Проверки

`npm run test:command-presentation` проверяет Unicode/emoji, newlines, host/script
option boundaries, malformed/missing/oversized payload. `cargo test --workspace
--locked` включает attention tests: настоящее исполнение failure/success/cancel,
dedup/conflict/restart acknowledgement, atomic rollback, recovery без replay,
paging/validation/quota и consistent v3/v4 backups. Workflow tests проверяют failures
до Task ID, build/readiness, runtime crash/exit, quiet stop/cancel, crash recovery и
отказ выдавать stale workflow/application при atomic rollback.

`npm run test:ui` использует настоящий Unix worker и WebSocket controller: decoded
script/raw argv, attention task link, потеря acceptance ACK и acknowledgement ACK,
explicit retry и сохранение после desktop/worker restart. Дополнительно — actual
workflow/application navigation, ACK и открытие target вне последних десяти workflow.

Только новые Windows tests, без повторения закрытого gate:

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/ARCHIVE.zip --workflow-attention-only
```

Сценарий запускается агентом через текущий worker, собирает новый тестовый source
offline и публикует checksum-verified report в shared. Он не устанавливает runtime.
[Новый gate](windows-workflow-attention-gate-2026-10-08.md) пройден: clean source
`3c59351`, 10 attention + 9 workflow tests и native desktop exit 0, 12 verified artifacts.
Архив создаётся с `--include-built-ui`. Новый сценарий включает native backend tests,
offline worker/desktop build и один заметно обозначенный тестовый desktop: настоящая
ошибка import → popup → точная workflow page → отдельное UI acknowledgement.
Рабочий worker оставить запущенным, рабочий desktop необязателен. [Прежний task-only
gate](windows-attention-gate-2026-10-07.md) повторять не нужно. Установленный runtime
после reboot — `f7c4878`, новый generic feed попадёт в следующее совместное обновление.
