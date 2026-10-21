# Основной native сценарий точной остановки — пройден

2026-10-21. Task `1615bebb-015e-4155-b263-082aac5a1631`,
run `20261021-090004-a229a8de`, succeeded / exit0. Один integration test
прошёл четыре полных сценария за 1182.94s. Три команды runner завершились exit0:
сборка supervisor, сборка четырёх настоящих old-role exe, actual Main pipeline.

Shared отчёт: `K:\VMs\shared\WinBridge\workflow-validation\20261021-090004-a229a8de`.
Все 17 файлов `report.ready.json` сверены независимо дважды. SHA-256:

- `report.json`: `f6af84ba71dd743a6dc54c18604232d73390ff8363e1ad0d054ea9c2d2fa0730`.
- `candidate-health-actor-proof.json`: `e7d567289fc3b02b6c6f83347ff23cfeed1c774fcd3c6f3dde9b3ad37d780e28`.

Локальная запись проверки: `.winbridge/update-preparation/shutdown-main-native-verified.json`.
Дополнительно verifier отклонил девять искажений реального proof: пропущенный case,
изменение runtime, второй stop, подмена birth, отсутствие lock, writable reader,
преждевременный exit, acceptance при отказе записи и выдуманная custody после crash.

## Проверенные версии

Source `2df5f44819e1189bc7748f4661b66075c865df16`, clean build
`1792573215563919400`, manifest SHA
`1abe41cb3b36c80800981a6fae818a378dd1ce62905231c93aa83b0a13093aff`.
ZIP `winbridge-20261021T085927Z-2df5f44-a4856110.zip`, SHA
`9654a6009cf6dd16649e930d1f1f69c97c12f6c0229ae746ae2a19b25783fc0f`.

Target `a294e03100712ecf5a3e5ed836ef008bf084b3c4`, manifest SHA
`84b3d02147ea34ad46212afed3a80e2633b2c7cb0dffdd4c4949f9cb34e3e6e7`.
ZIP `winbridge-20261021T043231Z-a294e03-6d95065c.zip`, SHA
`d0f79b40da71e624087d4ac2531ed56d30e018de24839ef70c4f210ca00d3607`.
Обе версии используют worker16/updater12. Более ранний target выбран явно:
проверяется same-schema lifecycle, не upgrade/migration. Проверенные target sources
заранее размещены в одном каталоге; source plan имеет ноль изменений. Полный source
apply имеет прежние отдельные gates. Четыре последовательных case переиспользуют
source/compiler cache; runtime, state, instance и installation у каждого свои.

## Результат

| Case | Update ID | Main PID | Shutdown / root |
| --- | --- | --- | --- |
| ACK | `90fa1ced-2dba-4460-8f33-7099c8686257` |14664| Exited rev3 / Replacing |
| Lost ACK | `30830710-6040-456d-85fa-ddd9ecf549fa` |15392| Exited rev4 / Replacing |
| Delayed callback | `90c8a559-d683-4428-bec1-21e2c2d21a4e` |15956| Exited rev3 / Replacing |
| Receipt write denied | `9a9c74a6-94fd-45aa-9141-155c5858f0b4` |5300| Uncertain rev2 / Stopping |

Worker — настоящий product Main с явной feature `update-validation`; обычные
build/install scripts её не включают. Runtime guard, открытие worker journal,
serve и закрытие callbacks выполняет обычный код Main. Feature добавляет fixture
startup и контролируемые faults. Supervisor запускается независимо от собственного
execution domain worker; внешний validation Job сохраняется только в fixture.
Production external-Job refusal и запрет breakaway сохранены.

Каждый случай реально собирает candidate и выполняет artifact/config/rehearsal,
health и drain. RequestShutdown отправляется ровно один раз, duplicate наблюдает
ту же попытку. Lost ACK закрывает listener и требует integrated cold reconciliation;
Uncertain остаётся в истории. Задержанный callback сохраняет receipt до ACK и
оставляет mailbox доступным; exit/custody в этот момент ещё отсутствуют.

После acceptance Main действительно выходит. Parent проверяет Exited + Replacing,
живой supervisor, удерживаемую им instance lock и точную receipt через настоящий
cold CLI после выхода Main. Обычный Main restart отказывает с нужным update ID;
независимая DB-проверка подтверждает активную lease. Все четыре прежних exe и manifest
сохраняют исходные bytes. В denied case receipt отсутствует, root остаётся Stopping;
worker завершает fixture driver явно для уборки тестовых процессов. Это завершение
не выдаётся за accepted shutdown.

Затем parent завершает свой validation Job. История читается после owned supervisor
crash, текущая custody отсутствует, instance lock свободна. Журналы, startup guards,
case directories и отчёты сохранены. Native проверка не меняет installed runtime.

## Ограничения и следующий блок

Этот gate закрывает основной stop/ACK/exit/cold сценарий. Дополнительные combined
faults — задержанный instance lock после OS exit и crash supervisor до подтверждения
exit — ещё нужны; примитивы lock/PID/epoch ранее проверены отдельно. U1 целиком
и public самообновление U1–U4 остаются открытыми. Перед U2 требуется final consistent
worker DB backup, затем consumption живого OS guard для replacement и activation.
Исторический Exited не создаёт разрешения продолжить после потери supervisor.

Windows available 48891133952→47274409984 bytes; очистка Windows не выполнялась.
Debian available 26.44 GiB; отдельно освобождено 227MiB неиспользуемого incremental
cache при отсутствии локальной сборки. Config/journal/report/controller не тронуты.
Installed worker `d052c6c`, generation `ee48ba22-7a9e-4ce3-8ed1-5080a90806a3`,
остался healthy/idle 0. UI-окон не было; desktop можно открывать/закрывать,
установленный worker оставить работающим. Ручное обновление сейчас не требуется.
