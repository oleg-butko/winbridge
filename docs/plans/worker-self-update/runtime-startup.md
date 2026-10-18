# U1: ранняя проверка известного runtime

2026-10-18. Worker Main перед `Worker::open_file` проверяет manifest
`winbridge-installed-set.json` рядом с exe. При его наличии обязательны local
updater config, readonly совместимый updater journal и совпадение installation/
scope, runtime directory и точного worker config. Проверяются manifest и четыре
exe; текущий worker descriptor берётся из собственного скомпилированного кода.
File ID, SHA-256, PID/birth/host/session связывают наблюдение с этим процессом.
Manifest не может назначить другой loaded build даже при правильных file hashes.

Guard передаётся worker и удерживает все пять Windows read handles до завершения
процесса. Повторное attach запрещено. Проверка выполняется до миграции рабочего
журнала; updater metadata читается через inspect, без открытия writer/migration
и без присвоения supervisor epoch. Source manifest binaries не сравнивается с
текущим src: это разные поколения и отдельные source guards.

`winbridge-worker --config PATH verify-runtime` использует тот же constructor,
печатает readonly identity и завершается. Worker journal, scheduler, controller
connection и update authority не открываются. Отсутствующий manifest означает
Bootstrap required для этой команды. Обычный legacy/dev `serve` без manifest
продолжает работать; это не включает update authority и не публикует manifest.
`serve-recovery` сохраняет отдельный readonly диагностический путь.

Три integration tests запускают настоящий Main из изолированных копий worker:
missing/corrupt/foreign manifest, changed image и fabricated loaded descriptor
отклоняются без создания worker state и без изменения updater DB. Другие образы
в этих отрицательных fixtures синтетические и не запускаются. Полная SHA-проверка
больших debug exe получает bounded 120 секунд, без подмены metadata.

`scripts/test-update-runtime-startup.ps1` предназначен для положительного native
gate: копирует четыре настоящих verified candidate exe в новый fixture runtime,
явно создаёт fixture manifest/metadata, вызывает readonly настоящий worker и
сверяет его PID/build/file identity. Затем повреждённые set/config проверяются
через настоящий `serve` до журнала. Сохраняются команды, output и длительность.
Все файлы остаются в собственном validation run; installed runtime не затрагивается.
Окон, сети и успешного scheduler startup нет. Fixture manifest не является
production bootstrap publication, и этот тест не доказывает worker health/activation.

Внешние RPC, worker schema 15 и updater schema 7 не меняются. Authenticated live
identity control, durable baseline до SourceApply, persistent cutover/activation
guards, publication, shutdown и runtime replacement остаются следующими шагами.
Само наличие известного набора не разрешает install и не завершает U1.

Дополнительный `scripts/test-update-runtime-lifetime.ps1` подготовлен для
настоящего `serve` в этом fixture runtime: уникальный instance ID, controller
отключён, direct commands/recipes недоступны, исходный worker state отсутствует.
Проверяет отказ write-open всех пяти файлов во время работы и освобождение handles
после остановки своего idle PID; bytes и updater DB должны сохраниться. Это forced
fixture stop, не graceful shutdown/activation. Original generated config сохраняется
как fixture artifact. Script принимает только путь внутри конкретного validation
run, проверяет source/helper/image hashes и публикует отдельный proof/ready pair.
Проверка времени жизни ещё не выполнена; рабочий runtime не используется.

Локально: 485 workspace tests, Linux/Windows Clippy all-targets, contracts/fmt,
scaffold, Python export/runner и PowerShell syntax passed. Initial full run один
раз получил unknown в быстром Unix report fixture; отдельный повтор и 30 repeats
прошли. Ревью устранило возможный exit до PID binding: report helper ждёт реальный
durable Running, без временного sleep. Production unknown/no-replay semantics
не менялись; повтор полного suite passed. Positive native gate пока впереди.
