# A2.2 — автоматический Windows gate domain revisions/atomic links

Дата: 2026-10-13. Результат: **passed**, A2.2 завершён; весь A2 остаётся открытым.
Проверен [контракт domain transitions](domain-transitions.md), clean commit
`df978dc5694250f8f2114d004235d820a54b0f24`.

## Запуск и доказательства

Установленный worker был connected/idle, desktop не требовался. Один approved
сценарий через controller собрал новый код offline в изолированном Windows
workspace/state/cache и выполнил четыре группы. UI-окон не открывалось.
Тестовые worker/processes использовали отдельные instance IDs и owned process trees.
Reference exe вызывался с `--describe` и `serve` только с НОВЫМ тестовым state;
рабочий runtime/config/DB не заменялся и не открывался reference subprocess.

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261013T145926Z-df978dc-666fbd10.zip \
  --task-cas-only \
  --task-cas-reference-worker 'K:\_proj-26\winbridge\runtime\winbridge-worker.exe'
```

| Свидетельство | Значение |
| --- | --- |
| Task | `9c994702-e113-4932-b21b-21fc2f3fc762`, succeeded / exit 0, PID 5352 |
| Run | `20261013-105937-b33d858a` |
| Сценарий UTC | 2026-10-13T14:59:46.1388597Z — 2026-10-13T15:04:37.9926237Z |
| Source commit | `df978dc5694250f8f2114d004235d820a54b0f24`, dirty false |
| Archive SHA-256 | `c27183811f33e33f1439a7660d86ec63d710212f540bca5c664d6676465dab06` |
| Candidate manifest SHA-256 | `4534d3f155263f38c109f1619a46e77208a0dcfd8ce730be0e76e1fe058a6107` |
| Report SHA-256 | `305fe301e6ba39ce8e9315308cf2d34b06cc9d49784ad8a2f40358ee4851ed0d` |
| Reference proof SHA-256 | `5ca9409fd4dda01bc23c2ad0a24b6be5a09d7aec5b95e912dbe62c0ab5a402ff` |
| Reference source commit | `b5d39f97045ca383012d1a9a9cf92e50ee1e73af`, dirty false |
| Reference exe SHA-256 | `35946d4857176248e9230c1c2f988286162347a1f155682af6399fbdfb7d23d5` |

Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261013-105937-b33d858a`.
Debian: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261013-105937-b33d858a`.
Ready marker содержит шесть artifacts (report, четыре logs и reference proof);
каждый проверен по captured bytes и SHA-256. Local dispatch request/task/output
retained в `.winbridge/workflow-validation/20261013-105937-b33d858a`.

## Проверенные риски

**125 actual Windows tests**, все четыре cargo commands exit 0:

| Группа | Passed |
| --- | ---: |
| task-cas-pure-fsm | 7 |
| task-cas-frozen-contracts | 14 |
| task-cas-native-writers | 103 |
| task-cas-older-worker-refusal | 1 |

16 новых domain tests проверяют stale import progress/cancel/terminal, workflow ABA,
application stop против поздней readiness, атомарность child/task/registry links,
stale parent rollback, stop/readiness/outcome attention, explicit ACK/dedup,
concurrent retries, immutable identity, revision exhaustion, exact schema-7 backup,
raw payload/record и attention sequence high-water, полный rollback migration и
relink legacy application без сброса recovered outcome/replay.

Существующие native tests выполняют реальные cancel/tree/timeout/PID/exit,
verified snapshots, process crash/restart/resume, two-project profile commands,
workflow import/build/start/readiness/log/artifacts и раздельный application lifecycle.
Frozen v1 wire и legacy journal tests проходят; protocol fields/keys остаются прежними.

Reference worker реально завершился с exit 1 и причиной
`journal schema 8 is unsupported; expected 6`. Test journal побайтно идентичен,
queued fixture не исполнялась; отсутствие старого exe не считается passing test.
Ignored reference check был явно выполнен отдельной четвёртой группой.

## Состояние после gate

Worker connected/idle, generation `02939593-8adf-4878-a346-473092697a4a`,
session `631c2e63-edc8-44f8-8cce-89abec7caf8b`; build/manifest/buildId
совпадают с preflight reference. `activeTaskCount=0`, maintenanceError и
journalRecoveryRequired отсутствуют. Установленная рабочая schema остаётся 6;
schema 8 проверена на новых изолированных journals. Рабочий journal получил одну
обычную task gate. Desktop можно закрыть; рабочий worker оставить запущенным.

Local gate: **213 workspace tests passed**, один native reference ignored;
Windows-target workspace/worker tests, worker Clippy, fmt, Python syntax/argument
guard, PowerShell AST и scaffold passed. A2.2 закрыт. Durable owner attempts/epochs/
incarnation и reconciliation до admission остаются **A2.3–A2.4**; CAS не fencing
физического исполнителя, global parallel queue и actors пока не вводились.
