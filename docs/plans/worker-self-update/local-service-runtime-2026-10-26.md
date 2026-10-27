# Локальный Worker из переданного состояния

2026-10-26. Продолжение [admission/transfer](service-admission-2026-10-26.md)
и [плана service activation](local-service-activation.md).

## Что реализовано

`apps/worker/src/startup.rs` отделяет получение state/authority от общей
инициализации Worker. Обычный startup по-прежнему получает lock, открывает Store
и начинает authority; активация потребляет sealed `PreparedServiceJournal`.
Второй Store/lock, миграция и дополнительный epoch при этом не создаются.
Переиспользуются recovery leases/tasks/imports/applications, policy и domain
witness. Worker хранит полный `LockedShutdownState` до закрытия соединения.
Generation берётся из прежнего physical StateOwned, а не генерируется заново.

`LocalService::prepare` требует actual loaded `RuntimeGuard`, frozen config
SHA/identity, прежние instance/state и checkpoint исходного parent/graph/window.
Config читается через общий `load_checked_config`; runtime/config pins проверяются
до инициализации. На Windows config удерживается открытым без разрешения записи/
удаления. При ошибке сохраняются исходный контейнер либо частично созданный
Worker с lock; failure не выдаёт повторный Fresh permit.

Для startup reconciliation нужны записи метаданных, хотя maintenance уже Quiesced.
Временный `ServiceStartupFence` разрешён только sealed consuming transfer. Каждая
write transaction проверяет schema17, точные writer receipt/header/epoch,
accepted shutdown, неизменный lease и первоначальный deadline. Root admission,
новая authority, dispatch и administration остаются закрыты. После initialization
scope удаляется; обычная блокировка записей возвращается до открытия IPC.
Неизвестный результат initialization не превращается в автоматический повтор.

`LocalService::start` использует общий `serve_inner` и настоящий local IPC, но
не запускает controller/control registration или scheduling/cleanup. Readiness
приходит после bind. До и после bind проверяются custody, runtime guard, точные
header/receipt/shutdown/lease и оставшийся исходный срок. `ActivationJournal`
запоминает monotonic deadline при open; private receiver ограничивает его прежним
handoff Instant. Ни transfer, ни initialization не начинают новый timeout.

Через обычный handler доступны прежние task/output/history и другие observations.
Любые mutations, включая отдельный QueueControl Set, отклоняются. Info и queue/
work/resource observations помечены readonly; legacy mutations/maintenance/
snapshot/workflow admission выключены. Frozen controller URL сохранён, но
соединение Disconnected, session/last-confirmed отсутствуют и показана причина.
Это отдельная локальная проверка обновления, не режим journal recovery inspection.

`RunningLocalService::close` останавливает и join-ит IPC и ждёт завершения
владельцев обработчиков, включая spawn_blocking. Нельзя подтвердить clean close,
пока они удерживают Worker. При неподтверждённом закрытии error сохраняет custody.
Drop аварийно останавливает сервис; clean handoff подтверждает только close.

## Проверки и точные границы

Debian: **820 passed / 0 failed / 17 ignored** в workspace; focused worker group
**13/0**, Python verifier **6/0**. Source logs:
`.winbridge/update-preparation/service-runtime-{unit,workspace,clippy-linux,clippy-windows,scaffold}.log`.
При финальном review добавлены повторная monotonic проверка после внешнего
checkpoint и единая read transaction для pre/post-bind scope check. После них
focused activation tests: **21/0**. Linux/MSVC workspace all-target validation Clippy на финальном source прошли;
fmt, scaffold292, diff-check и Python verifier6 также прошли. Native результаты
фиксируются отдельно.

Новые regression checks: настоящий IPC читает прежнюю cancelled task и UTF-8
output; сохраняются generation/authority/lock и user queue pause; отказываются
Submit, Queue Set и чужой server PID. Controller URL остаётся заданным, но
network session не создаётся. Bind conflict не считается readiness. Ошибки
config/checkpoint/deadline сохраняют state owner. Временная scope не открывает
root/admin/new epoch и повторно проверяет header на каждой transaction;
после окончания scope обычная write freeze снова обязательна.

Это **fixtures с synthetic replacement graph/process metadata**. Test-only
`prepare_fixture` обходит загрузку installed-set RuntimeGuard, чтобы проверять
сам общий Worker и OS IPC. Production API требует реальный RuntimeGuard.
Проверки не доказывают original-guard admission полного upgrade, приватный
service4 transport, parent validation normal IPC, actor25 или Main25.

Сценарий `--service-admission-only` расширен: 13 updater SQL/migration +13 worker
transfer/startup/IPC tests. Counts читаются из pinned ZIP, включая Tokio tests;
точные argv, logs и compiled commit/manifest обязательны. Fixture test exe
использует только local IPC, без TCP/controller и UI; firewall не меняется.
В Windows понадобится только установленный worker для запуска сценария.

Preflight task `2766aba0-0c01-48d6-9315-c443f936b736` succeeded/0; Debian свободно
16,722,239,488 bytes, K: 26,902,265,856 bytes. Evidence:
`.winbridge/update-preparation/service-runtime-preflight-20261026T204159Z-11cc286e/`.
Installed worker **0.1.1/source59249e6**, SHA
`536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712`, generation
`936909c6-d148-40cd-abf6-a0d996935994`, не заменяется.

## Windows fixture gate завершён

Run **20261026-192027-4bb92924**, task
**563f5d39-7d3b-4d43-8b2c-3796f15a0882** — succeeded/0:
**13 updater +13 worker tests**, без failed/ignored. Настоящие named pipe,
readonly task/output и queue observations проверены на Windows. Это library
fixtures с synthetic replacement graph, **не full same-child Main25/service4**.

| Pin | Значение |
| --- | --- |
| Producer | `1f653b217e29ec52a5a4723b4a3c0824d26ac183` |
| Archive | `winbridge-20261026T232016Z-1f653b2-fca7edae.zip`, 878 files |
| ZIP SHA-256 | `c3db99cc99512236324cd47d3d7dbd2f3aeee7951d99451ebf5d99c3022843af` |
| Manifest SHA-256 | `575da1a8f29392011d2a7706c1c0454fb64f21d9d408fb6565a75728d06e6216` |
| report.json | `2e79e94d894e1e393b59dc497e60e1f368bd6b48f3626d3c2c853e92b1d1df85` |
| service-history-native.log | `f29ecde6a719564812a1e48c32cdd3675dff26fffbbfb12b1e9c6212a6b6a165` |
| service-transfer-native.log | `8483be6b4587acaed4efd0ec1bbf6e7d64e7bf92204254c7388baa17aa51ff01` |

Все три ready artifacts проверены повторным чтением; compiled commit/manifest
обоих test binaries совпадают с чистым archive. Independent proof:
`.winbridge/workflow-validation/20261026-192027-4bb92924/service-runtime-verified.json`.
Рядом original request/fenced-request, terminal task-report, worker-proof и
отдельная readonly observation. Shared report:
`K:\VMs\shared\WinBridge\workflow-validation\20261026-192027-4bb92924`.

Observer session3784 завершён/exit0; log
`.winbridge/update-preparation/service-runtime-native.log`. Отдельный readonly
recheck того же terminal run прошёл, без submit/rebuild; log
`.winbridge/update-preparation/service-runtime-recheck.log`. Команда:

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261026T232016Z-1f653b2-fca7edae.zip \
  --service-admission-only \
  --verify-service-admission-run 20261026-192027-4bb92924
```

Compiler cache переиспользован из original owner20261017-061527-f4639971,
report SHA `22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`;
тесты исполнялись заново. Windows K: свободно до/после 32,790,589,440 /
32,810,373,120 bytes, Debian после 16,686,354,432 bytes. Очистка не выполнялась.
Установленный worker source59249e6/generation936909c6/incarnation5a8eb173
остался прежним, active0, maintenance/recovery errors=null. Controller5103
оставлен работать. Тестовые IPC/processes завершены, окон не было; desktop не
нужен, установленный worker оставить запущенным. Обновление runtime не требуется.

## Как продолжить после потери контекста

1. Прочитать этот checkpoint, handoff и прежний admission checkpoint. Сверить Git
   и Info работающей пары. Controller PID5103 на192.168.44.203:1143 уже запущен;
   сначала проверить его, не запускать второй. Desktop не нужен.
2. Native fixture gate выше уже завершён; не повторять его без новых изменений.
   При необходимости прочитать сохранённый terminal run readonly указанной
   командой. В будущих gate request сохранять до submit; при неопределённости
   читать сохранённую task, не создавать повторную операцию.
3. Добавить private service4 marker/DTO/receiver отдельно от frozen v1/2/3. После
   writer ACK receiver должен потребить тот же ActivationJournal/Fresh, проверить
   актуальный updater graph и оригинальный parent, вызвать prepare/start. Нельзя
   получать service grant из serialized ledger или продлевать startup window.
4. Подключить consuming parent send к `ActivationServicePreparation`, прочитать
   настоящий normal IPC с проверкой server PID, build/manifest/config/context/
   generation/schema/history. Частный Ready — только сигнал, не LocalVerified.
   Сохранить единственную immutable receipt/late facts, без повторного send.
5. Actor25/view17, isolated full Main25, actual retained19→20/refusal. Только затем
   новая controller session, lifetime transfer, commit/scoped maintenance release,
   remaining U2/U3/U4 и возврат к A4.3.

Source остаётся updater20/worker17/supervisor24/view16/private2+writer3.
Service4/actor25/Main25 ещё не реализованы. Public install и advertised activation
по-прежнему выключены. Native fixture proof не разрешает обновлять рабочий runtime.
