# Actual preparation faults: три Windows cases пройдены

2026-10-23. Task `50979d1f-6f38-4d0a-8f15-6e96a243222d`, run
`20261023-081649-b9bf5a72`, terminal succeeded/exit0. **40 native tests /10 groups**,
включая три full actual Main cases. Все24 ready files проверены независимо дважды;
после ревью verifier отклонил **48 искажений настоящего proof**.

Отчёт: `K:\VMs\shared\WinBridge\workflow-validation\20261023-081649-b9bf5a72`,
в Debian `/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-081649-b9bf5a72`.

| Файл | SHA-256 |
| --- | --- |
| report.json | `f789c259e08e310d50e0f6006b387acb752cd77c5819c10d25e823fbeb0f40f1` |
| candidate-health-actor-proof.json | `92f59491f1359782a8ec861f93d784fcefea5373642f8c498321fa7946e1f0f9` |

Producer clean `ddce86680e35bc3d8d263ad81db8dcf1f6eeec2b`, source archive
`winbridge-20261023T081637Z-ddce866-cb71cdf0.zip`, SHA
`c3b04601efdf15bd8082f67f890d836967e42d373f90f5e6c2b2099ebdf8ddcd`,
manifest SHA `df5f535b865527c8fb8faf5a93467c78605759b7c6e3ef9c1b5d944156acc529`.
Target cee06a7 и его exact pins прежние из
[happy gate](windows-runtime-preparation-main-gate-2026-10-22.md).
Updaters14/worker16, supervisor v16/view8. Candidate — limited same-schema fixture,
не production auto-update target. Новая replacement transaction из7f3a952
**не входит** в immutable producer этого gate.

## Что проверено

Каждый case имеет отдельные runtime/state/installation и проходит actual Main,
source/build/artifacts/health, drain/quiesce, exact exit и Verified consistent backup.
39 preliminary tests сохраняют preparation/directory/backup regression coverage;
один explicit-input integration test выполняет три настоящих pipelines.

| Case | Сохранённый результат |
| --- | --- |
| existing_directory | Indeterminate/revision1, пять Pending, directory receipt отсутствует. Созданный fixture sentinel сохранился с прежними File ID/bytes/SHA; adoption/overwrite нет. |
| changed_candidate | После health/backup изменён первый payload. Несовпадение обнаружено до directory create; Indeterminate/revision1 и пять Pending. Изменённый candidate сохранился, автоматического восстановления нет. |
| hold_before_receipt | Feature-only updater создал первую independent copy и остановился на bounded barrier. Сохранён Preparing/revision3, worker Issued, остальные Pending; receipt отсутствует. Marker связывает PID/build/input SHA/exact record и actual copy. После завершения собственного Job тот же record/partial file сохранился, cold view Indeterminate, physical custody=false. |

Во всех cases duplicate возвращает тот же attempt/prefix без нового IO. Original
runtime, preimages, Verified backup и worker DB сохраняются; после owned exit
отдельно сверены output prefix/sentinel и candidate files. Поле
`runtimePreparation.sourceBytesUnchanged` относится к worker journal — источнику
consistent backup; намеренная candidate mutation показана отдельно и не объявляется
неизменностью candidate. Normal startup остаётся заблокирован maintenance.
Private supervisor control/token не публиковались. Установленный runtime не менялся.

## Ревью результата

Первичный Python verifier проверял число preimages, но принимал подмену SHA одного
из них. Усилен **локальный verifier**, без повтора Windows: fixed manifest-first
порядок previous-runtime copies, exact old bytes/SHA/type и independent IDs, path
scope/chronology/directory identity, восстановление typed Rust receipt bytes и
сверка её SHA с `plan.proofs.preimagesSha256`. Настоящий proof прошёл усиленную
проверку, подмена отвергается. Добавлены две Python регрессии на UTF-8/порядок JSON
полей и unknown fields, всего7/0. Исправление verifier появилось после native
producer; оно не приписывается Windows-скомпилированной версии.

48 отрицательных mutations покрывают record phase/revision, owner/root/attempt,
projection/duplicate, manifest/source, preimage bytes/ID/digest, сохранность backup,
post-crash custody/candidate files и реальные held PID/input/copy/sentinel evidence.

Windows available **41,290,145,792 →39,767,875,584 bytes**, Debian при повторной
сверке26,872,422,400 bytes. Approved cache20261017-061527-f4639971 reused;
retained Windows directories не удалялись. Local preparation16/0 и обе all-target
Clippy прошли до export; actual Windows execution описано отдельно выше.

Полная локальная сводка/public proof:
`.winbridge/update-preparation/preparation-faults-native-verified.json`.
Это закрывает **первые три actual preparation faults**, не crash на всех возможных
IO points и не whole U2.2. Stream-fence/failed-receipt core tests сохраняют свою
synthetic-observer область. Whole replacement требует отдельного ledger/permit,
consuming custody, native rename faults и activation, затем U3/U4.

Тестовых UI окон не было. Все owned Jobs завершены; installed d052 worker healthy,
connected/active0/no maintenance/recovery errors. Desktop можно закрывать, worker
оставить. Для следующей короткой модели используются только isolated native data
tests; public install по-прежнему отключён.
