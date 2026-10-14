# A3.2a — идентичность пары, evidence и отпечаток policy

Дата: 2026-10-14. Часть [A3.2](paired-readiness.md), после закрытого A3.1.
A3.2a завершён: 300 workspace tests + focused canonical review, Linux/Windows
Clippy, TypeScript/UI и 36 Rust/TS cases passed. [Финальный Windows gate](windows-readiness-gate-2026-10-14.md):
39 tests, desktop check и шесть verified artifacts passed. **Полный A3.2/A3 открыт**:
producer квитанций, runtime readiness API и применение к новым execution contracts
пока не подключены. Сохранённая квитанция сама ничего не запускает.

## Реальный установленный маршрут

Пользователь обновил worker из `f128c9a`; Windows Info подтвердил clean build,
схему request contract 2 и catalog. Через настоящий stdio MCP → Debian IPC → WS
выполнен `fixture-success`: task `44f71380-dede-45eb-835a-c80aaf018648`, exit 0.
Повтор frozen запроса вернул тот же Task ID; другой incarnation отклонён до
передачи исходного запроса. Generation и session после проверки прежние.
Автоматический сценарий — `scripts/validate-installed-pair.py`; локальный отчёт
`.winbridge/architecture-transition/a32-installed-route-1.completed.json`.
Это smoke конкретного маршрута; он **не объявляет весь RPC pair-tested/enabled**.
Исходный scope/key записывается create-only до отправки; после потери ACK
сценарий не создаёт новый запрос и не перепривязывает старый.

## Идентичность и состояние

Protocol DTO фиксируют contract/version и digest настоящих request/response
JSON schemas, consumer/provider builds, instance/incarnation, ordered hop builds,
node IDs/endpoints, transport и project/policy digest. Session UUID намеренно
отсутствует в долговечной идентичности: свежесть проверяется отдельно.
Для первого известного маршрута предусмотрены MCP → IPC → WebSocket;
неизвестный транспорт требует отдельного поддержанного контракта.

Core evaluator отдельно возвращает supported, buildVerified, pairTested, enabled
и typed причины отказа. Changed policy/context/route аннулируют pair evidence;
при прежних consumer/provider builds доказательства сборки остаются историей.
Enabled требует полного совпадения, обязательного test profile/checks, свежего
маршрута и явного разрешения текущей policy. Empty/unknown metadata, dirty build,
partial checks и missing/invalid receipt разрешения не дают.

Readonly projection сериализуется для будущих UI/CLI/MCP. Dispatch guard принимает
только `ReadinessDecision`, который нельзя создать из полученных JSON boolean:
его закрытый constructor находится в evaluator. Внешний статус — информация,
а не полномочие. **Existing A2 admission и retries не изменены**; opt-in guard
будет подключаться при появлении конкретного нового execution contract.

## Действующая policy

Worker Info получает optional opaque `projectPolicies` и `contractSchemas`.
Schema digest вычисляется из настоящих DTO на каждой стороне; cached digest
связан с provider build и supported contract. CLI `--contract-schemas` показывает
local schemas. Native gate сравнивает Windows/Unix hashes, а не только номер «2». Known schema 1 связывает
SHA-256 каждого проекта с provider build. Hash включает загруженные project
settings, раскрытые recipes/profile bindings, worker settings, state scope,
delivery, output limits и retention. Другой проект не меняет fingerprint этого
проекта; общие действующие параметры меняют его. Наружу передаётся digest,
а не новая копия локального конфигурационного файла.

Pending save на диске не меняет fingerprint работающего worker. После ручного
restart появляются hash новых загруженных правил и новая generation.
Native settings test проверяет это вместе с сохранением прежнего running PID.
Rust/TypeScript имеют 12 общих negative/positive policy fixtures. Unknown schema,
null и будущие поля сохраняются при forwarding, но не интерпретируются как ready.
Desktop объясняет наличие/отсутствие policy metadata в «Совместимость компонентов».

## Local evidence store

`apps/controller/src/readiness_store.rs` хранит typed receipts в
`stateDirectory/contract-receipts-v1` на local disk. Private directory, bounded
256 KiB read, no-follow/nonblocking open и regular-file check защищают от
symlink/FIFO/повреждённого файла. ID — SHA-256 canonical typed receipt.
Write: новый pending файл → sync → atomic create-only hard link → directory sync
→ typed checksum/readback. Повтор возвращает прежний ID; существующая повреждённая
квитанция не перезаписывается. Незавершённый pending не является evidence.

Receipt — evidence, не credential и не защита от владельца локального state.
Validated test producer ещё предстоит подключить: он обязан проверять настоящие
reports/artifacts и exact tuple, а не принимать пользовательский `passed: true`.
Old receipts остаются историей; disconnect не удаляет их, но live enable снимается.

## Ревью и проверки

- Отдельная типизированная decision не позволяет использовать присланный
  `enabled: true` как внутреннее разрешение; есть compile-fail regression.
- Route включает transport и endpoints, а не только названия узлов.
- Post-gate review исправил зависимость schema/policy hashes от serde_json
  `preserve_order`: рекурсивная сортировка keys и golden digest проверяются
  с обоими набором features. Final native повтор выполняет тот же focused profile.
- Concurrent create-only dedup и reopen; corrupt/partial/future receipt,
  symlink target/store и FIFO отвергаются без подмены существующих файлов.
- Policy/context/build/hop/schema/profile/check changes и отсутствие route/policy
  отклоняются; enabled guard выдаёт причины до dispatch.
- Новый Windows профиль `ReadinessOnly` проверяет pure policy, frozen wire,
  настоящий Info/contract-2 IPC, settings save/restart и desktop compile.
  Он использует только compiler cache прежнего verified full-CAS run;
  source, test state и reports всегда новые. Installed runtime/state не меняются.

Следующий пакет A3.2b: producer + controller/MCP readiness observation и UI
основания разрешения. Он требует установленного worker с `projectPolicies`/`contractSchemas`;
отпечаток старого provider не подставляется из Debian config или candidate tests.
Новый worker устанавливается по прежнему firewall path. Ручной UI checklist
для этого перехода не требуется.
