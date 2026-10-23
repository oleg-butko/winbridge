# U2.2: замена runtime под сохранённым владением

2026-10-22. План следующего блока после [U2.1](final-state-backup.md).
Это проектирование; file replacement, activation и public install не реализованы.
U2.1 final backup fault gate завершён успешно; file/FSM primitives также прошли
native gate. [Handoff 2026-10-22](resume-2026-10-22.md) отделяет эти результаты от
ещё не реализованных binding/ledger/custody handoff/driver/activation.
В продолжении начата [preparation library schema14](runtime-preparation.md):
binding/ledger/model добавлены; IO/actor/full native admission пока впереди.

## Что уже можно переиспользовать

`RuntimeBaselineReceipt` фиксирует старый installed set, `RuntimePreimagesReceipt`
содержит пять независимых copies (manifest и четыре роли), candidate artifacts/
health receipt фиксируют новый комплект. Shutdown хранит acceptance и настоящий
`ShutdownExitGuard`, final backup — отдельную попытку и проверенную копию SQLite.
Updater owner lock/epoch и installation scope уже отделены от worker journal.
Все обычные startup/effect paths проверяют maintenance до открытия execution.

Supervisor выполняется из проверенной копии
`runtime/update/<id>/supervisor/winbridge-update.exe`, а не из заменяемого
`runtime/winbridge-update.exe`. Это существующий handoff в `supervisor::prepare_bound`;
его собственный image и control capability не включаются в заменяемый комплект.
Следующий worker/desktop/fixture/updater сохраняют постоянные имена в runtime.

## Нужное изменение владения

Текущий `ShutdownExitGuard::require` сверяет **весь старый pinned runtime**.
Эти пять handles запрещают write/delete; повторный immutable loan подходит backup,
но не замене. Просто drop guard или ослабление всех pins недопустимы.

Нужен отдельный consuming handoff `ShutdownExitGuard → RuntimeReplacementGuard`.
Оригинальные pinned exited process и locked worker state остаются в новом guard,
как и identity runtime/transaction directories. От old-set read pins освобождаются
только после admission replacement, verified final backup и отсутствия других loans.
`Custody` должен атомарно забрать единственный Arc (`try_unwrap`); при занятых loans
сохранить старую custody и отказать, не ждать с mutex на IO. Ни deserialization,
ни historical Exited/Verified не дают такого handoff.

После передачи status должен различать shutdown custody и replacement custody.
Старый `require(old runtime unchanged)` нельзя вызывать после первой законной
замены: новый guard сверяет immutable source identities/receipts и допустимый
текущий per-file progress. Это расширение модели владения, а не удаление проверок.

## Fresh admission и журнал

Одна новая операция привязывается к exact update ID/root revision, owner/epoch,
supervisor launch, исходному shutdown, final-backup intent/report/file evidence,
old baseline/preimages, candidate artifacts/health, config и directory File IDs.
Все данные перечитываются в одной admission transaction и сверяются с live guard.
Новая FSM не меняет семантику старых supervisor versions и view JSON.

Fresh admission возвращает non-Serde one-use permit. Повтор pending или terminal
запроса возвращает историю и не запускает файловый driver. ACK следует за commit;
сбой после ACK/потеря результата не разрешают replay. Один active driver slot
по-прежнему исключает одновременные backup/replace/abort effects.

Предполагаемые записи: immutable replacement intent; prepared file evidence;
current progress; append-only transition history; per-file move intents/receipts;
final manifest receipt. Нужны schema migration/old-reader refusal и чтение полной
causal history. Перед выбором следующего номера schema проверить текущий код.
Не предполагать, что helper, собранный с13, умеет читать future updater schema.
При изменении schema native old-role/candidate archives должны объявлять совместимые
версии; старые12/13 references остаются отдельными неизменными fixtures.

## Файловая последовательность

1. Проверить guard, config, parent identities, candidate/preimages/backup SHA и
   запас локального диска ≥6GiB плюс требуемые копии. Ничего не менять при blockers.
2. Создать все новые runtime files и новый installed manifest create-only в
   локальной transaction directory. Пиновать source payload; независимо проверить
   новые bytes/hash/descriptor и File IDs. Candidate payload не расходовать move.
3. После durable admission передать OS custody. До первой замены получить handles
   всех нужных файлов с rename/delete access и сверить exact old identities/hash.
   Если acquisition не удался, сохранить blocker, без частично выполненной замены.
4. Для каждого fixed filename: durable retire intent → move exact old file в
   отдельный create-only retained directory → identity/hash receipt. Затем durable
   place intent → move подготовленного new file в освободившееся имя → receipt.
   Нет overwrite неизвестного назначения и copy/delete fallback при cross-volume.
5. Новый installed manifest публикуется **последним**, отдельным intent/receipt.
   Должны совпасть все четыре actual files и manifest, installation/scope и candidate
   descriptors. Лишь atomic final receipt + trusted FilesVerified переводят root
   в VerifyingLocal. Raw lifecycle этого права не получает.
6. Supervisor сохраняет replacement custody до передачи activation driver.
   Наличие файлов или manifest не разрешает обычный запуск/scheduler.

Вместо path-based rename после освобождения pin предпочтителен owned file-handle
adapter: revalidate exact source после acquisition и переименовать тот же handle.
На Windows `SetFileInformationByHandle(FileRenameInfo)` поддерживает rename с
`ReplaceIfExists=false`, который отказывает при занятом назначении.
См. [FILE_RENAME_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)
и [SetFileInformationByHandle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle).
Режим доступа/sharing и lifetime надо проверить отдельным native test до подключения
к driver. Не использовать POSIX replace/delete flags для обхода занятых exe.

Набор файлов не становится атомарным от одного rename. Его целостность обеспечивают
maintenance barrier, manifest-last, полный per-file журнал и startup refusal.
После crash между retire/place может временно отсутствовать exe; после любой
неизвестной операции — ManualIntervention, readonly reconciliation и точные paths/
File IDs, без автоматического restore/retry. Previous copies и displaced originals
сохраняются. Rollback после нового owner/effects запрещён без отдельного контракта.

## Ограничения первого driver и проверка плана

Первый gate работает с isolated runtime и незапущенным desktop. Успешное открытие
exe для rename не заменяет desktop ACK/exact exit; production coordination с UI
добавляется в U3 до public installation. Работающие fixture/tasks уже исключаются
existing drain/quiesce. Нельзя закрывать чужой процесс по одному имени/PID.

Review выявил три точки, которые нельзя обойти «простым копированием»: удерживаемые
old-set pins, обновление manifest вместе с exe и дальнейшая передача instance lock
новому worker. Последняя относится activation: старую lock нельзя отпускать до
готового guarded new process, а новый journal owner/effects не должны появляться
до согласованного verification. Для неё нужен следующий отдельный контракт.

Порядок реализации U2.2:

1. Typed immutable binding/FSM и negative tests для mixed/changed sets, skipped
   transitions, CAS/epoch, duplicate/reopen и crash без реконструкции authority.
2. Durable ledger/migration/read snapshot; raw FilesVerified остаётся закрытым.
3. Consuming OS custody handoff и bounded owned rename adapter, independent native
   checks: wrong File ID, occupied destination, locked exe, same-volume and handles.
4. Consuming driver, intent до эффекта, full readback до receipt, responsive mailbox
   и отдельная projection. Подключить только новый versioned internal contract.
5. Actual Main → shutdown → final backup → replacement gate; locked file/disk full/
   collision и crash после каждого intent/receipt. Проверять no restart, old/new
   exact bytes, retained originals, startup refusal и повтор без новых эффектов.
6. Review готового блока и только затем activation/local health/controller
   confirmation. Public CLI/MCP/status/policy и U3/U4 остаются отдельными шагами.

## Начатый data-only фундамент

`apps/update/src/runtime_exchange.rs` реализует пер-файловый FSM и batch validation:
Prepared → RetireIssued → Retired → PlaceIssued → Installed. Неизвестный исход
сохраняет exact prefix в Indeterminate и не возвращается к issue. Rename receipt
обязан сохранить source File ID/hash/bytes/type; копия с тем же содержимым не
считается тем же перенесённым файлом. Пять fixed roles имеют один update/root,
независимые identities и последовательный порядок с manifest последним.
`advance` сверяет immutable spec и один exact causal transition; snapshot/reopen
не предоставляют fresh permit. Новые типы не подключены к IO/ledger/supervisor.

Шесть pure tests проверяют skipped/duplicate steps, copied/rebound files,
uncertainty/reopen, scope/format, manifest-last и mixed/aliased sets. Дополнительно
проверено отклонение unknown fields: unit variants Serde игнорировали лишнее поле,
поэтому Prepared/RetireIssued заданы empty struct variants с тем же JSON tag.
Они не являются OS proof; native replacement/actor/activation ещё отсутствуют.

## Owned file-handle adapter подготовлен

Windows-only `crates/platform/src/runtime_files.rs` держит read+DELETE handle,
запрещает другие write/delete opens и сверяет source ID/bytes/SHA. Переименование
использует тот же file handle и абсолютный путь, все ancestor directories удерживаются
no-delete handles от root до parent. `RootDirectory=NULL`, `ReplaceIfExists=false`;
volume и parent File ID проверяются до вызова.
Один объект допускает один OS attempt, включая отказ при collision; повторный
вызов не выполняет эффект. После ошибки readback исход неопределён, не retry.
Platform primitive не является installation permission; updater пока его не вызывает.

Три Windows tests пройдены: реальный rename/same ID и отсутствие повторного
move, occupied destination/no overwrite, wrong ID/hash/parent и held no-delete
read pin. Cross-check/all-target Clippy и native execution проходят.
В существующие primitives PS/Python runners добавлен отдельный режим
`--runtime-file-moves-only` / `-RuntimeFileMovesOnly`, без old binary reference и
без full desktop/candidate build. Используются generated fixtures и прежний approved
native test cache. Шаблон команды для будущего обоснованного повтора после новых изменений:

```bash
python3 scripts/validate-update-shutdown-primitives-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/<NEW-CLEAN-ARCHIVE>.zip \
  --cache-run 20261017-061527-f4639971 --runtime-file-moves-only
```

Сам шаблон не является новым прогоном. Native gate adapter уже пройден и описан
ниже; повтор без изменений не нужен. Durable binding/ledger и передача original
custody всё ещё не реализованы.

### История native findings 2026-10-22

Первый SDK run20261022-202619-3d8f4020/task3fc7949a завершился2/3, overall failed:
relative RootDirectory/leaf вызвал ERROR_INVALID_PARAMETER87 в positive rename.
Реализация заменена на NULL RootDirectory + absolute UTF-16 path; удерживаются
все его ancestor handles, final parent ID перепроверяется после их acquisition.
Это отдельный новый вызов новой тестовой операции, не fallback/retry прежнего.
Collision test теперь требует ERROR_FILE_EXISTS80/ALREADY_EXISTS183, чтобы любой
отказ неправильных параметров не выдавался за доказательство no-overwrite.
Positive test использует кириллицу/non-BMP filename и проверяет write denial
после move. Combined SDK mode также проверит6 data FSM/batch tests (всего9).
Cross-check/Clippy и исправленный native gate прошли; точные результаты ниже.

### Native primitive/model gate завершён

[SDK и FSM gate 2026-10-22](windows-runtime-file-moves-gate-2026-10-22.md) пройден9/0,
source6f060f0, run20261022-203417-4db1d2dc. Это закрывает primitive/model native
проверку, но не whole replacement. Новая plan_replacement_manifest отдельно
проверена локально: fixed four roles/target/source, inherited installation/scope,
новый canonical UUID без alias прежнего, без публикации/authority. Она не включена
в перечисленный native producer. Далее bind к saved proofs и durable preparation
до consuming OS custody; actual installation не включена.

### Manifest plan локально проверен

`installed_set::plan_replacement_manifest` формирует только данные: сохраняет
installation/scope прежнего known set, создаёт новый canonical Set ID, сверяет
candidate fingerprint, fixed role order, независимые image IDs и exact target
source commit/manifest SHA. UUID-alias прежнего Set ID не считается новым поколением.
Manifest не публикуется, approval/OS custody не создаются. Два pure tests и обе
all-target Clippy прошли. Эта функция отсутствует в native producer6f060f0;
следующий consuming driver должен связать её вход с saved candidate/health/backup
в одной admission transaction и сохранить intent до создания файлов.
