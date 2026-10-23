# U2.2: actual replacement fault matrix — подготовлена

2026-10-23. [Positive Main15 gate](windows-runtime-replacement-main-gate-2026-10-23.md)
пройден60/0. Подготовлены четыре actual fault cases; **новая матрица ещё не
исполнялась на Windows**. Схемы updater15/worker16, supervisor17/view9 прежние;
public install выключен. Release path не получает новый fault API.

## Cases

| Case | Инъекция | Ожидаемый сохранённый prefix / физическое состояние |
| --- | --- | --- |
| ExistingDirectory | До admission создать displaced directory с sentinel | Planned→Indeterminate1, без adoption/удаления; все old/prepared files прежние |
| LockedOldFile | Сторонний no-delete read handle старого worker | Planned→Indeterminate1; original state/instance locks и partial acquisitions resident. Release чужого handle не разрешает retry |
| ReceiptWriteDenied | SQL trigger игнорирует retired receipt4, но разрешает последующий Indeterminate | Saved RetireIssued3→Indeterminate4; old worker фактически в displaced, пять новых files ещё prepared. Снятие trigger не разрешает новый move |
| HoldBeforeReceipt | После verified OS retire первого worker ждать до owned Job kill | Journal остаётся RetireIssued3; marker содержит observed afterEffect4, но не commit/permission. Cold projection Indeterminate/custody=false, replay запрещён |

Три обычных ошибки возвращают retained failed guard supervisor; Root остаётся
Replacing. Crash case прекращает только собственный test Job при активном
resident guard. Все cases проверяют exact source/old/prepared File IDs/hash/size,
backup/preimages/worker DB preservation, live/cold status и обычный startup refusal.
Если worker уже retired, отдельный обычный запуск этого retained old worker должен
отказать по worker DB maintenance guard, не из-за отсутствующего executable.
Это не автоматический rollback и не запуск приложений проекта.

## Код и ограничения

- `apps/worker/tests/update_shutdown/replacement/faults.rs`: инъекции, наблюдение,
  дубликат после снятия причины, post-owned-exit проверки. Root/cases/source caches
  изолированы; runtime/config/journal установленного d052 не меняются.
- `apps/update/src/runtime_replacement/validation.rs`: bounded120-second barrier,
  только `cfg(all(windows, feature="update-validation"))`, clean native build.
  Input<=8192 bytes, immutable pin и полная привязка update/launch/owner/epoch/
  config/runtime/preparation attempt. Он лишь задерживает **уже проверенный** move
  перед receipt; не выдаёт execution authority. Original guard/owned handles
  остаются на stack, fence продолжает проверять owner и исходные state locks.
- `scripts/test-update-health-actor.ps1 -RuntimeReplacementFaults` и
  `scripts/validate-update-health-actor-native.py --runtime-replacement-faults`:
  оба архива требуют15/16, выбирается updater с validation feature. Четыре полных
  Main cases идут последовательно с одним cache lease; task timeout1 hour.
- `scripts/update-replacement-faults-proof.py`: independent schema/causal prefix,
  physical file locations, exact source/backup/preimages, withheld receipt marker,
  root CAS и отсутствие adopted custody. Header `runtimeApplied:null` означает
  partial/unknown isolated file effects, `installedRuntimeApplied:false`.
- `scripts/test-update-replacement-proof.py`: выбирает verifier по kind actual
  proof и проверяет намеренные искажения; original positive audit44/44 повторён.

**Не объявлять ожидаемые ошибки success whole updater.** Native test passed будет
означать только корректное поведение этих fault cases. Activation/local health/
controller confirmation, candidate updater feature compatibility и U3/U4 остаются.
На момент подготовки Windows cross-Clippy, Linux all-target Clippy и production
Windows check (без validation hook) прошли; package/protocol local tests прошли,
Python schema admission9/0. Exact окончательные counts/logs — в handoff.

Следом clean commit/export, затем новый native run с новым namespace и тем же
cache20261017-061527-f4639971. Проверить ready files дважды и actual proof mutation
catalog; не повторять terminal positive task665e7e2f или failed cf49a541. До нового
run измерить диск Debian/Windows, минимум6 GiB; retained Windows directories не
удалять. Worker оставить, desktop не нужен, UI-окон не будет.
