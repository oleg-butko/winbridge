# Retained Main29: отдельный copy-only gate 22→23

**Native retained22→23 passed,2026-10-29:** run20261029-202033-a2132bf8,
task18e1625b-d2bc-44d8-9b0b-949446ab685c succeeded0, producerfcc49dd.
Actual Windows copy/backup/old22 init+status refusal/cold20 passed;4 result files
verified twice,34 negative audits +142 reference audits. Native task выполнен
один раз. Первоначальный reader перепутал view.build (historical compiler record)
с producer BuildIdentity; исправлен со сверкой всей projection с pinned Main29,
7/24 portable tests passed. Перезапуск Windows не нужен. Исходный Python runner
завершён1 на ошибке reader; standalone fixed audit passed, это не worker failure.
[Результат и продолжение](finalization-migration-2026-10-29.md).Source19/23, root остаётся
AwaitingController/local7/lifetime Confirmed; новых finalization rows0, live grants0.
Installed0.2.0/source48/gen946 прежний/active0/errors=null; worker оставить,
desktop можно закрыть, окон не было. Далее physical factory/V8/worker Active/
keeper/two generations/U2–U4; public install off.

2026-10-29. [Ledger23 source](finalization-ledger-2026-10-29.md) проверен на Linux
и компиляцией MSVC. Следующая проверка — миграция consistent copy настоящего
[Main29](main29-native-2026-10-29.md), без нового worker, UI или установки.
Ниже — сценарий и подготовительные проверки; terminal результат приведён выше
и в следующем разделе.

## Frozen reference

- Main29 run20261029-155738-d847446d/task2cd973db-9769-4695-b601-9d75344ab0ad.
- Producer7ca206f: `winbridge-20261029T155611Z-7ca206f-895de5c2.zip`,
  SHA6bf175b1d57ebbcce5fec5ce0eafe4a3c004bc287aaa89ed46310a9ef54fdd6d.
- Target944a39c: `winbridge-20261029T143435Z-944a39c-3580f462.zip`,
  SHA1095726437ab9118aab7009286d5a5e7fdb4c9e493857f810c60676bd96d2715.
- Actual old22 updater в runtime archive6557b962-1699-4f61-950f-b3e287b560f5,
  exe SHAaa3b7d84461082692decd434faec2f0c568105047098562ccb9fff86ab0b22b3.
- Reference derivation сначала запускает полный Main29 independent reader,
  связывает archive location/file identity с target image и сохраняет хеши
  root/local7/writer/service/controller/lifetime. Не брать failed первый Main29.

Новый fixed profile Finalization22To23 требует source SDK23 и оба reference
archives SDK22. Старые profiles19→20/20→21/21→22 сохраняют прежние границы; запуск
их на новом SDK отвергается до чтения input. Нельзя менять user_version или
переписывать старые receipts, чтобы представить другой переход.

## Что выполнит сценарий

1. Проверит image/manifest и semantic rows исходного retained22 journal.
2. SQLite Backup API создаст отдельную consistent copy. Только она мигрирует23
   под owner lock с проверенным backup22 и увеличением epoch.
3. Сверит полные старые таблицы и все шесть typed records с source/backup/hash
   reference. Все три новые finalization tables должны оставаться пустыми.
4. Новый cold owner получит только Existing replacement data. Lifetime receiver
   со старым context22 откажет. Cold view20 должен быть NotResident, без process,
   liveness, observation или custody. Новая finalization intent не создаётся.
5. Запустит настоящий archived updater22 с init/status на копии23. Оба откажут
   UnsupportedSchema, main/WAL bytes копии и исходной базы сохранятся.
6. Independent reader дважды сверит ready files, точный выбранный native test,
   source/reference/task pins и34 негативные мутации. Булевы флаги сами по себе
   не заменяют сверку records, fingerprints, cold view и native execution.

Рабочий runtime, config, worker journal и оба controller1143 не меняются.
Нет supervisor/worker launch, UI окон, нового network executable или firewall
правил. Нужен только установленный worker как транспорт одного fenced task.
Native cache — workflow-validation/20261017-061527-f4639971/test-build-cache.
Windows directories не очищаются. Report содержит до/после free bytes и timings.

## Подготовка и команды

6 portable observer tests/33 negative audits passed. Общий набор четырёх migration
readers:23 tests passed; MSVC Clippy updater tests/update-validation/-D warnings
passed. Fmt/diff проверены. Обычный unittest discover не нашёл файлы с дефисами;
результат0 не принят за проверку: readers явно загружены через importlib/unittest.

Actual Windows preflight taskdce79220-47ba-437c-a822-7fd31299781b succeeded0:
parser errors0 для master SHA23f89ed0efcc02012082fbcfffc374e5d5c506f218d9c2aad356eb1643cd9f81
и wrapper SHA2941deabafa86dcf6ab8884c586750ac691bdb15817d2608c401de1647e8a9b8,
old22 image SHA прежний, available17765359616 bytes. Installed worker0.2.0/source48/
gen946f1128/context5a8eb173 прежний/active0/errors=null. Desktop не нужен.
Evidence `.winbridge/review-2026-10-29/finalization-migration-*`.

После clean commit/export (подставить новый archive path):

```bash
python3 scripts/run-update-finalization-migration-native.py \
  --source-archive /mnt/hgfs/shared/WinBridge/sources/NEW-CLEAN-23.zip \
  --reference-source-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261029T155611Z-7ca206f-895de5c2.zip \
  --reference-target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261029T143435Z-944a39c-3580f462.zip \
  --reference-run-info .winbridge/update-preparation/main29-204-20261029-155738-d847446d/run-info.json \
  --controller-config .winbridge/controller-snapshot.local.json \
  --cache-run 20261017-061527-f4639971
```

Сохранить run-info/acceptance и наблюдать тот же Task ID до terminal. Observation
 timeout не даёт права повторно submit. Затем readonly
`verify-update-finalization-migration-native.py` с теми же archives и новым
run-info, без Windows replay. Отдельно обновить этот статус/handoff/roadmap.

Этот gate не проверяет effect V8/Active/root finalization: immutable old22 root
остаётся AwaitingController, local7/lifetime Confirmed остаются historical data.
После migration gate продолжить physical factory/V8/Active/keeper/two generations
и U2–U4; public install остаётся off.

## Terminal evidence и исправление reader

Run-info `.winbridge/update-preparation/finalization-migration-20261029-202033-a2132bf8/run-info.json`,
shared `K:\VMs\shared\WinBridge\workflow-validation\20261029-202033-a2132bf8\result`.
Producerfcc49dd07a7a0fc681eadac04e609dd5cd29a955,
ZIP `winbridge-20261029T201912Z-fcc49dd-36dd97a6.zip`,
archive SHA9c69630ffe168e584dfc4b38f01a3d3f2fd5c9d91c24ce846c3833dda81e4d97,
manifest SHA2d7a419646fdbe4e95d9c5a0da56db6a803a087b4f9190c7c92d3726c72d0f37.
Report SHA29528c0601471022b143260632c5e3b493c140b43751770bed34d461005787dc,
proof SHAf32531bc786486c04f2e8c1234e6ed8a9c015b32638dc7eece6322e3ed053923.

Один actual native test прошёл за10.72s; compile+command74727ms, в том числе
сборка около60s. Все прежние таблицы сохраняют fingerprint
`a56f5d5a3e6c6d602385020762dcad12eb45dcc16c896058da25248626bce4b8`.
Source22/backup22 ownerEpoch2 → copy23 epoch3 → cold observation epoch4;
lastSequence17, installation79431863-4952-48fa-8200-af7968e9b5db и root
90bd1e09-013f-4233-b33c-342fd01428f4 прежние. Gate rows0; source/copy main/WAL
bytes не меняются от old22 init/status refusal. Cold view20 NotResident содержит
только historical key f10dbc94-c41d-45ab-b2a0-7273c6f4a45d, без live process/
custody/observation. В native сценарии не создавался ни один новый worker.

Первый independent reader неверно сравнил view.build с producer BuildIdentity.
Это исторический UpdateBuildRecord. Исправленный reader получает отдельный
proof context из уже SHA-pinned и полностью audited original Main29, сравнивает
все cold projection fields кроме нового observedAt, а producer identity по-прежнему
проверяет в proof.build. Reference/input/request/result файлы не переписывались;
новая source сборка и Windows replay не требуются. Добавлена регрессия с разными
compiler record/producer identity и34-я отрицательная мутация. При исправлении
также устранено затенение fenced context локальным proof context; before/after
actual build/gen/context независимо совпадают. Первоначальные failed reader logs
сохранены, не выдаются за successful runner exit. Generic old profiles не получают
новых параметров; fixed new profile требует original projection context.

Portable tests7 в новом reader/24 в четырёх readers passed. Первый unittest
 discover дал0 из-за имён с дефисами; проверен только explicit importlib suite,
0 не посчитан как pass. Fresh installed Info после проверки: source48c834c/
gen946f1128/context5a8eb173 прежние, active0/errors=null.
Windows available17654849536→17868443648 bytes; final Debian15987560448/shared
17924509696 bytes. Windows validation directories не удалялись.

Original runner322089/tool69943 terminal exit1 на reader exception; native task
succeeded0 и raw report.passedtrue. Fixed standalone audit exit0 —
`.winbridge/review-2026-10-29/finalization-migration-fixed-audit-v2.json`.
Затем повторить только readonly verifier, сравнить JSON и записать reader
commit/source SHA в create-only checkpoint. Terminal task18e1625b не submit снова.
Рабочий worker оставить, desktop можно закрыть; тестовых окон не было.

Следующий эффект по плану не сдан: physical factory/V8/Active/keeper/U2–U4.
Этот результат закрывает только retained22→23 migration и её cold/read boundaries.
