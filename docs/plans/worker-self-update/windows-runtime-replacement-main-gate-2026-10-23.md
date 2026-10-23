# U2.2: actual Main15 replacement — Windows gate пройден

2026-10-23. **60 native tests /0 failed**,12 test groups,20 ready files проверены
дважды,44 намеренных искажения actual proof отклонены. Task
`665e7e2f-e734-49cf-ae87-d83498a29cbd`, run `20261023-163606-15675844`,
terminal **succeeded / exit0**. Не resubmit. Actual Main test занял917.09 секунд;
все старые/новые runtime files и журналы находятся в отдельном test namespace.
Установленный d052 не обновлялся; UI-окон не было.

## Точная проверенная пара

| Поле | Значение |
| --- | --- |
| Producer | `8457004c315fc5718e216fd0f364d5800a083eee` |
| Source ZIP | `winbridge-20261023T163545Z-8457004-b8548a57.zip` |
| ZIP SHA-256 | `fc284b81054618e13e65fcc73a1463d10f126a41c10a8ff012e79a22ffdd9f1f` |
| Source manifest SHA-256 | `e91035d320acc54e0dd0bd5afd5ae90c02af52e69279fe5d3b80b49723d4f4ed` |
| Target fixture | `5fbc1cf2e37086dad8e65667ed3af31b1a16d9f7` |
| Target ZIP | `winbridge-20261023T091226Z-5fbc1cf-b4ac44a5.zip` |
| Target ZIP SHA-256 | `3891ad4689779c2eb2507b70be4ad5d2f599cf4816fb4b882d64f2de0006c6fb` |
| Target manifest SHA-256 | `98e52e10e99e25fbc8cf9864b4af3e4d83419e8a1297cbcd330dc61e5c4546f3` |
| Report SHA-256 | `54e65d7f2cab94cd35be3e9bc7ccf248d69052bd9e462e173f4c8fd407b33098` |
| Proof SHA-256 | `e3511ebfee87f81fcd9a60f7dbd7affdca3d83c3297fd6a5c46d1f02b929dec5` |

Оба архива updater15/worker16, internal supervisor17/view9. Target — limited
same-schema fixture, **не готовый production auto-update candidate**: его updater
не содержит полного нового actor/activation. До public drain нужна отдельная
versioned compatibility query реально проверенного candidate updater.

## Что подтвердилось

Реальные old roles и normal feature-gated product Main проходят baseline/preimages,
source/build/artifact health, drain/quiesce, точный shutdown, final backup и Prepared
copies. Предшествующие проверки сохранены в `beforeReplacement`; сообщение
`oldRuntimeUnchanged` относится к этому этапу, а не к финальному runtime.

Затем actual supervisor выполняет fresh admission-before-ACK и consumes тот же
original process/state/instance-lock guard. Неправильная v16 action отвергнута,
дубликаты возвращают тот же attempt. Две directory steps и20 file steps образуют
23 сохранённых состояния0–22. Старые файлы перемещены в `displaced-runtime`, новые
заняли постоянные имена, **File IDs сохранены**, manifest перенесён последним.
Prepared directory опустел; source payload, previous preimages, final backup и
worker DB/nonempty WAL сохранили bytes/identities. Native SQL read и independent
Python verifier восстанавливают exact causal prefix из immutable before-images.

Update ID `f0c6e634-1376-44cc-88ea-0e516143399e`, replacement attempt
`006828aa-6698-4223-893f-e007e83c7d83`. Root revision14→15,
`Replacing → VerifyingLocal`, только после verified FilesMoved22.
Resident custody=true после замены; после завершения **собственного** test Job
history осталось FilesMoved, custody=false. Обычный новый worker отказал в startup
из-за maintenance, без изменения worker DB. Activation/local health/controller
confirmation не запускались и не объявляются проверенными.

## Исправление harness и review verifier

Первый task `cf49a541-0abf-4aa3-bb64-295faca99381` /
run `20261023-160520-ee8053b8` дошёл до FilesMoved22/VerifyingLocal, но упал на
последующем `artifact::inspect_image`: строгий read pin правильно отказал при
resident READ+DELETE handle (sharing violation32). 17 ready files первого run
сохранены и проверены дважды; его нельзя выдавать за полный passed proof.

Исправление `8457004` оставляет production pins прежними. Private test reader
открывает только READ с share READ|DELETE, считает полный bounded SHA/File ID/size
и marker. Это observation, не pin/permission. Новый **actual native SDK regression**
проверяет совместимое чтение и rename, сохранённый запрет чужой записи/удаления,
а затем возвращение строгого pin после release. Он passed1/0 в составе60 tests.

Verifier review явно отвергает Python bool вместо typed integer и extra intent
fields. Audit сначала проверяет original actual proof, затем44 mutations: skipped
history, wrong IDs/paths/digests, changed preparation/preimages/backup/DB, invented
custody/admission/activation, false duplicate scope и counter-type substitution.
Новых Windows эффектов для verifier audit не требовалось.

## Где продолжать

Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261023-163606-15675844`;
Debian: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-163606-15675844`.
Полный proof, source/target pins,44 mutation names и измерения — в ignored
`.winbridge/update-preparation/replacement-main-native-verified.json`.
Проверка audit:

```bash
python3 scripts/test-update-replacement-proof.py \
  --proof /mnt/hgfs/shared/WinBridge/workflow-validation/20261023-163606-15675844/candidate-health-actor-proof.json \
  --pin .winbridge/update-preparation/replacement-main-native-verified.json
```

Windows free39,053,991,936→38,295,232,512 bytes; Debian перед run26,578,333,696
bytes, финальное измерение в local summary. Кэш20261017-061527-f4639971 reused;
никакие Windows validation directories не удалялись. Installed worker d052,
generation8f8d1fb9-e5b1-4984-a17e-d067db46268e, active0/no maintenance/recovery errors,
coverage complete. Desktop можно закрыть, worker оставить.

**Следом:** actual replacement fault matrix (occupied directory, incompatible
handle, receipt-write rejection, crash between effect/receipt), затем activation/
local health/controller confirmation и U3/U4. Positive gate завершён, **весь U2.2
и updater не завершены**, public install по-прежнему off.
