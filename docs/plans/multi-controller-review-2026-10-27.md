# Review multi-controller и совместимость с main — 2026-10-27

Проверены source `ac9e0cbf4845061fc7ce278383952400b58ba0b7` (`feat/multi-controller`)
и обновлённый `main` `c4b7aea23d15bda4827d853e4c273354702eb596`.
Main checkout обновлён `git pull --ff-only origin main` с9b47a9c. Работающая
Windows установка, controller/runtime configs, firewall и сервисы не изменялись.

## Результат review

Подтверждённых существенных дефектов в самостоятельной multi-controller ветке
не обнаружено. Проверены захват неизменного RPC origin, общий/per-peer budget,
удержание permits после timeout/disconnect, раздельные connection/projection
срезы, дедупликация acceptance, атомарность creator/cancellation metadata,
workflow inheritance, readonly dispatch CLI/MCP и отказ при изменившейся scope.

При объединении с main есть обязательная адаптация к новому startup/local-service
пути. Это не автоматически совместимое слияние: Git сообщает5 конфликтующих
файлов. Временное объединение подготовлено и проверено отдельно; main и feature
не слиты. Готовые source исправления и новый regression test сохранены в
[patch](multi-controller-main-integration-2026-10-27.patch).

## Что пришло в main

13 коммитов,97 файлов относительно9b47a9c:

- `5c9e965`, `f145b62`, `1f653b2`: service intent/current/history в updater schema20,
  consuming admission от original Fresh и передача того же connection/instance lock
  в shared Worker startup. Повторное открытие Store/новая authority не разрешены.
- `15429a4`, `5a0f83d`: временный local-only worker с настоящим IPC, trusted V4
  sender/receiver, независимая parent normal-IPC проверка; actor25/view17 и atomic
  LocalVerified/AwaitingController. Controller и scheduler пока не разрешены.
- `35e3e52`, `f0dd146`, `403a4fd`, `c4b7aea`: стабильный Windows activation runtime,
  явный FixtureLayout/private input2, отдельный declared controller URL/port1144,
  Unicode source manifest и согласованные canonical Windows paths.
- `cca3b97`: явное освобождение flock завершённого log writer, даже если дочерний
  процесс сохранил alias его file description. Это полезно и нашим Linux fixture tests.
- Остальные коммиты содержат proof/checkpoint и исправления validation. Полная
  same-child Main25/service проверка ещё не объявлена завершённой в main handoff.

## Обязательные изменения при слиянии

| Область | Совместимость / решение в проверочном варианте |
| --- | --- |
| `apps/worker/src/lib.rs` | Конфликт удаления старого constructor и нового dispatch/network gates. Использовать main `startup.rs`, сохранить attributed dispatch и общий network supervisor. Для `local_activation` не запускать controller connections/update-control/scheduler, не принимать мутации и не рекламировать workflow availability. |
| `apps/worker/src/startup.rs` | Текстового конфликта нет, но старый `from_parts` создаёт удалённые singular connection/projection fields. Подготовить `Links` с текущей generation для legacy/topology и передать их через WorkerParts. |
| `apps/worker/src/local_service.rs` | Подготовить suspended Links до consuming transfer. Ошибка topology/identity оставляет exact original Prepared owner в FailureOwner; не добавлять unwrap/panic или повторный Store open. Все configured links остаются disconnected с attempts0 и явной причиной. |
| `apps/worker/src/update_control/validation.rs` | Сохранить main declared URL equality и одновременно запрет multi-controller topology для single-route isolated fixture. Нельзя оставить старый unconditional URL=None либо разрешить недекларированные связи. |
| Worker journal | Main17, feature19. Это отдельная схема от updater20. Same-schema service/activation продолжают точный header check; cold17/18→19 backup и запрет migration при active update сохраняются. Native updater migration17→19 не объявлять реализованной. |
| CLI/MCP readonly, protocol, TS | Main не менял новые три readonly routes. DTO exports/TypeScript объединились автоматически; generation/scope/provider checks сохранены. |
| README/handoff/roadmap |3 текстовых конфликта. При реальном слиянии объединить текущий updater checkpoint и feature readiness. В compile probe оставлен feature текст; это не итоговая редакция merge documentation. |

Готовый source patch покрывает первые4 строки и regression test; он рассчитан
строго на эти два source pins после обычного merge с конфликтами. Не применять
его к installed runtime или к самостоятельной feature без main.

Воспроизведение в новом отдельном worktree от source `ac9e0cb`:

```sh
test "$(git rev-parse origin/main)" = c4b7aea23d15bda4827d853e4c273354702eb596
git -c merge.conflictStyle=merge merge --no-commit --no-ff origin/main
# Код1 ожидаем: конфликтующие файлы сохранены для разрешения.
git apply /absolute/path/to/multi-controller-main-integration-2026-10-27.patch
git add apps/worker/src/lib.rs apps/worker/src/startup.rs apps/worker/src/local_service.rs apps/worker/src/update_control/validation.rs apps/worker/src/store/activation_service_open/tests.rs
# README/handoff/roadmap разрешаются отдельно с сохранением обеих текущих целей.
```

Проверочный auto-merge tree: `5e30bf3f02ab277e5b971268971ea4b286acf727`;
исправленный probe tree: `ebecc8358dcc385cf38c62a330eae997b94ea5d5`.
Это source trees, не release build identities, не новые execution permissions.

## Проверки

Временный worktree: `.winbridge/review-multi-controller-main-20261027`.
Только owned Linux fixtures и loopback; Windows не опрашивался и не запускался.
Новый `staged_service_preserves_multi_controller_diagnostics_without_network_or_dispatch`
проверяет два listener, настоящий readonly local IPC, attempts0/disconnected,
отказ submit, отсутствие входящего TCP и освобождение exact instance lock при close.
Существующие tests по-прежнему проверяют обычный runtime с двумя live controller,
A submit/B cancel, общий project lease, reconnect и CLI/MCP.

Результаты исправленного probe (не самостоятельной feature):

- `cargo test --workspace --locked`: **879 passed /17 штатно ignored**,120 groups.
  Новый staged-service/two-controller test прошёл. Ошибок и повторных прогонов нет.
- `cargo check --workspace --all-targets --features winbridge-worker/update-validation --locked` прошёл.
- Workspace Clippy all-targets с `winbridge-worker/update-validation` и `-D warnings` прошёл.
- MSVC workspace all-targets check с тем же feature прошёл; это compile check,
  не выполнение Windows программы.
- Rust→TypeScript export `--check`, TypeScript typecheck, UI build, fmt/diff check
  и scaffold298 прошли. UI build сообщает прежний advisory о размере JS chunk.
- Source patch прошёл `git apply --check --reverse` на исправленном probe.
  Логи находятся в `/tmp/winbridge-review-merged-{check,tests,clippy,msvc,contracts,ui}.log`.

Feature documentation после добавления этого review проверяется отдельно; новые
capabilities, Windows readiness и migration permission этим отчётом не добавлены.

Перед сборками Debian free9.0 GiB; во время checks8.8 GiB.
`CARGO_INCREMENTAL=0`, reused `/home/user1/winbridge/target`, build caches не очищались.
Windows native gate19, service/Main25 и migration через updater остаются отдельными
незавершёнными проверками. Установленный worker0.1.1 остаётся без изменений.
