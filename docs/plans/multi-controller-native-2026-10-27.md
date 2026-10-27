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

Readonly native PowerShell Parser проверил оба обновлённых сценария из clean
archive6f01036/96d40329…: taskc7e11b20-2de8-404d-9ef1-c36f3cb2f4e8 succeeded/0,
ошибок0, новый worker не запускался. Root Python coordinator завершил свой
observation budget900s, сохранил last-bootstrap-observation и оставил только
owned controller processes; это не terminal Windows task и не cleanup proof.
Следующая Main25 candidate pin3435f0c остаётся отдельной от нового producer.

## Повторная проверка блокера

Повторный OS probe8e63be38-dfab-4d79-bcc9-3d4fdabe837f succeeded/0 подтвердил
тот же PID3524/exit4294967295/WAIT_TIMEOUT258; оба parent процесса signaled1.
CancelRequested остаётсяtrue, original task running/active1. Никакого нового
network gate, повторного cancel или изменения системных настроек не выполнялось.
Readonly Application/System event queryc6e2c957-3a64-458b-9af9-f6ce5c382d9e
succeeded/0: после начала run событий в доступной выборке нет; источник зависания
это не установило. Raw receipts сохранены в native run рядом с owner metadata.
Main по origin/main по-прежнемуba9557b; новый fetch не обнаружил изменений.

## Владение controller после завершения observer

Свежая проверка owner PIDs159563/159573/159585/159595 обнаружила все четыре уже
завершёнными; ss подтвердил свободные1145–1148 и работающий production1143/PID5312.
Запланированный SIGINT не отправлялся: предварительная проверка остановила teardown
при отсутствующем первом PID. Предыдущее утверждение о живых четырёх peers снято;
owner files сами по себе не доказывают liveness.

Выявлен дефект обоих native runners: Popen наследовал process group наблюдателя,
поэтому инфраструктура exec завершала peers вместе с CLI даже при unknown outcome.
Теперь controller запускается с stdin=DEVNULL/start_new_session=True. Owner metadata
остаётся обязательной; её ошибка до submit очищается через finally, а ошибка записи
owner нового/restarted peer останавливает именно только что созданный процесс.

Двухшаговый реальный эксперимент tools exec: обычный child162783 исчез после exit
parent, detached162784 сохранился; он затем явно остановлен. Отдельный actual
controller0.2.0/source3435f0c/PID162810 пережил завершение родителя, ответил через
свой Unix IPC на controller_status и затем был штатно остановлен SIGINT.
Receipt /tmp/wb-retain-1uyp2c0f/result.json; не было Windows worker/новых сетевых
прогонов. Это проверка Unix lifecycle harness, не Windows gate. Windows PID3524
и original cancellation остаются отдельным текущим блокером.

## Уточнение после свежей проверки пользователя

Пользователь сообщил: уведомлений Outpost нет, Process Explorer показывает один
установленный worker без дочерних процессов. Повторный API/CIM запрос подтвердил:
original bootstrap29421ccd… уже failed/exit1, activeTaskCount0; PID3524 отсутствует,
единственный worker14448 находится в production runtime. Readonly fresh-list task
dafb5009-ebc1-4453-be7e-a2d4155a79be succeeded/0, Windows free27,985,526,784 bytes.
Late terminal сохранён отдельно, старые диагностические receipts не переписаны.
Отсутствие завершения больше не текущий blocker; старые raw API поля HasExited=true
и WAIT_TIMEOUT не представлять как доказательство работающего child process.
Связь сбоя с Outpost не доказана. Следующий network run должен опираться на новые
partial logs/last Info и проверять readiness всех связей; никаких firewall changes.
Это не положительный native network proof, новый worker в этой проверке не запускался.

## Повтор по просьбе пользователя с наблюдением на Windows

Пользователь попросил больше диагностики и повторный run. Добавлены UTF-8
 diagnostics.jsonl с UTC/elapsed/bootstrap PID, source/build/migration boundaries,
PID каждого readonly CLI, отдельный server PID/image/config/SHA, primary Info
каждые несколько секунд и точный waitConfirmed при завершении. Ошибки Info
не замалчиваются. Ошибка записи диагностики не прерывает cleanup и исключает
успешный report. Connection budget240s записывается отдельно от updater deadlines.

Coordinator сохраняет startup-observations.jsonl с фактическими PID/status всех
четырёх controller, печатает изменения и хвосты stdout/stderr/diagnostics до ready.
При любой доступной новой связи читает readonly список всех worker links с их
ошибками. Это диагностические запросы; мутации остаются после подтверждения всех
связей. Новый run только с новым request/key/state после terminal прежней попытки.
