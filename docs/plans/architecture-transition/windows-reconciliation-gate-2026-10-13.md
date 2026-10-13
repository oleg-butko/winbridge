# A2.4.1 — Windows gate physical process reconciliation

Дата: 2026-10-13. Результат: **passed**, A2.4.1 завершён; полный A2/A2.4 открыт.
Проверен [контракт physical reconciliation](physical-reconciliation.md).
Implementation `5004f3d`, fixture policy correction `4da9af1`;
clean tested source `4da9af16f238b744e37103058cfccfa4c16c7fa1`.

## Исполнение и evidence

Требовался только установленный Windows worker, desktop был не нужен.
Approved `--task-cas-only` собрал candidate offline в отдельном workspace/state/cache.
Рабочий runtime/config/journal schema не обновлялись. Рабочий journal получил две
задачи gate: первый прогон остановился на ошибке подготовки теста, второй passed.

| Поле | Значение |
| --- | --- |
| Run | `20261013-152019-2ff8a535` |
| Shared report | `K:\VMs\shared\WinBridge\workflow-validation\20261013-152019-2ff8a535` |
| Source archive | `winbridge-20261013T191947Z-4da9af1-95f344f4.zip` |
| Archive SHA-256 | `23be74492505c06ddd9af5cf1a3ff2bc0fe2f38c55cbedb8d8f9a9a2e294d5d0` |
| Source manifest SHA-256 | `8199f0db23e2040c11c2355f885927e138bfaf5535a8f85843783151f0025cab` |
| Report SHA-256 | `b4b0ce53584b0f09b91bb01c0648cf27f91958867724c35670074d6de50f380c` |
| Ready SHA-256 | `0bbb0afb9fcf4635e6a5ed4e96e47df7218154f55ebf39541cea4ee9363fac5d` |
| Reference proof SHA-256 | `976c66ffceaf7898782f4288ae8fe529d5e082c41582641f318fd452809ec472` |
| Task ID / PID | `d2d3d2ca-3d98-4a84-a504-cb476e57942f` / `19616` |
| Task outcome | `succeeded`, exit `0` |
| Started / finished UTC | `2026-10-13T19:20:28.5533084Z` / `2026-10-13T19:25:46.8870732Z` |

Commit/archive SHA связаны с report; `sourceDirty=false`. Все шесть listed
artifacts проверены по размеру и SHA-256: report, четыре cargo logs и reference
proof. Ready marker опубликован после итоговых файлов.

| Cargo group | Passed | Exit |
| --- | ---: | ---: |
| Pure task FSM | 7 | 0 |
| Frozen protocol + worker contracts | 14 | 0 |
| Native writers / recovery / regressions | 142 | 0 |
| Actual old-worker refusal | 1 | 0 |
| Всего | **164** | |

## Что проверено

Все семь `reconciliation` tests прошли на настоящей Windows: PID/binding commit
атомарен при ignored update; physical identity нельзя заменить; foreign/stale
owners и ложный no-process receipt отклоняются; schema-10 migration сохраняет
raw records/keys/attempts и consistent backup; неудачная migration откатывается;
52 unresolved resources дают bounded inventory из 50 записей и не снимаются
новым epoch.

Suspended fixture не создаёт `runs.log` до binding commit. Коллизия имени Job Object
не меняет первоначальный объект. Failed binding не разрешает resume и не создаёт
повторный эффект; новый явный запрос после подтверждённого cleanup работает.

Живой потомок удерживает Job Object после выхода root процесса. Read-only probe
возвращает occupied; другой host/session даёт unknown. После остановки именно
тестового дерева, ignored lease release оставляет проект заблокированным и
показывает journal recovery required, не завершая worker. После ремонта и restart
physical reconciliation освобождает lease; новый запрос выполняется успешно,
старый key возвращает прежний indeterminate Task ID. `runs.log` содержит только
root, потомка и новый явный запуск.

Прошли прежние 15 lease tests с дополнительными assertions: завершённый import
сохраняет `verified`, commit и `finishedAtMs` в атомарном terminal write.
Profile crash regression, real resume/crash, cancellation/timeout/dedup, output,
retention, snapshots, workflows, ownership/epoch/CAS и atomic links тоже passed.
Actual установленный schema-6 reference отказал schema 11 с exit 1; отдельный
test journal остался byte-identical. Installed journal не использовался для
тестовых migrations/faults.

Первый run `20261013-151419-766148a1` (`5004f3d`) остановился на assertion нового
explicit request: fixture вручную записывала working directory без canonical
Windows prefix, и runtime отклонял несовпавшую resolved command. Fixture теперь
использует настоящий project resolver; protection не ослаблялась. Повторный
чистый архив и полный gate выше подтвердили исправление. Installed worker
оставался connected/idle, без maintenance/journal recovery errors в обоих прогонах.

## Состояние и следующий пакет

Installed build `b5d39f97045ca383012d1a9a9cf92e50ee1e73af` остался прежним:
generation `02939593-8adf-4878-a346-473092697a4a`, session
`631c2e63-edc8-44f8-8cce-89abec7caf8b`. После прогона connected, activeTaskCount=0,
maintenanceError/journalRecoveryRequired отсутствуют; build/generation/session
совпали с preflight. Desktop можно закрыть/оставить закрытым, worker оставить работать.

A2.4.2 ещё требуется: explicit restore/recovery mode, контролируемая новая
incarnation и разрешение unresolved legacy ownership. A2.4.1 не обнаруживает
потерянные dedup keys после rollback/copy старой БД. Поэтому installed worker
остаётся schema 6; deployment schema 11 не выполнен. Полный A2/A2.4 не закрываются.
