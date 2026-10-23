# U2.2: actual preparation fault gate

2026-10-23. Реализованы fixture, feature-only barrier и отдельные native runners.
[Native gate пройден](windows-runtime-preparation-faults-gate-2026-10-23.md):40 tests,
три actual cases,24 ready files twice,48 отрицательных mutations после ревью verifier.
Happy preparation и migration
уже проверены; [закрытая точка перед продолжением](resume-2026-10-23.md) остаётся
историей. Whole-runtime replacement/activation и public install не включены.

## Сценарии

Все три cases проходят actual Main/source/build/artifacts/health/drain/shutdown
и Verified final backup в независимых runtime/state. Затем:

1. `existing_directory`: в prepared-runtime заранее создан sentinel. Driver
   сохраняет Indeterminate revision1, все файлы Pending, directory receipt нет;
   sentinel сохраняется, повтор видит тот же attempt.
2. `changed_candidate`: после health/backup в собственный payload дописаны bytes.
   Candidate pin/hash не совпадают; Indeterminate revision1 до directory create.
   Намеренно изменённый source не восстанавливается и повторно не исполняется.
3. `hold_before_receipt`: новый updater тестовой сборки удерживает первую полную
   copy после IO до receipt. Сохранён Preparing revision3/worker Issued, остальные
   Pending. Public marker связывает actual PID/build/attempt/plan/File ID и input
   SHA. Внешний fixture завершает только собственный Job. Cold view Indeterminate,
   record и actual partial copy сохраняются; OS custody=false, повторного запуска нет.

Во всех cases проверяются duplicate, old runtime/worker DB/preimages/final backup,
а после owned exit — exact output prefix/sentinel и candidate bytes. Python verifier
отдельно проверяет full backup/shutdown proof, preparation scope/plan/manifest,
file identities, fault-specific outcome и cold history без permission.

## Изоляция hook

`runtime_preparation/validation.rs` компилируется только с `update-validation`.
Обычная production-сборка этот файл не читает. Hook принимает лишь фиксированный
`runtime/update/<id>/runtime-preparation-validation.json`, проверяет native clean
build, bounded input, exact update/launch/owner/epoch/config/runtime identity и
issued prefix. Произвольных путей/команд нет. Marker create-only; bounded120s wait
с authority fences заканчивается ошибкой без receipt, если test parent не закрыл Job.
Kernel pins остаются живыми; marker не является новым permission.

В fault режиме PS отдельно собирает validation updater и передаёт его явный путь
fixture. Только эта роль и validation worker берутся из test build cache; остальные
old roles настоящие из normal build staging. Actual descriptors/hashes снимаются
после копирования в isolated runtime и включаются в known manifest. Ни один такой
binary не устанавливается в рабочий runtime. Candidate cee06a7 остаётся limited
same-schema fixture, его нельзя превращать в production update target.

## Запуск и продолжение

После local checks нужен clean commit/export с built UI, затем существующий runner:

```bash
python3 scripts/validate-update-health-actor-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/<NEW-CLEAN-ARCHIVE>.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261023T005835Z-cee06a7-88d49ac8.zip \
  --cache-run 20261017-061527-f4639971 --runtime-preparation-faults
```

PS режим — `-RuntimePreparationFaults`, proof kind
`runtime_preparation_faults_native_proof`, три ordered cases. Один explicit-input
integration test содержит три full cases, поэтому число tests само по себе не равно
числу реальных pipelines. Timeout task1h; observation timeout не разрешает resubmit.
Перед run проверить свободное место и healthy idle installed worker; desktop не
нужен, тестовых UI окон нет. Один cache build за раз. После terminal сверить все
ready files и actual proof, сохранить hashes/negative mutations и обновить readiness.
До этого не считать fault block завершённым и не начинать consuming rename effects.
