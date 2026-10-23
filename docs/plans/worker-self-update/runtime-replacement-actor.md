# U2.2: replacement driver, supervisor v17 и передача контекста

2026-10-23. Это текущая точка продолжения после [custody library](runtime-replacement-custody.md).
Реализован внутренний путь от свежего admission до переноса файлов и
`Replacing → VerifyingLocal`. **Actual Main replacement gate ещё не запускался.**
Windows cross-check не подтверждает исполнение native IO. Updater целиком не готов,
публичные install/CLI/MCP capability выключены; установленный Windows runtime не менялся.

## Что изменилось

Контракты: updater journal **15**, worker journal **16**, новый supervisor **v17**,
projection **view9**. Схема SQLite не изменилась. Внутренние actions:
`ReplaceRuntime` и `ObserveRuntimeReplacement`; typed client —
`SupervisorHandle::replace_runtime(start)`. Нет downgrade/fallback на v16.
Запрос не принимает пути, разрешения, receipts, параметры resume или executable.
Старые actions v1–v16 и views1–8 сохраняют прежнюю форму; новое optional поле
`runtimeReplacement` в них не сериализуется. TypeScript DTO обновлены из Rust.

Карта кода:

| Файл | Ответственность |
| --- | --- |
| `apps/update/src/runtime_replacement/driver.rs` | Один blocking executor, directory intent/receipt и пять retire/place пар |
| `apps/update/src/runtime_replacement/physical.rs` | Исходные process/state locks и owned file handles, физические проверки, live custody signal |
| `apps/update/src/journal/runtime_replacement.rs` | Admission/CAS/history и единственный trusted `finish_runtime_replacement` |
| `apps/update/src/supervisor.rs` | Admission-before-ACK, work slot, дубликаты, resident guard, typed v17 client |
| `apps/update/src/journal/supervisor.rs` | Согласованный readonly view9 из одного SQLite snapshot |
| `crates/protocol/src/update/runtime_replacement.rs` | DTO phases/progress/custody, без executable authority |
| `apps/update/src/runtime_replacement/tests.rs` | Статусы active/cold/failed/completed и консервативная CLI диагностика |
| `crates/protocol/tests/update_health.rs` | Строгий v17 request без caller evidence |

Последовательность исполнения:

1. Supervisor проверяет exact v17/launch/revision/nonce, Active bound worker,
   свободный executor, Prepared files, Verified final backup и original custody.
2. Blocking admission под immutable loan сохраняет intent до ACK. Existing
   возвращает прежнюю запись, fresh permit выдаётся один раз. Loan освобождается
   до unique transfer; потеря ACK не отменяет уже сохранённую операцию.
3. Fresh permit потребляется blocking driver. Тот забирает **тот же** original
   `ShutdownExitGuard`, приобретает owned handles через проверенную library и
   сохраняет directory/file intents до соответствующих эффектов.
4. Пять файлов переносятся фиксированно: worker, fixture, desktop, updater,
   manifest. Старый файл уходит в `runtime/update/<id>/displaced-runtime`, новый
   переносится из `prepared-runtime` на постоянное имя runtime. Итоговая revision22
   означает пять retire и пять place receipts; manifest переносится последним.
5. `finish_runtime_replacement` требует exact FilesMoved record и живой guard,
   проверяет весь физический набор и original scope. Только затем в одной SQL
   транзакции меняет root на VerifyingLocal и пишет FilesVerified transition.
   Общий raw lifecycle по-прежнему не принимает FilesVerified/activation events.
6. Blocking result возвращает boxed guard supervisor. Guard остаётся resident
   и при обычной ошибке приобретения/переноса. Неизвестный эффект сохраняет
   исходный prefix как Indeterminate; автоматического retry/adoption/cleanup нет.

`FilesMoved` описывает файлы. Это **не** LocalHealthVerified, ControllerConfirmed,
Committed, восстановление принятия задач или запуск новой версии.

## Статус и исправление, найденное при ревью

View9 сообщает attempt/revision, directory, retired/installed count, pending
retire/place и reason. `physicalCustodyHeld` берётся из живого guard через atomic
signal, не восстанавливается из JSON/SQL. При cold read незавершённая операция
показывается Indeterminate без повторного исполнения. Завершённый FilesMoved
сохраняется как исторический факт, но cold custody=false. Ошибка может сохранить
custody=true, пока original locks физически удерживаются.

Обнаружено, что readonly CLI `journal status` и supervisor inspection жёстко
возвращали `runtimeApplied:false`. После подключения driver это стало бы ложным.
Nullable поле выделено в **JSON schema2** для `update_journal` и `supervisor_status`,
чтобы не менять значение старого boolean-контракта молча. Теперь оно консервативно
следует root phase: false до cutover, null для
Replacing/ManualIntervention и отсутствующего выбранного root, true для
VerifyingLocal/AwaitingController/Committed. Это наблюдение о файлах, не заявление
об activation. Функция exhaustive по enum, есть regression test.

## Проверки этой точки

Локальные проверки закончены; активных Cargo/native runs при фиксации нет.

| Проверка | Результат |
| --- | --- |
| `CARGO_INCREMENTAL=0 cargo test --workspace --locked` | **653 passed /0 failed /16 ignored** |
| Адресный CLI integration test после выделения JSON schema2 | **1 passed /0 failed**; source/config/runtime bytes сохранены |
| Linux all-target Clippy: update/worker/protocol, `--features update-validation -- -D warnings` | Passed |
| Тот же Clippy с `--target x86_64-pc-windows-msvc` | Passed; это cross-check, не native execution |
| `cargo fmt --all -- --check`, `git diff --check` | Passed |
| Генерация TypeScript и `npm run check:contracts` | DTO совпадают с Rust |
| `npm run typecheck`, `npm run build:ui` | Passed; прежнее предупреждение Vite о размере bundle |
| `python3 scripts/test-update-health-native-runner.py` | **7 passed** |
| `python3 scripts/test-native-preparation-runner.py` | **4 passed** |
| `npm run check:scaffold` | **244 Markdown files**, ссылки/примеры проверены |

16 ignored — существующие явно запускаемые native/parent-crash/старые executable
fixtures; они не объявляются пройденными новым локальным прогоном. Новые проверки
покрывают cold/live custody, pending retire/place counts, historical completion,
консервативную диагностику, strict v17 DTO и отсутствие нового поля во views1–8.

В первом полном прогоне synthetic baseline fixture получила OwnerBusy сразу
после `drop(owner)`. Отдельно test прошёл; причина совместима с кратковременно
унаследованным Unix fd при параллельном fork/exec. Fixture **без запуска настоящего
supervisor** теперь явно unlock-ит свой lock перед simulated handoff. Production
owner checks/ownership/retry policy не менялись. Затем полный прогон прошёл.
После изменения nullable CLI contract обновлён integration assertion: отсутствие
выбранного update не доказывает неизменность runtime; фактические bytes всё так же
проверяются отдельно. Для окончательной schema2 CLI test повторён адресно.

Логи в ignored `.winbridge/update-preparation/`:
`replacement-actor-workspace-tests.log`, `replacement-actor-cli-test.log`,
`replacement-actor-linux-clippy.log`, `replacement-actor-windows-clippy.log`,
`replacement-actor-contracts-check.log`; машинная сводка —
`replacement-actor-local-verified.json`. Они локальные, не входят в Git. Если их
нет в следующей среде, выполнить указанные команды заново.

Перед тяжёлыми проверками Debian free **27,080,372,224 bytes** (>6 GiB).
Окончательное измерение — в local verified summary. Очистка не требовалась.
Нового Windows build/run в этой точке не было: свежего Windows disk measurement
нет, его обязательно снять перед следующим native build.

Readonly повторно подтверждены terminal succeeded/0 обоих последних schema15
Windows tasks из таблицы ниже. Все **12 ready files проверены дважды**, report SHA
совпали, actual migration proof ещё раз прошёл текущий verifier. Installed worker
idle/healthy, recovery coverage complete. Tasks не пересылались, runtime не менялся.
Не считать эти metadata/model/migration gates проверкой нового driver.

## Окружение и готовые доказательства

Controller запускается после перезагрузки Debian:

```bash
./target/debug/winbridge-controller --config .winbridge/controller-snapshot.local.json serve
```

Listener `192.168.44.203:1143`, IPC
`.winbridge/controller-session/controller-local-development.sock`.
В этой сессии controller запущен заново; installed worker подключён, sourceCommit
`d052c6ce6a61c7fb6fa4c9ffa879363010fb6fd2`, generation
`8f8d1fb9-e5b1-4984-a17e-d067db46268e`, maintenance/recovery errors отсутствуют.
Меняющийся session/generation после запуска не означает обновление binaries.
Перед следующим run заново прочитать Info и убедиться в idle/recovery coverage.
Desktop не нужен; тестовых окон в этой точке не запускалось. Installed worker оставить.

Терминальные Windows task **не отправлять повторно**:

| Доказательство | Task / run | Граница доказательства |
| --- | --- | --- |
| [Schema15 metadata](windows-runtime-replacement-ledger-gate-2026-10-23.md) | `e292eb2f-33b6-4d7f-b281-a954b5f24d63` / `20261023-091331-b6f35ac8` | 11 data/SQL tests, без physical handoff |
| [Actual14→15/refusal](windows-replacement-migration-gate-2026-10-23.md) | `11987f79-3786-4155-aa39-6277d74d6950` / `20261023-092901-d1c837e1` | 1 integration test, 27 искажений proof отклонены; без fresh permission |
| [Actual preparation](windows-runtime-preparation-main-gate-2026-10-22.md) | `d9805159-8804-486a-8aae-95ddb6e12e79` / `20261023-012740-9feb95b9` | Schema14 Prepared copies, не replacement |
| [Preparation faults](windows-runtime-preparation-faults-gate-2026-10-23.md) | `50979d1f-6f38-4d0a-8f15-6e96a243222d` / `20261023-081649-b9bf5a72` | 40 tests /3 actual faults, 48 negative proof mutations |

Полные pins/proof сохранены в ignored `.winbridge/update-preparation/*-native-verified.json`.
Ссылки выше содержат SHA и исходные архивы. Исторические NEXT/status в более ранних
разделах handoff не актуальны; текущая инструкция — этот документ.

## Что делать следующим

Продолжение этой сессии: подготовлен **отдельный positive gate Main15**, ещё не
исполненный на Windows. Native helper — `apps/worker/tests/update_shutdown/replacement.rs`;
новый ignored test — `actual_main_and_supervisor_replace_runtime_without_activation`.
PS mode `-RuntimeReplacementGate`, Python mode `--runtime-replacement-gate`.
Оба архива явно требуют updater15/worker16; старые preparation modes сохраняют14/16.
Verifier `scripts/update-replacement-proof.py` независимо восстанавливает все23
состояния из immutable before-images, проверяет exact File IDs/paths/digests,
root CAS, before/after custody и отсутствие activation. До-stage proof вынесен в
`beforeReplacement`, чтобы не заявлять неизменность runtime после переноса.
Тест освобождает временный instance lock до ordinary startup refusal; иначе он
мог бы подтвердить неправильную причину отказа. Windows cross-Clippy passed,
Python archive/schema admission9/0. Native positive результат пока отсутствует.
Пункты1–7 ниже описывают требования к этому gate; implementation flag уже есть.

1. **Ревью окончательного driver/actor и actual Main15 positive gate.** Расширить
   `apps/worker/tests/update_shutdown.rs`, `scripts/test-update-health-actor.ps1`
   и `scripts/validate-update-health-actor-native.py` отдельным replacement mode.
   Новый replacement mode подготовлен выше; не запускать старый preparation gate на новых15
   архивах: он намеренно требует updater14/worker16.
2. Producer — чистый commit с новым actor17; target — отдельный чистый schema15
   архив `winbridge-20261023T091226Z-5fbc1cf-b4ac44a5.zip` из shared sources.
   Target SHA `3891ad4689779c2eb2507b70be4ad5d2f599cf4816fb4b882d64f2de0006c6fb`,
   manifest SHA `98e52e10e99e25fbc8cf9864b4af3e4d83419e8a1297cbcd330dc61e5c4546f3`.
   Это **ограниченный fixture**, не production install target. Совпадение схем
   не доказывает updater features. Прежний cee06a7/schema14 target не использовать.
3. Сохранить старую цепочку real old roles → actual candidate build/health →
   actual Main drain/shutdown → final backup → prepared copies. Старый cold
   shutdown reader/restart refusal/old hashes проверить **до** replacement:
   затем старые имена закономерно меняются и прежний full-runtime guard неприменим.
4. Отдельно проверить v16 отказ до admission, admission-before-ACK, дубликат того
   же attempt, отзывчивое наблюдение прогресса; затем actual old/new File IDs
   во всех десяти конечных путях, пять retained originals, empty prepared dir,
   manifest-last/history22 и root VerifyingLocal/revision+1. Сохранить исходные
   worker DB/WAL bytes, source/candidate payload/previous preimages/final backup.
5. Убедиться в resident custody=true до завершения owned Job. Завершить только
   собственный isolated Job, проверить history FilesMoved/custody=false, затем
   обычный новый worker должен отказать в startup из-за maintenance до DB effects.
   Не реконструировать handles из journal. Не писать в installed runtime.
6. Добавить исполнение Windows directory-preservation test из physical.rs и
   original Arc busy-restoration test. Эти проверки сейчас только локальные
   либо cross-compiled; native исполнение новой Windows directory ветки впереди.
7. Проверить ready files дважды, exact source/target/report/proof hashes,
   отрицательные мутации actual proof. Затем fault matrix: acquisition locks,
   occupied destination, receipt-write rejection, crash на intents/receipts.
   Ни ошибка, ни observation timeout не разрешают повторную отправку эффекта.
8. После этого activation/local health/controller confirmation с тем же retained
   owner и state locks. До production drain добавить versioned candidate-updater
   feature compatibility query, связанную с реально проверенным образом.
9. U3: desktop shutdown/update/restore visibility, migration/rollback и понятные
   popup/tray сообщения. U4: public orchestration и единственный ручной bootstrap
   установленного d052, затем сквозное самообновление/ошибка и manual recovery.

Native build cache: `20261017-061527-f4639971`, один build lease одновременно.
Перед сборками измерить Debian/Windows disk (минимум6 GiB), сохранить в отчёте.
`CARGO_INCREMENTAL=0`, reuse main target; Rust source не менять во время local Cargo.
Windows validation dirs/config/runtime/journal/reports не удалять. При обычной
ошибке isolated сценария исправить и автоматически повторить с новым run ID;
при проблеме installed worker или необходимости его обновления остановить
зависящие Windows действия и сообщить пользователю точную причину.
