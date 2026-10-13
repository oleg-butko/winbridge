# A2.4.2b.1 — Windows request fence gate

Дата: 2026-10-13. [Контракт](fenced-requests.md). Candidate `88a7fa5`.
Пакет b.1 завершён; archive preparation и cutover не входят в этот gate.

- ZIP: `winbridge-20261013T233001Z-88a7fa5-b312639f.zip`, clean source,
  SHA-256 `532764baa2a9990412feaa947ea1ca4ef3fc0847044e43936465bf5a7690380b`.
- Run: `20261013-193015-71150db1`, shared namespace `WinBridge/workflow-validation`.
- Task `03258208-f61a-4a55-8855-9dc20dd28ead`, PID 3120, succeeded/exit 0.
- Семь groups exit 0: pure FSM, resume guard, fenced protocol, frozen contracts,
  native writers, actual older-worker refusal и native desktop compile.
- **186 Windows tests** passed; desktop check выполнен на настоящей Windows,
  без UI-окон. **9 artifacts** проверены по размеру и SHA-256.
- Новый journal schema 12; установленный schema-6 exe отказал с exit 1,
  test journal остался byte-identical. Рабочая БД не использовалась этим тестом.
- K: до сборки 38,242,594,816 bytes, после 32,443,744,256 bytes (~30.2 GiB).
  Старые validation directories не удалялись.

Проверены stale context/version до effects, all-method refusal, immutable admission,
schema-11 backup/migration, actual CLI/local IPC и один process при повторе.
Unix paired controller/WebSocket/MCP test и browser ACK-loss gate также прошли;
общее число local workspace tests — 271. Windows TCP test peers не создавались.

Installed worker source `b5d39f9`, generation
`02939593-8adf-4878-a346-473092697a4a`, session
`631c2e63-edc8-44f8-8cce-89abec7caf8b` сохранились; connected/idle,
maintenance/recovery errors отсутствуют. Desktop остаётся закрытым, worker работает.
Установленный runtime не обновлялся. Полный A2 остаётся открытым.
