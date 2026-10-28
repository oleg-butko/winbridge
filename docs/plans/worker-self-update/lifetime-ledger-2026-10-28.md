# Lifetime ledger: schema22

2026-10-28. Продолжение [контракта](lifetime-contract-2026-10-28.md) и
[native OS proof](windows-lifetime-spike-2026-10-28.md). Этот шаг сохраняет
историю; production fresh admission и смена lifetime режима ещё не реализованы.

## Хранение и чтение

В `apps/update/src/journal/activation_lifetime.rs` добавлены три таблицы:
`activation_lifetime_intents`, `activation_lifetime_records` и
`activation_lifetime_history`. Intent содержит уникальный command ID и полный
immutable command. Текущая запись меняется через CAS, каждое изменение сохраняет
строку causal history. UPDATE/DELETE intent/history и удаление current запрещены
triggers; повторная смена terminal confirmation не допускается. SQL дополнительно
сверяет ID/revision индексов с JSON. В ревью добавлен явный NOT NULL к primary
ID intent/current: SQLite TEXT PRIMARY KEY сам по себе допускает NULL, а NULL FK
не требует существующего parent. Две SQL negative проверки запрещают такие
неадресуемые строки. Rust reader независимо проверяет всю цепочку,
включая непрерывные ревизии0–2 и неизменность command.

Production API сейчас только `activation_lifetime`/`inspect_activation_lifetime`.
Оба читают внутри одной SQLite transaction: retained physical6 → writer → service
→ local7 → confirmed controller → lifetime. Новый controller snapshot helper
переиспользует существующие проверки полного graph. Он не использует latest local
вместо сохранённого local7 и не реконструирует historical physical6 из более
позднего состояния. Flat hashes нового command проверяются против точных records,
а не только сравниваются друг с другом.

Если присутствует лишь часть intent/current/history, reader возвращает corruption,
а не отсутствие операции с возможностью повторить действие. Ключ другого запроса,
подмена controller record, Job/keeper/authority, original deadline и переписанный
causal prefix не принимаются. Одно чтение позднего факта не создаёт original
guard, nonce, live budget, Job handle, kernel ownership или permission.

SQL writers пока скомпилированы **только для unit tests**. Нет публичного JSON
writer или нового supervisor action. Private5/supervisor26/view18 сохраняются;
worker journal19 не меняется. Public activation/controller_confirmation=None.

## Миграция

`UPDATE_SCHEMA_VERSION=22`. Blank DB создаёт полный набор таблиц; старые схемы
проходят существующий последовательный migration path. Перед изменением старой
БД сохраняется consistent SQLite backup, проверяются integrity/header, берётся
existing owner lock, а epoch старого писателя закрывается. Само чтение schema21
через новый lifetime API возвращает None без migration/create. Конфликт с чужой
таблицей откатывает DDL/epoch; её данные остаются нетронутыми.

Synthetic migration tests прежних версий удаляют теперь и пустые новые lifetime
таблицы перед формированием своего старого schema fixture. Это test setup, не
операция очистки настоящих journals и не способ построить native reference.

Unit migration доказывает SQL/backup/fence на синтетической БД. **Actual retained
21→22 и old21 executable refusal пока не проверены на Windows.** Для них нужен
отдельный copy-only gate от сохранённого [Main26](main26-native-2026-10-28.md):
проверенный archive/runtime target b149eed/19+21, source/image/manifest pins,
согласованная копия original updater DB и SHA всех retained rows. Требуется
сохранить root AwaitingController, local7, writer/service/controller records,
их history/keys/sequence и исходный backup; получить пустой lifetime ledger и
подтвердить отказ actual old21 `journal init/status` без изменения новой копии.
Original archive/DB/WAL и установленный worker остаются неизменными.

## Проверки этого шага

11 SQL/model tests покрывают late fact, CAS/terminal, immutable/index constraints,
5 ignored-write fault points, checkpoint до/после INSERT,3 partial-ledger cases,
4 подмены causal history, включая согласованную подмену всех command copies,
key conflict/изменённый graph, consistent cold copy, concurrent WAL confirmation,
synthetic21 migration с backup/history/idempotency/epoch fencing и rollback при
foreign table collision. Примитивы получают synthetic validated graph;
это не native admission или kernel custody proof.

При локальном изменении schema проверять через полный workspace:

```bash
CARGO_INCREMENTAL=0 cargo test --workspace --features winbridge-worker/update-validation --locked
```

Supervisor unit fixture
исполняет `target/debug/winbridge-update`, поэтому отдельный `--lib` не пересоберёт
этот sidecar. В этом шаге readonly `--runtime-support` подтвердил старый schema21
binary рядом с новой22 library; такой смешанный unit run закономерно отказался
запускать supervisor. Полный workspace собирает согласованную пару. Это не
требование обновить установленный Windows worker.

Финальный полный workspace:946 passed/0 failed/17 ignored,124 test groups.
Linux и Windows MSVC workspace/all-target/update-validation Clippy `-D warnings`,
generated contracts, fmt/diff и scaffold318 passed. Финальный local test CLI
readonly support — schema22, public activation/controller_confirmation=null.
Ранние failed logs сохранены: незаданный application_id synthetic SQL fixture,
смешанный old21 sidecar в `--lib` и неполный старый integration migration setup
исправлены; ничего из проверок не исключалось и assertions не ослаблялись.

Evidence: `.winbridge/review-2026-10-28/lifetime-ledger-*`; primary final log —
`lifetime-ledger-workspace-repeat.log`, summary — `lifetime-ledger-workspace-summary.json`.
Перед full build Debian10,363,187,200 bytes и K: через shared24,749,158,400;
repeat storage также сохранён. Windows build/tasks/UI не запускались. Final
readonly installed Info: source48c834c/build1793126009319459400, generation946f1128,
active0/maintenanceError=null/journalRecoveryRequired=null. Runtime204 не менялся.

## Продолжение

1. Подготовить actual retained21→22 copy/refusal gate и независимый reader;
   выполнить автоматически через installed worker, без network/UI тестовых exe.
2. Fresh lifetime admission — только original resident controller-verified owner:
   current root/epoch/config/nonce/schema, immutable full graph, original live
   monotonic window до/после commit. Existing возвращает только data record.
3. Same-child private receiver и independent normal IPC, затем actor/view
   нового режима supervised quarantine. Не открывать dispatch на подтверждении.
4. Consuming keeper role, release file pins/journal authority для следующего
   поколения, final root commit и exact scoped maintenance release. Два поколения,
   maximum2 keepers/next-update delegation и faults проверять отдельными gates.
5. U2 management/fault matrix → U3 UI/attention → U4 actual installed update.

Новый schema сам по себе не объявляет эти пункты готовыми. Старые Main25/26,
20→21 migration и OS spike не нужно повторять вместо нового конкретного gate.
