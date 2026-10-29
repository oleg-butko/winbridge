# Finalization: root decision отдельно от рабочего режима worker

2026-10-29. Source foundation после [Main29](main29-native-2026-10-29.md).
Worker journal остаётся19; updater ledger становится23. Это ещё не завершённый
апдейтер: нет physical factory для finalization, V8 decoder/marker, delivery,
ReleasePending→Active на worker, keeper conversion или public install.

## Контракт и история

Proposed V8 DTO не регистрируют incoming route и не рекламируют capability.
V7 остаётся readonly. Finalization command связывает original V6 command с
хешами полного lifetime record, полного ShutdownRecord с exact exit и actual
resident query/response. Сохраняются installation/scope, original keeper/Job/
child/authority/config, exact old lease revision, target source manifest,
AwaitingController revision и единственная следующая Committed revision.

Полный shutdown record важен: существующий source graph хранит его hash, а не
hash отдельной acceptance receipt. Pure proposal проверяет весь historical
prefix и current accepted V7 link. Возраст отсчитывается от исходного observedAt,
а не от более позднего issuedAt. Limit10s проверяется перед/после записи; будущий
original owner также обязан дать один monotonic deadline. Это время свежести
наблюдения, исходный activation window остаётся прежним и не возобновляется.

FSM: ReleasePending → Confirmed либо ReleasePending → Indeterminate → Confirmed.
Indeterminate не создаёт второй send permit; late exact Active/release fact
можно сохранить, но нельзя откатить или переписать Confirmed. Receipt связывает
тот же loaded worker/Store authority, exact Released lease и mode Active;
исторические lease spec, projects, source directory, acceptedAt и quiescence
не переписываются. Generic post-shutdown Release по-прежнему запрещён.

## SQLite и следующий admission

Schema23 добавляет immutable intent, current record и causal history. В intent
сохранены original root и фактические query/response. One transaction записывает
root Committed, его transition/key/owner и ReleasePending intent/current/history.
Пять типов ignored writes и interruption до COMMIT проверяются с откатом всех
связанных строк. Retained reader сверяет весь graph, original local7 root edge,
committed edge, индекс/ключи, history sequence и current/readback.

`updates.active` сохраняет прежнюю root semantics: Committed имеет active0.
Внешний UpdateJournalInfo возвращает pending finalization как activeUpdateId;
новый admission остаётся закрыт. Historical graph readers используют отдельный
installation_info без рекурсивного чтения собственных finalization descendants.
Orphan/partial state, multiple unresolved decisions и Committed lifetime root
без intent — ошибка, а не свободный installation. Confirmed остаётся историей,
не live keeper или обещанием текущей сети. [Actual retained22→23 copy gate](finalization-migration-2026-10-29.md) подтвердил
сохранность старого graph и отсутствие новых grants. Pending root decision /
worker release / Active faults ещё не проверены на Windows.

Library decision API принимает non-Serde/non-Clone FinalizationDecisionPermit,
но его physical factory пока отсутствует. Только известный успешный commit может
выдать отдельный non-Serde FinalizationReleasePermit; Existing возвращает data.
Commit uncertainty оставляет только inspection того же key, без automatic replay.
Ни один CLI/MCP/mailbox не умеет вызвать этот новый эффект.

## Холодный запуск и release gap

Обычный CLI serve known runtime дополнительно проверяет pending updater до
открытия worker journal. Это необходимо на будущей границе SQL Released / Active
ACK: отсутствие active worker lease само по себе не доказывает завершение update.
Private activation и readonly verify-runtime сохраняют отдельное identity-only
поведение. Actual CLI regression проверяет отказ до Store creation при живом
pending updater root, даже без worker journal/lease/startup marker. Это ещё не
native crash proof границы Committed/Released/Active.

Будущий receiver обязан удерживать startup guard через SQL/memory переход,
согласовать retirement guard с этим pre-Store barrier и протестировать crash
между каждым шагом. Нельзя просто удалить marker после SQL release. Вопрос
совместимости worker schema19 решается по фактической реализации этого перехода;
пока новый worker journal format не вводился.

## Миграция и проверки

22→23 выполняется только под updater owner lock, с consistent VACUUM backup,
проверкой backup/schema/identity и fencing epoch. Старые schema fixtures сначала
убирают новые таблицы, чтобы действительно представлять прежний формат; production
migration не удаляет таблицы или историю.

Проверки: full workspace999/17 ignored/127 groups passed до последних review
changes; после них13 finalization tests и4 actual CLI startup tests passed.
Это1000 distinct tests, а не новый full workspace1000 run. Linux/MSVC all-target
Clippy с обоими update-validation features/-D warnings, fmt и scaffold333 passed.
В review дополнительно закрыты re-aging через issuedAt, старый build ID/target
source binding и pre-Store normal-start barrier. Test adapter и production gates
не ослаблены.

Команды (CARGO_INCREMENTAL=0, PATH=/home/user1/.local/bin:$PATH):

```bash
cargo test --workspace --locked --features winbridge-update/update-validation,winbridge-worker/update-validation
cargo test -p winbridge-update --lib --locked --features update-validation finalization
cargo test -p winbridge-worker --test update_runtime_startup --locked --features update-validation
cargo clippy --workspace --all-targets --locked --features winbridge-update/update-validation,winbridge-worker/update-validation -- -D warnings
cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc --locked --features winbridge-update/update-validation,winbridge-worker/update-validation -- -D warnings
cargo fmt --all -- --check
npm run check:scaffold
```

Полный набор сначала обнаружил stale local helper22 и одну намеренно повреждаемую
SQL fixture, которая не учитывала новый CHECK. Helpers пересобраны согласованно,
fixture теперь меняет key вместе с JSON и действительно проверяет читателя.
Ошибка Linux Clippy в test-only needless Box allocation исправлена. Failed logs
сохранены рядом с успешными, не переименованы в pass.

Уже сохранён actual Linux CLI proof: старый updater22 создаёт свой
пустой ledger, новый23 мигрирует его с backup, старый22 отказывает schema23,
DB bytes не меняются от этого отказа. Это Linux/empty-history scope, отдельно
от unit migration с Requested root/history и отдельно от будущего Windows gate.
Evidence `.winbridge/review-2026-10-29/finalization-*`, CLI report
`finalization-cli-22-to-23/report.json`. Main29 относится к замороженному19/22;
не выдавать его за native23/finalization gate и не повторять его вместо нового.

## Продолжать

1. Готово: [retained22→23](finalization-migration-2026-10-29.md) на consistent copy actual Main29 archive с старым22
   refusal, неизменными root/local7/writer/service/controller/lifetime histories
   и отсутствием придуманных finalization rows/custody. Working worker прежний.
2. Реализовать fresh consuming original-guard factory, authenticated normal-IPC
   V8 receiver и typed ReleasePending→Active на том же Worker/Store. Проверить
   peer PID/stamp, bound Committed intent, exact release receipt, commit uncertainty,
   lock ordering, startup guard, runtime/config pin retirement и dispatch gating.
3. Отдельный actor/current view для pending/unknown/Active; physical keeper
   conversion с сохранением original Job/output, закрытием write connection
   до отпускания всех authority Arc/file lock. Старый unread Ready не читать.
4. Actual new Main с partial Ready, root/release/Active, known cleanup и crash
   boundaries; next-update delegation/max2 keepers/two generations; U2–U4.

Последний Debian free15995731968/shared17770921984 bytes;
измерения сохранены, новых cache cleanups не было. Рабочий installed
worker0.2.0/source48c834c/gen946f1128 прежний, active0/errors=null; desktop не нужен.
