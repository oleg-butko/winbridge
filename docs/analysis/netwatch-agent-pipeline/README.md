# Netwatch через WinBridge: интерфейс для агента и развитие pipeline

Дата анализа: 2026-10-11. Исследованы WinBridge `b5d39f97045ca383012d1a9a9cf92e50ee1e73af`
и Netwatch `39409e56a26ef438adab3488afea504ef40f1363` из `/home/user1/netwatch`.
Это анализ текущего кода и предложения; описанные расширения не реализованы этим отчётом.

## Главный вывод

**Агент Netwatch уже может автоматически доставить свой commit, собрать Rust agent
и выполнить существующие Windows тесты через MCP, не меняя WinBridge.** Для этого
пользователь один раз регистрирует проект и предоставляет работающую пару
controller/worker. Результаты, вывод и отмена принадлежат Windows worker.

**Для полного цикла новой фичи есть две дополнительные задачи.** Сам Netwatch должен
поддерживать отдельный тестовый instance, включая launcher. В WinBridge полезно
добавить рабочую копию для сборки UI и pipeline с самостоятельными шагами тестов
и проверяемым отчётом. Сейчас эти части можно организовать прозрачным сценарием
в репозитории Netwatch, но WinBridge будет видеть его одной командой.

Существующий Windows gate уже подтвердил доставку, сборку и управляемый запуск
Netwatch. Однако он использует **дополнительный validation commit в отдельном clone**,
не запускает весь набор тестов Netwatch и не проверяет его Tauri launcher.
Перенос такого gate на каждую фичу без учёта этих различий даст ложную уверенность.

## Две стороны задачи

| Сторона | Что делает самостоятельно | Что запрашивает у другой стороны |
| --- | --- | --- |
| Агент в `~/netwatch` | Пишет фичу и её тесты, фиксирует commit, поддерживает сценарии Netwatch, вызывает опубликованные MCP tools, анализирует доказательства. | Регистрацию проекта, недостающую возможность WinBridge с воспроизводимым примером и критериями приёмки. |
| Разработчик в `~/winbridge` | Поддерживает доставку, выполнение, журнал, процессы, совместимость, API результатов и понятный UI. | Контракт тестового instance и предметные проверки от Netwatch; пример реального ограничения pipeline. |

Знание внутреннего SQLite, временных validation workers и файлов `~/winbridge/scripts`
не должно быть обязательным для обычного агента. Он пользуется MCP и сценарием
своего проекта. WinBridge не должен понимать бизнес-правила каждой новой фичи Netwatch.

## Содержание отчёта

- [Сценарий агента Netwatch](agent-workflow.md): настройка, реальные MCP вызовы,
  исходники, безопасные повторы и граница готовых возможностей.
- [Проверки и анализ результатов](testing.md): что тестировать для разных фич,
  как отличать проблему продукта от проблемы среды, какие доказательства сохранять.
- [Архитектура и предложения](architecture.md): устойчивый фундамент,
  ограничения, места расширения и порядок следующих изменений.

## Что проверено при подготовке

Исходники обоих репозиториев были чистыми до анализа. Netwatch не редактировался.
Сборка его тестов использовала отдельный target directory внутри игнорируемого
`.winbridge/research/netwatch-agent-pipeline`; зависимости брались offline.

| Проверка в этой сессии | Результат | Что она подтверждает |
| --- | --- | --- |
| `cargo test --locked --offline -p netwatch-agent` на Debian | 13 passed: 11 unit, IPC integration, transport integration; 0 failed. | Текущий Linux baseline Netwatch, локальные fixtures без публичных VPS. |
| `cargo test --locked --offline -p netwatch-launcher --no-default-features --lib` | 2 passed, 0 failed. | Library supervisor на Debian; native tray/WebView не выполнялись. |
| `npm run typecheck` в Netwatch | Passed. | TypeScript типы; взаимодействие с UI не проверялось. |
| `cargo test -p winbridge-controller --test mcp --locked --offline` | 5 passed, 0 failed. | MCP lifecycle, schemas, ограничения входа, отказ maintenance и ошибки offline dispatch. |
| Настоящие `initialize` / `tools/list` stdio adapter | 33 tools, compact catalog 21 602 bytes. | Каталог текущего adapter; команды на Windows этим чтением не отправлялись. |
| JSON пример submit из отчёта через настоящий MCP parser | Passed; дошёл до ожидаемого offline IPC error в отдельном временном config. | Форма запроса соответствует текущим DTO; Windows execution исключён отсутствием controller socket. |
| `npm run check:scaffold`, `git diff --check` | Passed. | Локальные ссылки документации, примеры проекта и отсутствие whitespace errors. |

При подготовке отчёта **нового Windows прогона не было**. Существующие доказательства:

- [Этап 4](../../done/stage0-7/stage4/windows-workflow-gate-2026-10-06.md):
  41 native тест WinBridge и отдельный Netwatch import/build/artifact/start/IPC/log/stop.
- [Этап 7](../../done/stage0-7/stage7/windows-projects-gate-2026-10-10.md):
  59 native тестов WinBridge, два service cycles, проверенные artifacts и recovery.
- [Изоляция Netwatch](../../validation/netwatch-validation.md): дополнительный
  commit, отдельные pipe/config/logs; основной Netwatch не менялся.

Числа 41 и 59 относятся к тестам WinBridge, а не к полноте проверки Netwatch.
Готовность функций WinBridge и статус завершённых этапов 0–7 этим анализом не меняются.

## Основания в коде

| Наблюдение | Источник |
| --- | --- |
| MCP использует тот же worker service; schemas получаются из Rust DTO. | [mcp.rs](../../../apps/controller/src/mcp.rs), [контракт MCP](../../guides/mcp.md). |
| Workflow имеет обязательные build/start/readiness/log и фиксированные пять шагов. | [WorkflowSubmission](../../../crates/protocol/src/workflow.rs), [worker workflows](../../../apps/worker/src/workflows.rs). |
| Snapshot исключает лишние файлы и проверяется перед выполнением. | `verify_tree` / `verified_generation` в [workspace-sync](../../../modules/workspace-sync/src/lib.rs), [worker execution](../../../apps/worker/src/lib.rs). |
| Рецепты и direct используют общий resolver, не владеющий процессами. | [CommandHandler](../../../crates/core/src/commands.rs), [runner](../../../modules/runner/src/lib.rs). |
| Изоляция нынешнего gate добавляется патчем в clone Netwatch. | [prepare-netwatch.py](../../../scripts/prepare-netwatch.py), [Netwatch gate](../../../scripts/test-netwatch-native.ps1). |
| Artifact catalog фиксирует исторические hashes, но не хранит копии файлов. | [artifacts.rs](../../../apps/worker/src/artifacts.rs). |

Пути Netwatch ниже относятся к отдельному репозиторию на указанном commit;
он не включён в WinBridge и не является его зависимостью:
`package.json`, `scripts/build-portable.mjs`, `agent/src/main.rs`, `agent/src/ipc.rs`,
`agent/tests/ipc.rs`, `agent/tests/transport.rs`, `launcher/src-tauri/src/supervisor.rs`,
`launcher/src-tauri/src/main.rs`, `launcher/src-tauri/tauri.conf.json`.
