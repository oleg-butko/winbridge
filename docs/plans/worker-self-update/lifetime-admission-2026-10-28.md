# Lifetime: original admission и consuming preparation

2026-10-28. Следующий source шаг после [actual21→22](retained21-to22-2026-10-28.md).
Это preparation API и receiver **data snapshot**. Worker decoder6, sender, смена
runtime mode, normal-IPC lifetime observation, keeper role/commit/release пока не
подключены. Source worker19/updater22, implemented private5/actor26/view18 прежние;
private6 определён как proposed contract, а не advertised support.

## Original owner до intent

`runtime_replacement/activation_lifetime.rs` добавляет consuming
`ActivationProcessGuard::prepare_lifetime_blocking`. Он принимает только тот же
`Box<ActivationProcessGuard>` после controller confirmation: actual child/Job,
original channel, released old-state custody и удерживаемый config pin.
`ActivationLifetimePreparation` не имеет JSON/Clone constructor, держит этот же
owner и одноразовый свежий permit. Ошибка сохраняет original custody в
`ActivationLaunchFailure`; Drop не называется подтверждением tree cleanup.

До intent проверяются candidate image identity и private6 support. Pattern для
поиска строится sender в runtime: updater library также linked в worker, поэтому
прямая ссылка sender на verbatim receiver marker могла бы ошибочно рекламировать
не реализованный receiver. Verbatim marker должен удерживать только реализованный
receiver. Current worker не добавляет black_box для6; положительная native проверка
receiver/factory возможна только после следующей реальной реализации decoder.

Топология/confirmation selection берутся из original pinned config и точной
сохранённой independent controller observation. Сверяются source path/file identity,
ограничение64KiB, bytes SHA/config instance и readonly pin; file/path повторно
проверяются перед/после SQL intent. Нет caller URL, отдельного process/Store,
повторного lock acquisition или нового startup timeout. Работа выполняется на
joined owner stack с ORIGINAL IO runtime через `on_original_io`.

## Durable admission

`journal/activation_lifetime/admission.rs` сохраняет Issued до любого будущего
private send. Одна Immediate transaction проверяет original authority/epoch,
supervisor PID/birth, nonce, current local7, точный root LocalHealthVerified →
AwaitingController, physical6/writer/service/controller causal graph и current
schema22. Контроль current scope переиспользует `activation_controller` helpers.
Stored controller receipt должен совпасть с original independent observation,
включая accepted projection и выбранный link. Это подтверждённый ранее факт;
живая связь будет отдельно проверяться normal IPC перед final commit.

SQL primitives теперь Windows/test; public JSON writer отсутствует. Fresh permit
возвращается после transactional readback и commit, а consuming factory повторно проверяет
original custody, Issued, config pin, pinned live child и ORIGINAL monotonic
deadline до экспозиции preparation. Existing возвращает только data record;
другой key или frozen topology/selection даёт KeyConflict. При post-commit failure
owner пытается записать Indeterminate по тому же key, только пока authority и
causal scope ещё актуальны; при их потере запись отклоняется. Original custody
остаётся в failure object, второго send нет. Поздняя запись факта не
даёт нового бюджета или разрешения.

`inspect_activation_lifetime_receiver` читает один readonly snapshot current
schema/root/owner/local7 + confirmed controller + original Issued lifetime.
Unknown/Confirmed уже не допускают initial delivery. Этот объект не является
OS-authenticated receiver: будущий caller обязан уже владеть тем же
RunningLocalService, parent/channel и original deadline, проверенными до/после
чтения. Холодное чтение record не создаёт эти ресурсы.

Private6 command/ready wrappers strict, связаны с exact command и typed receipt.
Сама preparation **не отправляет** эти frames и не меняет worker mode.

## Следующий runtime шаг

1. В `apps/worker/src/activation_controller.rs` после V5 Ready перейти к настоящему
   V6 receiver на том же stream/parent/RunningLocalService. Только после реализации
   decoder включить verbatim receiver marker6 в worker; не имитировать support
   ради factory-positive test. Сейчас старый child должен отказать до intent.
2. До чтения/принятия V6 сохранить все parent/UTC/monotonic startup guards.
   Сверить current data snapshot, pinned loaded identity/config, реальную Store
   authority/lease/instance lock и оригинальный Job. Lifetime intent должен быть
   durable Issued; входной command точь-в-точь равен сохранённому.
3. Supervised quarantine оформлять **typed state transition** от verification,
   без `deadline=None`, нового timeout или повторного Store open. Проверить места
   с immutable verification deadlines: `local_service.rs` LocalServiceScope/
   LocalService, `store/activation_service_open.rs`, `controller_service.rs` и
   worker private parent watchdog. После принятой decision оригинальный keeper,
   та же Job/Store/authority и maintenance freeze остаются обязательными.
4. Ready должен подтверждать реальный entry в quarantine внутри initial window,
   не только прочитанный JSON. Lost ACK сохраняет этот mode под original keeper;
   cold/Existing не повторяют вход. Entry receipt historical, а текущие parent/
   mode/liveness наблюдаются отдельно. Для продолжения режима Confirmed/Unknown
   history того же intent не должна ошибочно требовать нового Issued admission.
5. Parent sender потребляет original preparation один раз. Normal IPC на настоящем
   endpoint/PID независимо подтверждает тот же child/Store/authority и lifetime.
   Затем сохранить lifetime fact; root ControllerConfirmed пока не вызывать.
6. Actor27/view19 и полный native gate выбирать при подключении runtime. Нужен
   новый clean target worker19/updater22 с реальным decoder6, stable runtime204,
   original240s и existing network preflight. Старые Main26/migration/OS gates не
   replay как замену нового lifetime proof.
7. Consuming keeper role освобождает file/config pins и updater journal authority
   после проверенного перехода, сохраняя original Job/process/channel. Иначе
   следующий update заблокируется. Final commit/scoped release, два поколения,
   next-update delegation/max2 keepers и remaining U2/U3/U4 впереди.

## Проверки

Focused lifetime model:8 tests passed, включая новую full observation/topology
матрицу и V6 wrong-version/rebound/unknown-field refusals. Добавлены SQL existing
selection conflict и receiver-data causal checks; existing SQL tests проверяют
rollback до/после checkpoint и dropped/ignored writes. Они используют synthetic
metadata и не доказывают actual OS custody. Финальные workspace/update-validation:950 tests passed,17 ignored,125 groups.
Linux и Windows MSVC workspace/all-target Clippy с `-D warnings` прошли; generated
Rust/TS contracts совпали. Fmt/diff/scaffold проверены перед commit. Новых native
Windows execution tests в этом checkpoint нет: positive consuming factory и
реальный V6 entry требуют следующего decoder/runtime gate.

Evidence: `.winbridge/review-2026-10-28/lifetime-admission-*`; workspace summary —
`lifetime-admission-workspace-summary.json`. Последнее измерение: Debian
10,321,481,728 bytes свободно, K: через shared24,184,139,776 bytes; сборки ниже
6GiB не запускались, очистка не выполнялась. Readonly installed Info подтвердил
source48c834c/build1793126009319459400, generation946f1128, active0 и обе ошибки
maintenance/recovery=null. Production controller131887 не перезапускался.

Installed worker не меняется; Windows tasks/UI этого source шага не запускались.
Рабочий worker оставить, desktop не нужен. Public activation/install остаются off.
