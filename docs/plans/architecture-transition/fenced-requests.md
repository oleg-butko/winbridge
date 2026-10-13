# A2.4.2b.1 — запрос с закреплённой incarnation

Дата: 2026-10-13. Решение пользователя: новая пара/журнал, прежняя история как
архив. Этот пакет реализует request boundary; archive preparation, physical
cutover и безопасный выход из inspection ещё не реализованы. Full A2 открыт.

## Контракт и совместимость

В `crates/protocol` добавлены отдельные contract-2 DTO: RequestContext,
FencedRequest/Response, typed FencedReplyOutcome и совместимые wire wrappers.
Контекст — `{instanceId,journalIncarnation}`. Caller закрепляет его при создании
логического запроса вместе с его key; reconnect, config reload и новый Info
не обновляют контекст уже ожидающей операции. Server проверяет его до handler.
Запрос другой области не создаёт запись/attempt и не запускает процесс.

Legacy LocalRequest/Response, SessionMessage enums и fingerprint payload не
изменены. Untagged wrappers сохраняют v1 bytes; contract-2 frame не принимается
v1 parser. Hello/Welcome и транспортная protocolVersion остаются 1; версия
request contract отдельно равна 2. Info.fencedRequests рекламирует поддержку,
текущий context и разрешение legacy mutations. История исполнения остаётся у
worker; Scope не равен connection generation или durable owner epoch.

Schema 12 добавляет singleton journal_admission, связанный с incarnation.
Обычная миграция сохраняет legacy admission, raw history, authority и keys;
schema-11 consistent backup создаётся до миграции. Требование нового контракта
можно установить только в пустой БД с epoch 0, до первого запуска authority.
Его нельзя удалить, понизить или перепривязать. Это закрывает race между
проверкой старого запроса и изменением политики работающего worker. Общие guards
и RPC проверяют целостность этой границы; повреждённый header не разрешает effects.

Нормальный legacy runtime принимает прежние команды. В fenced journal v1 reads
остаются, v1 mutations запрещены. Это **не** подтверждение physical cutover:
низкоуровневая подготовка флага не является пользовательской процедурой recovery.
До следующего пакета нет CLI активации восстановленной пары или удаления marker.
Read-only inspection остаётся read-only и для правильного contract-2 context.

## CLI, MCP и desktop

`--config PATH invoke-v2 ENVELOPE.json` доступен в worker/controller.
CLI передаёт ровно сохранённый envelope; не получает новый context вместо caller.
Controller сначала запрашивает Info в захваченной сессии и проверяет поддержку/
scope, затем передаёт исходный context. Старый peer не получает mutation/fallback.
Network/local IPC сохраняют тип ответа и проверяют correlation, scope и contract.
Settings/cleanup остаются Windows local IPC; v2 не открывает remote maintenance.

MCP добавляет `winbridge_invoke_fenced` с generated DTO schema и обязательным
context. Nested catalog не рекламирует local maintenance/controller_status;
обычные инструменты не превращают старый request в новый автоматически.
Успех RPC означает получение ответа, а Task queued/running ещё не завершён.
Reply outcome различает responded, rejected_before_dispatch и unconfirmed.
Ошибка после возможного handler/effect, timeout или oversized reply не объявляет
операцию неисполненной; сохраняются исходные context/key для reconciliation.

Desktop создаёт BoundWorkerRequest при явном действии и хранит его с pending
task/import/workflow/settings/cleanup. Copy защищает его от изменения формы/Info.
Повтор отправляет сохранённое значение. Неподтверждённый или foreign-scope reply
не освобождает pending форму для неявной отправки нового key. Восстановление
draft по-прежнему не выполняет команду. Новый Tauri handler защищён main_only.

## Ревью и проверки

Дополнительное ревью исправило Worker::stop: embedded serve теперь завершается,
network task прерывается и join выполняется до возврата. RAII abort защищает
также преждевременный выход. Закрытие desktop не вызывает Worker::stop.
Сквозной тест использует настоящий controller, WebSocket, worker, IPC и MCP,
останавливает/перезапускает worker и подтверждает прежний scope/Task ID и один run.

Проверки: отказ stale scope/версии и всех 14 non-observation методов; v1 refusal,
actual CLI/IPC, отсутствие повторного процесса; immutable policy и запрет live
изменения; schema-11 backup/migration; matching context в inspection; повреждённый
header; byte-identical v1 wrappers и невозможность old-parser fallback;
UI bound request после config/Info/form change и неопределённого ответа.
Legacy migration fixtures удаляют новую таблицу при реконструкции старой схемы;
foreign-incarnation fixture заменяет весь header, чтобы продолжать проверять
stale observation/attempt при равных revisions, а не только malformed metadata.

Локальные gates прошли: **271 workspace tests**, Linux/Windows-target Clippy,
UI typecheck/build/contracts, отдельный bound-request test и browser gate.
Browser использует real Unix worker/controller;
fault injection сохраняет v2 frame и прежний ACK-loss сценарий. Windows group
добавляет protocol/worker request_context и native desktop compile без UI-окон.
Installed schema-6 worker/config/journal не изменяются.
Windows execution gate ещё впереди; cross-check не заменяет выполнение.

## Следующий пакет

Подготовка архивного журнала и новой пары с typed recovery states; source marker
сохраняется. Activation требует доказательств всей прежней execution domain,
включая пропавшие из backup операции, а не только текущего inventory. Новая
incarnation/пустая БД/новое имя pipe не считаются этим доказательством. При его
отсутствии preparation остаётся blocked. Затем Windows gate и общий update,
сохраняющий фиксированные exe paths; только после этого A3.
