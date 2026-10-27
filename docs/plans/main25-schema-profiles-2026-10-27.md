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
