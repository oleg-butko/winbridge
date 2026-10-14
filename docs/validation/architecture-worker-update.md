# Обновление Windows worker для A3.2

Нужно обновить runtime для actual paired Windows проверок. Native candidate gate
подтверждает изолированную версию; installed schema-6 worker ещё использует старые
контракты. Новый worker передаёт catalog и context для запросов contract 2.

Подготовленный пакет:
`K:\VMs\shared\WinBridge\sources\winbridge-20261014T104405Z-f128c9a-5070ec9f.zip`.
SHA-256 `f63b7fcf198b5aae82b87b61bf613f9d1266489596d029f56f519616d0a3ceff`.
Runtime code совпадает с версией, прошедшей 203 native tests. Debian controller
уже обновлён и принимает соединение на `192.168.44.203:1143`.

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
