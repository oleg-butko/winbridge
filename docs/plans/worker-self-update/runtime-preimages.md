# U1: сохранённый предыдущий runtime

2026-10-19. Trusted library реализована; supervisor mailbox, native gate, установка
и восстановление этого набора ещё не подключены.

После verified baseline, до source/compiler attempts, сохранить пять файлов:
known installed-set manifest и четыре exe. Каталог фиксирован:
`runtime/update/<Update ID>/previous-runtime`. Конфиги, журналы и runtime state
сохраняют прежние пути; архивирование exe не разрешает их миграцию или удаление.
Source A и binaries C по-прежнему независимы.

1. Admission проверяет exact active supervisor/owner/epoch, verified baseline,
   frozen config, фиксированный parent directory и свободное место. Без тяжёлого
   hashing/worker IPC сохраняет immutable intent с parent transition. В intent
   входят baseline digest, owner/launch, parent directory File ID, destination и
   available bytes. Pending запрещает source admission. До первого file write
   caller может подтвердить ACK и передать одноразовый Rust permit исполнителю.
2. Исполнитель закрепляет старые пять files, сверяет с baseline и живым worker
   через control v2, повторяет config/owner checks. Ошибка сохраняет pending и
   не начинает новые writes. Permit нельзя Clone/Deserialize или восстановить
   из pending после restart/drop.
3. Только fresh attempt создаёт каталог и пять файлов с create_new. Чтение bounded,
   каждая копия flush/sync + SHA/size/File ID; не использовать hardlinks или overwrite.
   Проверять authority/config/parent directory перед каждым эффектом.
4. Удерживать read pins копий до atomic receipt/parent transition. Receipt включает
   directory/File IDs и полный exact set; runtime исходники/exe остаются нетронутыми.
5. Готовый duplicate читает историю без I/O. Pending после drop/crash не возобновляет
   запись и не принимает найденные копии автоматически, даже при совпадении hashes.
   Partial/conflicting files сохраняются для диагностики; cleanup не скрывается.

Updater schema 9 хранит immutable intent/receipt. Предыдущий schema-8 updater
должен отказаться от нового журнала без записи. Сохранённые старые операции не
получают preimages автоматически; будущий installer явно требует baseline,
preimages, fresh health/approval и подтверждённый exact shutdown.

При будущем restore нужно заново проверить физический набор по receipt; сохранённый
JSON не означает текущую доступность файлов. Runtime replacement, publication
нового installed-set manifest и startup recovery marker — отдельная transaction.
Все частичные cutover states должны блокировать legacy startup до reconciliation.

Проверки: positive exact set/unchanged runtime; existing destination не перезаписан;
partial copy, changed source/parent, config/epoch/phase change, ignored journal writes,
receipt tamper, lost ACK/drop-before-driver, duplicate после completion, migration
8→9 и previous-version refusal. Synthetic fixtures не подтверждают health exe.

Десять targeted tests проходят на локальном диске: exact copies/independent IDs,
существующие/partial/late-extra/изменённые copies, fencing, ignored writes,
retention/tamper и migration 8→9. DB fixtures используют synthetic descriptors;
production execute дополнительно опрашивает настоящий bound worker до/после I/O.
/tmp может быть tmpfs меньше 6 GiB: fixtures используют /var/tmp или локальный
Windows test-build-cache. Порог не ослаблен. Перед новой сборкой измерено
32,498,028,544 bytes свободно в Debian. 510 workspace tests, Linux/Windows Clippy all-targets, fmt/generated contracts,
typecheck/scaffold passed. Windows-only pin/release test скомпилирован; его реальное
выполнение входит в следующий native gate. Supervisor v8 пока не подключён.

Review: original и copied pins удерживаются до commit; final worker observation
не скрывает позднюю замену/добавление copy — перед receipt набор и SHA проверяются
повторно. Windows pins запрещают write/delete; Unix adapter проверяет состояние
при revalidate и не заявляет обязательную блокировку файлов. Restore всегда
требует fresh physical verification независимо от сохранённой receipt.

Следом supervisor v8: fast durable admission/status и blocking copy driver,
проверка mailbox во время held worker observation, real Windows files/cancel и
previous schema-8 updater refusal. Только затем health/approval/exact shutdown.
