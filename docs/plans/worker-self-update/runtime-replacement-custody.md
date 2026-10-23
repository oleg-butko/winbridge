# U2.2: consuming custody и owned rename library

2026-10-23. Добавлены native guard, directory/file operations и guarded SQL success
callbacks. **Это trusted Rust library: supervisor actor/public install их ещё не
вызывают; positive actual Main replacement gate ещё впереди.** Schema15/worker16 и
supervisor v16/view8 не менялись. Результаты прежних metadata/migration gates не
подтверждают новые Kernel handoff/rename paths.

## Передача исходного владения

`shutdown::Custody` — непрозрачный Rust container, не DTO. `take_for_replacement`
сначала проверяет exact saved Planned record/current permission через journal,
затем проверяет исходный exit guard через immutable loan. Медленные SQL/file checks
проходят без slot mutex. После отпускания своего loan `Arc::try_unwrap` под коротким
mutex отдаёт тот же original guard; если есть racing/outstanding loan, **тот же Arc**
возвращается в slot. Locks/handles не пересоздаются из journal/JSON.

Fresh non-Serde permit теперь несёт и exact original ShutdownIntent из Verified
backup. Поле существует только в памяти, durable intent/контракты не меняются.
Public Rust hosting API не регистрируется в CLI/MCP и не предоставляет wire callers
способ создать kernel proof. Старый `ShutdownExitGuard::require` продолжает проверять
весь старый runtime; общий exit/state prefix вынесен без ослабления условий.

`RuntimeReplacementGuard::acquire` потребляет permit и unique original guard:

1. Exact owner/epoch/launch/root/Planned повторно проверяются. Fixed configs,
   final backup, source/preimages и все directory ancestors удерживаются pins.
2. Получить пять prepared handles read+DELETE с exact File ID/hash/bytes. Старые
   read pins в этот момент ещё удерживаются; existing displaced directory запрещён.
3. Ещё раз проверить original guard, anchors и current authority. Разделить
   **тот же** guard на retained exited-process/state/instance-lock custody и old
   read set. Отпустить только старые read pins; процесс/state/lock остаются живыми.
4. Получить пять old read+DELETE handles с повторной полной проверкой. Acquisition
   failure не переоткрывает старые pins и не повторяет попытку. `RuntimeReplacementFailure`
   сохраняет boxed guard с original/exited custody, anchors и partial acquisitions.

## Операции и receipts

Сохранённые directory/file intents предшествуют эффектам. Directory create — один
create-only attempt, затем actual directory pin/ID до receipt. `execute_next_move`
сам выбирает первый incomplete file и разрешает лишь RetireIssued/PlaceIssued;
caller не задаёт пути, index или actual evidence. Используется native-tested
`OwnedRuntimeFile::move_new`, absolute destination/ancestor pins/no overwrite,
один OS attempt, без copy/delete fallback. После IO проверяются actual moved handle,
все остальные owned files, anchors и current authority. Возвращённая data receipt
сама по себе не является commit.

Guard проверяет physical locations по каждой фазе: old installed/retained и new
prepared/installed. Поэтому первая законная замена не ломает проверку оставшихся
файлов. Проверяется и exact directory composition: prepared содержит только ещё
не перенесённые copies, displaced — только получившие retire receipt old files.
Неизвестные/missing entries сохраняются и блокируют дальнейший IO.

`UpdateJournal::update_runtime_replacement` сначала проверяет causal transition и
current original scope, затем требует native guard и actual next physical prefix,
и лишь после этого атомарно пишет current/history. Indeterminate не требует,
чтобы пропавшие файлы появились снова, и блокирует дальнейшие эффекты guard.
Любой IO error сохраняет ownership в failed guard; нет cleanup/retry/adoption.
FilesMoved пока не становится trusted FilesVerified/root transition или activation.

## Проверки и ближайшее продолжение

Local updater package217/0 (6 ignored), workspace648/0 (16 ignored), Linux/Windows
all-target Clippy с update-validation прошли. Unit test подтверждает exact Arc
restoration/no resource drop при outstanding loan и one-time unique take.
Windows-only directory test определён и компилируется: unknown file/subdirectory
не удаляются; **этот новый test ещё не исполнялся на Windows**.

Не выдавать static ownership/type checks или cross-check за positive OS handoff.
Следующий обязательный вертикальный сценарий:

1. Blocking driver: fresh permit → unique resident custody → guard → directory/
   per-file loop до FilesMoved. Возвращать retained guard и при regular errors;
   неизвестный outcome хранит prefix, startup запрещён.
2. Supervisor **новый** contract/view: admission-before-ACK, один work slot,
   отзывчивый status, custody из live object отдельно от saved state. Дубликат
   не выдаёт fresh permission/driver. Contracts1–16/views1–8 не переосмысливать;
   DTO/TS обновить.
3. Trusted FilesVerified только после exact full moved set, atomic root transition;
   raw lifecycle этого права не получает. Activation — следующий отдельный шаг.
4. Actual Main15 → source/build/health/drain/shutdown/backup/preparation → new
   replacement action. Проверить genuine custody, five old IDs в displaced,
   five new IDs в runtime, empty prepared directory, manifest-last/22 transitions,
   сохранённые source/preimages/worker DB, duplicate и cold history/custody=false.
5. Failure gates: locked acquisition, occupied destination/unknown entries,
   directory/file receipt failure и crash на intents/receipts. Не реконструировать
   новый guard из record. До public cutover нужен candidate-updater compatibility
   check до drain, затем activation/controller confirmation/U3/U4.

Для isolated15 fixture уже есть source archive5fbc1cf из metadata gate. Это limited
same-schema target без всего будущего actor/activation feature set; не использовать
его как production update target. Installed d052 пока нужен лишь как TaskSubmit
executor; ручного обновления сейчас не требуется. Никаких services/firewall/mount
changes и cleanup retained Windows directories этим блоком не выполняется.
