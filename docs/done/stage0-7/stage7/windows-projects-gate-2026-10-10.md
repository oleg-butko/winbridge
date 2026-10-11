# Windows gate: два прикладных проекта — 2026-10-10

**Passed. Этап 7 завершён.** [Сценарий и статические контракты](../../../guides/project-integration.md)
проверены на настоящей Windows через установленный worker, без desktop и ручных
шагов. Два проекта используют общий build profile и прежний task service.

Run `20261010-182417-737310b6`, outer task
`3f8a8caa-14cc-464d-b8c3-00c7ab2ade24`, succeeded/exit 0.
Report: `K:\VMs\shared\WinBridge\workflow-validation\20261010-182417-737310b6`.
15 artifacts проверены из Debian по actual bytes/SHA-256.

- report.json: `cd11f064976133f16a8143c53e96c2a3ac372fad548c6e7cf8449970f35f2b16`.
- report.ready.json: `31138a7df7ecad56cb9685fb596fe45e9c92750cf23ee88c740ce182c3a69f5c`.
- projects-proof.json: `b34873ce2c5b58cc3321fd244f333bc72ad370f8f232e2c98599eb3e53e47776`.

## Код и изоляция

Native archive `winbridge-20261010T222255Z-6e38c55-899937b3.zip`, clean
`6e38c5516ddf4f9110ba5d41a439d9e2bbf8cd4b`, ZIP SHA-256
`a565525cc4250946bbde5a35b2e71af8da76206b4d71915d58a8cc8759232946`.
Coordinator build ID `1791671102072798700`, source manifest SHA-256
`9e3cf2d8028b219fd4359ab8794be3706e4458fe6d3ca000c4eaa9e7e6474b95`.

Каждый проект имеет отдельные local source/runtime/cache/logs. Test coordinator
и worker application имеют новые instances/SQLite и controllerUrl=null. Netwatch
role desktop, peers empty, private instance; production pipe не использовался.
Source clones/validation commits/patches и Debian ready pins сохранены в
`WinBridge/projects-validation/20261010-182417-737310b6/preparation`.

| Проект | Base code commit | Точный validation commit | Manifest SHA-256 |
| --- | --- | --- | --- |
| Netwatch agent | 39409e56a26ef438adab3488afea504ef40f1363 | e0deb1fb31e495316fb869e02f793bec18f08c60 | 623d37efbda1d963cd2ec2cb847b88ab0b5af7bf3737ba29c01d639fa60ca6be |
| WinBridge worker | 6e38c5516ddf4f9110ba5d41a439d9e2bbf8cd4b | 078bf7467cfc7ecade45bdc3de3dd1b4988626ee | a3cfeed801a3a57353d04146f6d728c2630e49f21431362364c1b9038359da4c |

WinBridge application build указывает base commit и sourceDirty=true: дополнительные
adapters прозрачны, их не выдаём за unmodified release. Source manifest SHA-256
`8f321c0a92ed03159ca10d33051f7440f7b31c6e3203193400f54fec9104daa3`
сверен с подготовкой. Build artifacts привязаны к точному validation commit/pin.

## Реальные результаты

59 native Rust tests: 34 core/runner/protocol, 25 profiles/workflows. Включены
actual IPC, direct prohibition, same-key/conflict, Unicode argv, profile edits,
queued/recovered recipe guards, cancel, owned crash, failure rollback и recovery.

| Проект | Workflow ID | Build task ID | Результат |
| --- | --- | --- | --- |
| Netwatch | 75b3ad74-86b9-41a1-90c3-d044bf261797 | 71de08e6-04b1-494c-ac25-ac570f7f6d89 | import/build/artifact/start/owned-PID IPC/log succeeded; приложение явно stopped |
| WinBridge | de20e562-89ef-4feb-ada9-605426fe21fe | 6c52fc7f-144f-4cba-82e1-dc24c192b414 | import/build/two artifacts/start/readiness/log succeeded; private worker IPC проверен; приложение явно stopped |

Оба build tasks сохраняют recipeOrigin.profileId=rust-service. Start recipes
локальные; recipePins и actual generation cwd сохранены. Повтор workflow key
до/после completion и после restart возвращает прежний ID, не создаёт процесс.
Coordinator generation меняется
`272867b4-cebf-49e8-9c9c-08216d04001c` → `98cb869a-eef4-496b-91e1-0d2f67f708ce`;
startup history succeeded, приложения stopped, build commands/origins неизменны.

Запущенная worker application действительно обслуживает свою очередь/IPC:
fixture success task `35fb4854-a4ae-4723-bf77-c67d1cea5d19` succeeded/exit 0.
Tree task `742669a2-3d8e-4002-89e3-af43369f7f47`, PID 20448, child PID 20084.
Остановка application завершает wrapper/worker/fixture/grandchild; IPC становится
недоступен. После **явного** restart private worker её generation меняется
`233b0018-b980-4c0d-97fa-27f3b5ed2db0` → `8d6387c0-1391-4f3d-b75f-5db862d54e46`.
Прежний tree key возвращает тот же task ID со статусом indeterminate; success key
сохраняет успех. activeTaskCount=0, автоматического повторения нет.

Все тестовые процессы остановлены. Installed Windows worker clean fa7e4d7
сохраняет generation `372266b6-3dee-4d87-9de2-ccab896dbcf8`; final activeTaskCount=0,
maintenanceError/journalRecoveryRequired=null. Его exe/config не заменялись.
Исходный Netwatch и пользовательские файлы не редактировались.

## Сохранённые неудачные попытки и пределы проверки

Debian preparation `20261010-181635-261550f8` завершилась до Windows submit из-за
quoted Unicode Git paths. Windows run `20261010-181812-f65b8114`, task
`f253d60d-3400-437d-8e70-d5a2c388f750`, failed: оба application cycles/stop уже
прошли, затем harness передал null PID в Get-Process. 11 artifacts проверены и
сохранены. Оба дефекта исправлены, старые каталоги/keys/results не переписывались;
финальный gate использует новый explicit request. [Ревью](review-projects-2026-10-10.md).

153 workspace Rust tests, workspace Clippy/all-targets Windows check, final
protocol check/regression, fmt, DTO/scaffold, TypeScript и UI build прошли.
Новая подпись unknown source provenance проверена typecheck/build; отдельное
native desktop окно не открывалось. Общие UI timeline/profile/pins уже проверены
первым [profile gate](windows-profiles-gate-2026-10-10.md) и browser integration.
SDK/dynamic plugins не добавлены: независимо поставляемого consumer пока нет.
