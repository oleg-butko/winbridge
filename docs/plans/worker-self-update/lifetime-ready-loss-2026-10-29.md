# Main28: неполный private Ready и позднее подтверждение

2026-10-29. Следующий source шаг после [Main27](main27-native-2026-10-29.md).
Source worker19/updater22/private6/supervisor27/view19 сохраняются. Новых public
RPC/support/schema/actor версий нет. Installed worker0.2.0 не меняется.
**Main28 запущен, пока не завершён:** run20261029-091025-be7fc543,
task393a8003-e229-4b41-93c7-b65eea2fa992. Не повторять submit: читать тот же task.
Run-info `.winbridge/update-preparation/main28-204-20261029-091025-be7fc543/run-info.json`,
log `.winbridge/review-2026-10-29/ready-loss-main28-run.log`. Runner PID247139,
test controller247140 на.203:1144/state `/tmp/wb28-x7geapm4`; production131887
прежний. До terminal + independent reread gate не объявлять пройденным.

Clean producer0426a1338ac8e2ba4fbec36405fd331d44fceaee,
ZIP `winbridge-20261029T090948Z-0426a13-1eb6a3ef.zip`,
SHAe5fb588d41c5f88e1ba460c95650307fd3764e382160bbd1f1c553d07b05e62f.
Distinct target90fc5b4 с pins из Main27 оставлен прежним.

## Fault boundary

Только supervisor с `update-validation` и clean native build может прочитать
`runtime/update/<updateId>/lifetime-ready-loss-validation.json`. Это отдельный
create-new fixture input, не новый production config. Его strict JSON содержит
schema1/kind и canonical SHA всего original LocalVerified7 record. Чужой update,
attempt, keeper, child, Job или изменённый record не принимаются. Старые state/
writer validation inputs и marker bytes не меняются.

После durable Issued и единственного V6 send этот режим читает **только4 bytes**
заголовка private Ready из исходного authenticated stream. Размер должен быть
1..32768; body остаётся непрочитанным. DTO/receipt не декодируются, normal probe
не вызывается. Сам stream остаётся в original owner; Job/file/config/state custody
не освобождается. Время чтения — остаток исходного Instant, marker UTC также
должен находиться внутри original window.

Create-new marker `activation-lifetime-ready-body-loss.json` связывает original
record/command/input SHA, parent PID/build, header length, bodyBytesRead0 и время.
Затем принудительная ошибка проходит существующий uncertainty path:
Issued → Indeterminate, с тем же owner/key и без private resend. Даже неожиданное
Ok helper не может завершить delivery как success. Production build без feature
не читает этот input и не содержит fault branch.

Это **partial delivery на стороне принимающего supervisor**, не доказательство
ошибки записи child или закрытия kernel pipe. Эти другие IO faults остаются
частью U2 matrix. Непрочитанный body не разбирается повторно и не становится
следующей private командой; reconciliation использует отдельный normal endpoint.

## Native fixture

Новый exact ignored test:
`actual_main_reconciles_partial_lifetime_ready_after_original_deadline_without_replay`.
GateInput явно требует ready-loss вместе с lifetime/controller/service; Main25–27
оставляют false. Main28 kind отличается от Main27, SDK фиксирован19/22.

1. После настоящего controller confirmation создать exact-scope input по local7.
2. Отправить Enter один раз и потерять только его supervisor dispatch ACK.
3. Дождаться durable Indeterminate и idle error view, не вызывая Observe, который
   начал бы reconciliation. Saved/observed receipt отсутствуют, child жив.
4. Удерживать исходный owner до original deadline, проверяя того же child. История
   до reconciliation остаётся ровно Issued + Indeterminate.
5. После expiry выполнить fresh normal IPC через original owner. Проверить actual
   same child/Job/Store/authority и immutable entry receipt внутри initial window.
   Только этот независимый факт переводит history в revision2 Confirmed.
6. Проверить closed submit/roots/maintenance, duplicate без send/нового observedAt,
   unchanged worker bytes, owned cleanup/positive Job.empty и cold NotResident.

Parent не получает новый grant window при диагностическом bounded IPC. Target
можно оставить90fc5b4: его V6 receiver уже проверен, fault находится в producer
supervisor, а child остаётся обычным clean V6 candidate.

## Runner и independent reader

`run-update-lifetime-ready-loss-main-native.py`/`verify-update-lifetime-ready-loss-main-native.py`
выбирают Main28, `main28-204` namespace и explicit kind. Порт1144, stable
`K:\_proj-26\winbridge\runtime-test-204`, отдельный instance/state/controller,
original240s и network preflight900s сохраняются. Working runtime не заменяется.
Только штатное архивирование test slot после доказанного exit; Windows directories
не удалять. Повтор unknown submission запрещён: смотреть тот же saved context/key.

PS `-ActivationLifetimeReadyLossGate` требует five native boundary cases и отдельную
exact selection Main28. Отчёт экспортирует Ready-loss input/marker наряду с прежними
markers. Reader проверяет exact source/target, весь service/controller prefix,
трёхзвенную history, отсутствие receipt до reconciliation, позднее observedAt,
исходное окно entry, marker frame length по later immutable receipt, files SHA и
отдельные input bytes. Main27/default reader не принимает Ready-loss coverage.

Negative audits:37 lifetime (включая6 coherent key aliases) +15 Ready-loss;
24 local/27 controller/10 прежних marker audits переиспользуются. Marker source
и отдельные файлы должны совпасть с full proof, не только друг с другом.

Пример после clean commit/export и окончательного preflight:

```bash
python3 scripts/run-update-lifetime-ready-loss-main-native.py \
  --source-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261029T090948Z-0426a13-1eb6a3ef.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261028T224438Z-90fc5b4-273ea619.zip \
  --cache-run 20261017-061527-f4639971 \
  --worker17-reference 'K:\VMs\shared\WinBridge\releases\20261026-182213-6cd81f3e\winbridge-worker.exe' \
  --worker17-reference-sha256 536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712 \
  --test-runtime-directory 'K:\_proj-26\winbridge\runtime-test-204' \
  --profile worker19-updater22 \
  --controller-config .winbridge/controller-snapshot.local.json \
  --controller-binary .winbridge/controller-runtime-updates/20261028T071841Z-203/winbridge-controller-0.2.0
```

## Source checks и ревью

969 workspace/update-validation passed/17 ignored,126 groups. Пять focused checks:
реальный duplex body остаётся доступен на том же stream, expired window не читает
header, malformed/truncated header не считается успешным fault, exact original
record/command scope и unknown-grant/input-kind отказ. Metadata в unit fixtures
synthetic; это не Windows OS custody proof. Linux/MSVC workspace all-target Clippy
с update-validation/`-D warnings`, fmt/diff прошли.52 Python tests в8 readers passed;
из них6 новых Main28 tests проверяют15 negatives,37 shared negatives, coherent
aliases, frozen Main27, exact native selection и обязательные5 boundary cases.

Ревью до native gate исправило GateInput, затенённый ShutdownInspectionRequest
в test closure, Linux-only unused reexport и потенциальную ложную exact-selection
проверку. Main28 проверяет свою test line прямо, без замены Main27 имени в логе.
Ранние failed source logs сохранены, проверки не ослаблялись.

Main27 повторно перечитан readonly после shared reader refactor: весь JSON audit
идентичен,60 files twice/37 lifetime negatives, старые SHA не менялись. Это не
новый Windows run. Source logs: `.winbridge/review-2026-10-29/ready-loss-*`.

Final Windows parser/storage/slot preflight task52def8ef succeeded0: parser errors0,
slot processes0/worker EXE absent, K:22,921,400,320 bytes. Ранний inline parser
request был отвергнут лимитом32768 argument bytes до store.accept; recent tasks
не содержат его key. Вместо большого argv используется отдельный SHA-checked
WinBridge validation input. Runtime/config/journal не менялись, replay не было.
Перед source checks Debian>10GB/shared>23GB; измерения сохранены. Cleanup не было.
Installed0.2.0/gen946f1128 прежний, active0/errors=null; desktop/окон не было.

## Продолжение

Clean source export с existing built UI, один Main28 и independent reread. До
этого partial-Ready native behaviour не объявлять готовым. Потом consuming
permanent keeper role (освободить file/config pins и updater journal authority,
удержать original Job/process/channel), final root commit/scoped maintenance
release, next-update delegation/max2 keepers/two generations и оставшиеся U2–U4.
Public install off. Рабочий worker оставить; desktop не нужен.


Повторное readonly чтение после terminal (не выполняет Windows RPC):

```bash
python3 scripts/verify-update-lifetime-ready-loss-main-native.py \
  --profile worker19-updater22 \
  --run-info .winbridge/update-preparation/main28-204-20261029-091025-be7fc543/run-info.json \
  --source-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261029T090948Z-0426a13-1eb6a3ef.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261028T224438Z-90fc5b4-273ea619.zip
```
