# Windows проверки первого сценария этапа 5

Автоматический прогон выполнен 2026-10-07 через установленный worker `523fc5b`.
Он не заменял рабочие exe/config/state. Новый source `6b855f6` проверялся в отдельном
local source/cache/temp через `test-windows-workflow.ps1 -AttentionOnly`.

## Проверенная версия и отчёт

- Source commit: `6b855f6c62afd2e351093261f3d4ce9a8c060964`, dirty=false, 171 files.
- ZIP: `WinBridge/sources/winbridge-20261007T084040Z-6b855f6-74ac59f6.zip`.
- ZIP SHA-256: `27e7510dbba8247d076fdc5bab4b2c7eda2eb3f7e3ecb197eeddcd8265bd800b`.
- Task: `719111f4-cdfd-4ac4-8926-f6bc548eeed2`, succeeded, exit 0.
- Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261007-044046-eebbd202`.
- Debian report: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261007-044046-eebbd202`.
- report.json SHA-256: `006784bf39af25640bed803eee08d27668ee2855427e0d6df758605e7c000f3f`.
- Platform windows, passed=true; все report.ready.json checksums сверены после чтения из HGFS.

## Семь native tests

1. Настоящий fixture failure создаёт одно error attention; success/cancel тихие.
2. Cursor pages по 50, pending count, unknown IDs, UTF-8/message bounds и validation.
3. Crash recovery добавляет warning, задача indeterminate, replay отсутствует.
4. Лимит 1000 pending explicit requests: overflow отказ, original retry работает,
   automatic task outcome сохраняется, acknowledgement освобождает место.
5. Injected attention write failure откатывает task finish; успешный retry атомарен
   и не дублирует attention. Ошибка здесь была специально предусмотренным тестом.
6. Потеря ACK/dedup, payload conflict и acknowledgement сохраняются после restart.
7. Existing v3 journal получает consistent SQLite backup, old task сохраняется;
   повторное открытие v4 не создаёт лишний migration backup.

Cargo test выполнялся с bundled SQLite, locked/offline; семь passed, ноль failed,
runtime tests заняли 4.88 s после 34.71 s compilation.

## Остальные доказательства и границы

71 Rust workspace test, Clippy/fmt, Windows all-targets compile, TypeScript/contracts,
UI build и standalone Linux desktop check прошли. Browser integration с настоящим
Unix worker/WebSocket controller проверил attention links, потерю acceptance и
acknowledgement responses, original retry, desktop/worker restart, decoded PowerShell
и отдельный raw argv. Сам PowerShell в Debian подтвердил Unicode для используемых
EncodedCommand aliases. После Windows gate UI decoder дополнительно согласован с
.NET Base64 padding/trailing-bit поведением; Rust native-tested code не изменён.

Это проверка первого сценария, весь этап 5 не закрыт. Native tray/popup/sound/settings,
workflow/app attention, history/export и MCP ещё запланированы. Новые ручные Windows
UI наблюдения не назначались и не записываются как пройденные.

После прогона основной worker generation `bff7f8da-af71-432b-83af-739a020d9e37`
по-прежнему connected, исходная controller session
`7fa9c4d3-eae9-42d1-8d83-966e2d79bd13` сохранена. Установленный desktop получит
читаемый EncodedCommand при следующем общем обновлении из новых исходников.
