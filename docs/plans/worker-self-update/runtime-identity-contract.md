# U1: общий контракт runtime identity

2026-10-18. `RuntimeImageEvidence` и `LoadedWorkerIdentity` перенесены в
`crates/protocol`, добавлены generated TypeScript-типы. Старые Rust paths
`winbridge_update::artifact::ImageEvidence` и
`winbridge_update::installed_set::LoadedWorkerIdentity` сохранены через re-export.
JSON-поля, порядок сериализации и существующие journal digests не меняются;
worker/updater schemas остаются 15/7. Legacy RPC, Info и supervisor/control
versions не расширены этим переносом.

Два golden tests фиксируют прежние JSON bytes и roundtrip. Неизвестные поля
`canInstall`/`shutdownGranted` отвергаются. DTO остаются обычными наблюдениями:
deserialize не проверяет настоящий процесс, filesystem или authority. Только
constructor с удерживаемыми pins и authenticated peer может дать fresh evidence.
Общий тип не разрешает остановку, запись файлов или installation.

## Следующий vertical scenario

1. Readonly worker control v2 `ObserveRuntime`, отдельный от v1 lease commands.
   Он возвращает собственную loaded identity из живого guard. V1 ответы сохраняют
   прежний shape; v2 не должен допускать mutating v1 commands или effect fallback.
2. Сохранить существующие проверки exact supervisor PID/birth, immutable worker
   binding, updater owner/epoch, installation/scope и config. Identity process/build
   должны совпасть с bound worker. Конфиг с pending restart не даёт fresh acceptance.
3. Clone guard под коротким mutex; hashing выполняется вне Store/global locks.
   До и после I/O проверяются authority/config. Устаревший процесс/epoch не получает
   актуальный результат. `Registration.image_sha256` остаётся SHA updater;
   loaded worker SHA берётся отдельно из `identity.image`.
   Пути из DTO не назначают policy: local roots и четыре фиксированных имени
   берутся из проверенного installation config/manifest.
4. Следующий coordinator закрепляет known manifest/четыре images и fresh live
   identity в durable baseline до SourceApply. При отсутствии baseline старую
   операцию нельзя задним числом объявить безопасной для установки.
5. New worker activation получит отдельное отношение к install attempt. Старый
   `SupervisorSpec.worker` и его context/generation/process не перепривязываются
   к новому процессу. Историческое наблюдение не становится свежим file grant.

[Control v2 / supervisor v6](runtime-live-control.md) реализованы следующим блоком;
durable baseline и activation ещё не реализованы. Положительный actual Main/pins gate существующего constructor
сохранён в [native отчёте](windows-runtime-startup-gate-2026-10-18.md). Перенос DTO
проверяется локальными контрактными/receipt tests и Windows cross-check; новый
native gate будет включать следующий runtime control scenario.

Проверки переноса: два exact-byte golden tests, frozen legacy contracts, четыре
installed-set tests и 11 compiler report/artifact ledger tests passed. Workspace
check, Linux/Windows Clippy all-targets, fmt, generated contracts, TypeScript и
scaffold passed. Windows process/file adapter code не менялся; отдельный новый
native runtime gate этим переносом не заявляется.
