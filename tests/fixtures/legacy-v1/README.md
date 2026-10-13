# Frozen legacy v1 baseline

Создано 2026-10-13 для A0 [архитектурного перехода](../../../docs/plans/architecture-transition/README.md).
Provenance: DTO, fingerprint и SQLite schema из production source `a42cd7e`
(тот же runtime code в planning commit `d524e68`). Это **синтетические** данные,
не экспорт пользовательского журнала и не запись реальной Windows сессии.
Схема 1 воспроизводит историческую форму таблиц; отсутствующие новые поля
проверяют backward reader. Версия wire 1 не означает версию SQLite schema 1.

| Fixture | Что закрепляет |
| --- | --- |
| `session-hello.json` | Прежний bootstrap с закрытым capability enum. |
| `session-submit.json` | Session envelope, два correlation IDs, прямой argv и Windows пути. |
| `task-fingerprint.txt` | Точные UTF-8 bytes компактного dedup payload: `requestId` пуст, `snapshotId: null` отсутствует. Финальный LF файла не входит в fingerprint. |
| `task-record.json` | Persisted launch intent: running без PID, два события; без более новых optional/default fields. |
| `task-statuses.json` | Семь существующих wire states и terminal projection. |
| `worker-info.json` | Минимальный старый read-only Info. Unknown metadata можно прочитать, но старый codec его не сохраняет. |
| `journal-v1.sql` | Изолированный журнал старой схемы с тем же payload/record, создаваемый только внутри TempDir. |

Тесты:
[protocol](../../../crates/protocol/tests/legacy_baseline.rs) и
[Store](../../../apps/worker/tests/legacy_baseline.rs).
Они используют настоящие DTO/Store; исполняемых процессов и network connection
не создают. `K:\fixtures\tool.exe` не существует и не должен запускаться;
PID 4242 в тесте — только значение read model, не адрес для OS signal.

Не регенерировать fixtures из новых DTO при изменении кода: это скроет регрессию.
Новый контракт получает новые fixtures/adapter tests, старые остаются. Изменение
самого baseline требует объяснения совместимости в review/ADR. Import/workflow/app
и полная mixed network/TS matrix добавляются до изменения соответствующих domains;
этот набор их не сертифицирует.
