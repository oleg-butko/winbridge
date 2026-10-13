# A2.3.1 — автоматический Windows gate ownership/attempt tokens

Дата: 2026-10-13. Результат: **passed**, A2.3.1 завершён; весь A2 открыт.
Проверен [контракт ownership context](ownership-context.md), clean commit
`ad9b1f998dd16b93a57cc3d6360c343b9f3d5bb0`.

## Запуск и доказательства

Установленный worker был connected/idle, desktop не требовался. Один approved
сценарий собрал кандидат offline в отдельном Windows workspace/state/cache.
UI-окон не открывалось; root/child fixtures и process probes используют
CREATE_NO_WINDOW. Faults вводились только в изолированные test workers/DB.
Reference executable запускался с НОВЫМ тестовым state для проверки отказа
новой схеме; рабочий runtime/config/DB не заменялся этим сценарием.

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261013T155338Z-ad9b1f9-8c13c0a2.zip \
  --task-cas-only \
  --task-cas-reference-worker 'K:\_proj-26\winbridge\runtime\winbridge-worker.exe'
```

| Свидетельство | Значение |
| --- | --- |
| Task | `087435ea-188d-4636-ae56-04b362acab6b`, succeeded / exit 0, PID 9524 |
| Run | `20261013-115405-a1c5c6ef` |
| Сценарий UTC | 2026-10-13T15:54:12.0320125Z — 2026-10-13T15:58:59.2092836Z |
| Source commit | `ad9b1f998dd16b93a57cc3d6360c343b9f3d5bb0`, dirty false |
| Archive SHA-256 | `749fd365aa2141834a9f6a12409ee668123323fb0ee1ed752c91db5c6b6a0707` |
| Candidate manifest SHA-256 | `e81446baf21519ddc10c34623401cb96665a3b4519e3e61daf3ba6a2b4a65a7b` |
| Report SHA-256 | `fb471ae005ad004273f5e3b402e78bd8b526c7be68db1599b51f8f0cfa6e0cd7` |
| Reference proof SHA-256 | `8584b3e97ce12c538dde34345884375d13dd54433f19ba334c824912d1bc5b77` |
| Reference source commit | `b5d39f97045ca383012d1a9a9cf92e50ee1e73af` |
| Reference exe SHA-256 | `35946d4857176248e9230c1c2f988286162347a1f155682af6399fbdfb7d23d5` |

Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261013-115405-a1c5c6ef`.
Debian: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261013-115405-a1c5c6ef`.
Ready marker содержит шесть artifacts: report, четыре native logs, reference proof.
Captured bytes каждого файла проверены по размеру и SHA-256. Local request/task/
output retained в `.winbridge/workflow-validation/20261013-115405-a1c5c6ef`.

## Проверенные риски

**141 actual Windows tests**, четыре cargo groups exit 0:

| Группа | Passed |
| --- | ---: |
| task-cas-pure-fsm | 7 |
| task-cas-frozen-contracts | 14 |
| task-cas-native-writers | 119 |
| task-cas-older-worker-refusal | 1 |

Все 16 новых ownership tests прошли. Два concurrent claims дают одну durable
попытку. Stale context/snapshot отвергается всеми writer families, даже при
совпадении revision; PID/output/import progress/finish прежнего attempt не меняют
журнал. Output и cancel revisions не мешают действующему owner сохранить actual exit.
Callback не переключает operation и administrative scope. Injected callback panic
проверяет восстановление scope; ожидаемый panic trace в log не является сбоем gate.

Ignored attempt INSERT откатывает dispatch, реальный fixture не стартует; после
исправления storage он исполняется ровно один раз. Completion fault откатывает
terminal state и attention. Сохранённый attempt нельзя удалить, заменить или
снова открыть. Epoch overflow/activation fault не дают нового права исполнения.
Schema-8 migration сохраняет raw bytes, dedup, consistent backup и не выдумывает
попытки для старых operations; migration collision откатывает tables/version.

Real-tree test наблюдает PID родителя и дочернего fixture, вводит owner-change fault
в test DB и подтверждает остановку обоих процессов. Старый worker перестаёт
обслуживать запросы; terminal outcome не подделывается. Новый owner сохраняет
`indeterminate` и завершает attempt receipt при recovery без повторного запуска.
Это проверка cleanup действующего process owner, а не разрешение takeover живого
executor через SQL. Physical-owner reconciliation до admission остаётся A2.4.

Существующие checks также проходят: lifecycle/cancel/tree/timeout, output/control,
domain CAS/atomic links, attention/retention/history, verified snapshot imports,
actual crash/restart/resume, two-project profiles, workflow/build/start/readiness.
Frozen wire v1 и journal compatibility tests проходят без изменения payload/keys.

Реальный прежний executable завершился exit 1:
`journal schema 9 is unsupported; expected 6`. Test journal byte-identical,
queued fixture не запускалась. Обычно ignored reference test здесь явно выполнен
четвёртой группой; отсутствие старого executable не засчитывается как успех.

## Состояние после gate и следующая работа

Worker connected/idle; activeTaskCount=0, maintenanceError/journalRecoveryRequired
отсутствуют. Generation `02939593-8adf-4878-a346-473092697a4a`, session
`631c2e63-edc8-44f8-8cce-89abec7caf8b` и build совпадают с preflight. Рабочая
schema остаётся 6; новая schema 9 проверена в изолированных journals. Рабочий
journal получил одну обычную task gate. Desktop можно закрыть; worker оставить.

Local gate: **229 workspace tests passed**, reference test ignored; Windows-target
workspace/worker tests, Clippy/fmt, PS AST, Python syntax/argument guards и scaffold
passed. После final fixture change повторены ownership/runtime и Windows tests check.

A2.3.1 закрыт. Далее **A2.3.2 durable project/resource leases** с atomic operation
association, затем **A2.4 physical reconciliation и explicit restore recovery mode**.
Общий A2 и actor/global parallel migration пока не закрыты.
