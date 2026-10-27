# Actor25/view17 и атомарное завершение локальной проверки

2026-10-26. Продолжение [service4](service-v4-2026-10-26.md).
Actor25/view17 и атомарный local-proof/root transition реализованы в source;
full Windows Main25 пока не выполнен. Installed worker0.1.1/source59249e6 не заменяется.

## Реализованный вертикальный путь

`SupervisorHandle::activation_service(start)` использует только contract25.
`None` означает volatile dispatch ACK/busy, а не durable admission или успех.
Старые actions/contracts1–24 и JSON views1–16 сохраняются; view17 добавляет
optional `activationService`. Rust DTO экспортированы в TypeScript.

FilesActor хранит независимые native состояния StartingService, Serviced,
ServiceUnconfirmed и ServiceFailed. Только Written может передать исходный
process guard в один blocking driver slot. Actor назначает один command key;
повторный start после dispatch наблюдает прежний результат и не создаёт второй
permit/send/child. Cold actor с сохранённым Confirmed не получает native owner.
Completion сверяет ожидаемый intent, command key, physical StateOwned и local
receipt; ошибочные результаты сохраняют custody и не становятся Ready.

После независимого normal-IPC probe сохраняется service Confirmed. Затем
`UpdateJournal::finish_local_activation` под тем же оригинальным guard, current
root/owner/supervisor и точным сохранённым service record атомарно фиксирует:

- activation revision7/`LocalVerified` с тем же compatibility receipt;
- root `VerifyingLocal` → существующий `AwaitingController`, revision+1 и
  единственную transition `LocalHealthVerified` с key, связанным с service command.

Это **не root Committed**, controller handshake, lifetime transfer или maintenance
release. Raw lifecycle API по-прежнему не принимает LocalHealthVerified. Ошибка
commit остаётся неопределённым исходом без replay. Metadata proof может быть
поздним фактом, не продлением original startup grant. Worker epoch не меняется.

View17 читает все parent/service records в одном SQLite snapshot и накладывает
только реально resident process observations. Cold Confirmed показывает
NotResident; processAlive остаётся null. Положительный live status требует
Serviced, совпадающих saved/local receipts, живого исходного процесса, отсутствия
driver error и root AwaitingController. Writer/state projections используют
тот же снимок, не отдельный повторный аудит.

## Review: IPC liveness и log flock

В service4 найден и исправлен пробел: после Ready receiver следил только за
parent/channel. Теперь он одновременно следит за завершением normal IPC task.
Если listener/service остановился, receiver завершает verification, а не остаётся
живым до hold timeout. `wait_until_stopped` cancel-safe, запоминает join outcome;
последующий close не poll-ит завершённый JoinHandle повторно и не скрывает ошибку.
Новый real-IPC regression проверяет monitor → повтор observation → close,
сохранение lock до close и освобождение после drain.

Во время полного workspace повторился WouldBlock диагностического Unix logger.
[Отдельный review/fix cca3b97](review-actor25-2026-10-26.md) воспроизвёл retained
open-description mechanism и добавил explicit unlock при завершении writer.
5 focused tests и независимый OS experiment прошли. Worker/update locks не
менялись; Windows logger не менялся. Первоначальный failed workspace не считается
успешным; полный повтор прошёл833/0, 17 ignored.

## Проверки

Уже пройдены: updater library246/0 (4 ignored); mailbox v25 с настоящим local
supervisor1/0, включая неправильные version/scope/token/worker-command и cold
no-adoption; worker transfer/startup/local IPC/monitor15/0; TypeScript typecheck.
Mailbox сначала использовал старый `target/debug/winbridge-update` и получил EOF.
Это исправлено сборкой helper до запуска: `cargo build -p winbridge-update
--features update-validation --locked`. Рабочий Windows worker не затрагивался.

SQL tests проверяют rollback coupled proof при ignored root/history writes,
единственный переход AwaitingController и отказ stale/repeated root. Это lifecycle
metadata fixtures, не физическое разрешение перейти к новому worker.
Protocol tests проверяют strict request/view без permission fields; старые views
проверяются на отсутствие activationService. После исправления logger полный
workspace: **833 passed / 0 failed / 17 ignored**. Финальное review выделило
shared writer projection, чтобы view17 не показывал cold writer рядом с live
service; после этого updater library246/0 (4 ignored) повторён. Clippy workspace/all-targets с feature winbridge-worker/update-validation
и `-D warnings` прошёл для Linux и x86_64-pc-windows-msvc.
Generated contracts `--check`, TypeScript typecheck, fmt и scaffold295 passed.
Cross-check не является исполнением нового кода на Windows.

Source logs: `.winbridge/update-preparation/actor25-{update-unit,helper-build,
mailbox,worker-unit,workspace,workspace-repeat,check-linux,check-windows,
clippy-linux,clippy-windows,contracts,contracts-check,typecheck,scaffold}.log`.
Preflight task180a088d-f861-41f7-b28c-78c4606bd43e succeeded/0, evidence
`.winbridge/update-preparation/actor25-preflight-20261027T002033Z-42df3e0c/`:
Debian16,599,867,392 bytes free, K:33,053,761,536 bytes free. Worker SHA536d8637…,
generation936909c6… прежние. Controller5103 работает192.168.44.203:1143.

## Следом full Main25: что обязательно сделать до Windows запуска

Нельзя подменить configured-but-disconnected controller доказательством с
controllerUrl=None. В полноценном сценарии старый normal Main тоже увидит frozen
controller URL и откроет сеть. Поэтому постоянный exe slot нужен уже здесь,
до controller activation phase. Не запускать network exe из нового hash path.

1. Подготовить reusable activation test runtime с эксклюзивным lease/owner marker,
   отдельный от production. Рассмотреть совместимость с существующим
   `runtime-test/winbridge-worker.exe`: firewall уже использует этот постоянный
   путь, но `Open-TestWorkerRuntime` сейчас разрешает только binary/marker/lease,
   не installation manifest/config/update-state. Нельзя просто добавить файлы в
   этот слот в обход его контракта.
2. Объявить фактический stable runtime path ДО baseline/preparation/manifest и
   suspended launch. Не переносить работающий exe, не применять junction или
   менять сохранённые path/manifest contracts после admission. Legacy Main tests
   используют root/runtime; им нужен явный fixture runtime resolver с default
   прежним поведением, а не подмена production root.
3. Config/state/reports остаются изолированными по run. Если reused slot содержит
   прошлый inactive installed-set/update-state, после проверки lease/marker и
   actual process/Job exit сохранять его целиком в архив прежнего run, с locator/
   hashes/file identities. Не удалять validation directories/config/journals.
   Не считать архивные records живой custody и не переписывать их source paths.
   При неизвестном владельце/процессе/файле отказать, не очищать и не ослаблять guards.
4. Для network preflight использовать отдельный controller1144; основной5103 не
   заменять. До startup admission получить firewall reachability proof для точного
   stable exe, разрешённого пользователем; первоначальный permission wait900s
   отделён от original startup window240s. Firewall rules не менять.
5. Main25 должен немедленно dispatch service после writer confirmation, до тяжёлых
   hashes/history/legacy audits. `activation_state::execute_inner` и
   `activation_writer::execute` сейчас имеют fast path до writer, но writer execute
   затем сразу делает полный audit. Добавить отдельный service branch до этого
   аудита, сохранив старый Main24 proof и семантику.
6. Проверить actual same child/lock/generation, один epoch+1, preserved task/output/
   user queue pause/accepted shutdown, blocked new roots/queue Set, настоящий
   normal IPC и configured URL без новой controller session. Дубликаты/lost ACK/
   cold observe не дают replay. Затем подтвердить actual process/Job exit.
7. Добавить независимый verifier нового proof и negative mutations; actual retained
   updater19→20/old19 refusal на consistent copies остаются обязательны.

Предыдущие gates15429a4/16+14 и1f653b2/13+13 не доказывают actor25, атомарный
local root transition или новый monitor. Их source pins/history сохранены.
Только после положительного full Main25 продолжать controller session/lifetime/
commit/release, remaining U2/U3/U4 и A4.3. Public install/activation/controller/
desktop capabilities по-прежнему выключены. Ручное обновление сейчас не требуется.
