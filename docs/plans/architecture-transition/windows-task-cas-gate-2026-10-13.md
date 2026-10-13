# A2.1 — автоматический Windows gate task revisions/CAS

Дата: 2026-10-13. Результат: **passed**, A2.1 завершён; весь A2 остаётся открытым.
Проверена [первая durable поставка](durable-transitions.md) из clean commit
`8f631ddea2d3fdb5147487929ad3521b2fb5a092`.

## Запуск и источник

Windows worker был подключён и idle, desktop не требовался. Через controller
принята одна task. Source archive/manifest проверены, candidate tests собраны
offline в отдельном Windows workspace. Тестовые UI-окна не открывались.
Рабочий exe использовался ещё в двух коротких subprocesses: `--describe` и отказ
`serve` с отдельным НОВЫМ config/state. Установленный журнал ими не открывался.

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261013T135903Z-8f631dd-b7a7ca9f.zip \
  --task-cas-only \
  --task-cas-reference-worker 'K:\_proj-26\winbridge\runtime\winbridge-worker.exe'
```

| Свидетельство | Значение |
| --- | --- |
| Task | `eecd118f-c11c-411d-9ac3-f7ebd8850526`, succeeded / exit 0, PID 8116 |
| Run | `20261013-095922-543e9452` |
| Время сценария UTC | 13:59:32–14:03:04 |
| Source commit | `8f631ddea2d3fdb5147487929ad3521b2fb5a092`, dirty false |
| Archive SHA-256 | `4b22ca50c1423199412ad5b7e502aac2be2030751a0b6fa564f3ba849ad6a82b` |
| Candidate manifest SHA-256 | `64d6f2b329021167b968e6b6bc07c1f7a365d73b136204083d37eb84bb116241` |
| Report SHA-256 | `ac2871ed4b6e96a4bc2f7d77c62bd5ce7f694835c7798cf17ec21befdd2a2196` |
| Reference proof SHA-256 | `4308081f3fc6d78027ecb93b9b1e69b01bceab2a9d8264c1b25dfc7fae665d9c` |
| Reference build | `b5d39f97045ca383012d1a9a9cf92e50ee1e73af`, dirty false |
| Reference exe SHA-256 | `35946d4857176248e9230c1c2f988286162347a1f155682af6399fbdfb7d23d5` |

Shared report directory:
`K:\VMs\shared\WinBridge\workflow-validation\20261013-095922-543e9452`.
На Debian: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261013-095922-543e9452`.
Source/cache/test state находятся под
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261013-095922-543e9452`.

## Что проверено

Четыре cargo commands завершились с exit 0. **107 настоящих Windows tests:**

| Suite | Passed | Назначение |
| --- | ---: | --- |
| core task_fsm | 7 | Прежняя v1 проекция и phase guards. |
| protocol legacy_baseline | 6 | Frozen wire/session contracts. |
| worker legacy_baseline | 8 | Исторический journal/keys/payload, ACK/reopen/recovery и cursors. |
| worker attention | 10 | Outcome/attention consistency и migrations. |
| worker history | 5 | Поиск, cursors и diagnostics. |
| worker maintenance | 9 | Settings, retention plans/receipts и автоматическая очистка. |
| worker output | 4 | Byte bounds, реальные IPC/streams, dedup/control/cursor restart. |
| worker runtime | 7 | Реальные процессы, cancel/timeout/tree и unknown-outcome recovery. |
| worker snapshots | 7 | Import/project lease, schema migration, recovery и snapshot regressions. |
| worker task_cas | 15 | Stale writes/cancel/exit, ABA, rollback output/attention/cleanup, concurrent dedup, schema-6 backup/failure и revision exhaustion. |
| worker task_fsm | 4 | Launch/PID commit faults и отсутствие replay. |
| worker workflows | 24 | Workflow/application lifecycle, faults/crash recovery. |
| worker task_cas — explicit ignored reference test | 1 | Actual old executable refuses schema 7 without modifying the test journal. |

Reference test пропущен только в обычном общем прогоне. В последней native группе
он запущен явно через `--ignored --exact` и прошёл. Старый subprocess вернул
ожидаемый exit 1 и причину `journal schema 7 is unsupported; expected 6`.
Сравнение файла test journal до/после отказа — побайтно равен; task revision 0,
record прежний, внешняя fixture команда не запускалась.

Driver проверил размеры/SHA-256 всех **6 artifacts** из ready marker: report,
четыре logs и reference proof. Candidate commit/dirty совпали с source report,
reference BuildIdentity точно совпал с info установленного worker.
До экспорта прошли 197 Debian workspace tests (1 explicit reference test ignored),
Windows-target workspace/tests check, Clippy/fmt/AST/Python/scaffold/diff checks.

## После прогона и границы

Post-gate info: generation `02939593-8adf-4878-a346-473092697a4a`, session
`631c2e63-edc8-44f8-8cce-89abec7caf8b`, прежний installed build, connected,
activeTaskCount 0, maintenanceError/journalRecoveryRequired отсутствуют.
Установленные exe/configs не заменены, working journal получил одну task gate;
test schema migrations выполнялись только в изолированных databases.
Desktop можно закрыть, working worker оставить работать.

Это подтверждение candidate A2.1, а не обновление установленного runtime.
CAS защищает task lifecycle/output/retention, но пока не даёт durable owner/epoch/
attempt fencing, journal incarnation или guard остальных domain writers.
Ресурсы физического старого процесса от одной revision не освобождаются.
Следующий пакет A2.2 расширяет versioned guards на import/workflow/application и
explicit attention; затем следуют ownership и startup reconciliation.
