# Продолжение после установки worker 0.1.1

2026-10-26, America/New_York. Performance-ветка слита в `main`; ограничения её
старых checkpoint на разработку updater больше не относятся к текущей ветке.
Предыдущий runtime checkpoint — [подтверждённый writer](resume-2026-10-25-writer-verified.md).
Дальнейший сценарий — [локальный сервис](local-service-activation.md).

## Установленная версия проверена

Пользователь установил [выпущенный worker 0.1.1](../../releases/worker-0.1.1.md).
Через работающий controller получены фактические Info и результаты короткой
PowerShell-задачи **ca8c7760-8bd0-4680-b4d2-d7fdaa19b325**, `succeeded`, exit 0.
Новые exe не запускались, UI окон не было; проверка не заменяла config/state.

| Наблюдение | Результат |
| --- | --- |
| Build | `0.1.1`, source `59249e6e5341fe13542a7768cec9107e19aad222`, dirty=false |
| Manifest | `f7497e8deb6f05f5b45e7863577c99d3a511491315348baaa78625a7f4aacb69` |
| Exe SHA-256 | `536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712` |
| Windows process | PID 21776, birth `2026-10-26T18:52:06.8652572Z` |
| Generation | `936909c6-d148-40cd-abf6-a0d996935994` |
| Controller | `ws://192.168.44.203:1143`, connected |
| Session | `c1a7e0b6-edb6-46e2-85ba-2f0fc6461c76` |
| Journal incarnation | `5a8eb173-4193-4332-b00f-67e3850b2c97`, сохранена |
| Recovery | complete=true; maintenanceError/journalRecoveryRequired отсутствуют |
| После проверки | activeTaskCount=0; прежнее приложение stopped, workflow succeeded |

Actual exe: `K:\_proj-26\winbridge\runtime\winbridge-worker.exe`.
Actual config: `K:\_proj-26\winbridge\runtime\winbridge-worker.snapshot.local_203.json`,
SHA `bad46778644fcebd8beeb4f5d495e45687ed8d2725c3c5d15c97471b6facbbf3`, прежний.
Installed-set manifest отсутствует. Controller PID 5103 не перезапускался.

Старая task `bf07a5ed-aa0d-4d94-b780-e524e66d269e` читается как succeeded/0,
сохранены 6492 bytes output (stdout/stderr в sequence order), SHA
`6aab97ade493fa9ea8a471d3cd867254dcd280fff0540fdf10010921c1b2895a`.
Новая generation/session не изменили исторические IDs и результат.

Полное локальное evidence:
`.winbridge/update-preparation/installed-0.1.1-20261026T190328Z-04b1bbe5/`:
`submission.json` сохранён до отправки, затем `accepted.json`, `terminal.json`,
`verified.json`. SHA последнего:
`b5a3766c0c7d644ab0684e440b0517b5216c8ec806dcd04543e6b7097c863c24`.
Содержимое evidence не коммитится, как и runtime/config/journal.

Перед сборкой: Debian 16,982,433,792 bytes free; Windows K:
27,685,994,496 bytes free — оба выше 6 GiB. Сборки используют основной target,
`CARGO_INCREMENTAL=0`, без удаления Windows validation directories.

CPU за 20.07s проверки: 24.37% одного логического ядра. В это время worker
обслуживал принятую измерительную задачу и запросы её статуса. Это **не idle
benchmark**, не повтор matched-history сравнения 98.88→1.94% и не доказательство
такой нагрузки на реальной истории в простое. Данные сохранены без подмены.

## Что добавлено в исходники updater

**Updater journal schema 20**, worker journal по-прежнему 17. Сервис пока не
запускается: это формат хранения и проверенное чтение, без runtime admission.

- `apps/update/src/journal/activation_service.rs`: immutable intent, current
  record с CAS revision, append-only causal history; один command на update.
- Readback проверяет ключ/индексы, всю цепочку Issued → Confirmed или Issued →
  Indeterminate → Confirmed, exact frozen command и confirmed writer binding.
- Чтение через `activation_service` / `inspect_activation_service` использует
  одну SQLite transaction и существующую полную проверку writer/backup/activation
  graph. После последующих фаз берётся неизменяемый StateOwned revision 6.
- На схемах до 20 readonly query возвращает отсутствие нового record и не
  мигрирует БД. Normal open 19→20 требует свободного owner lock, делает consistent
  verified backup и повышает updater owner epoch. Worker epoch не изменяется.
- Примитивы записи пока компилируются **только в тестах**. Нет public JSON-write
  API, CLI action, service permit, private receiver4 или новой capability.
  Сохранённый record не позволяет открыть Store, усыновить child, перепослать
  команду, подключить controller или освободить maintenance lease.

Проверки в `journal/activation_service/tests.rs` охватывают неизвестный исход и
late receipt без reissue, stale CAS, игнорируемые SQL writes и rollback всех
частичных записей, immutable triggers, подмену ключей/graph/history/result,
readonly cold reopen, сохранность writer rows при migration, backup/owner fencing/
root dedup и атомарный отказ migration при конфликте с чужой таблицей.
Старые synthetic migration fixtures удаляют новые пустые таблицы прежде, чем
создать прежнюю схему; существующие проверки не исключены.

### Проверки этого source

- `cargo test --workspace --locked`: **804 passed, 0 failed, 17 ignored**,
  включая все 10 новых SQL/migration tests и прежние 7 pure service tests.
- Linux и `x86_64-pc-windows-msvc`: `cargo clippy --workspace --all-targets
  --features winbridge-update/update-validation --locked -- -D warnings` прошёл.
- `cargo fmt --all -- --check`, `git diff --check`, `npm run check:scaffold`
  прошли; scaffold проверил 290 Markdown files и локальные ссылки.

Все Rust-проверки выполнялись с `CARGO_INCREMENTAL=0` на основном target.
Логи `.winbridge/update-preparation/service-ledger20-{workspace,clippy-linux,clippy-windows}.log`.
Windows target check не является выполнением Windows программы.
Native service gate не запускался: receiver и действующий сервис ещё отсутствуют.
Пройденный release gate worker 0.1.1 относится к source `59249e6`, а не к этому
новому формату updater20. Установленные файлы больше не изменялись.

## Как продолжить

1. Добавить consuming admission от resident Written guard. Текущий owner/root,
   supervisor, physical6 и confirmed writer проверяются в одном snapshot; original
   monotonic startup window не продлевается. Intent фиксируется до одной send.
   После commit uncertainty — только чтение той же команды, без replay.
2. Подключить приватный service4 receiver и consuming передачу ActivationJournal,
   Fresh permit и полного LockedShutdownState в существующий Worker constructor.
   Не вызывать normal Store::open, begin_authority, migration или повторный lock.
3. Переиспользовать reconciliation/IPC handler. Maintenance Quiesced и прежняя
   пользовательская пауза сохраняются. Local-only service не подключается к сети.
4. Проверить настоящий normal IPC и PID сервера pipe, exact build/config/schema/
   generation/context и прежние tasks/output. Только этот факт подтверждает
   service receipt; private Ready недостаточно. Затем actor25/view17.
5. Новый isolated Main25 и actual **retained19→20/refusal** на consistent copies,
   включая old immutable writer history. Не переименовывать прежний gate19 в20,
   не повторять terminal task и не считать synthetic migration native proof.
6. После положительного service gate: новая controller session → durable Job
   lifetime transfer → commit/scoped release → remaining U2 → U3 → U4 → A4.3.

Сетевые тесты используют постоянный отдельный exe slot и 900s firewall budget.
Для activation fixtures надо сохранять их manifest/path contracts; нельзя просто
переносить уже запущенный child в release runtime-test. Рабочий worker оставить
запущенным; desktop сейчас не нужен. Public install остаётся выключенным.
