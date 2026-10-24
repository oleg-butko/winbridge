# Actor21 whole-set read binding: actual Windows run выполняется

2026-10-24. **Результат ещё не известен.** Task
`01d7bf76-6c53-427c-8cd9-81d8f8798962`, run `20261024-153856-fe2e4f76`,
producer `74e8839`. Первое authoritative get_task: running. Не resubmit.

Ожидаются не менее95 native tests/19 групп/21 commands: прежняя real Main цепочка
до Prepared20, новый Bind/Observe21, strict reader refusal до handoff и success
после, exact five-file read set, write/delete denial, original instance lock,
duplicate/cold no-adoption, actual17→18/refusal и four-case file primitive.
Новый worker не запускается и state не передаётся; public install/activation off.

## Immutable pins и наблюдение

- Producer `74e8839634a36bc56508df2d5f3b9eb11397982f`:
  `winbridge-20261024T153800Z-74e8839-fe777660.zip`, SHA256
  `af1ff79024b1e88dd20440a2a2329bad59116e0e35fbe7d220338cd27020b3d8`, manifest
  `959daf2e7303ee227723d6b4352000eab574e96a600fc506506eee34a844e820`.
- Fixture `7f6a029c1fd626077adc034a96b54d63da551df5`:
  `winbridge-20261024T103055Z-7f6a029-3e94038a.zip`, SHA256
  `4f17e0e0c5f4be899a3e4456b53240c0a9bd9b02b5a5aad2dee458ba29e7a788`, manifest
  `7deaebcf66a9e447169bdd65d8991572bcfb6625719ba5e7945b9552a7fbe86b`.
- Migration reference:
  `.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
- Metadata:
  `.winbridge/workflow-validation/20261024-153856-fe2e4f76/activation-preparation-run.json`.
  `activationFiles=true`; будущий verified result имеет имя
  `activation-preparation-verified.json` того же run. Private request/EncodedCommand/
  config/token не печатать.
- Shared reports:
  `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-153856-fe2e4f76`.
- Observer log `.winbridge/update-preparation/activation-files-native-main.log`;
  при запуске exec session62989/PID123026. Проверять handles/get_task заново.
- Cache `20261017-061527-f4639971`, один build lease.
  Debian free перед submit16,472,887,296 bytes; Windows measurement будет в report.

Readonly recheck **прежнего task**, без submit:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T153800Z-74e8839-fe777660.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T103055Z-7f6a029-3e94038a.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --activation-files --verify-run 20261024-153856-fe2e4f76
~~~

Не терять `--activation-files`: mode является частью сохранённого scope.
Observation timeout не означает terminal. Сначала get_task по Task ID, затем
наблюдать тот же run. Обычные исправления изолированных build/test scripts и
fresh retries разрешены после terminal диагностики. Installed worker unexpected
behavior/update requirement останавливает зависимые Windows действия.

## Что уже проверено, а что нет

Для producer74e8839: workspace716/0 (16 ignored), final Linux/MSVC all-target
Clippy, protocol-jsonschema55/0, generated contracts/TS/UI, fmt/diff/scaffold263,
Python5+9 passed. Логи `activation-files-*` в `.winbridge/update-preparation/`.
Legacy preparation87/0 readonly recheck сохранён, он не доказывает новую ветку21.
[Native20 preparation87/0](windows-activation-preparation-gate-2026-10-24.md) и
[one-file native4/0 pins](runtime-activation.md) относятся к прежним producers.

До final proof whole-set branch не считать выполненным. После terminal audit/
review продолжать [activation plan](runtime-activation.md): process creation/
private receiver, original state handoff, local/controller/lifetime, U2 faults/
U3/U4. Для будущего create admission binding guard должен быть consumed вместе
с его живым deadline/nonce; JSON и cold Snapshot не дают этих ресурсов.

Нужен только installed d052 worker. Desktop не нужен, UI-окон не будет;
рабочий runtime/config/journal не меняются. После terminal явно сообщить:
desktop можно закрыть, installed worker оставить работать.
