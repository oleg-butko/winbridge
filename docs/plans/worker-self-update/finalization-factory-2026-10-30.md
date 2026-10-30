# Исходный владелец: одно V8 delivery и независимое Active confirmation

2026-10-30, .204/main. Worker19/updater23; installed worker0.2.0 не менялся.
Этот этап продолжает [confirmation writer](finalization-completion-2026-10-29.md).

## Реализация

Internal supervisor29/view21 подключает FinalizeActivation и отдельный
ObserveActivationFinalization к исходному FilesActor. Только оригинальный
Serviced guard с сохранённым lifetime key начинает решение. Cold journal/view,
повтор JSON, неизвестный результат SQL и прежний internal28 такого права не дают.
Caller timeout/disconnect не отменяет уже принятую работу; далее читается тот же
owner/command, V8 не пересылается. Stop с живым service owner отклоняется, чтобы
случайно не закрыть его Job вместо будущего keeper conversion.

Consuming factory сначала проверяет оба receiver markers в закреплённом образе,
затем получает свежий V7. Исходный monotonic capture deadline (10 секунд) сохраняется
до admission, не начинается заново после hash/SQL. Known RootCommitted/ReleasePending
потребляется ровно один раз. Полный ProcessCustody (child, Job, channel, оба output
readers) перемещается; ВСЕ остальные verification/config/file/старые exit owners
закрываются. Новый guard не имеет cold constructor, Deserialize, Clone или detach.
До known COMMIT ошибка возвращает прежний guard, после него — отдельную custody.

Одна V8 отправка идёт через normal IPC с проверкой настоящего PID. Частично прочитанный
старый V6 канал закрывается без чтения. Даже успешный V8 требует новой независимой
Active observation; её opaque capture permit передаётся production writer. Unknown
ответ записывается как диагностический Indeterminate и сверяется чтением, без нового
release/launch. Live view показывает реальные custody/liveness, retirement и attempt,
current observation, bounded output tails и readers status. Cold21 показывает только
историю, nullable physical fields и NoOriginalCustody.

При ревью изменено продолжение known SQL COMMIT перед последним seal: только сам
RunningLocalService удерживает исходные commit/readiness/OS peer, с тремя попытками
через250/1000/5000ms. Повторная JSON-команда не запускает продолжение и не повторяет
SQL. После исчерпания попыток остаётся диагностический Pending, бесконечного polling
или hashing в idle нет. Windows-only regression проверяет именно эти границы;
synthetic fixture metadata не выдаётся за положительный native proof.

## Проверки и native Main30

Полная проверка текущего source:1033 workspace tests/17 ignored (128 groups),
Linux/MSVC workspace all-target Clippy/update-validation с -D warnings,
contracts/typecheck/scaffold/fmt/diff passed. Adapter отдельно3/3 passed.
Логи `.winbridge/finalization-main30-review-204/summary.json`; build UI passed.
Новый Python reader имеет3 synthetic regression tests/21 отрицательную мутацию;
это проверка reader, не Windows evidence. Disk перед сборкой: Debian18,564,452,352,
Windows(shared)21,779,820,544 bytes free.

Подготовлен новый явный Main30: worker19/updater23, отдельные run/verify wrappers
и independent checker. Старые Main25–29/profiles/reference reports не переименованы.
Требуются два разных clean source snapshots с built UI, оба с internal29.
Проверка использует разрешённый сетевой runtime-test-204/1144; Cargo test EXEs
со случайными путями выполняют только local IPC. Installed runtime/config не меняются,
desktop не нужен, окон не будет. Worker17 reference фиксируется отдельно от installed.

Fault injection существует ТОЛЬКО в updater update-validation build: после одного
настоящего V8 sender/OS PID admission читаются4 байта normal frame header, тело
выбрасывается. Следующее соединение читает Active и подтверждает ledger. Worker V8
receiver остаётся production code без этой инъекции. Сценарий также проверяет actual
same Job/process, один SQL release, исторический V6 deadline без продления, config
write и atomic replacement, foreign peer/JSON PID refusal, настоящую fenced cmd.exe
задачу и один внешний эффект после duplicate. Owned test Job затем закрывается;
cold21 не присваивает себе процесс. Ready files/hash/size проверяются дважды.

**Main30 пока не запускался.** Ни положительный physical V8/Active, ни постоянный
keeper, ни полное самообновление этим документом не объявляются готовыми.

## Продолжение

После clean source/target export выполнить один Main30 task, сохранить ID и читать
тот же task до terminal. Ошибка reader исправляется перечитыванием сохранённого proof,
без повторения Windows effects. Ошибка native scenario требует конкретного разбора,
с сохранением failed report/slot и без автоматического переиспользования unknown key.

Затем confirmed original custody нужно перевести в постоянный keeper: убрать все
updater journal/authority references, завершить control actor с явным durable Keeper
состоянием (не Stopped), продолжая владеть Job/readers. Следующее обновление требует
корректной передачи владения, max2 keepers/two generations. Это отдельный этап;
RootCommitted и Active fact не означают, что updater больше не блокирует следующий
owner или что выполнены U2–U4. Public installation остаётся выключенной.

### Два clean pins для Main30

Producer c19e1ec2da36b120a0eb601fbeadfd32888b1c13 экспортирован с built UI:
`winbridge-20261030T043128Z-c19e1ec-4f45aa8b.zip`,
SHA256 bb2c18d637f0bcf524215d5a594060091d775f7ea59f99a40d776c41155578ad,
6,913,465 bytes/1116 files, dirty=false. Следующий documentation-only checkpoint
служит отличающимся target pin; Rust/DTO/runtime code одинаков. Это проверяет
реальную замену source/target identities, не объявляя новый пользовательский релиз.

### Native run начат; reader checkpoint

Task7cb7211b-0b85-4a1c-8327-7e52f7639ab8/run20261030-043200-319f3e87,
source c19e1ec → target8fdcc42. Первый source ZIP выше; target ZIP
`winbridge-20261030T043147Z-8fdcc42-0ef25be7.zip`,
SHA256 911e7c3713feff1d4141d8123f7a11b4f27ff1c5556d5777020fd000ab1dbe5f.
Повторно не отправлять: наблюдать исходный task/run-info.
Native finalization24 + v29 mailbox1 + bounded known-seal1 уже passed;
Main30 effect ещё не выполнен. Унаследованный resident filter теперь включает
14 tests (добавился pending finalization receiver), поэтому Main30 задаёт явный
reviewed14, тогда как старый Main29 reader сохраняет default13. Никакие старые
отчёты/схемы не переименованы. Reader6+3 regressions passed; новая проверка сохраняет
SHA всех11 Python observer dependencies и Git commit/dirty, проверяя файлы повторно.
