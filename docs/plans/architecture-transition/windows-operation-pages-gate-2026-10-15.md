# A3.3 — нативный кандидат и граница обновления, 2026-10-15

[Контракт/read model](operation-pages.md). **Кандидат прошёл gate; actual installed
A3.3 пока не завершён.** Worker `35b716a` исправен, но operation-read-model 1
ещё не объявляет. Полный A3 остаётся открыт до обновления и actual new/new route.

## Windows candidate

Code `02970dbea5567e5bf005041d59640f9611513b8f`, build `1792055097723993000`,
source manifest SHA `29e8529b02f49a7aeb4d610699fb14039ce3445043a379921f687b4e945e832d`.
Run `20261015-050417-8b310ea9`, task `ce8a33b6-0380-487b-9955-ecdc158dfa26` succeeded/0.
Report SHA-256 `eb82ad9cf9b7450a0bd1b0963f0bba243c3db95f84af61eedf4fd93d4266f0c8`; 10 опубликованных файлов
проверены по ready manifest. **48 tests, 8 groups и native desktop compile passed.**

Проверены typed query/decimal precision, scope/filter-bound cursors,
watermarks/clock rollback/reopen, все четыре вида операций, один SQLite snapshot
для записей и ресурсов при concurrent writer, readonly archive, actual Windows
named pipe → принятая fixture task → terminal/output → operation page.
Unknown contract/foreign scope/malformed payload не создают задач. Также прошли
прежние readiness/policy/frozen wire/Info/request-context/maintenance проверки.
Desktop собирался как проверка Rust/native контракта; native UI окна не запускались.

Первый run `20261015-050108-e9515e07` из `545166d` тоже прошёл 48 tests,
но desktop `--locked` выявил пропущенную зависимость `schemars` в отдельном
lockfile. Исправление `02970db` добавило одну строку, без смены версий пакетов.
Полный повтор выше прошёл. Это ошибка кандидата, рабочий worker не прерывался.
Перед экспортом теперь также проверен отдельный lockfile без Windows linker:

```bash
cargo metadata --manifest-path apps/desktop/src-tauri/Cargo.toml \
  --locked --offline --filter-platform x86_64-pc-windows-msvc --format-version 1
```

Кэш `20261015-050417-8b310ea9` переиспользовал проверенный cache owner
`20261014-054619-d5d99c71` через сохранённый receipt. Локальное состояние тестов
изолировано; конфиги/runtime/journal Windows и старые validation directories
не изменялись/не удалялись. Windows free before/after:
`56247431168` / `56181665792` bytes.
Debian free перед локальными проверками 29.6 GiB, после них ~28 GiB;
`CARGO_INCREMENTAL=0`. Места достаточно, очистка не потребовалась.

## Debian и текущая установленная пара

316 workspace tests + отдельная negotiation regression passed. Linux и
Windows-target Clippy, TS generation/check/typecheck/build, полный actual Unix
worker browser scenario (включая общий журнал/filter/detail без admission)
и 29 shared projection Rust/TS cases passed. Frozen RPC-2 schema SHA прежний:
`dd86d35b8b9ed59aca0bb438491e906b9c25c69970f0121226fdee99b98e9a12`.
Read-model schema SHA:
`7c078b1bfedbdf2b1488fe782f6ab52e35ba33f9b8f28e560aa683458d9c9749`.

Controller `545166d2e4b712ed93805bd3fe34b0b5538e48b3`, build
`1792054890343016109` прошёл actual CLI/MCP/WS/IPC consumer tests;
artifact SHA совпал с proof и запущенным executable. Более поздний `02970db`
меняет только desktop lockfile, controller binary не пересобирался.
Он запущен на прежних WS/IPC endpoints, generation `ff109089-6ab5-4abc-a470-c9988b62576a`,
serve session `16347`. Windows worker build/generation/scope сохранены;
новый session `c9ea293f-14a8-4205-a3a7-daf740120c23`. Старый controller artifact сохранён
локально для возврата; configs/firewall/mount/services не менялись.

Actual MCP pair `paired-20261015-050741-83112b53` passed: provider task
`8a091586-0a0e-44d5-960a-567c4954740a`, pair task
`74478525-d7fb-4520-80c2-b6e98a99aabb`, обе succeeded/0.
Receipt `9e912c90128634cfe3a035e985ffb86a3e6ddd89b49aaaed00f2b27e2a536bfb`.
Windows worker cache получил новую квитанцию. Actual-data browser renderer
проверил четыре readiness facts, receipt, exact task navigation и stale connection.
Это проверка renderer на Debian с Windows metadata, а не native window automation.
Новый запрос operations к установленному `35b716a` отвергнут до dispatch:
поддержка контракта отсутствует, legacy fallback не выполняется.

## Следующий запуск

[Обновить worker/desktop](../../validation/architecture-worker-update.md) одним
обычным prepare-сценарием. После обновления агент сам проверит actual build,
operation-read-model/schema, IPC/WS/CLI/MCP страницы и UI на реальных данных,
затем закроет A3 и перейдёт к A4 (очередь/global resources/fairness).
Текущий native gate не заменяет эту installed-проверку.
Desktop можно закрыть; worker оставить работать до самого обновления.
Ручной UI checklist не требуется. Local evidence:
`.winbridge/architecture-transition/a33-*` и `.winbridge/workflow-validation/20261015-050417-8b310ea9`;
shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261015-050417-8b310ea9`.
