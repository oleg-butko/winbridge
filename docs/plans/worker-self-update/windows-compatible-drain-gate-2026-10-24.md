# Production drain17: actual Main и миграция прошли

2026-10-24. Task **a4f17a0d-4eea-4af2-8f2c-78bc6235c0af**,
run **20261024-085627-3a106020**, producer **628c0f6** —
terminal **succeeded/exit0**, **33 native tests /10 groups /12 commands**.
27 ready files проверены дважды;38 Main +19 migration mutations отвергнуты.
Readonly recheck той же accepted task также passed. **Не resubmit.**

## Что выполнено на настоящей Windows

Actual worker Main/normal serve создал bound supervisor и прошёл baseline,
preimages, source apply, реальную candidate build/artifact verification и health.
Support query записал Checked/rev5/history6. Lost ACK не повторил процесс;
duplicate и старый health view сохранили прежнее поведение.

Новый entry19 запросил production drain и **сохранил Blocked до ACK**, поскольку
activation/controller_confirmation/desktop_update ещё не реализованы. На каждую
возможность сохранены missing_contract и driver_not_implemented. Empty/fake
success не подставлялся. Root остался CandidateVerified/rev10; drain intent,
BeginDrain и shutdown отсутствуют. Source lease остался SourceHeld/rev1.
Duplicate вернул то же решение, попытка legacy v13 отвергнута, normal IPC отвечал.

После explicit fixture abort source lease освобождён, root FailedBeforeCutover;
cold read сохранил Blocked и прежний support. Все четыре old runtime image
bytes/file IDs сохранились. Saved decision/readback не создали fresh permit.
runtimeApplied/installedRuntimeApplied/windowsOpened=false.

Operation links:

- Update `101651ca-55b4-4600-b331-9993a07c29fb`.
- Support attempt `c8a94a7d-06ff-4bf6-9c81-c9e631859b6f`, revision5.
- Decision attempt `60ccab89-26ce-4823-a2ef-afaf28962605`.
- Owner epoch2, parent revision10, beginDrainCount0/requestShutdownCount0.

Actual16→17 миграция сделана только на новой consistent Backup API копии
настоящей support16 history run20261023-233438-99674bc1. Epoch2→3,
installation/scope/lastSequence13 и все прежние rows сохранены, backup16
проверен, gateRows0. Historical context не допустил fresh admission. Actual
старый16 exe отказал init/status17 без изменения DB/nonempty WAL. Original
reference image/manifest/DB/WAL сохранены.

## Source, target и proofs

- Producer `628c0f65ef47b216f3f52006001cba3ecc0480a1`.
  ZIP `winbridge-20261024T085529Z-628c0f6-471d2793.zip`;
  SHA256 `1ec59abbc9db7e807c0ab45b59f2381ba06052df38bf745ab3df98deaa983eb3`;
  manifest `29d3f513a0966d92c34f723ea94140a73d40667ef80950e2dd5dc8327aac99e0`.
- Fixture `efccfbe73930763f6925cbd3d26efc15d6713a37`.
  ZIP `winbridge-20261024T003539Z-efccfbe-b7a614ff.zip`;
  SHA256 `06056b8bee56c7b49c72abc48d24a619324a9d75f4981d767ed2bc1cc62c4a67`;
  manifest `94832d6dfe58dc4268a3dd4b3e77865378a538dbb7762374673822d1d3e7ef84`.
- Report SHA256 `c00c643d1496ff9a9f9db97e0f08de8f06f5ebdb2bbff24ca06759c946c8be66`.
- Main proof SHA256 `d3e279a686f3c66933dd30ed27409b9fa2fdf33c019a3ef51b8a8ccdf888fc6f`.
- Migration proof SHA256 `03d016e0fbeea4f51ec3987d47cfc56ebf5028826266c4ebebaef42fc854f5c1`.
- Verified pin:
  `.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
- Shared: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-085627-3a106020`.
  Local Windows runtime сохранён под `snapshot-fixture/workflow-validation`
  того же run, не очищать и не мигрировать оригинал следующими тестами.
- Логи observer: `.winbridge/update-preparation/compatible-drain-native-owner.log`
  и `compatible-drain-owner-readonly-recheck.log`.

Readonly recheck, без нового submit:

~~~bash
python3 scripts/validate-update-support-actor-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T085529Z-628c0f6-471d2793.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T003539Z-efccfbe-b7a614ff.zip \
  --cache-run 20261017-061527-f4639971 \
  --compatible-drain \
  --reference-pin .winbridge/workflow-validation/20261023-233438-99674bc1/support-actor-verified.json \
  --verify-run 20261024-085627-3a106020
~~~

## Границы результата и продолжение

[Первые две попытки](windows-compatible-drain-first-attempt-2026-10-24.md)
terminal failed из-за main stack overflow. Heap boxing был недостаточен;
отдельный owner thread8MiB и test-only stage messages проверены полным Main
сценарием этой попытки. Startup/maintenance/owner checks не ослаблялись.

Это положительный gate **обязательного отказа до drain**, не успешная установка
всего updater. Положительный production admission потребует настоящих недостающих
drivers. Следом [activation](runtime-activation.md): intent/FSM/ledger, original
custody handoff, exact launch/local health/controller confirmation, remaining U2
faults и U3/U4. Public install off, весь updater не готов.

ActivationSource model в commitcae9a45 добавлена после export628c0f6; её local
tests/Clippy не являются частью этих33 native tests и не доказывают OS activation.

Windows free bytes29,266,731,008→28,588,085,248; Debian при закрытии около25.19GB.
Очистки не было. Installed d052 unchanged, generation8f8d1fb9-e5b1-4984-a17e-d067db46268e,
active0/maintenanceError=null/recoveryRequired=null/coverage complete.
Native runs завершены. UI-окон не было; desktop можно закрыть, installed worker
оставить работать. Ручное обновление сейчас не требуется.
