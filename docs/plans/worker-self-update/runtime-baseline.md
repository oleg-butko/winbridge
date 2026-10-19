# U1: durable baseline установленного runtime

2026-10-18. Trusted library; supervisor mailbox и public update ещё не подключены.
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
ещё впереди. Сейчас Windows повтор проверяет schema 7/live-control fix отдельно.

Следом: internal supervisor v7 admission/status, responsive hashing, native
positive receipt fixture и previous-version refusal. Затем runtime preimages,
candidate health/rehearsal, approval, exact shutdown и installation transaction.

Локально: 498 workspace tests passed, включая восемь baseline DB fixtures.
Migration fixtures старых версий удаляют новые таблицы перед downgrade; production
migration не использует IF NOT EXISTS для обхода неожиданных таблиц.
