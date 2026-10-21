# U2.1: финальный consistent backup worker

2026-10-21. Следующий блок после [U1 exact shutdown](windows-shutdown-supplemental-gate-2026-10-21.md).
План реализации; backup driver ещё отсутствует. Public update остаётся отключённым.

Backup должен выполняться после exact exit и acquisition исходной instance lock,
до первого изменения runtime. Тогда копия содержит shutdown acceptance и последние
callback writes. Ранее сохранённые executable preimages не заменяют backup БД.
Root Replacing пока означает только достигнутую границу; наличие backup обязательно
перед любым file intent. FilesVerified/local health/controller confirmation уже
закрыты от raw lifecycle; новые drivers откроют их только с собственными receipts.

## Порядок реализации

1. Уточнить typed input/report/FSM и отдельный capability marker старого worker.
   Readonly SQLite export должен поддерживаться **до** admission shutdown; worker,
   который не умеет экспортировать собственную schema, нельзя останавливать ради
   неподдерживаемого продолжения. Эта проверка добавляется к existing preflight.
2. Добавить updater schema13: immutable backup intent, attempt/PID/resume/delivery
   history и receipt. Migration создаёт consistent backup, fences прежнего owner,
   не добавляет physical custody к историческому Exited. Старый updater12 должен
   отказываться от13 без изменений. Worker schema16 для readonly export не меняется;
   compatibility metadata явно объявляет поддерживаемую updater schema.
3. Admission потребляет живой ShutdownExitGuard из supervisor custody и проверяет
   exact update/attempt/owner/epoch, root revision, shutdown receipt, original state
   File IDs, config и old image. JSON-поля и сохранённый Exited не создают permit.
   Во время blocking IO mailbox остаётся доступен; guard принадлежит driver,
   после retirement возвращается supervisor. Второй driver в это время запрещён.
4. Выполнить bounded owned helper через pinned **старый** worker image: durable
   attempt/PID до resume, identity/Job/output capture и подтверждённое завершение.
   Helper открывает оригинальный Store readonly, без Worker construction, recovery
   latch, owner claim, миграции, scheduler и controller. Output path определяется
   локальной policy и update/attempt, находится в локальном update directory.
5. Использовать SQLite backup API (`Store::archive_read_only` — существующий
   фундамент), create-only destination и запрет его подмены во время записи.
   Не копировать открытые DB/WAL/shm как обычные файлы. Неполный результат сохранять,
   не перезаписывать и не считать готовым. Оригинальные config/journal не менять.
6. Report связывает точные input SHA, old helper descriptor/image/process,
   source state/authority/schema, исходную acceptance и backup SHA/size/File ID.
   Проверить integrity, application ID, schema, incarnation/owner epoch, lease и
   shutdown receipt в полученной копии; затем повторно source/guard/pins.
7. Только после подтверждённого helper cleanup и полной проверки атомарно сохранить
   receipt. Повтор наблюдает историю. Missing ACK/result не запускает helper второй
   раз; partial/unknown outcome удерживает barrier и запрещает replacement.
   После потери supervisor новый процесс не восстанавливает guard из backup/JSON.

## Проверки и review плана

Pure tests: duplicate/CAS/epoch, failed writes/readback, полная12→13 migration с
сохранением shutdown history и отказ old12. Для реальных schema12 references доступны
old-set/None runtime завершённых Windows gates; manifest SHA есть в verified proof.
Native: exact old helper, WAL data, output/meta/history/dedup сохранены, source bytes
и recovery markers неизменны, existing/partial destination не затронут, wrong input/
file identity/schema/receipt отклонены, PID-before-resume и crash/cleanup проверены.

Разделение обязанностей: helper знает worker schema и экспортирует данные; updater
владеет attempt/receipt, OS guard и решением продолжать. Helper report не даёт права
заменять файлы. Backup не является разрешением восстановить более старую БД после
claim нового owner или новых effects. Restore/rollback потребует отдельного контракта.
Сначала native gate этого блока, затем U2.2 file replacement и activation;
installed runtime не использовать как fixture. Ручной bootstrap остаётся в U4.


## Начатый слой контракта

В `crates/protocol/src/update/state_backup.rs` добавлены authority fingerprint,
backup file evidence и typed report v1. `validate_envelope` проверяет сохранение
incarnation/epoch/fingerprint, связь acceptance/lease/state, helper host/session,
отдельную file identity, число страниц/размер и отсутствие source/runtime effects.
Синтетический contract test отклоняет 23 искажения и неизвестное поле processExited.
Это проверка данных; filesystem scope, сохранённая попытка, kernel process/cleanup
и физический guard проверяются будущим consuming driver. На этапе contract-only
worker capability, CLI и legacy RPC не расширялись;
executable backup пока отсутствует. Состояние schema13 описано ниже.


`apps/update/src/state_backup.rs` добавляет data FSM и immutable canonical binding
к полной Exited-записи. Подготовлены пять synthetic tests: порядок PID/resume,
reopen без повторного issue, terminal uncertainty, cleanup до запуска/без resume,
подмена отчёта и alias input/backup/source. Обе Clippy проверки прошли.
JSON не создаёт разрешений; export CLI и consuming driver ещё не реализованы.


## Durable outbox schema13

Добавлены immutable intent, current record с CAS и полная append-only history.
Каждое чтение сверяет все последовательные переходы FSM, исходный shutdown,
health/config/schema, supervisor/owner/epoch и policy directory. Fresh admission
требует живой `ShutdownExitGuard`; повтор возвращает сохранённую запись без нового
permit. Запись очередного перехода также требует original custody, неизменный
Replacing root и File ID каталога. Это API хранения данных: оно не запускает helper
и не даёт разрешения менять runtime по одному JSON report.

Migration12→13 использует существующие consistent backup, owner lock и epoch fence.
История не наделяется custody; старые databases не получают выдуманную попытку
backup. Synthetic downgrade fixtures удаляют новые дочерние таблицы до старых.
Шесть новых SQLite tests покрывают CAS/history, reopen/uncertainty, skipped/changed
transitions, ignored writes/atomic rollback и owner-locked migration. Полный native
migration/refusal с actual schema12 shutdown history остаётся частью U2.1 gate.


Review существующего `Store::archive_read_only` выявил отпускание create-new handle
до открытия SQLite. Исправлено: handle удерживается до integrity/authority check,
Windows delete sharing запрещён, path File ID сверяется, SQLite открывается без
CREATE. Три actual SQLite tests проверяют WAL rows и неизменность source bytes,
existing/partial destination, stale authority и подмену пути. Это локальная
проверка; Windows no-delete sharing ещё требует native gate.

Проверки 2026-10-21: все группы workspace покрыты — 603 passed в сумме, 16 ignored
native/helper tests. Общий `--no-fail-fast` выявил только гонку короткого Unix probe;
после fixture-синхронизации повтор всей updater lib прошёл97/0. Ранее исправлена
гонка copied rehearsal image; повтор группы в общем прогоне прошёл5/0. Production
launch/identity checks не ослаблены. Clippy update+worker all-targets: Debian со
всеми features и Windows с `update-validation` прошли. Bundled SQLite Windows
cross-build требует отсутствующий `lib.exe`; его C build проверяется на Windows,
не объявляется подтверждённым cross-check. Fmt/diff прошли. Logs находятся в
`.winbridge/update-preparation/state-backup-ledger-*` и не включаются в Git.


Readonly helper inspection добавлена отдельным API: один SQLite snapshot связывает
сохранённую attempt/PID/input, текущие owner/epoch/active update/Replacing revision
и Active supervisor. Доступны только ResumeGranted/Running; ранее завершённая или
неопределённая попытка не получает нового разрешения. Текущий same-schema U2 reader
требует updater13 без migration; поддержку future migration нельзя предполагать
по наличию этого reader. JSON остаётся данными; физические pins/current self PID и
live owner обязан проверить будущий exporter. Новые две проверки прошли, targeted
state-backup suite13/0 и обе updater all-target Clippy прошли. Положительный полный
joined export и native file-sharing ещё не проверены; CLI пока отсутствует.


## Old-helper exporter реализован, положительный native gate впереди

`--export-update-state INPUT SHA` — отдельный ранний Main branch. Marker contract1
включён в worker; normal Worker startup не вызывается. `prepare_export` определяет
runtime по текущему executable, сверяет self image/build/PID, saved input/grant,
единственный readonly updater snapshot и текущий live supervisor. Удерживаются
pins source state, runtime, обоих configs, input и существующего policy parent.
Выходной путь получается из проверенной policy и attempt; CLI не принимает его.

Store открывается readonly собственной schema16. Проверяются source acceptance и
authority; SQLite backup API сохраняет final WAL data. Копия повторно читается,
сверяются page size/count/application ID, incarnation/epoch/fingerprint и receipt;
SHA/size/File ID считаются streaming с удержанием файла. Worker schema16 использует
application ID0; exporter сохраняет его, не вводит новую marker/migration. Отчёт
не разрешает replacement/restore. Ошибка сохраняет partial destination.

Две actual CLI negative проверки и три archive tests прошли, обе all-target Clippy
update+worker прошли. CLI success и native file sharing пока не проверены: для них
нужен consuming supervisor driver с durable PID-before-resume и OS custody. Legacy
v14 shutdown не превращается в backup-capable install; новый driver обязан проверять
marker до shutdown admission. Public update пока закрыт.


## Consuming library подготовлена

`capture_final_state` принимает fresh permit и ссылку на original OS exit guard.
Input создаётся create-only; Issued сохраняется до process create, actual PID/grant
до resume, cleanup до приёма report. Проверяются actual backup bytes/hash/File ID,
original guard и pinned config/input; Verified сохраняется лишь после них. Ошибка
не повторяет процесс; сохраняет terminal uncertainty, если original owner ещё
вправе писать. Runtime replacement/restore функция не выполняет.

Driver и helper используют общий streaming `StateBackupFilePin`; данные SQLite не
ограничены лимитом executable image. Дополнительно проверены parent File ID до
input creation и исходный File ID при writer→read-pin handoff. Targeted suite14/0,
CLI2/0 и обе update+worker all-target Clippy прошли. Consuming library ещё не
вызывается supervisor и не выполнила положительный native export. Следующий шаг —
v15/view7 actor, затем полный Windows gate; старые v1–14 ответы остаются прежними.


## Supervisor v15/view7 подключён, native gate ожидается

v15 вводит отдельную остановку с обязательной проверкой export marker до admission,
запуск final backup и readonly observation. v1–14 сохраняют прежние действия и JSON.
Fresh admission выполняется blocking и коммитится до ACK; затем диспетчеризуется
один driver. Повтор читает прежнюю attempt и не создаёт helper. Mailbox продолжает
отвечать во время копирования.

Для custody используется immutable Arc того же original guard: supervisor хранит
основную ссылку, driver удерживает loan до cleanup, mutex не блокирует IO/status.
Это эквивалентно возврату guard после retirement по правам/сроку удержания; новый
kernel guard из rows/JSON не создаётся. View7 отдельно показывает сохранённый backup
и physical shutdown custody; исторический Verified не выдаёт разрешения на effects.

610 workspace tests, обе update+worker all-target Clippy, generated TS/typecheck и
fmt прошли. Actual mailbox проверяет отказ без custody/Quiesced и отсутствие effects,
v14 JSON остаётся прежним. Native success/backup faults/migration12→13 и old12 refusal
ещё не выполнены. Public installation не включена.


Подготовлен `-FinalStateBackupGate` / `--final-state-backup-gate`: один actual Main
v15 stop → retained custody → external owned parent → export/Verified, с настоящей
завершённой task/output/dedup history и сравнением всех SQLite rows/DB/WAL bytes.
Private capability fixture остаётся только в локальном `.private` файле и не входит
в whitelist shared reports. Она доступна лишь в validation feature; normal build
не получает этого API. Обе Clippy, cross-check fixture, UI build и ZIP tests прошли;
сам native gate пока не запускался. Target C для него должен быть schema13.


Native attempts 2026-10-21: first stopped because fixture wrote runs.log in source
(correct guard refusal); seed moved into target/history-seed cache. Second passed
20 native tests and reached actual copy Verified/row/hash/custody checks, but fixture
rejected SQLite's absent→empty WAL sidecar creation. Normalization now permits only
that zero-data distinction; main DB/nonempty WAL and all logical rows remain exact.
A negative fingerprint test detects changed frames/main data. Both attempts retained.

Separate lightweight --backup-migration-only fixture is prepared for consistent
clone of actual proven12 shutdown DB, full retained history migration13 and old12
CLI refusal. Cross-check/Clippy pass; native migration and final corrected Main
success/duplicate are pending. Source original DB/config/runtime never used as a
migration destination. No production behavior/security boundary changed by fixes.
