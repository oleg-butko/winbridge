# Activation preparation18: actual Windows gate пройден

2026-10-24. Task **c1675a54-b250-4fa9-b2d3-66c653033180**,
run **20261024-111115-9272d0e2**, producer **b12ecc6** — terminal
**succeeded/exit0**, **87 native tests /16 групп /18 commands**.26 ready files
проверены дважды; **72 Main +12 migration mutations** отвергнуты. Readonly recheck
той же задачи пройден; новых задач для исправления observer не создавалось.

Update `25e60857-aa69-45e7-ae42-5ec5fd735140`, activation attempt
`9bfedee0-f0bf-4efd-b382-84f1f588aae0`, Prepared/revision0/history1.
Replacement attempt `50e00357-6a6d-4e9c-981b-99bf50cd5e2d`, FilesMoved22;
root VerifyingLocal15/ownerEpoch2 не изменился от capture20.
Set `7eb5f7db-b796-4305-8b0e-a46ee306c28c`.

Python observer первоначально ошибочно ожидал FilesVerified строкой. Фактический
wire contract — tagged object `{"kind":"files_verified"}`. Исправлена только
независимая проверка и добавлены отрицательные мутации bare-string/extra-field;
нативный Main и его сохранённые файлы не менялись. Версия observer внутри
immutable producer archiveb12ecc6 содержит прежнюю ошибку; readonly recheck
нужно выполнять из текущего checkout. Это не native worker failure.

## Что проверено

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

Пройдено87 native tests/16 групп плюс supervisor/old-set builds.
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
  Не печатать private request/EncodedCommand/config/token. Проверенный итог:
  `activation-preparation-verified.json` в том же каталоге.
- Shared reports:
  `/mnt/hgfs/shared/WinBridge/workflow-validation/20261024-111115-9272d0e2`.
- Первый observer log `.winbridge/update-preparation/activation-native-main18.log`
  завершился ошибкой wire shape. Итоговый readonly log:
  `.winbridge/update-preparation/activation-native-main18-readonly.log`.
  Активных native/Cargo/observer runs нет; старые handles не возобновлять.
- Cache run `20261017-061527-f4639971`, единственный build-cache lease.
  Debian free перед submit16,531,836,928 bytes, после recheck16,538,542,080 bytes.
  Windows free26,685,263,872→27,716,300,800 bytes; cleanup не выполнялся.

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

## Proof hashes и границы результата

- Report SHA256 `be27b7f4a633b788fbc7c15be816059bb2d97838f0976fddd983035c475dd9d7`.
- Main proof SHA256 `92d5cc1829990a2562cb440d545dfa518ba97f2dd6500a1e7a763fe19c2552b4`.
- Migration proof SHA256 `75eb329f66e4286df00eb28ff845c436f70c53d94132abe1815fe2af11b4e52c`.

Actual17→18 на копии: epoch2→3, installation/scope/lastSequence13 и все прежние
rows/support/Blocked decision сохранены; backup17 проверен, activation rows0,
новое admission из старого контекста отвергнуто. Старый17 init/status18 отказал
без DB/nonempty WAL writes; original reference DB/WAL/images/manifest сохранены.

Native zero launch/handoff поля обозначают scope metadata-only entry; это не
универсальный Windows process monitor. Никакого create/resume/state-handoff
callback в подготовке нет; source/intent/reader/guard и реальное состояние
проверены вместе. Будущий execution gate обязан иметь actual PID/Job/receiver
и наблюдаемые receipts вместо декларации нулевых счётчиков.

Это proof завершённой **подготовки**, не actual activation и не успешного
самообновления. Activation/controller_confirmation/desktop_update capabilities
пока не объявлены; production drain по-прежнему должен Blocked при их отсутствии.
Следом consuming file-handle bridge, original lock/state handoff, suspended
launch/private worker receiver, local/controller/lifetime proof и оставшиеся
U2 faults/U3/U4. Installed d052/generation8f8d1fb9-e5b1-4984-a17e-d067db46268e
не изменился; active0, maintenance/recovery errors=null. Desktop можно закрыть;
installed worker оставить работать. Ручное обновление пока не требуется.
