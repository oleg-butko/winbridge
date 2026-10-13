# A1 — автоматический Windows gate

Дата: 2026-10-13. Результат: **passed**, A1 завершён.
Проверена [Task FSM](task-fsm.md) из commit
`34f21f578a2d2e3cdb2925ea842a4823ee8439f3`, source dirty = false.

## Запуск и источник

Рабочий worker был подключён и свободен, desktop не требовался. Debian driver
отправил одну durable direct task через controller. Сценарий распаковал pinned
archive, проверил manifest и собрал tests offline в отдельном локальном каталоге.
Тестовые UI-окна не открывались. Команда видна в журнале; PowerShell
EncodedCommand отображается desktop в декодированном виде.

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261013T132323Z-34f21f5-0ce9fbdb.zip \
  --task-fsm-only
```

| Свидетельство | Значение |
| --- | --- |
| Task ID | `e0f271c9-6a21-445f-9eba-a4e85382a762` |
| Итог | `succeeded`, exit 0, process PID 16048 |
| Run | `20261013-092340-93391277` |
| Время сценария UTC | 13:23:50–13:26:32 |
| Архив SHA-256 | `44382ec04ec36efe26ec449a52fb993be9d3e5ed615146caba1a03928d9c23b9` |
| Report SHA-256 | `35d82c7a5335d6f0d196474c493a8c977409e5c31a08663390e1b8b1068e884d` |
| Рабочий worker build | `b5d39f97045ca383012d1a9a9cf92e50ee1e73af`, dirty false |
| Рабочий worker generation | `02939593-8adf-4878-a346-473092697a4a` |
| Session | `631c2e63-edc8-44f8-8cce-89abec7caf8b` |

Shared report directory:
`K:\VMs\shared\WinBridge\workflow-validation\20261013-092340-93391277`.
На Debian тот же каталог находится под
`/mnt/hgfs/shared/WinBridge/workflow-validation/20261013-092340-93391277`.
Test source/cache/state находятся под
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261013-092340-93391277`.

## Проверки

Три cargo commands завершились с exit 0. **66 настоящих Windows tests:**

| Suite | Passed | Что подтверждено |
| --- | ---: | --- |
| core `task_fsm` | 7 | Guards, legacy projection, cancel/exit race, terminal immutability, recovery без relaunch. |
| protocol `legacy_baseline` | 6 | Frozen v1/session/task wire contracts. |
| worker `legacy_baseline` | 8 | Историческая БД, ACK/dedup/reopen, fingerprint, recovery/cancel и output cursors. |
| worker `attention` | 10 | Attention вместе с durable outcome; повтор и recovery. |
| worker `runtime` | 7 | Реальные процессы, output, timeout, cancellation tree, restart и дедупликация. |
| worker `task_fsm` | 4 | Launch commit abort/ignore без spawn, PID commit failure → indeterminate без replay, cancel/PID race. |
| worker `workflows` | 24 | Совместимость workflow/application lifecycle и crash recovery. |

Driver прочитал `report.ready.json` и проверил размеры/SHA-256 всех четырёх
artifacts: report и трёх logs. Report подтверждает Windows platform, exact archive
hash, source commit и отсутствие dirty source. Ноль failed tests.
В Debian до экспорта прошли 182 workspace tests, Windows-target workspace и
protocol/core/worker integration tests check, Clippy, fmt, PowerShell AST,
Python syntax, scaffold/local links и diff checks.

## После прогона и границы

Installed worker остался подключён, generation/session/build прежние,
activeTaskCount = 0, maintenanceError/journalRecoveryRequired отсутствуют.
Установленные exe/configs не заменялись; рабочий journal получил запись одной
задачи gate. Test instances использовали свои отдельные state/workspaces.
Desktop можно закрыть; рабочий worker оставить запущенным.

Это gate нового candidate runtime, а не обновление установленного worker.
A1 не добавляет CAS, durable attempts/epochs, multi-project global scheduler или
новый public protocol. Они остаются следующими пакетами. Исходы внешних команд
при неизвестном результате по-прежнему не повторяются автоматически.
