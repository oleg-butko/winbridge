# Совместимость multi-controller с main ba9557b

2026-10-27. Продолжение [предыдущего review](multi-controller-review-2026-10-27.md).
Проверяется feature source `2ececad` и main
`ba9557b3aab7c556efef9fc33c35246cd12edae6`.
Main checkout обновлён `git pull --ff-only origin main` сc4b7aea.
Feature и main не слиты; Windows не использовался.

## Новые изменения main

С прошлого review6 коммитов,22 файла,2073 добавленных/44 удалённых строки:

| Коммиты | Изменение |
| --- | --- |
| `76ede7a` | Зафиксирована native проверка layout и prerequisites archive; это source-pin-specific evidence. |
| `c56f018` | Архивирование retired activation runtime под original resident slot lease, native ownership/path/process checks и receipts. |
| `bb80fff` | Полный Main25 test: после writer confirmation немедленный service start на том же child, lost ACK без replay, normal IPC/history/pause/refusal и cold exit. Добавлен network preflight до supervisor admission. |
| `8a4e10b` | Producer/target source pins и раздельная цель target build. |
| `38dfe85` | Cargo artifact с hard links хешируется как bounded reproducible input; strict single-link runtime image guards сохранены. Independent Main25 audit и3 Python boundary tests. |
| `ba9557b` | Native cache regression1/0 прошёл; полный Main25 ещё не пройден. Сохранённый slot требует отдельной разовой conservation операции владельца; здесь она не выполнялась. |

Схемы не менялись: main worker17/updater20, наша ветка worker19. Public updater
и installed worker0.1.1 не обновлены этими source изменениями. Shared queue,
scheduler, RPC/DTO и multi-controller configuration в новых6 commits не менялись.

## Результат совместимости

Архитектурного противоречия с1–4 controller нет, но обычное автоматическое
слияние не готово. Сохраняются5 текстовых конфликтов: README, worker lib,
validation, handoff и roadmap. Нужна прежняя адаптация shared startup/local-only
service с сохранением запрета network/scheduler/mutations до release.

**Новая подтверждённая compile incompatibility:** после прежнего integration patch
проверка `cargo check -p winbridge-worker --tests --features update-validation`
отказывает с `E0609` в `apps/worker/src/update_control/validation/network.rs:36`:
старое поле `worker.controller_connection` удалено в feature. Git этот новый файл
объединяет автоматически, поэтому только разрешения текстовых конфликтов недостаточно.
Лог: `/tmp/winbridge-main-refresh-before.log`.

В исправленном probe preflight читает primary connection из worker Links. При этом
он явно требует legacy `controllerUrl`, совпадающий с declared fixture URL, и
отвергает topology. Это не новый multi-controller activation grant и не молчаливый
выбор одного из нескольких endpoint. Сохранены900s ожидание firewall до admission,
строгие URL/session/generation/freshness checks и исходный activation budget240s.

Добавлен `preflight_reads_legacy_link_but_never_adopts_a_topology_primary`:
legacy connecting/connected states читаются, чужой URL отклоняется, конфигурация
с3 controller и primary третьим не принимается как single-route fixture.
Прежний source patch дополнен этим изменением и тестом:
[полный patch для новых pins](multi-controller-main-refresh-ba9557b.patch).
Старый patch остаётся историческим для c4b7aea.

**Отдельная несовместимость Windows audit:**
`scripts/update-activation-service-proof.py` на строках71,123,149 явно ожидает
worker journal17/updater20. Он правильно откажет proof с worker19. Перед native
Main25 из объединённой ветки нужна явная адаптация verifier к закреплённой схеме
нового producer и новый positive proof; здесь это не ослаблялось. Нельзя глобально
заменить17 на19: например preparation view17 — другой, frozen контракт.
Actual worker17→19 migration через updater и updater19→20 proof остаются разными
задачами. Прежний single-controller/schema17 native proof не подтверждает их.

Native layout/archive prerequisites и cache regression из main полезны нашей ветке,
но не подтверждают новый worker19,3/4 Debian или полный same-child Main25.

## Проверочное объединение

Detached worktree `.winbridge/review-multi-controller-main-refresh-20261027`.
Auto-merge tree `666d10b2ef2ad39662170341ed8ba1145f019f5a`; source patch покрывает
все6 изменяемых Rust files, включая новый network preflight. Документальные
конфликты в probe разрешены feature-текстом ради compile/test; при реальном merge
нужно сохранить оба актуальных checkpoint. Это не окончательная merge documentation.

Воспроизведение только в отдельном worktree от feature2ececad:

```sh
test "$(git rev-parse origin/main)" = ba9557b3aab7c556efef9fc33c35246cd12edae6
git -c merge.conflictStyle=merge merge --no-commit --no-ff origin/main
# Конфликты ожидаемы; затем применить полный новый patch.
git apply /absolute/path/to/multi-controller-main-refresh-ba9557b.patch
git add apps/worker/src/lib.rs apps/worker/src/startup.rs apps/worker/src/local_service.rs apps/worker/src/update_control/validation.rs apps/worker/src/update_control/validation/network.rs apps/worker/src/store/activation_service_open/tests.rs
# README/handoff/roadmap разрешаются отдельно.
```

Patch рассчитан на эти source pins и обычные merge conflict markers; его нельзя
применять к standalone feature или installed runtime. Проверка reverse apply на
исправленном probe прошла. Исправленный source tree:
`6cae382779cd40761ff9bea4685fa0bb71ee2f33`. Это не release build identity.

## Проверки и свободное место

Только Linux owned fixtures/loopback и cross-target compilation. Windows worker,
его config/journal/runtime/firewall и архивные slot files не опрашивались/не менялись.

Перед сборками Debian8.8 GiB; во время полной feature build стало2.8 GiB.
14 updater tests отказали с `Preimages deferred: preserve 6 GiB plus copy bytes`
и `Preparation needs 6 GiB free plus all independent copies` — сработала защита
свободного места. Новые компиляции остановлены; первая попытка Clippy прекращена.
После завершения процессов удалены184 старых неиспользуемых воспроизводимых ELF
cache files только из `target/debug/deps`, выбранных старше начала этой проверки.
Active executable inodes, текущие test outputs, root controller binary, configs,
journals, reports и все Windows каталоги сохранены. Controller PID5312 продолжил
работу. Освобождено примерно10 GiB, available стало13 GiB.
Manifest удаления: `/tmp/winbridge-main-refresh-cache-cleanup.json`, факт удаления:
`/tmp/winbridge-main-refresh-cache-cleanup-completed.json`. Это generated cache,
backup для него не требовался. Test logic/space guards не изменялись; повторный
прогон выполняется после cleanup. CARGO_INCREMENTAL=0, основной target reused.

Финальные проверки исправленного probe после cleanup:

- `cargo test --workspace --features winbridge-worker/update-validation --locked`:
  **894 passed /17 штатно ignored**,120 groups. Network preflight, staged local-only
  service и actual3/4-controller tests прошли; повторных отказов нет.
- Workspace Clippy all-targets с update-validation и `-D warnings` прошёл.
- MSVC workspace all-targets compile check с update-validation прошёл. Это
  компиляция Windows веток, не выполнение native сценария.
- Rust→TypeScript export `--check`, TypeScript typecheck/UI build, fmt/diff check
  и scaffold301 probe прошли. Feature documentation отдельно:scaffold292 passed.
- Python Main25 evidence boundary tests:3/0. Это проверка формата и отказов,
  не новый positive native audit. Source patch reverse apply check прошёл.

Логи: `/tmp/winbridge-main-refresh-tests-retry.log`,
`/tmp/winbridge-main-refresh-clippy-retry.log`, `/tmp/winbridge-main-refresh-msvc.log`,
`/tmp/winbridge-main-refresh-contracts.log`, `/tmp/winbridge-main-refresh-ui.log`.
Исходные ошибки/первый тестовый лог сохранены отдельно; они не выданы за успешный run.
Никакой Windows/native readiness этим review не добавляется.
