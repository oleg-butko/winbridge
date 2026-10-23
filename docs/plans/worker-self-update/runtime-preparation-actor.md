# U2.2: create-only preparation driver и supervisor v16

2026-10-22. Продолжение [schema14 library](runtime-preparation.md).
Код driver/actor добавлен; [сквозной Windows gate пройден](windows-runtime-preparation-main-gate-2026-10-22.md):
40 native tests/10 groups,16 ready files twice,22 negative proof mutations.
**Runtime rename, activation и public installation по-прежнему отключены.**

## Что выполняется

`prepare_runtime_files` потребляет исходный non-Serde preparation permit и
заимствует оригинальный `ShutdownExitGuard`. Saved `Planned` не создаёт такой permit.
Original exited process/state/instance lock и старые пять runtime pins остаются
у supervisor; copy driver не передаёт и не освобождает их для replacement.

Перед IO driver проверяет текущий owner/epoch/launch/root/record и frozen local
policy, pin worker/updater config, exact final backup и пять previous-runtime
copies. Candidate payload и цепочки каталогов удерживаются kernel handles.
`PinnedDirectory` удерживает все ancestors; на Windows они запрещают delete/rename,
на development Unix revalidation обнаруживает смену пути. Нужны local/safe paths и
исходные directory File IDs. Место повторно проверяется перед первым create:
6 GiB плюс все новые copies, без автоматической очистки.

DirectoryIssued сохраняется до `create_dir`, затем фактический directory ID — до
первой копии. Для каждого fixed filename сохраняется `Issued`, выполняется
create-only copy, сверяются size/SHA/type/новый независимый ID, сохраняется receipt.
Pinned источники и все предыдущие copies удерживаются до финального commit.
Manifest создаётся последним из фиксированных bytes typed plan. Перед receipt
проверяется полный ожидаемый состав каталога; источники и old runtime не расходуются.

Поток копирования вызывает короткий authority/root fence перед create и каждой
записью ≤64 KiB. Перед/после каждого файла проверяются полный causal graph и pins.
`Pinned::copy_new_checked` не предоставляет permission: его caller обязан иметь
сохранённый intent. Прежний `copy_new` использует тот же IO-код без нового callback.

При ошибке driver читает последнюю сохранённую revision и пытается зафиксировать
тот же prefix как `Indeterminate`. Partial output и занятые destinations сохраняются;
нет очистки, overwrite, повторного create или реконструкции permit. Если callback
не может сохранить uncertainty из-за смены authority, исторический unfinished
record отображается `Indeterminate` при отсутствии текущего driver.

## Actor и контракт

Новые internal actions — `PrepareRuntimeFiles`/`ObserveRuntimeFiles`, только
**supervisor v16 / view8**. Worker schema16/updater schema14 прежние. Raw caller не
передаёт пути, SHA или receipts. Admission идёт в blocking work slot; mutation ACK
отправляется после commit и перед dispatch. Потерянный ACK не отменяет принятую
подготовку и не разрешает второй driver. Повтор видит тот же attempt.

Mailbox отвечает на status отдельно от IO. Admit/execute loans удерживают тот же
Arc guard; оригинальная custody остаётся после retirement. View8 содержит фазу,
attempt/revision, каталог, число готовых файлов, issued filename, total bytes и
причину. Исторический Prepared не означает наличие OS custody или permission
следующего шага. Оно показывается отдельно от candidate_shutdown physical custody.

v1–15 не получают нового смысла; их view не сериализует `runtimePreparation`.
Новый typed client проверяет exact v16/view8/update/launch и не делает fallback.
Generated TypeScript должен соответствовать protocol; новые DTO не регистрируются
как public MCP/CLI update capability.

## Проверки и следующие действия

Добавлены реальные local filesystem tests для пяти copies/manifest с сохранением
источников, occupied destination, failed directory/file receipt и stream-fence
failure после 64 KiB. Проверяется retained issued prefix и отказ повторного исполнения.
Эти tests используют synthetic observer, не являются actual supervisor admission.
Также добавлены directory identity/rebinding, projection и strict v16 request tests.

На Debian `/tmp` расположен на небольшом tmpfs. Filesystem fixture размещается в
основном ignored `target/runtime-preparation-test-temp` и удаляется своим TempDir;
production disk threshold не ослаблен. На Windows используется fresh validation
TEMP на K:, не shared и не reused test state.

Перед признанием нового блока завершённым нужны:

1. Local workspace636/0,16 ignored; после path review16 preparation/2 directory
   tests, обе all-target Clippy, TS/contracts/typecheck/UI build passed. Native
   fixture также прошёл Windows all-target Clippy. Это не native execution.
2. Actual Main→source/build/health/drain/shutdown→final backup→PrepareRuntimeFiles,
   включая быстрый status, immutable attempt при дубликате, старые views без поля,
   пять actual ID/hash/size receipts, прежние runtime/source/DB bytes и startup refusal.
3. Faults после directory/file intents и receipts: occupied destination, stream
   fence, callback failure, supervisor crash; history не даёт новую custody/dispatch.
4. Actual old13 updater отказ новой14 БД без изменения; старый13 helper не считается
   совместимым с14. Согласованные old-role/candidate fixtures собирать из14 archives.
5. Consuming replacement guard, rename ledger/driver и manifest-last trusted
   FilesVerified, затем activation/controller confirmation/U3/U4.

Новый `-RuntimePreparationGate` / `--runtime-preparation-gate` подготовлен в
`test-update-health-actor.ps1` / `validate-update-health-actor-native.py` и actual
`update_shutdown` fixture. Gate пройден на producer9a02ca9/targetcee06a7. Он сохраняет actual files,
immutable attempt/duplicates, legacy-view omission и post-owned-exit history без
custody; Python отдельно сверяет фиксированные manifest bytes и их SHA.
Для target schema14 уже есть clean архив cee06a7:
`winbridge-20261023T005835Z-cee06a7-88d49ac8.zip`, SHA
`ce86846bbe82153286744a769b188eb3ff4dece82656e9af62d5328a29543272`.
Это другой source generation, подходящий кандидат для same-schema подготовки;
production установленный d052 остаётся обычным TaskSubmit executor.
Target cee06a7 — fixture для same-schema copy test, он ещё не имеет всего нового
updater actor. Worker rehearsal/schema compatibility не доказывает поддержку
всего updater pipeline у candidate updater. Перед public cutover нужен отдельный
versioned compatibility/feature check candidate updater; partial fixture не должен
стать production auto-update target только на основании совпадения schema14.
