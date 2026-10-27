# Debian .204: controller 0.2.0 установлен

2026-10-27. Пользователь установил Windows worker0.2.0 и поручил обновить Debian
controller. Обновлён только user process на **192.168.44.204:1143**. Конфиг,
systemd/службы и firewall не менялись, новых Windows команд/тестов не запускалось.

## Установленный процесс

- PID **166074**, controller0.2.0, clean source
  `48c834c00c658f11feec066c9ae4650a4e6cff7a`.
- SHA256 controller:
  `bfd2694006cf587bb68725db6e6e80cd9ddbfe86e6386cbaaf5b3cd139dbe977`.
- EXE: `/home/user1/winbridge/.winbridge/controller-runtime-updates/20261027T211517.857967Z/winbridge-controller-0.2.0`.
- Config прежний: `/home/user1/winbridge/.winbridge/controller-snapshot.local.json`.
- State прежний: `/home/user1/winbridge/.winbridge/controller-session`.
- Controller generation: `c7b5de88-6d35-4db3-be62-23b48cc43287`.

Повторная сборка не нужна: использован проверенный Linux image, уже находившийся
в target/debug; при запуске сделана независимая копия. Normal controller source
между producer48c834c и main591b471 не менялся. Старый PID5312 был controller0.1.0;
его binary inode уже не совпадал с файлом в target, поэтому старый image сохранён
именно из `/proc/5312/exe` до SIGINT. Exit подтверждён перед новым запуском.
Новый процесс запущен отдельно от shell с stdin=DEVNULL и отдельным логом;
после завершения запускающего script проверен фактический PPID1 и работающий IPC.

## Результат проверки

Через новый readonly `controller-connections` подтверждены **оба реальных Debian**:

| Link | URL | Состояние | Controller generation |
| --- | --- | --- | --- |
| debian-203 (primary) | ws://192.168.44.203:1143 | connected | d3443e5e-9ddd-4f55-a525-f4105f4f3136 |
| debian-204 | ws://192.168.44.204:1143 | connected | c7b5de88-6d35-4db3-be62-23b48cc43287 |

Общий Windows worker0.2.0/source48c834c:

- Generation **946f1128-2c30-42b6-b369-8acda852a834** — не менялся при restart controller.
- Instance **local-development**; journal incarnation
  **5a8eb173-4193-4332-b00f-67e3850b2c97** — прежний.
- История: все10 прежних task IDs сохранились; activeTaskCount0.
- `maintenanceError=null`, `journalRecoveryRequired=null`; config SHA до/после совпал.
- `.203` attempts1, `.204` attempts2: второе соединение восстановилось после
  остановки старого controller. Сохранённый `.204.lastError` о reset без closing
  handshake относится к этому restart; текущее connection.error=null/connected.

Это подтверждение simultaneous deployment двух разных Debian, дополнительно к
сохранённому native gate четырёх controller-процессов. Новые execution tests не
проводились. Controller .203 не обновлялся и не перезапускался этой задачей.

При первом Info сразу после подключения `controllerProjection` ещё не успел
прийти; первоначальная проверка script остановилась на отсутствующем поле. Процесс
оставлен работающим, fresh Info затем подтвердил новый controller build и scope;
второй процесс не запускался, команды не replay-ились.

## Evidence и повторное наблюдение

Локальный каталог:

```text
/home/user1/winbridge/.winbridge/controller-runtime-updates/20261027T211517.857967Z/
```

В нём `preparation.json`, `owner.json`, `before.json`, `old-exit.json`,
`controller.log`, `connections-query.json`, `connections-after.json`, `verified.json`
и копии нового/старого EXE. Runtime evidence/config/state в Git не добавляются.
Старая копия EXE — воспроизводимый binary artifact, не копия конфигурации или БД.
Сборок не было; до операции Debian free27,032,526,848 bytes,
shared Windows volume23,085,756,416 bytes.

```bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json status
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json info
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json controller-connections \
  /home/user1/winbridge/.winbridge/controller-runtime-updates/20261027T211517.857967Z/connections-query.json
```

Последняя команда привязана к сохранённой instance/incarnation. Если journal
сменится, отдельно получить новый context из fresh Info; не менять исходные
envelopes принятых задач. Старые receipts проверок других builds не дают readiness
новому build автоматически; выполненная здесь проверка только readonly.

## Ручной запуск после остановки и rollback controller

При свободном порте1143 и отсутствующем другом controller с этим config:

```bash
/home/user1/winbridge/.winbridge/controller-runtime-updates/20261027T211517.857967Z/winbridge-controller-0.2.0 \
  --config /home/user1/winbridge/.winbridge/controller-snapshot.local.json serve
```

Если нужен rollback именно Debian controller: сначала проверить владельца порта
1143 и его command/executable по owner.json, штатно завершить этот controller
SIGINT и подтвердить exit. Затем точная команда запуска сохранённого старого image:

```bash
/home/user1/winbridge/.winbridge/controller-runtime-updates/20261027T211517.857967Z/previous-winbridge-controller \
  --config /home/user1/winbridge/.winbridge/controller-snapshot.local.json serve
```

Старый image SHA256:
`6cb8146a07434832892b9276726eca4e866baebfb92331cdc58c725d0ec2bc52`.
Это rollback controller, не Windows worker/journal; новые диагностические wrappers
при нём недоступны. Не запускать оба controller на одном порте/IPC одновременно.
