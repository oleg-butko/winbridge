# Windows workflow/application attention gate — 2026-10-08

Новый сценарий этапа 5 **прошёл автоматически на настоящей Windows** через
установленный worker. Рабочий runtime не заменялся; ручной UI gate не потребовался.

- Clean source: `3c5935135adf116e23014c31b3f6f6cf459538f5`.
- ZIP с built UI: `winbridge-20261008T100112Z-3c59351-8ca60b4a.zip`, SHA-256
  `e5f37399476c7a6d4a0d823aa1335e4a877a51e7dc92e03d24956343d6f86786`.
- Outer task: `7e2cc850-e9ae-4205-a12f-ea27d399efb2`, succeeded, exit 0.
- Shared report: `WinBridge/workflow-validation/20261008-060121-324c2ef1`.
  Windows: `K:\VMs\shared\WinBridge\workflow-validation\20261008-060121-324c2ef1`.
- `report.json` SHA-256:
  `f5512e89434690ec036636e6f7f03226d78eee1b4e3f2dadbdc3c13c10a39988`.
  Все **12** файлов сверены по фактическим bytes/SHA из `report.ready.json`.
- Время: 10:01:26–10:04:48 UTC. Offline worker и native Tauri desktop builds exit 0.

## Native backend

**10 attention + 9 новых workflow tests прошли**, остальные старые workflow tests
не повторялись. Проверены v4→v5 consistent backup, ID/ACK/dedup и sequence high-water;
taskless import failure, один parent event при build/readiness failure, quiet cancel/stop;
app crash, самостоятельный exit 0 и unknown recovery без ещё одного external start.
Исторический startup success сохраняется после падения приложения.

Намеренные SQLite errors подтверждают rollback terminal record/event, отказ выдавать
stale workflow/application и pause admission. Сбой записи readiness блокирует обе
связанные записи. После repair/restart появляется одно корректное событие.

## Настоящий desktop

Изолированный instance `tray-gate-75c368737a384ea986354e2ebf315da9` использовал локальный
пустой delivery root: отсутствующий пакет вызывает реальный import failure до Task ID.
Desktop PID 2660 завершился через actual UI save/quit с **exit 0**.

Baseline workflow `95d5f6fe-4840-4211-9ba1-616798bd0416` и его событие
`bdfb6fd4-b774-41b6-a541-e54b2f71b415` прочитаны UI и явно подтверждены. После скрытия
main window новый workflow `14a83c9b-ce4b-4137-9063-ef7af401b855` создал событие
`5151bf27-4d59-49b3-9cf3-955be1c46b9f`. Настоящий popup не захватил foreground focus;
его кнопка открыла подтверждённую worker страницу второго workflow. Открытие не
подтвердило событие; actual React «Прочитано» отдельно записало ACK. Pending count стал 0.
Сохранены args/PID/timestamps и понятные checkpoints всех шагов.

## Cleanup и границы

После UI quit отдельный тестовый worker оставался доступным; затем harness остановил
только свой worker PID 19196, дождался output drain и сохранил immutable local reports.
Cleanup errors отсутствуют. Рабочие exe/config/state, Netwatch и прежние shared reports
не менялись. Read-only Info после gate: installed source `f7c4878`, generation
`8c5d8c94-697c-4e9f-9cdb-765930ee36f3`, activeTaskCount=0, journalRecoveryRequired=null.

Локально прошли 89 workspace Rust tests, Clippy -D warnings, standalone desktop Clippy,
Windows MSVC all-targets check, contracts/typecheck/UI build, PowerShell parser,
Python syntax и browser integration с настоящим Unix worker/controller. Application
page и target вне recent ten проверены browser integration; Windows UI gate выше
проверяет workflow target. Старый tray/settings gate без новой причины не повторялся.

Пользователю сообщено: тестовые окна закрылись, рабочий desktop можно завершить,
установленный worker оставить работающим. Workflow/application attention завершён;
история/диагностика/MCP ещё предстоят, весь этап 5 не закрыт. Новый код попадёт в
совместное обновление worker/controller/desktop; gate не устанавливает runtime.
