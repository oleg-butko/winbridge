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
