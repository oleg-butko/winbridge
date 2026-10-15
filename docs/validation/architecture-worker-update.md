# Текущее обновление: A3.3 — 2026-10-15

Новый общий журнал требует `winbridge.operation-read-model` v1 в worker и нового
UI в desktop. Установленный `35b716a` исправен; новый контракт в нём отсутствует.
[48 нативных тестов и desktop compile](../plans/architecture-transition/windows-operation-pages-gate-2026-10-15.md)
прошли автоматически. Controller Debian уже обновлён и работает; ручной UI checklist
не нужен. Полный A3 закроется после автоматической проверки установленной пары.

Точный ZIP и SHA-256 сообщает агент; актуальная запись находится в
`K:\VMs\shared\WinBridge\sources\architecture-update.latest.json`.
Берите ZIP из этой записи, а не из исторических разделов ниже.

1. Закройте desktop и остановите worker через Ctrl+C.
2. Обновите `K:\_proj-26\winbridge\src` содержимым каталога `winbridge` из ZIP.
   Существующие local configs, `target`, `.winbridge` и runtime journal сохраняются.
3. В PowerShell выполните без `-SkipBuild`:

   ```powershell
   Set-Location K:\_proj-26\winbridge\src
   .\scripts\prepare-windows-snapshot.ps1
   ```

4. Запущенный desktop можно закрыть; worker оставьте работать и сообщите об обновлении.
   Агент сам проверит actual build/schema, страницы через IPC/WS/CLI/MCP и UI.

Сборка остаётся в `src`, exe в постоянном `K:\_proj-26\winbridge\runtime`:
новый firewall path не создаётся. Journal schema 12 и RPC 1/2 сохраняются,
новая миграция БД не требуется. Пакет содержит проверенный runtime code; позднейшие
изменения — документация. Квитанция новой установленной пары появится после её
реальной проверки, а не заранее из отчёта кандидата.

## История обновлений

# Обновление для A3.2c — 2026-10-14

Новая сводка «Совместимость компонентов» требует поддержки readonly projection 1
в worker/desktop. Установленный `b2aeb9f` исправен, но этого контракта ещё нет.
[41 native tests и desktop compile](../plans/architecture-transition/windows-projection-gate-2026-10-14.md)
прошли; actual new/new presentation проверяется после обновления. Ручной UI checklist
не нужен. Debian controller и consumer evidence обновляет агент.

Точное имя итогового ZIP и SHA-256 сообщает агент; они также сохранены в
`K:\VMs\shared\WinBridge\sources\architecture-update.latest.json`.
Пакет содержит runtime code, прошедший gate; последующие изменения — report/docs
и Debian verifier. Не выбирать старый ZIP из исторических разделов ниже.

1. Закройте desktop, остановите worker через Ctrl+C.
2. Обновите обычный `K:\_proj-26\winbridge\src` содержимым каталога `winbridge`
   из ZIP. Existing configs, `target`, `.winbridge` и runtime journal сохраняются.
3. Выполните без `-SkipBuild`:

   ```powershell
   Set-Location K:\_proj-26\winbridge\src
   .\scripts\prepare-windows-snapshot.ps1
   ```

4. Запущенный desktop можно закрыть; worker оставьте работать и сообщите об обновлении.
   Агент проверит фактический build/metadata и новый route автоматически.

Постоянный runtime `K:\_proj-26\winbridge\runtime`, firewall path, RPC 1/2
и schema 12 сохраняются. Новые окна для автоматической проверки не нужны.
Candidate report не включает новую пару заранее: actual binaries получают новую
квитанцию только после их проверки.

## Предыдущее обновление для A3.2b — 2026-10-14

A3.2a прошёл [39 native tests и desktop check](../plans/architecture-transition/windows-readiness-gate-2026-10-14.md).
Для следующего пакета нужны actual `projectPolicies`/`contractSchemas` от installed
worker; `f128c9a` этих полей ещё не передаёт. Controller обновляет агент.

Проверенный code ZIP:
`K:\VMs\shared\WinBridge\sources\winbridge-20261014T135448Z-b2aeb9f-7bf64364.zip`.
SHA-256 `09f5c70b8e47620133ba85f90b34db808f827450d0fc3f1e90d261c905600d7f`.

Закройте desktop, остановите worker через Ctrl+C, распакуйте в обычный `src`,
затем выполните:

```powershell
Set-Location K:\_proj-26\winbridge\src
.\scripts\prepare-windows-snapshot.ps1
```

Без `-SkipBuild`. Configs и journal сохраняются; schema 12 прежняя.
Worker запускается по прежнему `K:\_proj-26\winbridge\runtime\winbridge-worker.exe`,
правило firewall остаётся прежним. Desktop после подготовки можно закрыть;
worker оставьте запущенным и сообщите об обновлении. Ручной UI checklist не нужен.
Полный A3.2 ещё открыт: после обновления агент продолжит producer/readiness API.

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
