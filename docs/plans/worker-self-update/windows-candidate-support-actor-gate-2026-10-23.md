# Support actor18/view10: actual Windows Main gate пройден

2026-10-23. Task **adf48103-c02b-40e6-9442-6d089882c48c**, run **20261023-233438-99674bc1**,
terminal **succeeded/exit0**. **23 native tests /6 groups /8 commands**.
21 ready files сверены дважды; independent verifier отверг28 mutations.
После controller-only исправления verifier выполнен readonly --verify-run той же
task; новый Windows submit для этого не выполнялся.

| Pin | Значение |
| --- | --- |
| Producer | db0105a135256f93d30e273c449fdd937bdc776d |
| Source ZIP | winbridge-20261023T233334Z-db0105a-8e9bd9e8.zip |
| Source ZIP SHA256 | cfaa944add1d1562998855cb84b61c7d5b6dcfd9f4d54a23674d7c0b4bff0aa3 |
| Source manifest SHA256 | 2d8eeedc781aa5a6ef23705c1e3fb2b76b850f79c4c949e9e91fed25d13da59b |
| Target fixture | 7186bb652a6148b2866dc966a055f041de2d67a9 |
| Target ZIP | winbridge-20261023T224917Z-7186bb6-59eeb744.zip |
| Target ZIP SHA256 | 94ed85726985c32ee6678e9bc1d1c013c4f9813df2be6fd7206a7642cf695a49 |
| Target manifest SHA256 | e4740f078caf2743768e78c90246df3b5bdb308fd69195821ae03ae000ff3484 |
| report.json SHA256 | 43aa6217c5bdd86f596389f53e34d4bd1918f4556b05976dfafb8bf8f1ab5ba8 |
| candidate-support-actor-proof.json SHA256 | 3158cc0b9213ca20661e679a508dad380aabe813efcd6391c28f85eb02b09682 |

## Что проверено

- Model/SQL12, protocol2, policy6, frozen views1 и real mailbox1:22 prerequisites.
- Actual product Main в отдельном owned Job выполняет baseline/preimages/source/
  build/artifacts/health, затем новый v18 query. Candidate — distinct clean7186
  schema16; executor стандартный, validation feature только у test worker.
- Lost ACK: request отправлен ровно один раз; приложение не читает response,
  дожидается committed admission через readonly metadata, затем закрывает pipe.
  Supervisor выполняет единственный fresh permit. Checked revision5/history6,
  exact PID/Job/resume/cleanup/report binding сохранены.
- Root остаётся CandidateVerified/revision10. Checked хранит шесть blockers:
  отсутствуют activation/controller/desktop contracts и соответствующие drivers.
  Duplicate возвращает тот же record/report, без нового процесса.
- Parent дополнительно читает **normal serve IPC**, а не control pipe.
  Build/generation совпадают с actual Main. SourceHeld revision1 сохраняется,
  RequestShutdown count0. Старые четыре image bytes/File IDs прежние.
- Явный fixture abort подтверждает physical cleanup, release revision2,
  FailedBeforeCutover и остановку supervisor. Test worker затем завершается
  только по fixture cleanup. Saved support/history/cold Checked не меняются.
- Verifier связывает exact run/runtime paths, update/owner/root, producer/target,
  command/process, worker/config/schema/lease/images и passive view shape.
  Подмена root, generation, cleanup, blockers, history, counters и добавление
  authority fields отвергаются.

Update **a738cc69-29b7-4fbc-a042-e2620df3d671**,
support attempt **c84ee635-98d6-480a-86e5-d8cb1d24eb45**, owner epoch2.
Updater16/worker16; actor18/view10. Query1 и contracts15/16/17 сохраняют смысл.

## Исправленные ошибки сценария

Первый run20261023-231817-d6b1374c / task
fe719061-08e8-4f5a-a0a7-6c6b225dd3a5, producerede7318, terminal failed/1.
Actual support Checked5 уже был записан, затем parent отправил normal Info в
update-control pipe и получил EOF. Исправлен только endpoint в harness;
runtime guards/pins не ослаблялись.19 ready files failed run сверены дважды,
полного success proof нет. Namespace сохранён, повторного исполнения его
эффектов не было. Новый run выше использует новую isolated installation.

Controller verifier review190d180 добавил exact run/final worker scope. После
успешного Windows retry выявлено shadowing path переменной root словарём update.
Переименован входной путь expected_root; readonly recheck того же terminal
результата прошёл. Это не потребовало новой сборки/запуска на Windows.

## Граница готовности и продолжение

Это native исполнение нового **journal-bound support driver через Main**.
Production drain entry здесь намеренно не вызывается:
productionDrainEnforced=false. Legacy DrainCandidate13 остаётся frozen.
Нельзя объявлять обязательный pre-drain gate реализованным по этому тесту.

Следом supervisor19/view11 и atomic production support→drain record/schema17,
actual16→17/refusal, затем Main должен запросить новый entry и получить
durable blocker **до BeginDrain**. Далее activation из resident guard,
local/controller health, remaining U2 faults, U3/U4. Public install off.

[Код и дальнейшая форма реализации](candidate-support-actor.md).
[Текущий handoff](../../handoff.md).

## Окружение и readonly recheck

Shared: /mnt/hgfs/shared/WinBridge/workflow-validation/20261023-233438-99674bc1.
Local pins: .winbridge/workflow-validation/20261023-233438-99674bc1/support-actor-verified.json.
Observer logs: .winbridge/update-preparation/candidate-support-actor-native-retry.log
и candidate-support-actor-native-readonly-recheck.log.

~~~bash
python3 scripts/validate-update-support-actor-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261023T233334Z-db0105a-8e9bd9e8.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261023T224917Z-7186bb6-59eeb744.zip \
  --cache-run 20261017-061527-f4639971 \
  --verify-run 20261023-233438-99674bc1
~~~

Команда только наблюдает ту же task, не отправляет новую. Source/target archives,
failed/succeeded namespaces и reports сохранить.

Disk bytes Windows **31360487424→30656147456**,
Debian **25311584256→25318359040**;
cache reused, минимум6 GiB соблюдён, очисток не было.
Installed d052 connected/active0, generation8f8d1fb9 прежняя, recovery/
maintenance errors=null. Worker оставить, desktop можно закрыть; окон не было.
