# Windows: compatibility, rehearsal и owned probes

2026-10-20. Gate исходников `d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`
пройден: 210 native tests / 19 groups, полный build четырёх exe и проверки
реальных entry points. Это schema 9/15, supervisor v9, а не новый schema-10
health-driver. U1/U2/U3 и самообновление целиком ещё не завершены.

- Task `c53e6a24-0c23-41d8-ae7e-957561a1208c`: succeeded / exit 0.
- Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261020-051354-bd565b6c`.
- ZIP: `winbridge-20261019T160046Z-d052c6c-f53add22.zip`, SHA-256
  `5c1d4622220e6e41f5c4223523ac6502b2fdfeca6da0890ed6b323a3d15e5196`.
- Все 47 ready entries проверены по размеру/SHA-256. SHA-256 `report.json`:
  `cf90231bd186487dcebc48ae32408ed3760f7b9913dc64ea2f1a32ecddb8c441`.
- Windows free bytes before/after: 20,766,711,808 / 20,178,165,760.
  Кэши переиспользованы от первоначального владельца
  `20261017-061527-f4639971`; Windows каталоги не очищались.

Проверены четыре observer scenarios: запрет создания, сохранение PID до resume
с отказом без исполнения canary, отмена private long process при смене authority,
обычный exit/capture/tree cleanup. Также прошли прежние source/compiler/abort,
baseline/preimages, v3 runtime compatibility и frozen contracts.

Полный candidate build проверил role/build/source identities и независимые
payload files. Actual config helper прочитал точные bytes без journal/scheduler;
malformed journal canary не изменился. Actual rehearsal создал новый пустой
worker journal/instance/private IPC, подтвердил readiness и остановку, совпадение
schemas 15/9, отсутствие controller/direct commands/production state. Actual
normal Main и runtime pins также проверены. Ни один standalone JSON не принят
как durable health approval, никакая production migration/install не выполнялась.

## Восстановление контрольных exe

Первый run `20261020-050445-b9b12c09`, Task
`83cdd535-b4b5-4b30-a1ae-afb89319d7de`, завершился failed/1 до тестов:
старого schema-8 updater по сохранённому validation path уже не было.
Один ready entry проверен; SHA report:
`d90fc96e1658704deb2f33476d4f74000ff663d8795928f065bfa850b352c5a2`.
Installed worker работал исправно; его обновлённый schema-15 exe больше не
подходил и как schema-14 negative reference.

Контрольные binaries собраны из прежних проверенных ZIP в отдельной папке
`workspaces\snapshot-fixture\workflow-validation\reference-20261020-ebb7c513`:

| Binary | Source commit | Schema | SHA-256 |
| --- | --- | --- | --- |
| worker | 1f3aa9f642871cd3960d2f2b123b9738462ab4b0 | 14 | 4ef2f46dc87273a275ca8756c7b1a1c8f1ba1c82f3fc6d66c41fcaf4220bcdb0 |
| updater | b21aa28efbc7ef82f0afaedf1ed15d9b059c5613 | 8 | f9e9585d4b3c41c8758da8e4a0c83da9ae2a1c7bf6c0dc33a010d9ff204af3fb |

Task `237bef9b-e982-4741-8694-5283be7da935` succeeded/0. Shared folder с тем же
reference name содержит script/report/provenance: отдельные source SHA для обеих
сборок, result report SHA и размеры диска. Предшествующая служебная сборка
`reference-20261020-6b689191` failed из-за отсутствующего bundled-sqlite flag;
флаг исправлен, автоматический повтор разрешён пользователем. История сохранена.

После основного gate installed source d052c6c, generation
`1a3f242c-7da5-4daf-82fd-eb65566dc7cd`, active tasks 0, connection connected,
maintenance/recovery errors отсутствуют. GUI-окон не открывалось. Desktop не нужен;
установленный worker оставить работающим. Ручное обновление не запрашивается.
