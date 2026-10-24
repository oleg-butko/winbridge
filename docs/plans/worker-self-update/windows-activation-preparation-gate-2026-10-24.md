# Activation preparation18: actual Windows gate выполняется

2026-10-24. **Результат ещё не известен.** Accepted task
`c1675a54-b250-4fa9-b2d3-66c653033180`, run `20261024-111115-9272d0e2`,
producer `b12ecc6`. При первом authoritative наблюдении task running,
`health-supervisor-build` компилирует protocol. Не resubmit и не объявлять gate
пройденным по наличию отдельных логов.

## Scope и проверяемые свойства

Один изолированный Main18 scenario выполняет прежние shutdown/backup/preparation/
replacement шаги, затем supervisor20 готовит activation metadata под original
guard. Lost ACK/duplicate, Prepared до ACK consumption, root/files/worker DB/WAL
preservation, instance lock и cold history без custody проверяются отдельно.
Новый worker не запускается, state не передаётся, public activation/install off.
Обычный serve после owned supervisor exit должен отказать до writer open.

Тот же task проверяет actual17→18 на новой consistent SQLite Backup API копии
successful17 history. Backup17, epoch и все старые rows/support/Blocked decision
сохраняются; новые activation tables пусты. Actual старый17 exe должен отказать
init/status18 без DB/WAL writes. Original reference DB/WAL/images не меняются.

Ожидается не менее87 native tests/16 групп плюс supervisor/old-set builds.
Независимый observer проверяет ready files дважды, causal source/context/health
digest/root edge и live/duplicate/cold views; затем искажённые доказательства.
Это preparation gate, не доказательство actual worker activation или полного update.

## Неизменяемые pins и продолжение наблюдения

- Producer `b12ecc6348f146adb04e702b7417c5f2d14dafa9`:
  `winbridge-20261024T111043Z-b12ecc6-42feb78e.zip`, SHA256
  `704ba5903e013571e1ba1270c64e6d44243381335b448bb9cc00ea3fba1b8c6d`, manifest
  `4c2b3229901bdb06b860bf44f9455656dc8b03c35aea294dc5647d8b0984c0ab`.
- Fixture `7f6a029c1fd626077adc034a96b54d63da551df5`:
  `winbridge-20261024T103055Z-7f6a029-3e94038a.zip`, SHA256
  `4f17e0e0c5f4be899a3e4456b53240c0a9bd9b02b5a5aad2dee458ba29e7a788`, manifest
  `7deaebcf66a9e447169bdd65d8991572bcfb6625719ba5e7945b9552a7fbe86b`.
- Migration reference pin:
  `.winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json`.
  [Его terminal33/0 proof](windows-compatible-drain-gate-2026-10-24.md).
- Local metadata/request:
  `.winbridge/workflow-validation/20261024-111115-9272d0e2/activation-preparation-run.json`.
  Не печатать private request/EncodedCommand/config/token. Проверенный итог должен
  появиться как `activation-preparation-verified.json` в том же каталоге.
- Shared reports:
  `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-111115-9272d0e2`.
- Observer log `.winbridge/update-preparation/activation-native-main18.log`;
  при запуске exec session14613/PID109817, их актуальность перепроверять.
- Cache run `20261017-061527-f4639971`, единственный build-cache lease.
  Debian free перед submit16,531,836,928 bytes; Windows measurement будет в report.

Readonly наблюдение **этой же задачи**, без нового submit:

~~~bash
python3 scripts/validate-update-activation-preparation-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T111043Z-b12ecc6-42feb78e.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261024T103055Z-7f6a029-3e94038a.zip \
  --cache-run 20261017-061527-f4639971 \
  --reference-pin .winbridge/workflow-validation/20261024-085627-3a106020/support-actor-verified.json \
  --verify-run 20261024-111115-9272d0e2
~~~

Observation timeout не означает terminal: читать get_task по сохранённому Task ID.
До нового run сначала получить terminal исход прежнего и сохранить диагностику.
Исправления изолированного build/test сценария и fresh namespace retries разрешены.
При неожиданном поведении **installed worker** или необходимости его обновления
остановить зависимые Windows действия и сообщить причину.

Локально перед export: Linux/MSVC all-target Clippy update/worker с
update-validation passed, focused Rust28/0, Python4+9 passed, fmt/diff/scaffold
passed. Полный workspace713/0 из checkpointdaa88da относится к actor20 до нового
harness; это не исполнение новых Windows-only integration tests.

Нужен только installed d052 worker, desktop не нужен; UI-окон не будет.
Installed runtime не меняется. После terminal явно сообщить, что desktop можно
закрыть и installed worker оставить работать. Затем audit/review и
[physical activation bridge](runtime-activation.md), а не public enable.
