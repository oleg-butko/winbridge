# Windows gate: live runtime identity и query retirement

2026-10-18. Clean source `256b1820e5e0b296efa43cc304c2e90cb516665b`,
run `20261018-204941-965a8934`, Task `1276429a-ff9b-46c3-81ed-673de1a85395`:
**succeeded / exit 0, 165 native tests / 19 groups**. Updater schema 7,
worker schema 15 в candidate. Schema-8 baseline в этот архив не входила.

Архив `winbridge-20261019T004930Z-256b182-b0bbdddd.zip`, SHA-256
`eb9930e600a7f8788dfc117d3d8e5b76ab5808ae61e22ed3180e98d2993ac658`.
Все 43 файла из report.ready.json проверены по размеру и SHA-256.
Report SHA: `d57e5d937afe745fea91ed1af1eacc0d5b92ab1dd13abdedaaafddd1a3139b55`.
Windows free before/after: 21,688,569,856 / 21,313,851,392 bytes.
Повторно использован build cache `20261017-061527-f4639971`; результаты тестов
получены заново. Windows validation directories не удалялись.

Пройдено:

- Worker control v2 и protocol DTO/negative version/binding/config tests.
- Supervisor остаётся доступным при held readonly query, отклоняет настоящий
  duplicate и принимает три следующих запроса подряд без паузы после ответа.
- Positive copied-process fixture: настоящий Worker actor, guard собственного
  PID/build/File ID и supervisor IPC; config changes after hash отклонены,
  Store доступен, нет lease/slot/source/install effects. Это test harness;
  другие role descriptors синтетические, candidate health не подтверждают.
- Полная сборка Rust/desktop, queries четырёх exe и точная проверка payload files.
- Actual worker Main: readonly startup identity, ошибочные manifest/descriptor/
  config отвергнуты до worker journal.
- Actual normal serve: PID 14116, пять files pinned до forced owned idle stop,
  после exit handles освобождены и bytes/updater DB сохранены. 11,027 ms,
  accepted tasks 0. Это проверка lifetime, не graceful shutdown или activation.

Startup proof SHA:
`eb944e6c6e757eb88ef3ad5aca6547ed96e11d9b90123f54f698ddfba161aac5`.
Lifetime proof SHA:
`4d4ba094bd0f9cd48f13483bbe4bd86a26a5cacd62055d157e3499ecd5cca64b`.
Оба proof входят в общий ready marker этого combined pipeline.

Предыдущие два failed runs сохранены: первый обнаружил неверную single try_lock
проверку при обычном scheduler contention; второй — публикацию ответа до удаления
JoinSet entry. Исправления проверены заново этим gate.

Installed worker остался source 1f3aa9f, build 1792228466051801600,
generation `994671b4-e10c-469c-8515-b2aa71a4e3dc`: свежий Info показывает connected,
activeTaskCount 0 и error null. Установленные exe/config/journals не заменялись.
Desktop не требовался, окон не было; установленный worker нужно оставить работающим.
U1 целиком, baseline/preimages/health/shutdown/activation ещё не завершены.
