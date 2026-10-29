# Main29: current resident V7 через actual keeper

**Попытка20261029-151031-f024760d terminal FAILED.** Не продолжать/повторять её
submit и не считать Main29 passed. Предыдущий running checkpoint ниже — история.

231 предварительная native проверка passed; pipeline дошёл до same child11644,
Ready64922ms/StateOwned82187ms, saved lifetime Confirmed после original expiry.
На первом Internal28 ответе test-only `activation_writer::call` panicked:
`Unknown native activation fixture contract`, потому что switch заканчивался27→19.
Response прошёл version/launch/error checks, но его content/phase не сохранён:
это не достаточное доказательство успешного V7. Installed worker не затронут.

Failed task772361c4-ae8d-4ebe-8a6b-28fbce1df0fb/exit1, report.passedfalse;
original deadline1793288351603. Runtime archive:
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\tmp-main29-204-20261029-151031-f024760d\shutdown-main-cases\runtime-archive-5d309495-0ee4-45c5-ba57-71980721a9b9`,
plan2bba0ed9abe242eac7c0c4c9fffda786f88b5c444119f043cb7a1104d01ac8f3/10 entries.
Runner302633/controller302634 terminal и отсутствуют; exit code controller в
failed path отдельно не сохранялся, поэтому не объявлять exited0.
After info: installed build/gen/context unchanged, active0/errors=null.
Новый readonly cleanup taskc8326230-5513-4c57-81ed-b9b14473b457 succeeded0:
slot processes0/worker EXE absent, K:18,840,248,320 bytes. Directories не удалялись.

Fix: shared `tests/support/activation_reply.rs` теперь используется actual native
caller и portable integration test. Он знает24→16…28→20, проверяет version/launch/
update/schema и required block, не принимает errors или unknown versions даже
как None/busy. Три portable regression cases с реальным SQLite cold view20 passed;
новая группа resident-fixture-reply обязательна в PS и independent reader.
Проверки original owner, deadline, Job, ledger, actor и V7 receiver не ослаблены.
Рекомендованный обычный scenario fix/repeat уже авторизован пользователем;
повторное разрешение не требуется. До repeat подтвердить новые source checks и
parser SHA, сделать clean commit/export. Target944a39c прежний, нужен новый run.

Fix checks завершены:3 portable actual-adapter tests passed, Linux/MSVC
workspace/all-target Clippy/update-validation/-D warnings и fmt/scaffold прошли,
58 Python tests в9 readers passed. Это targeted проверка fixture-only change;
предыдущий full workspace983 не выдаётся за новый full986 run.
Mandatory resident-fixture-reply группа теперь запускает эти3 tests на Windows
перед дорогим pipeline, а reader требует её exact selection/count/cases.

Новый Windows preflight46ab6ada-962e-475e-bef7-bed912b797f5 succeeded0:
actual PS SHA516e53a0e7dcb28b65d6e5a12d8609f7c11d5ed29f2bfd173edf08468b0e174a,
parser errors0, slot204 processes0/worker EXE absent, K:18,850,488,320 bytes.
Working worker прежний, новый run ещё не отправлен. После commit/export можно
повторить автоматически; failed attempt остаётся самостоятельным failed отчётом.

**Main29 запущен, пока не завершён:** run20261029-151031-f024760d,
task772361c4-ae8d-4ebe-8a6b-28fbce1df0fb, producerfcee32a→target944a39c.
Run-info `.winbridge/update-preparation/main29-204-20261029-151031-f024760d/run-info.json`,
log `.winbridge/review-2026-10-29/resident-main29-run.log`, runner302633/tool session6497.
Test controller302634/.203:1144/state `/tmp/wb29-3rsanhqa`, production131887 прежний.
Native supervisor/worker build passed, resident model checks начались. Не повторять
submit: читать тот же task/context/key до terminal + independent reread.
Installed0.2.0/source48 прежний; desktop не нужен, working worker оставить.

2026-10-29. Source сценарий готов; **новый Windows gate ещё не запущен**.
Предыдущие [Main27](main27-native-2026-10-29.md) и
[Main28](main28-native-2026-10-29.md) terminal, их не повторять. Этот сценарий
проверяет [V7](resident-observation-2026-10-29.md) и
[actor28/view20](resident-actor-2026-10-29.md) на настоящей Windows.
Public install, root commit, release, Active и permanent keeper остаются off.

## Новый сценарий

Exact ignored test:
`actual_main_observes_resident_connection_after_partial_ready_without_replay`.
PS switch `-ResidentConnectionGate`, runner/verifier
`run-update-resident-main-native.py`/`verify-update-resident-main-native.py`.
Отдельные kind `activation_resident_main_native_proof` и
`activation_resident_main_native_gate`; namespace main29-204.
Старые wrappers сохраняют resident=False и не relabel-ятся как Main29.

1. Выполнить прежний actual local/controller/lifetime prefix с потерей private
   Ready body: header4/body0, Issued→Indeterminate→Confirmed после original240s.
2. На том же child/keeper/Job/Store через supervisor28 дважды запросить current
   resident observation. Получить разные canonical requestId, прежний lifetime
   key/entry и current accepted frame/link после original expiry. View20 имеет
   только compact current block; исторический prefix остаётся отдельным V27.
3. Из actual другого test process послать V7 с правильным полным command: specific
   peer rejection, независимо от знания JSON PID. Дополнительное поле processId
   отвергается strict envelope; version6 не получает fallback.
4. Сравнить updater snapshot, lifetime history и worker bytes до/после. Root
   AwaitingController и quiesced readonly lease не меняются, submit остаётся
   закрыт. Новый private read/send/release/launch не разрешается.
5. Прежний controlled cleanup доказывает actual exit/Job.empty. Новый cold view20
   сохраняет historical lifetime key, но оставляет process/observation/custody
   неизвестными и не запускает ничего.

Cold `UpdateJournal::activation_resident_snapshot` читает compact preparation base
и lifetime key **в одной SQLite transaction**. Detailed schema1 используется для
содержимого, внешний schema20 — для wire; historical V4/V5/V6 extensions не
дублируются. Все существующие views1–19 имеют прежние shapes. Live actor патчит
тот же cold projection только при наличии actual original owner.

## Actual producer running pins

Clean sourcefcee32aedc90963766dd8df3089531857c663c82,
archive `/mnt/hgfs/shared/WinBridge/sources/winbridge-20261029T150652Z-fcee32a-3ed7bff4.zip`,
SHA1346a694ba4e27fb6b588b2502e50ece21436d2e42d9bcbe920869219fe1317b,
manifest749106606a22924e472fba0cc38258bfeb39daf7460220f8e57179af672f74ac.
UI собран перед export,1070 files, capture clean. Не экспортировать текущий
последующий documentation commit вместо producer и не повторять этот run.
Generated source pin `.winbridge/update-preparation/resident-main29-producer-fcee32a.json`.

## Pins и запуск

Distinct target уже опубликован и прочитан archive_pin:
`/mnt/hgfs/shared/WinBridge/sources/winbridge-20261029T143435Z-944a39c-3580f462.zip`,
commit944a39cf0e864fa4ce0f0f6797f57fb0cd2c4b95,
SHA1095726437ab9118aab7009286d5a5e7fdb4c9e493857f810c60676bd96d2715,
manifest7fe8cc6e2a701d05547a03f714191391119a43eff933725619c6f30280351346.
Это actual V7 receiver, source worker19/updater22; его actor27 не используется
для координации нового теста. Producer должен быть новым clean commit с Main29.

После commit/build UI/clean export producer запустить один раз:

```bash
python3 scripts/run-update-resident-main-native.py \
  --source-archive /mnt/hgfs/shared/WinBridge/sources/PRODUCER.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261029T143435Z-944a39c-3580f462.zip \
  --cache-run 20261017-061527-f4639971 \
  --worker17-reference 'K:\VMs\shared\WinBridge\releases\20261026-182213-6cd81f3e\winbridge-worker.exe' \
  --worker17-reference-sha256 536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712 \
  --test-runtime-directory 'K:\_proj-26\winbridge\runtime-test-204' \
  --profile worker19-updater22 \
  --controller-config .winbridge/controller-snapshot.local.json \
  --controller-binary .winbridge/controller-runtime-updates/20261028T071841Z-203/winbridge-controller-0.2.0
```

Fixture controller использует.203:1144, stable test slot204 сохраняется. Network
preflight900s отделён от original startup240s. Installed runtime не заменяется,
desktop не нужен, окон нет. При неизвестном ACK читать saved context/key/taskId
из `.winbridge/update-preparation/main29-204-<run>/`, не submit повторно.
Рабочий worker оставить запущенным; desktop можно закрыть.

## Independent reader и предел доказательства

Reader связывает kind/schema19/22/source/target/exact selection, весь прежний
service/controller/lifetime/Ready-loss prefix, два V7 views, actual foreign peer,
JSON spoof/version rejection, unchanged snapshots/bytes/history и cold facts.
Отдельно требуется реальный Debian worker_connected event для session/generation
из обоих current headers. Незнакомые поля и permission flags не допускаются.

Новые29 negative mutations дополняют24 local +27 controller +37 lifetime +15
Ready-loss и10 прежних marker checks. Перед pipeline обязательны native группы:
13 resident model/actor cases,1 bound V28 mailbox и1 Windows named-pipe OS peer.
Unix socket case не заменяет named-pipe proof; пустая выборка не считается pass.

`projectionSha256` связывает полный accepted frame в trusted worker, но body
этого frame независимо не экспортируется. Reader проверяет header/scope/current
session/shape digest и actual producer; он не объявляет независимую проверку
полного readiness body. В результате явно
`fullProjectionBodyIndependentlyVerified:false`. Это не проверка полного IO fault
matrix, forced reconnect, Active, permanent keeper или следующего обновления.

## Source проверки перед native

983 workspace/update-validation passed/17 ignored/126 groups, Linux/MSVC
workspace/all-target Clippy с -D warnings и fmt/diff прошли. Windows cross-check
нового integration test первоначально нашёл слишком узкий pub(super) у helper:
расширено только до grandparent native test module, production visibility не
менялась. Повтор прошёл. Новых portable Rust tests не добавляли: existing frozen
view/cold resident/mailbox regression исполняет изменённый cold projection.

58 Python tests в9 readers прошли, включая6 новых synthetic Main29 tests:
29 mutations, full historical prefix, actual activity requirement, точная native
выборка, coherent lifetime aliases, frozen defaults и mandatory Windows boundary
selections. Эти fixtures не являются Windows execution proof.
Main27/Main28 readonly reread после shared reader изменений идентичен прежним
JSON audit, без новых Windows RPC.

Windows parser/storage/slot preflight task9a18f07c-a35f-468f-8be5-fa8c38380e05
succeeded0: parser errors0, slot processes0/worker EXE absent,
K:20,331,155,456 bytes. SHA actual checked PS source:
4354fee6a29d099507b71a8f9f8957b378db9812afa382adbdf13384d17fadbc.
Parser input опубликован create-new по SHA в WinBridge/validation-inputs, маленький
argv1270 bytes; source execution/build в preflight не выполнялись.
Installed0.2.0/source48c834c/gen946f1128/active0/errors=null прежний.
До тяжёлых source проверок Debian17,706,803,200/shared20,324,823,040 bytes, выше6GiB.
Новой cleanup не было. Logs: `.winbridge/review-2026-10-29/resident-main29-*`.

После native terminal обязательно independent reread. Затем продолжить durable
finalization/root decision/scoped ReleasePending→Active, consuming permanent
keeper/отпускание pins и всех updater authority references, full native proof,
next-update delegation/max2 keepers/two generations и U2–U4.
