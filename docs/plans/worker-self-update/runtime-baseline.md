# U1: durable baseline установленного runtime

2026-10-18. Trusted library и internal supervisor v7; public update ещё не подключён.
Updater schema 8, worker schema 15. Это проверка исходного состояния перед source
apply, а не разрешение остановки процессов или установки новой версии.

До source/compiler попыток updater сохраняет immutable intent: update/request,
installation/scope, exact supervisor launch/owner/epoch и frozen worker binding.
Запись атомарна с `RuntimeBaselineAdmitted`; корневая фаза остаётся `Requested`,
revision увеличивается. Пока intent без receipt, BeginPreparation и source plan
запрещены. Обычный lifecycle API не может записать baseline events.

Trusted driver читает known installed-set manifest и закрепляет пять локальных
файлов. Supervisor своим PID запрашивает worker control v2 ObserveRuntime.
Проверяются четыре роли/размеры/SHA/File ID, runtime directory identity, manifest,
PID/birth/host/session, generation/context/build/config и сохранённая installation.
После hashing повторяются local pin/config/owner checks. Receipt сохраняется
атомарно с `RuntimeBaselineVerified`; manifest/files pins живут до commit.
Source snapshot A и установленная binary версия C могут различаться: каждый
проверяется по собственному manifest, их source commits не приравниваются.

Повтор готового baseline возвращает историю без новых file/IPC effects. Pending
baseline можно дочитать только в исходном owner epoch и с тем же binding.
Перезапуск не перепривязывает intent к новому worker. Source/compiler история
исключает позднее добавление baseline. Старые операции после migration сохраняются
без выдуманной baseline; они ещё пригодны для прежней source-only диагностики,
но будущий install driver обязан потребовать подходящую baseline явно.

Reader проверяет receipt и обе parent transitions в одном SQLite read snapshot:
request, sequence/revision, events/keys, owner/epoch и времена. Изменение receipt,
игнорирование обязательного INSERT, конфликты idempotency и неверный frozen runtime
не дают подтверждения. Синтетические DB fixtures не подтверждают здоровье exe.

Migration 7 → 8 использует прежний exclusive owner lock и consistent SQLite backup,
повышает epoch, сохраняет историю и ничего не принимает автоматически. Updater
старой версии обязан отклонить schema 8 без записи. Полный native gate schema 8
ещё впереди. Schema-7 live control уже прошёл отдельный native gate.

Internal supervisor v7 добавляет CaptureRuntimeBaseline / ObserveRuntimeBaseline.
Admission сохраняет intent до ACK; blocking capture не удерживает mailbox.
Typed projection различает NotAdmitted / Pending / Verified. V7 view имеет schema 2;
v1–v6 по-прежнему возвращают schema 1 без нового JSON поля. Driver failure остаётся
видимым, status не повторяет hashing/IPC; source admission сохраняет запрет.

Следом: native positive receipt fixture и previous-version refusal. Затем runtime preimages,
candidate health/rehearsal, approval, exact shutdown и installation transaction.

Локально: 500 workspace tests passed, включая восемь baseline DB fixtures,
actor failure/no-replay и typed projection/legacy-shape checks.
Migration fixtures старых версий удаляют новые таблицы перед downgrade; production
migration не использует IF NOT EXISTS для обхода неожиданных таблиц.

Native clean 34f39ed, run `20261018-210729-e33e2cbd`: all 175 tests / 19 groups
passed, including the positive baseline/control case and previous updater refusal.
The final script failed before the full candidate build because both PS/Python
proof checks still assumed schema 7. They now require the exact constant from the
verified source/archive. Portable version/mismatch regressions passed; combined
native retry is still required. Installed worker stayed healthy and unchanged.
