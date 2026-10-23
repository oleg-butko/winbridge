# U2.2: журнал подготовки независимого runtime

2026-10-22. Следующий блок после [плана замены](runtime-replacement.md) и
[закрытия U2.1](windows-state-backup-faults-gate-2026-10-22.md).
Реализованы data FSM и trusted library ledger; production IO/actor для подготовки,
replacement custody, rename driver и activation пока отсутствуют.

## Контракт и admission

Updater journal **14**, worker journal **16**. Supervisor остаётся **v15/view7**:
новой команды подготовки и UI projection пока нет. Public install отключён.

`admit_runtime_preparation` сначала читает существующую запись. Повтор возвращает
историю без нового permit. Fresh admission требует исходную owner lock/epoch,
активный supervisor launch, неизменённый root `Replacing` и живой оригинальный
`ShutdownExitGuard`. Наличие saved `Exited`/`Verified` не заменяет этот guard.

В одной SQLite transaction повторно читается полный сохранённый граф:
baseline, пять preimages, exact candidate artifact set, verified health, exact
shutdown и **Verified final backup**. Проверяются связи update/owner/epoch/launch,
loaded worker, config, runtime/transaction directory, root request/source и revision.
Plan хранит hashes этих records и supervisor spec; reader заново строит plan из
графа и сравнивает его целиком. Отдельные правдоподобные JSON не дают admission.

Worker config проверяется по actual bytes/ID; runtime/parent directory IDs и
final backup file ID/size/SHA проверяются физически. Fresh health должна иметь
compiled-current updater schema. Исходный OS guard продолжает удерживать прежний
runtime и instance lock. Кандидат пока не запускается и не перемещается.

Новый manifest формируется из точного candidate и прежней installation/scope.
У него отдельный canonical Set UUID. Четыре бинарника берутся из
`runtime/update/<id>/candidate/payload`, а manifest — из фиксированного JSON
encoding typed manifest. Назначение всех пяти файлов:
`runtime/update/<id>/prepared-runtime/<fixed-name>`.
Create/copy driver должен создать независимые файлы, сохранив candidate payload.

Диск измеряется в admission: нужно ≥6 GiB **плюс** сумма всех пяти новых копий.
Plan сохраняет измерение. Перед фактическим IO driver должен перепроверить
свободное место и pin всех источников/parent directories.

## FSM и сохранённая история

```text
Planned → DirectoryIssued → Preparing → Prepared
                            │
                            └─ для каждого fixed file: Pending → Issued → Verified
Любой незавершённый prefix → Indeterminate (без повторного issue)
```

Directory intent сохраняется до create, file intent — до каждой копии.
Receipt сверяет exact destination/size/SHA/query type и новый независимый File ID.
IDs не могут совпасть с old set, candidate sources, другими prepared files или
prepared directory. Порядок: worker, fixture, desktop, updater, manifest.
`Prepared` достигается только после всех пяти receipts. Из него можно получить
data-only `FileExchangeBatch`; это не rename/activation permission.

Три tables: `runtime_preparation_intents`, `runtime_preparation_records`,
`runtime_preparation_history`. Intent/history retained immutable, current обновляется
только с exact CAS revision и неизменяемым plan; terminal outcome immutable.
Обязательные INSERT/UPDATE проверяют ровно одну row и полный readback. Чтение
восстанавливает все causal transitions, а не только проверяет последний JSON.

Trusted callback требует исходный non-Serde permit и owner/epoch/launch/root.
Перед nonterminal/Prepared переходом повторно проверяет OS guard, backup и actual
directory/prepared files. Сначала проверяется immutable plan/causal transition,
затем используются пути callback. Переход к `Indeterminate` фиксирует тот же prefix
без требования, чтобы исчезнувшие файлы снова существовали: запись ошибки не
разрешает эффект. При новом owner старые callbacks отклоняются.

Сейчас callback — библиотечная запись, а не готовый driver. Сам driver должен
владеть create-new handles, pin/hash/readback, проверять authority до/после IO и
потреблять fresh permit. После потери процесса новый caller получает только
историю, не право продолжить копирование из saved `Issued`.

## Миграция и совместимость

Обычный updater migration добавляет14 с проверенным consistent backup13 и новым
epoch. Уже сохранённые backup/health records остаются историей. Fresh admission
не принимает health, проверенную для прежней schema13.

Readonly final exporter теперь требует **свою compiled updater schema**, а не
литерал13. Это не поддержка future schemas у старого helper. Полные следующие
actor gates должны собирать согласованные old roles и candidate schema14.
Архивы31fa/6f и их gates остаются доказательствами schema13 и не подменяются новой
версией исходников. Frozen `--backup-migration-only` пока проверяет именно12→13
на соответствующем архиве; для actual13→14/refusal нужен отдельный native gate.

## Проверки и пределы доказательств

Новые 10 synthetic tests проверяют:

- directory/file intent order, all-five completion и data-only exchange plan;
- aliases/changed receipts, неверные scope/roles/manifest bytes/space;
- JSON reopen, terminal uncertainty и строгие unknown fields;
- CAS/history/retention, ignored writes с rollback всех coupled rows;
- causal history corruption и consistent13→14 backup с сохранением old root rows.

Это не положительный actual Main admission целого графа. Такой gate требует
supervisor-команды, live original custody и нового согласованного schema14
runtime. Отдельно остаётся actual old13 executable refusal с неизменностью БД.

Локально: весь `winbridge-update` package —199 passed/0 failed/6 ignored,
Linux/Windows all-target Clippy с `update-validation` —pass. Миграционные fixtures
старых schemas обновлены: при создании старой БД они удаляют также schema14 tables;
production migration не получила `IF NOT EXISTS` и не принимает подменённую схему.
Rust fmt, Python compile и scaffold/link check проходят.

Лёгкий native mode запускает эти 10 tests на Windows с прежним approved build cache,
без installed update, desktop windows или full candidate build:

```bash
python3 scripts/validate-update-shutdown-primitives-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/<CLEAN-SCHEMA14-ARCHIVE>.zip \
  --cache-run 20261017-061527-f4639971 --runtime-preparation-only
```

Результат этого native mode фиксируется отдельно после terminal task/ready hashes;
шаблон выше не является заявлением о пройденном прогоне.

## Следующая вертикаль

1. Create-only preparation driver под fresh permit и immutable original custody
   loan; intent до directory/file IO, actual pins/readback до receipt.
2. Новый versioned supervisor contract/view и actor dispatch после commit;
   duplicate только наблюдает, status не блокируется файловым IO.
3. Actual Main→backup→preparation и faults на каждом issued/receipt, changed scope/
   collision/disk full/owner change/crash; actual13→14 preservation/refusal.
4. Только затем consuming `ShutdownExitGuard → RuntimeReplacementGuard`, rename
   ledger/driver и trusted manifest-last `FilesVerified`, затем activation/U3/U4.

Код: `apps/update/src/runtime_preparation.rs`, `runtime_preparation/tests.rs`,
`journal/runtime_preparation.rs`, `journal/runtime_preparation/tests.rs`.
