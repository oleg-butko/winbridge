# Lifetime supervisor27/view19 и новый Main27

2026-10-28, Debian .204, ветка `feat/updater-lifetime-runtime-204`.
Продолжение [source V6 runtime](lifetime-runtime-2026-10-28.md), base90fc5b4.

## Подключение к supervisor

- Internal27 `EnterActivationLifetime` потребляет исходный controller-verified
  process guard, готовит fresh Issued и вызывает единственный private V6 send.
  Повтор Enter возвращает состояние прежней попытки. Сохранённая история и
  отсутствие native original owner не дают повторного admission/send.
- Отдельный `ObserveActivationLifetime` выполняет fresh bounded normal IPC,
  удерживая тот же guard. Он не пишет в private stream и не открывает новый
  startup window. Даже после потери ответа/истечения verification deadline
  действительный original owner может записать поздний факт того же entry.
  При ошибке guard возвращается в actor, старое наблюдение не выдаётся за новое.
- Worker resident mode остаётся supervised quarantine: tasks/import/admin
  закрыты. Actor не выполняет root commit, maintenance release или keeper-role
  conversion. Drop исходного guard всё ещё закрывает Job.
- View19 различает durable phase/entry receipt, resident process/liveness,
  actual original custody и normal observation с отдельным observedAtMs.
  Перед положительным resident view проверяется current continuation graph.
  Cold journal view — NotResident, без выдуманных PID/live mode/custody.
  Views1–18 сохраняют прежний JSON shape, новые поля в них отсутствуют.
- `ActivationTiming` показывает original-owner/graph+Job/privateReady/normalIPC/
  durable confirmation и отдельные этапы readonly reconciliation. Diagnostic
  budget на чтение не становится новым разрешением на запуск.

## Проверки source

963 workspace/update-validation tests passed,17 ignored; новые cold mailbox,
strict protocol/view и original-scope completion regressions включены. После
финальных уточнений проверок root/epoch/nonce выполняется focused repeat.
Linux/MSVC workspace all-target Clippy с update-validation/`-D warnings`,
TypeScript/UI build прошли. Полные локальные logs:
`.winbridge/lifetime-actor-xxkz63ch/`.

Python lifetime reader имеет4 synthetic tests/31 negative mutations. Прежние
Main25/Main26 profiles frozen; новый Main27 явно требует worker19/updater22 и
новый proof kind. Controller/service reader regressions и multi-controller
inputs passed. Native proof не выводится из cross-compilation/synthetic metadata.

## Native Main27

Подготовлены отдельный ignored Main27 Rust test, `-ActivationLifetimeGate`,
runner/readonly reader и строгий independent audit. Producer и distinct V6 target
должны быть clean source19/22 с built UI. Main25/26 не переименовываются в Main27.
Используются approved stable `K:\_proj-26\winbridge\runtime-test-204`, порт1144 и
отдельный временный Debian controller; installed runtime не подменяется.

Сценарий теряет только supervisor dispatch ACK, наблюдает тот же entry, дожидается
истечения исходных240s, снова наблюдает quarantine через actual normal IPC,
проверяет same child/Job/authority, closed submit, duplicate/no replay и cold
NotResident после owned cleanup. **Private Ready loss не инжектируется**:
`privateReadyLossTested=false`. Это отдельная оставшаяся fault-проверка.

Preflight task `a5bad2ac-12d9-4470-bd8f-c17a8576332e` succeeded: свободно Windows
24,863,621,120 bytes, в runtime204 нет worker EXE/process, только idle reservation
metadata. Рабочий worker PID2532 по точному installed path оставлен. Debian перед
проверками25,597,980,672 bytes; cache cleanup не требовался. Повторно используется
native cache `20261017-061527-f4639971`, Windows directories не удаляются.

**Actual Main27 ещё не выполнен.** Результат и exact pins будут добавлены после
одного запуска и независимого перечитывания. Рабочий worker оставить запущенным;
desktop и тестовые окна не нужны.

## Следом

После native подтверждения: private Ready uncertainty fault gate; consuming
permanent keeper role с освобождением file/config pins и updater authority;
final root commit/scoped release; next-update delegation/max2 keepers/two
generations и оставшиеся U2–U4. Public install/activation остаются выключены.
