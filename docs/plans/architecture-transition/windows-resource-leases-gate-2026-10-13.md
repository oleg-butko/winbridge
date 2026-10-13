# A2.3.2 — автоматический Windows gate durable resource leases

Дата: 2026-10-13. Результат: **passed**, A2.3.2 завершён; весь A2 открыт.
Проверен [контракт resource leases](project-resource-leases.md), clean implementation
`9d1fa9d76b4e84cd4d623d0edf09b2f53bbd10b7`.

## Запуск и evidence

На Windows требовался только установленный worker, desktop был не нужен.
Approved `--task-cas-only` сценарий собрал candidate offline в отдельном
workspace/state/cache. UI-окон не появлялось. Executables/configs/schema рабочего
runtime не обновлялись; рабочий journal получил одну задачу автоматического gate.

| Поле | Значение |
|---|---|
| Run | `20261013-130210-1b9361d0` |
| Shared report | `K:\VMs\shared\WinBridge\workflow-validation\20261013-130210-1b9361d0` |
| Source archive | `winbridge-20261013T170135Z-9d1fa9d-1da71ca9.zip` |
| Archive SHA-256 | `f986ba61e4f4ff5091fb0b7ffef7d19d50a327abdae195db06d827de2d0296a4` |
| Source manifest SHA-256 | `cfbf57dd119a7273469dfc87debbbd38624c0ab77db379afb7be38d207914f9b` |
| Report SHA-256 | `65f614c8a3a77fd469e7705e9fed5ea74b235bb9b0f5e0b8cb688aa4e25e9b89` |
| Ready SHA-256 | `c4bfb53fe1ca529391af668527169f84ec84cdbe62ec1655946776d6b3876ef3` |
| Reference proof SHA-256 | `4ca1795a2fdb45e2acb009f627c7c53e9ac7c225a695faa5f439bcf779c97e25` |
| Task ID / PID | `789e716b-dc64-419b-8d1d-8656b032d855` / `16504` |
| Task outcome | `succeeded`, exit `0` |
| Finished UTC | `2026-10-13T17:07:17.2020100Z` |

Report связывает результат с commit и archive SHA, sourceDirty=false. Все шесть
listed artifacts прошли проверку размера и SHA-256: report, четыре cargo logs,
reference proof. `report.ready.json` опубликован после итоговых файлов.

| Cargo group | Passed | Exit |
|---|---:|---:|
| Pure Task FSM | 7 | 0 |
| Frozen protocol + worker contracts | 14 | 0 |
| Native writers / resource leases / runtime regressions | 135 | 0 |
| Actual old-worker refusal | 1 | 0 |
| Всего | **157** | |

## Что проверено

Все 15 новых lease tests прошли на Windows: task/import конкуренция за проект,
независимые проекты, root reservation против claim, borrowing linked child,
ignored insert/release и rollback intent/attempt/outcome/receipt, immutable proof,
new epoch без освобождения неизвестного ресурса, recovery durable cleanup receipt,
legacy multi-owner группы с сохранением PID/unknown PID, schema-9 backup/migration
rollback, две реальные последовательные команды без duplicate spawn и copy recovery.

PID commit fault дополнительно подтвердил cleanup настоящего процесса и допустимый
следующий запуск. Ready приложение сохраняет lifetime lease и допускает обычную
команду проекта; его позднее завершение не снимает lease этой команды. Injected
handoff failure сохраняет mutation reservation, не объявляет startup успешным и
показывает journal recovery required. Faults вводились только в isolated test state;
ожидаемые diagnostic errors/panic traces не означают отказ installed worker.

Прошли прежние guards: owner-change остановка дерева, cancel/timeout/dedup,
output/control, domain CAS/atomic links, attention/history/retention,
verified imports, actual crash/restart/resume, profiles и workflow/readiness.
Crash profile подтвердил, что unknown process блокирует только свой проект,
не перезапускается по прежнему ключу, а здоровый проект выполняет новый профиль.

Actual schema-6 reference executable отказал новой schema, exit 1:

> journal schema 10 is unsupported; expected 6

`journalUnchanged=true`: отдельный test journal byte-identical. Installed executable
использовался для describe/refusal с новым test state; его рабочая DB не подменялась.

## После проверки и следующая работа

Installed worker `b5d39f97045ca383012d1a9a9cf92e50ee1e73af` остался connected/idle,
activeTaskCount=0, maintenanceError/journalRecoveryRequired отсутствуют. Build,
generation `02939593-8adf-4878-a346-473092697a4a`, session
`631c2e63-edc8-44f8-8cce-89abec7caf8b` совпали с preflight.
Desktop можно оставлять закрытым или открыть для обычной работы; worker оставить.

Local gate: 245 workspace tests; финальные leases/task_fsm/workflows — 44 tests.
Windows-target workspace/worker tests, Clippy all-targets, fmt, diff check,
PowerShell AST, Python syntax/pre-IPC guards и scaffold passed.

A2.4 остаётся открытым: доказательство физического состояния старого process owner,
recovery mode и explicit restore с новой incarnation. Автоматическое освобождение
unknown lease по epoch/timeout/terminal result отсутствует. Candidate не заменяет
installed runtime до завершения этого пути; A3 read model, A4 resource keys/global
scheduler и A5 actors следуют по принятому плану.
