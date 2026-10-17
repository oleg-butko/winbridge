# Общие ресурсы нескольких проектов — A4.3

Кандидат проверен 2026-10-17: [80 native tests и desktop compile](../plans/architecture-transition/windows-resource-scheduling-gate-2026-10-17.md).
Actual installed контракт ожидает обновления worker/desktop. [План и ревью](../plans/architecture-transition/resource-sets.md). Рабочие настройки не меняются.

Лимит работ ограничивает общую нагрузку. Наборы ресурсов дополнительно защищают
объявленные конфликты между проектами: общий cache, binary, IPC, port, interactive
slot. Запуск получает **весь набор сразу** либо ждёт, не удерживая его часть.
Встроенные project leases остаются обязательными.

После готового update можно явно задать root config, например:

```json
"resourceScheduling": {
  "resources": [
    {"id": "windows-builds", "kind": "build", "capacity": 2},
    {"id": "common-cache", "kind": "cache", "capacity": 1},
    {"id": "netwatch-binary", "kind": "binary", "capacity": 1},
    {"id": "netwatch-ipc", "kind": "ipc", "capacity": 1}
  ],
  "projects": [
    {
      "projectId": "netwatch",
      "task": [{"resourceId": "windows-builds", "units": 1}],
      "build": [{"resourceId": "common-cache", "units": 1}],
      "start": [
        {"resourceId": "netwatch-binary", "units": 1},
        {"resourceId": "netwatch-ipc", "units": 1}
      ]
    }
  ]
}
```

Это фрагмент; project ID должен быть зарегистрирован. IDs обозначают реальные
общие объекты выбранных команд. WinBridge не угадывает побочные эффекты argv и не
резервирует OS port/IPC вместо запускаемой программы. Kind: build/cache/binary/
ipc/port/interactive. Build capacity 1..64, остальные exclusive (1). До 64 resources,
32 bindings и 16 requirements на полный набор. Units не превышает capacity.

`task` применяется к обычным задачам и добавляется к workflow build/start.
`import` — самостоятельный набор импорта. `build`/`start` добавляют требования
соответствующих workflow steps. Не повторяйте resource ID в общем и дополнительном
наборе. Для нескольких проектов добавьте bindings с общим resource ID там, где
действительно требуется взаимное исключение. Отсутствие policy сохраняет legacy mode.

Требования сохраняются до ACK; workflow children наследуют их из принятого parent.
Редактирование config не заменяет требования старой задачи. Новая capacity
применяется при следующем claim; lowered capacity не отменяет текущих владельцев.
Если нужный ресурс удалён из policy без активного owner, принятые задачи ждут
исправления настройки. Отказ/ожидание не создаёт новый процесс.

Для первого включения policy завершите/отмените legacy очередь и подтвердите
освобождение старых физических owners: их требования нельзя достоверно восстановить
из произвольной команды. Worker явно отклоняет небезопасное включение. Отключать,
удалять или менять kind удерживаемого ресурса до cleanup также нельзя. Настройки
применяются после ручного restart; уже принятый payload не перепривязывается.

Build/cache принадлежат исполняемому шагу; после durable application Ready они
освобождаются. Binary/IPC/port/interactive остаются у application process до cleanup.
Direct task/import без application handoff держит весь набор до конца. Terminal
status без physical proof оставляет uncertainty и блокирует повторную выдачу.
TTL или закрытие desktop ресурс не освобождают.

Desktop показывает capacities, used/uncertain units, owners и их operation/lease IDs.
Причина ожидания записывается в общий журнал операции. Offline/inspection срез
помечен явно; unknown metadata не выдаётся за действующую policy. MCP Info передаёт
тот же readonly `contractCatalog.resourceScheduling` с provider/context/generation.
Сокращённый inventory помечается, счётчики показанных ресурсов остаются полными.

Один project/workspace сохраняет прежнюю консервативную workflow reservation.
Этот пакет даёт shared resources между независимыми projects, не обещает несколько
произвольных workflow applications в одном неразделённом каталоге.

Schema 14 добавляет frozen requests/claims/loaded policy и делает consistent backup
schema 13. Source history, keys и прежние RPC 1/2/operation-page v1 сохраняются.
