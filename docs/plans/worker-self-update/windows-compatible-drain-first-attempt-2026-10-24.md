# Schema17: миграция прошла, первый Main gate завершился ошибкой

2026-10-24. Producer `cdea687`, fixture `efccfbe`.
Run **20261024-003723-8bd37767**, task
**73081bc6-ea78-45e1-a79e-cc5f3d5a27c3** — terminal **failed/exit1**.
Не повторять этот namespace и не объявлять полный gate пройденным.

## Что подтверждено

До Main выполнены **32 native tests /9 groups**: support model/SQL12,
contract2, policy6, frozen views1, mailbox1, compatible model/SQL7,
contract1, mailbox1 и actual migration1. Старый runtime set собран и проверен.
Все23 ready files прочитаны дважды и совпали; migration proof независимо
проверен,19 negative mutations отвергнуты.

Actual16→17 выполнена на новой consistent SQLite Backup API копии истории
успешного support run `20261023-233438-99674bc1`. Owner epoch2→3,
installation/scope/lastSequence13, все прежние строки и support5/history6
сохранены. Migration backup16 проверен; gateRows0, stale admission запрещён.
Actual старый16 exe отказал init/status17 без изменения DB/nonempty WAL.
Original reference image/manifest/DB/WAL сохранены.

## Почему полный gate не прошёл

Main успел записать `05-artifacts.json`, затем `candidate_health(true)`
получил `unexpected end of file`. `06-health.json` и support proof не появились.
Новый production drain entry ещё не был вызван. Не считать отсутствие полного
proof доказательством исхода незавершённого health attempt; его не replay.

Readonly diagnostic task **5794c6a0-4d3e-4d41-897f-093f649d18c6** завершилась
succeeded/0. Она прочитала только stdout/stderr supervisor этого isolated run.
В stderr: `thread 'main' (3788) has overflowed its stack`.
Это ошибка тестовой новой версии, не установленного worker и не сети/firewall.

Большая async mailbox-машина передавалась по значению в `Runtime::block_on`
на главном потоке exe. Исправление размещает её на heap через отдельный
non-inlined constructor `supervisor::run` до polling; весь прежний обработчик
находится в `run_inner`. Не увеличиваются глобальные stack settings и не
ослабляются startup/maintenance/owner checks. Окончательное доказательство
исправления — новый actual Main gate, одного cross-check недостаточно.

Local supervisor tests8/0 (2 ignored) и обе all-target Clippy update/worker с
update-validation/-D warnings прошли. Логи в `.winbridge/update-preparation/`:
`supervisor-stack-owner-tests.log`, `supervisor-stack-linux-clippy.log`,
`supervisor-stack-windows-clippy.log`. Изменение не вводит новую schema/action.

## Привязки и сохранённые данные

- Source `cdea6877d70465d2f84d82663f4c7f82919fc397`, ZIP
  `winbridge-20261024T003701Z-cdea687-cc873843.zip`,
  SHA256 `f140a22ac0e27949467f8504693d3f76e91869968bb781556e6805b9bdb775f7`,
  manifest `c2130e1123901d2a324afca872b3f89fcf336d58ccf5c5f82a6b28717379a9bc`.
- Target `efccfbe73930763f6925cbd3d26efc15d6713a37`, ZIP
  `winbridge-20261024T003539Z-efccfbe-b7a614ff.zip`,
  SHA256 `06056b8bee56c7b49c72abc48d24a619324a9d75f4981d767ed2bc1cc62c4a67`,
  manifest `94832d6dfe58dc4268a3dd4b3e77865378a538dbb7762374673822d1d3e7ef84`.
- Report SHA256 `b269e89f766996407309ca14f7fdfc7867f628e3f4700718bda91e368f745fe4`.
- Migration proof SHA256 `051a92cfe9a6942c783607fe4cf97ecf694802f31816942d8d2a6cc5d6eb4432`.
- Shared: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-003723-8bd37767`.
- Full bounded verification summary:
  `.winbridge/update-preparation/compatible-drain-first-failed-verified.json`.
- Observer log: `.winbridge/update-preparation/compatible-drain-native.log`.
- Diagnostic request/task ID:
  `.winbridge/update-preparation/compatible-health-diagnostic-task.json`;
  не печатать private gate input/control files и не копировать их в shared.

Windows free bytes30,325,956,608→29,446,848,512. Ничего не очищалось.
Installed d052, generation8f8d1fb9-e5b1-4984-a17e-d067db46268e,
active0/maintenanceError=null/recoveryRequired=null/coverage complete.
UI-окон не было; desktop можно закрыть, установленный worker оставить.

## Продолжение

Локальные проверки stack fix, clean commit/export нового producer и новый
isolated run с теми же fixture17/reference16/cache. Предыдущую папку сохранить.
Проверить33 tests/10 groups, полный Main proof,38+19 negative mutations.
После gate — review и activation из original resident custody, затем U2 faults,
U3/U4. Public install выключен, полный updater ещё не готов.
