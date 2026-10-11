# Изолированный цикл Netwatch

Для проверки этапа 4 используется настоящий Netwatch agent 0.3.0 из отдельного
Git clone. Исходный `/home/user1/netwatch` и установленный рабочий Netwatch не меняются.
Пользователь согласовал дополнительный validation commit: он добавляет аргументы
`--instance-id`, `--runtime-directory`, `--log-directory` и прозрачный build script.
Snapshot содержит именно этот commit; исходный commit и diff сохраняются в отчёте
подготовки. Это тестовый вариант Netwatch, а не выпуск основного приложения.

Настройки тестового экземпляра: `role: desktop`, `connectionMode: direct-ip`, пустые
`peers`. Он не слушает TCP и не подключается к внешним узлам. Config, лог, build cache
и IPC находятся в отдельной области зарегистрированного проекта. В Windows pipe
имеет имя `netwatch-agent-<SID>-winbridge-<run>`; в Debian используется сокет в
отдельной runtime directory. Production pipe не используется даже для probe.

## Подготовка агентом

```bash
python3 scripts/prepare-netwatch.py \
  --controller-config .winbridge/controller-snapshot.local.json
```

Сценарий читает registry Windows worker, создаёт clone и дополнительный commit,
публикует verified пакет в shared и сохраняет `workflow.request.json`, `prepared.json`
и `isolation.patch` в `.winbridge/netwatch-validation/<run>`. Он не запускает команду
на Windows. По умолчанию использует уже зарегистрированный `snapshot-fixture`;
`--project` выбирает другой зарегистрированный проект. Netwatch отделён от рабочего
приложения независимо от названия WinBridge project.

После обновления worker профиль можно передать командой controller `workflow`
или скопировать параметры в «Цикл разработки», выбрать соответствующий проект и
сохранить именованный профиль. Новый запуск требует свежей подготовки: runtime
directory одного прогона повторно не перезаписывается. Повтор прежнего idempotency key
только читает прежнюю цепочку.

Проверка готовности — JSON-line `health` через локальный IPC. Worker сверяет PID
владельца канала средствами ОС и PID/service/status в ответе. Подключение ограничено
500 ms на попытку и 4096 bytes на ответ; общий readiness deadline задаёт профиль.
Другой процесс, старый лог и наличие файла executable не подтверждают готовность.
Результат IPC, фактические build/start commands, output, artifact hash и лог приложения
видны в тех же workflow/task/application панелях.

## Автоматический Windows прогон

Для regression используется дополнительный test worker без WebSocket внутри
локального каталога текущего зарегистрированного проекта. Это позволяет проверить
новый код через работающий fixed worker без замены его exe и новых firewall правил.
Агент готовит профиль с `--workspace-root` для этой отдельной тестовой регистрации,
затем запускает:

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/<archive>.zip \
  --netwatch-profile .winbridge/netwatch-validation/<run>/workflow.request.json
```

`test-windows-workflow.ps1` сначала проверяет manifest исходников и запускает Rust
regression, затем `test-netwatch-native.ps1` выполняет import/build/artifact/start/IPC/log,
проверяет dedup, явно останавливает приложение и сверяет сохранённую историю startup.
Отчёты и реальные output находятся в `WinBridge/workflow-validation/<run>` в shared;
`report.ready.json` фиксирует SHA-256 каждого файла. Старые прогоны сохраняются.

Проверки выполняются offline и требуют уже установленных Cargo/MSVC/PowerShell/Git
и локальных зависимостей Netwatch. Подробные ручные Windows UI checks пользователь
отложил. При неожиданной ошибке подключённого worker агент останавливает работу и
запрашивает обновление, сохраняя task/request/report для разбора.
