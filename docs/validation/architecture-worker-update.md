# Обновление Windows worker для A3.2

Нужно обновить runtime для actual paired Windows проверок. Native candidate gate
подтверждает изолированную версию; installed schema-6 worker ещё использует старые
контракты. Новый worker передаёт catalog и context для запросов contract 2.

1. Остановите старый worker через Ctrl+C в его PowerShell. Desktop должен быть
   закрыт. Во время последней проверки активных задач/приложений не было.
2. Обновите `K:\_proj-26\winbridge\src` содержимым каталога `winbridge` из нового
   source ZIP в `K:\VMs\shared\WinBridge\sources`. Существующие local configs,
   `target` и `.winbridge` сохраняются; их нет в ZIP и удалять их не требуется.
3. В PowerShell:

   ```powershell
   Set-Location K:\_proj-26\winbridge\src
   .\scripts\prepare-windows-snapshot.ps1
   ```

   Не добавляйте `-SkipBuild`: старые exe нужно пересобрать. Сценарий обновляет
   постоянный `K:\_proj-26\winbridge\runtime`, сохраняет настройки/state и запускает
   worker с `winbridge-worker.snapshot.local.json`. Новый firewall path не создаётся.
4. Оставьте worker работать. Запущенный desktop можно закрыть; для автоматического
   paired gate он не нужен. Сообщите, что worker обновлён; версия и соединение
   будут проверены из Debian автоматически.

Обычный schema-6 → 12 upgrade создаёт consistent local backup перед миграцией,
сохраняет историю/keys и не является restore старой БД. Старый worker не открывает
новую schema; не откатывайте exe поверх мигрированного journal. Recovery/archive
нужны для отдельного восстановления backup, не для этого обычного upgrade.
Debian controller обновляется агентом; config, mount, firewall и services не меняются.
