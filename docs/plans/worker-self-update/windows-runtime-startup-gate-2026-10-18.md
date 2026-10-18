# U1: known-runtime startup и lifetime — Windows gate

2026-10-18. Candidate source `d3c9ed65a7ab7785d7150e8b0969728b209628ac`.
Run `20261018-160143-62159cbf`, Task
`7d5526c6-d8be-4e09-81f0-7795f5cc8d07`: succeeded, exit 0.
Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261018-160143-62159cbf`.

Пройдены 159 native tests / 17 groups, полный Rust/native desktop build,
четыре actual artifact queries, exact-file inspection и положительный startup
через настоящий worker Main. Все 38 файлов `report.ready.json` сверены по bytes/
SHA-256. SHA-256 `report.json`:
`e28c657970639a3398f59a3176d942548a484bb03b8f0b0e8980b300507e4cdd`.
Архив `winbridge-20261018T200132Z-d3c9ed6-05ca5fa1.zip`, SHA-256
`72ff8dfcc62dde1c3dea1cb40b55c570c503f231702c2ffca79dba2a4a25c238`.

`worker-runtime-startup-proof.json` подтверждает actual `verify-runtime`: PID 14188,
compiled descriptor и образ соответствуют известному fixture manifest. Команда
не создала worker state и не меняла updater DB. Missing/corrupt/foreign manifest,
changed image, fabricated worker descriptor и alternate config отклонены; `serve`
не открыл worker journal. Три Rust integration tests также прошли на Windows.
Known manifest создан явно только для fixture, production bootstrap не выполнялся.

## Дополнительная проверка времени жизни

После публикации основного ready marker отдельная Task
`1c5bfea2-b598-4782-9bbe-a70184375472` succeeded/0. Использовались те же binaries
без новой сборки. Script commit `cbcaa31`, SHA-256
`67c29f1afc8605b1a9c6e1b56d4992a4c130d187ce004774ffdc6239d6e79c65`.

Настоящий `serve`, PID 22832, получил уникальный instance ID; controller отключён,
recipes пусты, direct commands запрещены, worker state до запуска отсутствовал.
Info подтвердил expected build и отсутствие задач. Write-open manifest и четырёх
exe отклонялся во время работы. После forced stop именно своего idle PID все пять
файлов открылись для записи, bytes/SHA остались прежними, updater DB не менялась.
Это проверка lifetime guard, не graceful shutdown, migration rehearsal или activation.
Создан только собственный fixture worker journal; рабочий journal не использовался.

Основной ready marker не изменялся. Late proof имеет отдельный
`worker-runtime-lifetime-proof.json.ready.json`; его bytes/SHA тоже проверены.
SHA-256 `worker-runtime-lifetime-proof.json`:
`a5708f972fd9543e7d72dc283b58075593a5f8d45a9ded1d7035326d1ea03f86`.
Проверка включена в следующие combined runs; её runner integration проверена
по syntax/mock admission, полный combined run с этой дополнительной строкой
не повторялся. Отдельный actual execution выше подтверждён.

## Измерения и границы

Readonly verification debug binaries заняла 9,435 ms; lifetime scenario — 10,620 ms.
Будущий activation driver должен учитывать время file verification до появления
IPC; обычные 5 секунд connection timeout не являются достаточным startup deadline.
Нужны bounded phase/status observations, без ложного вывода о rollback по тайм-ауту.

Windows free before/after основного gate: 22,141,669,376 / 22,341,156,864 bytes.
Debian перед native gate: 32,649,232,384 bytes. Cache owner
`20261017-061527-f4639971` переиспользован; Windows validation directories не удалялись.

После обоих прогонов installed worker source `1f3aa9f`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние; idle 0, errors none, recovery coverage
complete. Окон не было, тестовый worker остановлен. Desktop можно оставить закрытым,
установленный worker — работающим.

Startup/pin block завершён. Authenticated live identity/control, durable old-runtime
baseline до SourceApply, exact shutdown, runtime transaction и activation впереди.
Весь U1 остаётся открытым; U2–U3/public install/bootstrap ещё не готовы, A4 на паузе.
