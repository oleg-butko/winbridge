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
и физический guard проверяются будущим consuming driver. Текущие worker capability,
CLI, updater schema12 и legacy RPC не расширены; executable backup пока отсутствует.


`apps/update/src/state_backup.rs` добавляет data FSM и immutable canonical binding
к полной Exited-записи. Подготовлены пять synthetic tests: порядок PID/resume,
reopen без повторного issue, terminal uncertainty, cleanup до запуска/без resume,
подмена отчёта и alias input/backup/source. Обе Clippy проверки прошли. Таблицы13,
экспорт SQLite и consuming driver ещё не реализованы; JSON не создаёт разрешений.
