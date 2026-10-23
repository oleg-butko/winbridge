# U2.2: durable replacement admission, schema15

2026-10-23. Добавлены journal15 tables и trusted library admission. Package216/0,
workspace647/0 (16 ignored), обе all-target Clippy/Python7/0/scaffold240 прошли.
[Native ledger11/0 gate](windows-runtime-replacement-ledger-gate-2026-10-23.md)
пройден; **actual14→15/refusal и positive admission под настоящим original guard
ещё не подтверждены**. [Data model](runtime-replacement.md)
уже прошла отдельный schema14/native12-test gate; он не подтверждает новый journal15.
Whole rename/activation и public update по-прежнему отключены.

## Журнал и полномочия

`runtime_replacement_intents`, `runtime_replacement_records`,
`runtime_replacement_history` связаны с сохранённой preparation. Intent immutable,
current меняется одним CAS step, history append-only; terminal rows не меняются.
Joined read восстанавливает полный preparation graph и сравнивает exact Prepared
record с immutable replacement intent. Нельзя подставить похожие независимые JSON.

Fresh admission требует current installation owner/epoch, exact supervisor launch,
unchanged Replacing root и fresh health текущей compiled schema. Он проверяет
оригинальный `ShutdownExitGuard`, final backup, frozen configs, prepared/source/
previous files и directory identities. Exact prepared directory содержит только
пять fixed files. Pins удерживаются через SQLite commit; worker config сверяется
по полной evidence, включая File ID, payload directory тоже удерживается по его
сохранённому File ID. Старый runtime остаётся под original read pins.

Только после commit возвращается non-Clone/non-Serde `RuntimeReplacementPermit`.
Existing/reopen — только observation, fresh permission не возникает. Свежий admission
поддерживается на Windows; metadata inspection остаётся переносимой. Physical pins
admission не подменяют consuming guard: перед будущим IO он должен заново сверить
актуальное состояние и передать исходные locks/handles без реконструкции из JSON.

Пока открыт только `mark_runtime_replacement_indeterminate`: current authority,
original permit, exact saved prefix и readable reason. Он не исполняет IO и не
создаёт success receipt. Production callback успешного rename **не существует**:
его добавлять с transition-aware consuming physical guard, не с ослабленным старым
`ShutdownExitGuard::require`. Actor/RPC и projection для replacement ещё не подключены.

## Проверки и последующие шаги

Пять ledger tests используют synthetic metadata: все22 causal SQL transitions,
duplicate/stale CAS, immutable terminal/history, rollback coupled writes при
RAISE(IGNORE), reopen issued rename/terminal uncertainty и tampered history.
Отдельный migration test использует настоящий SQLite backup/open14→15, проверяет
root preservation, backup integrity, epoch fencing и пустые replacement tables.
Это generated14 fixture без actual Prepared graph и без старого executable.

В existing primitives runners добавлен отдельный режим
`--runtime-replacement-ledger-only` / `-RuntimeReplacementLedgerOnly`:
6 aggregate data tests +5 SQL ledger/migration tests, generated fixtures, explicit
updater15/worker16 source guard. Старый model-only режим исключает namespace
`journal::runtime_replacement`, сохраняя собственные6 aggregate tests.
Никакой desktop/candidate build или Windows UI не нужен.

Native run20261023-091331-b6f35ac8 terminal succeeded/0,11 tests/6 ready files twice;
полный workspace и обе Clippy passed. Повтор той же task не нужен. Этот metadata gate
не заменяет positive original-custody admission.

Затем обязательно проверить **actual retained14→15 и actual old14 refusal** на
отдельной consistent-копии прежнего successful14 Prepared history. Reference —
run20261023-012740-9feb95b9, full pins в прежнем report; source reference остаётся
readonly. Новый owner получает history, не новый permit. Старый14 `journal init/status`
должен отказать15 без изменений DB/WAL. Не переделывать прошлый13→14 gate в новый
с тем же названием и не запускать compiled14 helper на15 как совместимый.

Для этого добавлен отдельный ignored Windows integration test
`apps/update/tests/runtime_replacement_migration.rs`, режим
`--replacement-migration-only` / `-ReplacementMigrationOnly` в прежних primitives
runners. Local reference pin — `.winbridge/update-preparation/replacement-schema14-reference-pin.json`:
он получен после повторной сверки16 ready files и полного approved Main proof14,
содержит old manifest/source pins и **canonical SHA всего original Prepared record**.
Test сверяет этот SHA до копирования и возвращает exact record в public proof;
Python независимо сравнивает его с исходным approved snapshot. Все old SQL tables,
включая preparation intents/current/history, входят в retained rows fingerprint.
Старый updater14 связан exact raw manifest bytes и настоящим artifact query.
Native execution нового test ещё впереди; metadata11/0 gate его не включает.

```bash
python3 scripts/validate-update-shutdown-primitives-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/<NEW-CLEAN-SCHEMA15-ARCHIVE>.zip \
  --cache-run 20261017-061527-f4639971 --replacement-migration-only \
  --reference-receipt .winbridge/update-preparation/replacement-schema14-reference-pin.json
```

После этого consuming original custody, retained directory и owned rename driver,
native per-effect faults и activation. Для fresh whole pipeline нужны **оба schema15
archives**; прежний cee06a7/schema14 не подходит. Перед production drain остаётся
обязательным candidate-updater feature/compatibility check. Current supervisor v16/
view8 и worker journal16 не меняются; public installation не включается.
