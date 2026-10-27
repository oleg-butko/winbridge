# Windows worker0.2.0: native multi-controller gate

2026-10-27. После включения mainba9557b и Main25 profiles готовится worker0.2.0.
Номер версии отражает topology1–4 и несовместимую назад schema19, а не установку.
Installed0.1.1 и рабочие configs/state автоматически не меняются.

`validate-multi-controller-native.py` запускает четыре отдельных controller процесса
на .204:1145–1148, последний primary. Это проверяет native Windows worker с четырьмя
независимыми сетевыми сессиями; не выдавать за одновременный запуск VMs .203+.204.
Controller и worker должны иметь один clean source commit; archive/manifest verified.
Mutation envelope сохраняется до каждого submit/cancel, unknown outcome не replay-ится.
Bootstrap идёт через прежний worker, фикстуры имеют свои state/project roots.

`test-multi-controller-native.ps1` удерживает build-cache lease, собирает normal
worker без update-validation и повторяет cold released17→19 migration для exact
нового source. Копии migration используют только local IPC. Сетевой worker запускается
только по разрешённому пользователем пути runtime-test-204/winbridge-worker.exe.
Там нет пользовательского config/state. До build нужен6 GiB floor; source повторно
проверяется после завершения. Нет UI, служб, firewall modifications и Windows cleanup.

Сетевой сценарий: четыре concurrent project/tree, одна очередь на общий project,
cross-controller dedup, creator по session, остановка и перезапуск одного controller
без отмены принятой задачи, сохранение первоначального creator, cancellation другим
caller, отсутствие descendant processes, продвижение очереди и общий stdout/history.
Фактические fixture executions считаются по runs.log; network ready и завершение
проверяются по одному journal context/generation. EXE экспортируется с SHA/descriptor.
Независимый semantic audit,13 negative mutations и два чтения ready bundle завершают
проверку. Новый автоматический run допускается только после terminal предыдущего.

Пока это подготовленный сценарий, native proof и готовый release не объявлены.

## Локальная подготовка

Первый полный test build0.2.0 стартовал при13 GiB, но новые версии binaries заняли
ещё8 GiB.14 updater tests отказали на корректном6 GiB floor (available5.3GB), а не
на несовместимости версии. Новые сборки остановлены; удалены только устаревшие
неиспользуемые воспроизводимые Linux test executables, с проверкой live process
inode и исключением libraries/текущих binaries. Windows/runtime/reports не менялись.
Удалены294 старых test binaries/22,507,358,480 bytes; libraries исключены.
Plan и deletion receipt — /tmp/winbridge-worker020-old-test-cache-{plan,removed}.json.
После cleanup26 GiB; запускается повтор workspace/update-validation проверок.

Повторный Linux workspace/update-validation прошёл894/0/17ignored. Python profiles6/0,
fmt, syntax/help нового Python driver, согласованность versions/lockfiles,
TypeScript, UI build и scaffold304 прошли. Native результаты ещё впереди.

## Первый native run и текущий Windows blocker

Release source3435f0c667370e94485f4451b4c68ec80fe9810c,
archive winbridge-20261027T152246Z-3435f0c-a6ad7fcc.zip,
ZIP SHA fba59ac32b5cdbc35788a229dabc3b52a8899f81fcceb63bf4e3eee667fb03b6,
manifest SHA df6581163015f026252adedf691a9a3b2b8645c6dd1bc09671ee22cfdeb62952.
Run20261027-153506-fe00e6f2/task29421ccd-9e84-457d-87d0-59e3b99e35c0.
Normal worker build65.849s, exact-source cold17→19 passed12.10s.
Worker EXE SHA90e09bcbcbe1a5c1c385d9de602c5cb619e1c6860a0160302439e80d7f4752fd.
Это пока не release и не положительный network proof.

Один controller — неизменённая копия действующего0.1.0/source2aac553/dirty=true,
SHA6cb8146a07434832892b9276726eca4e866baebfb92331cdc58c725d0ec2bc52.
Остальные три —0.2.0/source3435f0c/clean, pinned Linux copy
/tmp/winbridge-controller020-3435f0c,
SHAa97cf3ce0390704aa9f5c2cc901f2f2f66bd10b1575dc9c74e07b80d6778c182.
Optional legacy controller parameter зафиксирован отдельно в coordinator metadata;
старый controller не обязан поддерживать новые readonly wrappers, их проверяют
новые peers. Existing production controller не перезапускался.

Network ready не получен; новые task mutations в тестовый worker не отправлялись.
Powershell harness сообщил connection failure, process exit unconfirmed и потерял
stderr из-за ожидания ReadToEnd. Report/pins сохранены. Runtime slot НЕ переиспользовать.
Raw Windows probe: PID3524, HasExited=true/exitCode4294967295, но wait object258
(WAIT_TIMEOUT), image unavailable; родители5748/19792 exited1/signaled.
CIM ещё перечисляет этот worker. Original bootstrap task остаётся running;
штатный fenced cancel принят, cancelRequested=true, managed tree termination pending.
Запрос и ответ cancellation сохранены до/после отправки; никакого resubmit.

Readonly diagnostic tasks8f3b35da…,d41b0a79…,0363b1f6… succeeded/0 через другой
зарегистрированный project fixture. SecurityCenter2 сообщает Outpost Firewall Pro;
чтение NetFirewall rules вернуло Access denied. Причина pending termination пока
не доказана; нет разрешения менять/ослаблять firewall или системные настройки.
Пользователю задан вопрос о firewall/pending notification. Installed worker0.1.1
отвечает, но active1 ожидает cleanup; переключение обратно на.203 пока НЕ объявлять.

Harness исправлен для следующей попытки: worker-process.json сразу после spawn,
stdout/stderr копируются в files по мере поступления, partial logs не затираются,
last Info сохраняется при connection timeout. Бounded capture/exit подтверждаются
отдельно; незавершённый pipe disposal не должен блокировать публикацию error report.
Эти новые diagnostics ещё требуют native проверки. Новый run только после
подтверждённого завершения прежней задачи и отсутствия worker в test slot.
