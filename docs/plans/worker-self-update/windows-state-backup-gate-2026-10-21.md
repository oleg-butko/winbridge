# Windows U2.1: final backup success и миграция 12→13

2026-10-21. Оба начатых прогона завершены успешно. Установленный worker не
обновлялся, runtime replacement не выполнялся, UI-окон не было. U2.1 остаётся
открытым до fault gate. [Как продолжить следующую сессию](resume-2026-10-21.md).

## Результаты

| Проверка | Task / run | Итог |
| --- | --- | --- |
| Final backup actual Main | `b6c13fd7-14ee-4b5f-9158-ee9fba87b347` / `20261021-155003-a33e1e12` | succeeded/exit0; 22 tests, 7 command groups; 12 ready files проверены дважды, 15 искажений proof отклонены. |
| Actual schema12 history →13 | `9efab2b5-0357-4e80-9b42-4ab2fb540a55` / `20261021-154240-4bdadc21` | succeeded/exit0; 1 native integration test; 6 ready files проверены дважды, 7 искажений proof отклонены. |

Отчёты: `/mnt/hgfs/shared/WinBridge/workflow-validation/<run>` в Debian,
`K:\VMs\shared\WinBridge\workflow-validation\<run>` в Windows.
Оба каталога и исходные evidence сохранены; очистка не выполнялась.

| Файл | SHA-256 |
| --- | --- |
| Happy `report.json` | `361883fc55e7a1279ff65db605f1ee5c585f688b13cf92a7369f57ddc65b4e87` |
| Happy `candidate-health-actor-proof.json` | `27d84e4ff254f371292c4a8e5be600da7c16022601e44427d5cb32d606fc70cb` |
| Migration `report.json` | `b7317e4066b847ec03e957ee78654a3a32abf4564b13cce82a298a2ac5422047` |
| Migration `state-backup-migration-proof.json` | `25fcdf902b216367b4c593497bcdd279670d81511b3b750a44883d9cf83f5538` |

Local verification records (ignored, не коммитятся):
`.winbridge/update-preparation/state-backup-main-native-verified.json` и
`state-backup-migration-native-verified.json`. В них сохранены полный public proof,
метрики, hashes и результаты отрицательных проверок; private client туда не входит.

## Что доказывает happy gate

Producer — clean `8bd7629a7c4e01bd27bc314296aa2218815677d6`, build
`1792597816265396700`, source manifest
`2990e5b95dd0bccd469bb320582bd0d9c859b4dfd9a3b6f4b57f24031775b090`.
Изолированный normal Main/serve использовал реальные четыре old-role exe;
candidate собирался из другого clean commit `31fa634` (updater13/worker16).

Команды gate: build updater; negative DB/WAL fingerprint1; state-backup FSM/data/
ledger15; SQLite archive3; negative CLI2; build old set; full actual Main case1.
Полный Main case занял456.09s. Сначала выполняется настоящая seed task с output;
повтор её key возвращает ту же task. Затем v15 marker-required stop и получение
original lock/exit guard, запрос backup внешним owned parent и actual old helper.

Update `3522a9c3-a481-4ab1-bc15-1747ec1a675d`, backup attempt
`1352c167-82bf-4894-a9c2-c4fb61a57521`, Main PID5792, helper PID1996.
Сохранённый backup Verified rev5, root Replacing; `runtimeApplied=false`.
Копия245760 bytes, SHA
`417eda5d52500862784bc17cb776c1d30ae8d9d4611235f6035cb1cb51beeb58`.
Она находится в **тестовом** runtime под `shutdown-main-cases\None\runtime\update\<update-id>`.

Сверены все логические rows SQLite, включая1 task и3 output rows, dedup и shutdown
acceptance. Source/copy row SHA
`f6bfb70ae667e442e538bf2affb37a7f22605fa0580f5b720d1aa78987a3886e`.
Source main/nonempty WAL bytes, incarnation/epoch, config и recovery marker не
менялись. Отсутствующий и пустой WAL считаются одним отсутствием данных;
непустой WAL и main DB по-прежнему сверяются побайтно через SHA.

Проверены actual input/image/process binding, file identity/hash, readonly exporter,
owned helper Job cleanup, live custody и duplicate без новой attempt. Parent затем
завершает только собственный test Job; сохранённый shutdown не создаёт custody
при readonly inspection. Дополнительная проверка backup history после этого crash
добавлена в финальный scaffold, но **ещё не исполнялась в native producer**.

Повторная независимая сверка проверила bytes/SHA всех12 ready entries, точные ZIP
source/target и весь typed proof. Verifier отклонил изменения source, списка cases,
intent/input, helper process ID, SHA backup, readonly flag, authority, cleanup,
custody, duplicate revision, rows/recovery/no-new-attempt и ложный runtimeApplied.
Это проверка самого verifier, а не дополнительные fault executions Windows.

## Что доказывает migration gate

Взят сохранённый actual schema12 shutdown graph из ранее успешного U1 gate
`20261021-090004-a229a8de`, caseNone. SQLite consistent clone мигрирован в13.
Проверены retained rows, shutdown history/Exited, consistent backup12, epoch2→3,
отсутствие выдуманной backup attempt и physical custody. Реальный старый updater12
отказывается от нового журнала командами init/status, ничего в нём не меняя.
Исходный schema12 reference остался неизменным. Это миграция **updater journal**;
production worker migration/restore этим не реализованы.

Reference pin лежит в `.winbridge/update-preparation/state-backup-schema12-reference-pin.json`.
Для восстановления этого несекретного pin:

```json
{
  "kind": "state_backup_schema12_reference",
  "runtimeDirectory": "K:\\_proj-26\\winbridge\\workspaces\\snapshot-fixture\\workflow-validation\\20261021-090004-a229a8de\\shutdown-main-cases\\None\\runtime",
  "manifestSha256": "c9d1a667f0ca52577a0364de190f0044b09258b0f3763c2ebd518fae9143a1b5",
  "updaterSha256": "d4b271f6bde7e9ef79c68a2029a190c90898a5520040e1fd76eca0541872644d",
  "updateId": "90fa1ced-2dba-4460-8f33-7099c8686257",
  "sourceCommit": "2df5f44819e1189bc7748f4661b66075c865df16",
  "sourceManifestSha256": "1abe41cb3b36c80800981a6fae818a378dd1ce62905231c93aa83b0a13093aff",
  "sourceProofSha256": "e7d567289fc3b02b6c6f83347ff23cfeed1c774fcd3c6f3dde9b3ad37d780e28"
}
```

## Архивы и команды выполненных прогонов

Shared sources: `/mnt/hgfs/shared/WinBridge/sources`.

| Роль | ZIP / SHA-256 |
| --- | --- |
| Producer | `winbridge-20261021T154220Z-8bd7629-15e90820.zip`; `7134e6141a0544e8ff8b9861c5bdce1bb8541c9db50995536af58f40f156c691` |
| Candidate target | `winbridge-20261021T150720Z-31fa634-9a28b4a2.zip`; `b4d03a1c8f4dbf81f36c3196f60a3b6f118dcebf6c96f0d71dc4274ab208349b` |

Target manifest SHA
`d6f1147843f555d9dba3b9f8a60b30a8bf5d72a2f47a78735d3bbbb85875c270`.
Для истории ниже **команды уже завершённых прогонов**, а не поручение повторить их:

```bash
python3 scripts/validate-update-shutdown-primitives-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261021T154220Z-8bd7629-15e90820.zip \
  --cache-run 20261017-061527-f4639971 --backup-migration-only \
  --reference-receipt .winbridge/update-preparation/state-backup-schema12-reference-pin.json

python3 scripts/validate-update-health-actor-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261021T154220Z-8bd7629-15e90820.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261021T150720Z-31fa634-9a28b4a2.zip \
  --cache-run 20261017-061527-f4639971 --final-state-backup-gate
```

## Исправленные ошибки fixture и сохранённые неуспешные попытки

- `20261021-150949-b0d8a5f1`, task `2cce27d5-0d1b-43e4-b069-8830061928c1`,
  source `d419e1c`:20 tests прошли, Main остановлен source guard из-за fixture
  `runs.log` в source cwd. Seed перенесён в `target/history-seed` (`fb5fc8a`).
- `20261021-152039-f65802ae`, task `4bfb2b62-8eb5-49c5-961b-f2f4a7a48b89`,
  source `fb5fc8a`:20 tests прошли, actual backup достиг Verified, но fixture
  посчитал создание пустого WAL изменением данных. Main DB SHA оставался
  `e5812b5dc7563140bec133813b46a72bc26ca0ee3d550338a28627ab58b6c148`.
  Исправлена только нормализация absent/empty WAL с negative regression (`8bd7629`).

Оба прогона остаются failed; успешная третья попытка не меняет их историю.
Source guard и проверка nonempty/main SHA не ослаблены. Установленный worker
не проявлял ошибочного поведения. Его новая версия для этих исправлений не нужна.

## Завершение проверок и ограничения

Финальный validation-only scaffold для четырёх fault cases прошёл обе Clippy
all-target проверки, fmt/diff. E0502 из последней компиляции устранён: update ID
копируется из proof до его мутаций. Full workspace610/0 относится к ранее
проверенному v15 коду; повтор всего workspace после scaffold не заявляется.
Подробные команды и log filenames находятся в resume документе.

Windows free space: migration35516891136→35321716736 bytes;
happy35304185856→34747117568 bytes. Debian около26GiB. Native cache переиспользован,
validation directories не удалялись. Installed d052 healthy/active0, без maintenance
и recovery errors, прежняя generation; controller PID3902 оставлен работающим.
Все тестовые процессы завершены. Desktop можно открывать/закрывать, installed
worker оставить работающим. Новых прогонов после запроса пользователя не запускали.

Следующий шаг — закончить и выполнить **backup fault gate**, затем review U2.2
replacement/activation. U2.1, U2–U4 и всё самообновление пока не завершены.
