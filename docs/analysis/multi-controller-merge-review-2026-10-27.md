# Ревью multi-controller перед слиянием

2026-10-27, Debian .203. Проверен диапазон `ba9557b..9877b9e`
из `origin/feat/multi-controller`; review branch основана на9877b9e.
Существенных дефектов общего runtime/очереди/журнала при ревью не выявлено.
Ветка слита в main через отдельный merge commit; worker0.2.0 ещё не объявляется установленным/выпущенным.

Проверены topology validation и stale connection tickets, общий/per-controller
RPC budget, удержание permit до завершения blocking handler после timeout,
независимые reconnect и readiness projections. Один Worker/Store/scheduler
сохранён; disconnect не отменяет принятую задачу. Local Info показывает primary,
сетевой Info — связь своего caller. Первая durable attribution/cancellation
записывается в одной транзакции с эффектом, повтор ключа сохраняет прежнего автора.
Workflow передаёт источник своим children. Контракты CLI/MCP/desktop negotiated,
неизвестные schema/context не превращаются в legacy mutation.

Миграции17/18→19 сохраняют старую историю с unknown creator и consistent backup;
они не разрешаются поверх active updater. Main25 profiles17/20 и19/20 явно
проверяют schemas обоих source archives. Изменение backup validation использует
общий FixtureLayout и exact external runtime, без реконструкции actor custody.

## Исправления ревью

- Native coordinators `run-update-service-main-native.py` и
  `validate-multi-controller-native.py` теперь flush/fsync create-only metadata
  перед отправкой запроса. Smoke-проверка подтвердила отказ повторного создания
  и сохранность первых bytes. Это не обещание exactly-once внешнего эффекта:
  unknown result по-прежнему требует сверки исходного key/context.
- Статический UI harness больше не наблюдает filesystem (`watch:null`).
  Исходный обход Rust/desktop target caches исчерпывал inotify и замедлял тест.
  Состояние harness меняется через IPC; live reload ему не нужен. Настройки ОС
  не менялись, caches для обхода ошибки не удалялись.

## Независимые проверки

`cargo test --workspace --features winbridge-worker/update-validation --locked`:
**895 passed,0 failed,17 ignored**,122 test groups. Linux и Windows MSVC workspace
all-target/update-validation Clippy `-D warnings` прошли. Generated contracts,
fmt, TypeScript, UI build, scaffold и Python Main25 verifier7 tests прошли.

Три Chromium UI сценария connections/attribution/cancellation с настоящим Linux
worker прошли, включая stale generation, потерю IPC, old-worker negotiation и
реальную отмену owned process. Sandbox включён. Playwright download был недоступен
через локальный proxy; использован существующий `/usr/bin/chromium` через
предусмотренный `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH`. Финальные логи без ENOSPC.
Это не Windows native UI gate.

Сохранённый native Windows run **20261027-183307-dda61a12** перепроверен без
повторения команд: **4 controller,5 задач,16 negative audits,15 ready files twice**.
Producer48c834c, archive SHA
`d11deff1be30d9b376c64417a0b700498eb85f24d154c7533edcad0b143f0bdd`,
manifest SHA `310e6f7fef74b6f8e56b693c9cec028acf68afb7679c9176d349a6c15bcac3dd`,
report SHA `e5d431ac54619aaec367ac9862f53e065dab6dc5a32d86e0ecd73a9657966d1e`.
Отдельный cold17→19 proof также проверен. Production Rust/UI code между этим
producer и9877b9e не менялся; исправление TaskSummary verifier в9877b9e корректно
сопоставляет frozen summary с полями full TaskRecord и отдельно проверяет exit0.

Evidence `.winbridge/review-multi-controller/`: workspace, Clippy, UI final logs,
native-network-audit.json, source/report pins. Debian перед build22GB, после~14GB;
очистка Windows/runtime/config/journal не выполнялась. Installed0.1.1 уже вернулся
к controller.203: generation9b924962, incarnation5a8eb173, connected/active0,
maintenance/recovery errors=null. Production controller5103 оставлен работать.

## После слияния

Продолжить полный Main25 на source worker19/updater20. Использовать постоянный
`K:\_proj-26\winbridge\runtime-test-204\winbridge-worker.exe`, отдельный controller
на.203:1144 и новые state/instance/report. Подтвердить actual firewall connection;
разрешение другого IP/порта не выводится из старого gate. Рабочий0.1.1 остаётся
launcher, desktop не нужен. Старый runtime-test/conservation план.203 не менять;
новый путь уже выделен и проверен пользователем/агентом.204.

Multi-controller native proof не заменяет Main25, actual retained updater19→20,
controller/lifetime/commit/release и U2–U4. Public install остаётся выключенным.
