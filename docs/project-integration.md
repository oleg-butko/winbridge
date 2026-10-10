# Подключение следующего проекта

Этап 7 проверяет следующий реальный проект после Netwatch: сам WinBridge worker
как изолированное приложение. Оба проекта используют один [профиль сборки](../examples/service-profile.json)
и прежние runner/sync/workflow handlers. [Native gate прошёл](windows-projects-gate-2026-10-10.md):
59 Rust tests, оба application cycles и 15 verified artifacts. Этап 7 завершён.
Installed runtime и основной Netwatch не меняются.

## Что задаёт проект

1. Зарегистрировать существующий local workspace root, который не пересекается
   с другими project roots и shared delivery. Config сохранить через Settings;
   применяется после ручного restart worker. SourceRoot Debian задаётся отдельно
   в controller registry. `cwd` не является sandbox; сборка доверенная.
2. Выбрать общий recipe profile. Для этого примера snapshot содержит читаемый
   `winbridge-service-build.ps1` с параметрами RuntimeDirectory, LogDirectory,
   TargetDirectory. Это соглашение **тестового профиля**, а не обязательный API
   WinBridge: другой проект может выбрать обычный cargo.exe или свой script.
3. Добавить локальный `start-service` recipe с точным executable/argv. Netwatch
   запускает agent напрямую с отдельным instance; WinBridge использует явный
   PowerShell wrapper, который переносит stderr/stdout дочернего worker в log
   и stdout управляемой задачи. Script находится в проверенном snapshot и виден
   в preview состава, параметры — в command и recipePins.
4. Задать workflow build/start recipe IDs, readiness, logPath и artifact paths.
   Для Netwatch используется существующий IPC/PID health probe; для WinBridge —
   actual worker readiness line после открытия private pipe. Gate дополнительно
   вызывает настоящий IPC запущенного приложения, а не доверяет только тексту.

Профиль раскрывает `${workspaceRoot}` один раз относительно зарегистрированного
корня, даже когда command cwd находится в generation. Каждый проект имеет свои
application/runtime, application/logs и build-cache. Проекты могут запретить direct
commands и использовать только fixed recipes. Раскрытые команды и происхождение
сохраняются до ACK; [контракт profiles](project-profiles.md) описывает limits и
изменение конфигурации. Восстановление формы/конфига не запускает workflow.

## Границы, подтверждаемые вторым потребителем

| Контракт | Владелец | Обязательство вызывающего worker |
| --- | --- | --- |
| `CommandHandler` → `CommandDefinition` | core + runner direct/recipe implementations | Проверить policy, paths и pin, сохранить resolved command/origin до ACK; сравнить frozen command перед launch. |
| `metadata` / `local_metadata` | workspace-sync | Preview отделять от успешного import; exact manifest pin передать verification. |
| `import_with_transfer` / `verified_generation` | workspace-sync | Держать общий project lease, сохранять callbacks/progress, проверять cancel, фиксировать durable outcome. |
| Workflow children / application / log | worker + log-reader | Один Store/queue/process owner; reconcile durable child key перед новой acceptance. |

Callbacks sync передают реальные phases/files/bytes/transfer; ошибка callback
прерывает операцию. Adapter не открывает task journal и не принимает задачи.
Runner не запускает процесс. Один worker сохраняет ACK/journal/leases, владеет
Job Object, читает output и завершает деревья. Closing UI/CLI/WebSocket не отменяет
принятую работу. Unknown external outcome становится indeterminate; нет automatic
replay. Каждая функция вызывается через тот же CLI/MCP/desktop read model.

Эти interfaces статические, внутри одного Cargo workspace и одного выпуска.
DTO/protocolVersion=1 и config schemaVersion=1 остаются прежними. Для следующего
проекта не нужен новый RPC, task service, plugin loader или внешний SDK. Package
versioning понадобится только при появлении независимо поставляемого потребителя;
этому сценарию оно не нужно. Другие ОС/dynamic plugins требуют отдельной задачи.

## Автоматический сценарий

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/<archive>.zip \
  --projects-only
```

Нужен только работающий installed Windows worker; desktop закрыт, окон не появляется.
Driver сначала вызывает подготовку: создаёт два отдельных clones, делает явные
validation commits, сохраняет полные patches/base commits, публикует immutable
packages. `/home/user1/netwatch` только читается. WinBridge validation build identity
указывает base code commit + sourceDirty=true, поскольку adapters — отдельные
тестовые правки; artifact sourceCommit указывает точный validation snapshot commit.
Export manifest с actual hashes входит в этот commit, его bytes тоже проверяются.

Preparation и Debian ready pins находятся в новом
`WinBridge/projects-validation/<run>/preparation`; snapshots — в packages/deliveries.
Команда до submit записывается в `.winbridge/workflow-validation/<run>/request.json`.
Тестовый coordinator собирается из проверенного archive и запускается под новым
instance без controllerUrl. Его два projects находятся внутри отдельного test run
зарегистрированного workspace. Все SQLite/caches/runtime/logs на local Windows disk.

Gate проверяет оба реальных import/build/artifact/start/readiness/log cycle,
fixed recipe origin, dedup до/после completion, собственные IPC/tasks worker
application, nested process tree termination, явный restart её journal с прежним
indeterminate task ID и restart coordinator с прежней успешной startup history.
Рабочий worker никогда не перезапускается. Общие profile-edit/cancel/crash tests
также запускаются; закрытые этапы с native desktop заново не повторяются.

Отчёты — `WinBridge/workflow-validation/<run>/projects-proof.json`, task/output JSON
и logs. `report.ready.json` закрепляет actual bytes/SHA-256 каждого файла. Debian
сверяет их, исходные ready pins, commit coordinator и healthy/idle прежнюю generation
installed worker. Старые reports/snapshots не переписываются и не очищаются.
