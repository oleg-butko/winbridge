# Multi-controller: ревью после установки worker0.2.0

2026-10-27, Debian .204. Base: main591b471 плюс manual package/installation
commits4bc1710/f4c53f5. Ветка `refactor/multi-controller-review`.
Проверены общий runtime, reconnect tickets/permits, per-controller projections,
creator/cancellation journal, readonly CLI/MCP и избыточность новых тестов.
Ошибок, требующих срочно заменять установленный worker, не выявлено.

## Исправления

1. **Native gate зависел от версии установленного EXE.** Cold17→19 в
   `test-multi-controller-native.ps1` и Main25 `test-update-health-actor.ps1`
   брал reference из рабочего `runtime/winbridge-worker.exe`, ожидая SHA0.1.1.
   После установки0.2.0 такой прогон неизбежно падал, в том числе после сборки.
   Теперь оба driver получают явный `--worker17-reference`; Main25 дополнительно
   сохраняет прежний `--worker17-reference-sha256`. Path/hash обязательны вместе
   для profile19, запрещены для historical profile17. Shared Python helper
   отвергает installed path, относительные/UNC/traversal; native проверяет hash
   до Cargo. `InstalledWorker` по-прежнему указывает на **настоящий production EXE**
   для защиты test slot; подмена его старой reference-копией не используется.
2. **В диагностическом harness первый ephemeral port освобождался до выделения
   второго.** OS мог выбрать тот же port для обоих controller, вызывая ложный
   timeout. Теперь обе reservations удерживаются до получения обоих адресов,
   а преждевременное завершение fixture server обнаруживается сразу. Между
   освобождением reservations и production `serve` остаётся обычный bind gap;
   абсолютного резервирования чужого процесса эта схема не обещает.
3. Три readonly маршрута controller используют общий helper получения ready
   sender/Info. Envelope negotiation и ответ остаются на **одной захваченной
   сессии**, типизированные decoders/contract/schema/context проверки отдельные.
   Existing negative matrix расширена другой instance, с проверкой отсутствия
   worker dispatch. Протокол и журнал не менялись.

## Какие тесты удалены и какие сохранены

Удалён `apps/worker/tests/multi_controller_model.rs`:2 теста/243 строки и один
отдельный Rust integration executable. Он соединял вручную созданный `Sessions`
с прямым `Worker::handle_fenced`, не проверяя реальный transport или сохранение
controller origin. Его сценарии теперь проверяются более сильными тестами:

| Удалённая модельная проверка | Оставшееся покрытие |
| --- | --- |
| Concurrent same-key acceptance, очередь одного проекта, один процесс | `two_real_websockets_share_queue_survive_one_disconnect_and_reconnect_without_replay` и Store concurrent duplicate test |
| Другой request ID при том же idempotency key | В реальном two-WebSocket тесте второй caller теперь использует другой request ID |
| Disconnect не отменяет принятую задачу, чужой cancel продвигает очередь | Тот же two-WebSocket тест и roundtrip3/4 controllers |
| Два независимых проекта действительно выполняются одновременно | `both_controllers_run_independent_projects_in_one_worker` |
| Permit освобождается независимо от длительности task, stale completion | Core Sessions/worker RpcGuard тесты; сетевой gate проверяет RPC observation отдельно от процессов |

Сохранены быстрые unit/property-style проверки DTO, topology1–4, global/per-peer
limits, stale tickets/projections и metadata transaction rollback. Они проверяют
иные границы, чем end-to-end. Сохранены old controller0.1.0 и journal17/18→19
migration/refusal: исторический формат остаётся входом совместимости.
Linux TCP/CLI/MCP, browser UI и native Windows process/Job/migration не заменяют
друг друга. Завершённые Windows gates при ревью не переигрываются.

## Ускорение без сокращения ожиданий production

В worker unit test две независимые проверки стадий handshake ждали5s последовательно.
Теперь они запускаются `tokio::join!` с отдельными worker/state/socket и теми же
настоящими5s deadlines. Измерение тела теста **10.05s→5.07s**; build отдельно
**49.70s/45.65s** для этих двух команд. Это один локальный замер, не оценка всего suite.

`npm run test:multi-controller-ui` выполняет один полный browser run с
connections+attribution+cancellation. Не запускать дополнительно все три
`test:controller-connections`/`test:operation-attribution`/`test:operation-cancellation`
для обычного общего gate: cancellation mode уже включает общий connections/creator
сценарий. Отдельные команды сохранены для диагностики. Combined Chromium/Linux IPC
прошёл с реальной отменой отдельного owned `/bin/sh` process и без Windows.

Ещё один источник пересборок — `crates/protocol/build.rs`: он наблюдает все Git
inputs и при каждом запуске выдаёт новый BUILD_ID. Правка README/скрипта может
перекомпилировать protocol и все зависимые crates. Это не следствие числа тестов.
Менять его в этом patch не стали: dirty/source/manifest identity участвует в
проверках совместимости и updater. Практически следует сначала завершать правки,
затем выполнять пакет проверок, переиспользовать уже собранные test executables
для execution-only измерений. Отдельный будущий эксперимент должен измерить
invalidation и сохранить обнаружение untracked/hidden-index/manifest edits.

## Следующий native запуск

Не нужен для использования уже установленного worker; при следующем новом source
gate оба runner должны использовать **сохранённый** worker17 artifact:

```text
--worker17-reference K:\VMs\shared\WinBridge\releases\20261026-182213-6cd81f3e\winbridge-worker.exe
```

Для Main25 profile `worker19-updater20` также:

```text
--worker17-reference-sha256 536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712
```

Изменённый PS пока не выполнялся на Windows. Python preflight/report verifier
проверки не являются native execution evidence. Использовать новый pinned source,
включающий новый parameter, controller из того же source, новый config/state/report
и прежний согласованный test slot. Installed worker, оба production controller,
firewall и журналы ревью не меняет; updater plan остаётся отдельным.

## Завершённые проверки

- Controller lib17 tests и real CLI/MCP diagnostics1 test passed, тела3.34s/0.72s.
- Worker lib:112 passed/0 failed/3 прежних ignored, тело34.85s. Запускался напрямую
  ранее собранный test executable `winbridge_worker-d6965d2086a7a52c`: worker Rust
  inputs после его компиляции не менялись; последующие изменения — только Python/
  PS/docs/controller tests. Это execution-only проверка, не новая clean release build.
- Worker readonly diagnostics integration:2 passed/0 failed, тело0.08s.
  Итого **132 Rust passed**, три требующих внешних fixtures ignored сохранены.
- Python:6 reference/preflight +9 Main25 proof +4 retained migration proof tests.
  Изменённые Python modules прошли compile/help; old reference EXE в shared заново
  прочитан, его SHA совпал с released536d8637… . Рабочий Windows EXE не использовался.
- Один combined browser/real Linux worker IPC сценарий connections+creator+cancel
  прошёл с sandboxed `/usr/bin/chromium`.
- Clippy controller/worker all-targets Linux и MSVC (`-D warnings`) прошёл.
  fmt, diff и scaffold310 прошли. Полный workspace заново не запускался:
  изменённые подсистемы проверены отдельно; предыдущие895 не выдаются за новый run.
- До build Debian26,853,490,688/shared Windows volume23,083,208,704 bytes;
  после26,055,483,392/23,081,820,160. Очистка не потребовалась, Windows builds не было.
- Заключительное readonly connections наблюдение через **установленный** retained
  controller: debian-203 и debian-204 connected/error=null, worker generation
  946f1128 прежний. Новый refactor controller не устанавливался.
