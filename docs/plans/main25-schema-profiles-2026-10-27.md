# Main25: явные schema profiles и cold worker migration

2026-10-27, feature.multi-controller на Debian.204. Пользователь разрешил выполнить
план и вручную подключил installed worker0.1.1 к.204. После последнего Windows
прогона сообщить, что worker можно переключить обратно на.203.

## Реализация

- Mainba9557b включён в feature с ранее проверенной startup/Links адаптацией.
  Временный LocalService не запускает network/scheduler/mutations; Main25 preflight
  по-прежнему требует явный single-route fixture, не выбирает primary из topology.
- `update-activation-service-proof.py`: profiles `worker17-updater20` (исторический
  default) и `worker19-updater20`. Worker schema проверяется в context, writer
  binding и service compatibility. Updater20, view17 и startup240s не меняются.
  Unknown profile/schema, bool/float/string coercion и смешанные17/19 отклоняются.
- Независимый verifier связывает выбранный profile с константами из обоих verified
  ZIP/source manifests и затем проверяет proof. Ожидаемая версия не берётся из proof.
- Новый create-only runner сохраняет original fenced context/key/request до submit,
  не replay-ит неизвестный исход, использует отдельный controller1144 и stable
  `K:\_proj-26\winbridge\runtime-test-204`. Чужой runtime-test и pending
  conservation plan не используются и не перемещаются. Controller остаётся при
  indeterminate task, закрывается после terminal. Windows desktop не нужен.
- Отдельный ignored native test создаёт реальную историю released worker17 на
  новом state, затем запускает worker19: same task/output/dedup, verified SQLite
  backup17, legacy unknown creator, новые creator/cancellation после restart,
  old17 refusal без изменения main/WAL frames и два marker executions без replay.
  Reference EXE копируется по явному SHA, installed state/config не открываются.
  Это cold17→19, не migration authority для active updater handoff.

## Проверки и execution pins

До build Debian13 GiB. Readonly Windows preflight task
`a0c6924a-360e-4e38-98d4-0993fcee6857` succeeded/0:
free31,514,517,504 bytes; runtime-test занят прежним reservation, runtime-test-204
не существует. Installed worker source59249e6/generation772e0679/journal incarnation
5a8eb173 остаётся прежним. Никаких production replacements/cleanup/firewall changes.

Linux workspace894 passed/17 ignored; Python profile/evidence boundary tests6/0.
MSVC workspace/all-targets/update-validation check и Clippy `-D warnings`, UI/typecheck,
fmt/scaffold303 прошли. Source/target pins и native результаты записываются по завершении.
Не считать подготовленные profiles или synthetic boundary tests native proof.

## Первый закреплённый producer

Source a127894f8fdcd225ce1144c88365b9c28863816d, archive winbridge-20261027T132020Z-a127894-342c8cf0.zip;
ZIP SHA f32a8534de582ca14ae1d14ff3ac7d192ca0a52f071cef2e6b4a7b547807a196;
manifest SHA c9510d35ffe0cad11002d73b7e5e1ace44eded783cce33d3ac4957db05209a5f.
Следующий docs-only commit служит отдельным target pin с тем же кодом.
Эти archives содержат worker19/updater20 и built UI; это ещё не native proof.

## Первый native bootstrap

Run20261027-132156-64322013/task5426a8e2-2a0b-4c15-8173-7b2fbb42ba75
завершился failed/1 до native test commands и до reservation runtime-test-204.
Причина — добавленная PS hashtable+array вместо array+array при включении cold
migration command. Исправлено оборачиванием первого command в массив. Это ошибка
harness, не Windows worker и не migration/Main25 proof. Test controller1144 закрыт,
installed worker0.1.1/generation772e0679 оставлен. Старый run/report сохранён.

## Второй native run и найденный дефект fixture

Run20261027-132426-9b2db297/taskd0a1bf3c-c2b2-4672-83c4-8ce3f2f5fbe2,
producer b731ad54385c1618e97228f5c5f923bd100bdbd1 (ZIP552b91dd…,
manifest8478b15f…), targetd500fc5.137 prerequisite tests passed;
actual worker17→19 cold migration passed12.57s, отдельный независимый audit
прочитан дважды (proof SHA581aad4d548189dea155fc8e7df9da5e907d505fa97556790da4c63bdc45af77).
Это не полный Main25 proof.

Сетевой worker сначала получил10013; пользователь разрешил постоянный
`K:\_proj-26\winbridge\runtime-test-204\winbridge-worker.exe` в firewall.
После этого controller1144 подтвердил actual sessionc7f4770f…/generation6c5640f5…;
правило нужно сохранять. Cargo hashed test binaries и случайные копии не наследуют
сетевое разрешение этого пути. Cold migration использует только local IPC.

Full Main25 остановился на финальной копии worker state: supervisor stderr
`Export validation scope rejected`. Helper feature hook оставался на schema1 и
`case/runtime`, хотя Main25 использует Input2/external reserved runtime.
Исправление использует общий `Input::fixture_layout()` с source/manifest/marker,
исключительной script lease и production disjoint checks; exact helper basename,
state/input/config/build проверки сохраняются. Это не разрешает миграцию при
active updater и не меняет production exporter без validation feature.
Добавлены final-backup public view и assert с driver_error; экспорт supervisor
stderr теперь учитывает stable runtime и его архив.

Слот автоматически заархивирован под original lease и native retirement proof:
reservation915d85be-3fba-46d9-b8be-7ff83b53b407,
archive plan SHAa7c26ab703c3f230b50a80f091146ed356744ebc7f196b70c3537aa9b79fc502.
В runtime-test-204 остались только два базовых slot файла, owned processes отсутствуют.
Readonly diagnostic tasksb7ed9f54… и5cf02e23… succeeded/0; private files/SQLite
не экспортировались. Test controller закрыт, installed worker прежний; Windows
free29,977,870,336 bytes после run. Failed report и архив сохранены.
Следом новый clean producer и новый Main25 run; исходная попытка не replay-ится.

Исправление прошло targeted fixture layout5/0, Python6/0, MSVC worker all-targets
с update-validation, fmt и diff whitespace check. Debian свободно13 GiB.

## Третий run: copy исправлена, обнаружена policy epoch в normal startup

Source73752e12adc8e5c0467f67937adc13c7d08ebcd3,
ZIPe0008181ae7210538b3114ce5d8d4e1322189798efc87323a341f4fe44a2ff10,
targetd500fc5. Run20261027-150519-77f7cadf,
task8598db08-edcf-426d-a420-f2e8984a5c24 failed/1.137 prerequisites и cold17→19
прошли; actual Main639.08s дошёл до confirmed LocalService, затем assertion
preserved_rows failed. Это не положительный Main25 proof.

Readonly compare через системную winsqlite3.dll, с SQLITE_OPEN_READONLY и snapshot
transactions для24 tables, выявил только journal_authority(epoch/authority),
worker_journal_activations(0→1) и resource_policy.epoch. Все task/output/policy
payload/attribution rows неизменны. Private contents не выводились и не копировались.
Diagnostic taskc289da33-f594-47aa-86eb-94dbce007036 succeeded/0; предыдущий readonly
probe22939e59… отказал на long path, исправлено extended local prefix.

Причина последнего hash mismatch: обычный initialize_startup/configure_resource_scheduling
привязывает policy epoch к новому владельцу. Main24 hash учитывал только два writer
изменения и ошибочно считал эту normal-startup metadata повреждением истории.
Исправление сохраняет exact Main24 hash. Для Main25 проверяется ровно singleton1,
точная схема policy table, фактический next owner epoch, и только этот integer
нормализуется к baseline при общем hash. Policy payload и все другие значения
остаются внутри hash. Cold observation повторяет ту же проверку. Новый независимый
binding явно записывает before/after epoch и normalized column; worker19 profile
требует его,17 historical default не меняется. Добавлены4 negative audits и native
регрессия: неверный epoch/изменённая policy/новый столбец не скрываются нормализацией.

Runtime автоматически заархивирован: reservationb1d51e73-ac56-4821-9bd5-e781e759ab3d,
plan SHA4d194852c125039c78cfb1ed856f25bf515152372ff3fba8519bb06e9dc259d3.
Failed artifacts сохранены, исходная попытка не replay-ится. Python7/0 и MSVC
all-targets check прошли; новый положительный Main25 run ещё нужен.

После исправления: Linux workspace Clippy all-targets/update-validation и MSVC
all-targets/update-validation Clippy `-D warnings` пройдены, Python7/0, fmt и
scaffold304. Native normalizer regression и новый full Main25 отложены до
подтверждения termination PID3524 из независимого multi-controller run; slot не
переиспользовать. Изменения после release-source3435 в normal worker коде только
устраняют needless borrow внутри feature-only validation hook; normal product
поведение0.2.0 не меняется. Source/target будущего Main25 фиксировать отдельно.

Новый clean producer6f01036aa03adc6350cfe483113f2a503edfcb67 экспортирован:
winbridge-20261027T155350Z-6f01036-3d2ffe26.zip,
ZIP SHA96d403293a08d59b3d608cfb282911f2d5a7b0490ac07d94ee80f972fb3a074c.
Следующий target — worker0.2.0/source3435f0c из отдельного verified archive.
Policy regression использует catch_unwind для ожидаемых отказов: verifier проверяет
точный выбранный test argument и successful count, не требует contiguity строки
имени/ok через намеренно напечатанные panic diagnostics. Native запуск пока отложен.
