# Compiler actor и отмена до установки — U1, 2026-10-18

Это внутренний trusted API; публичные CLI/MCP update и установка runtime пока
отключены. A4 остаётся на паузе. Рабочий установленный worker не обновляется
этими проверками.

## Управляемая сборка

Updater config schema 3 добавляет локальную build policy: абсолютный путь к
PowerShell 7 и timeout до часа. Schema 1/2 продолжают поддерживаться без этой
возможности. Агент не выбирает executable, script или произвольные аргументы.
Supervisor проверяет frozen config и свой pinned image; script
`src/scripts/build-windows-update.ps1` обязан входить в verified manifest.
Сборка выполняется в прежнем `src`, результат — в
`runtime/update/<updateId>/candidate`. Caches остаются в локальном `src`.

Internal supervisor contract v3 принимает `BuildCandidate`: compiler Intent и
одноразовое разрешение сохраняются **до ACK и dispatch**. Blocking driver владеет
тем же journal/epoch; mailbox продолжает отдавать согласованные наблюдения.
Повтор запроса возвращает прежнюю попытку. Worker lease остаётся SourceHeld;
успех процесса не означает проверку candidate или разрешение на замену exe.

## Отмена

Internal contract v4 добавляет `AbortBeforeCutover` и чтение новых состояний.
В v2 не отправляются неизвестные ему `aborting`/`aborted` состояния. Отмена
доступна только когда source не находится в Applying/Blocked и compiler
отсутствует либо имеет Completed + подтверждённый cleanup. Активная/неизвестная
попытка не получает автоматический restart или освобождение worker.

1. Транзакция сохраняет parent Aborting и Abort Intent, привязывая owner/epoch,
   идентичность runtime root и hashes source/build receipts. Только свежая
   admission выдаёт неповторяемое разрешение на дальнейшие действия.
2. Driver заново проверяет source root/manifest, отсутствие compiler root и
   Windows Job children, bytes/SHA сохранённых stdout/stderr.
3. Verified receipt сохраняет точную worker lease и Release command. Worker
   принимает Release из Aborting только при совпадении этой durable квитанции;
   после проверки файлов повторно сверяет authority/proof перед своим commit.
4. После подтверждения Released одна updater-транзакция сохраняет Abort
   Completed, parent FailedBeforeCutover и supervisor Stopped. Prepared source,
   к которому file effects ещё не применялись, становится Aborted; Verified
   source сохраняется. Runtime exe не меняются.

Lost Release ACK проверяется одним readonly Observe; команда не отправляется
заново автоматически. Ошибка проверки/неизвестный результат → Blocked и
ManualIntervention, без снятия барьера и автоматического продолжения.
История proof неизменяема; повтор Abort не выдаёт новое разрешение.

Updater journal schema 5 необходима из-за новых состояний и abort receipts.
Миграция 1–4 выполняется под owner lock с consistent SQLite backup; увеличивает
fencing epoch и не перепривязывает старые source/compiler/supervisor grants.
Предыдущий updater обязан отказаться от schema 5 без записи.

## Проверка и оставшаяся работа

Проверяются реальные child supervisor/worker IPC, source apply, параллельное
чтение статуса, одиночный compiler при дубле, запрет raw Release и отмены busy
compiler, сохранение блокировки при local edit. Отдельный explicit PowerShell
fixture выполняется на Debian/Windows; он не собирает и не устанавливает рабочий
runtime. Общий Windows gate дополнительно собирает четыре настоящих candidate
exe и проверяет старые версии на отказ от новых schemas.

Live compiler output для UI, независимая проверка полного набора artifacts,
точная остановка worker/desktop, file replacement, verification boot, уведомления
и public capability остаются следующими частями U1–U4.

Local gate: 453 workspace tests и отдельный real PowerShell actor fixture passed; Linux/Windows Clippy all-targets, fmt, contracts, typecheck/build:ui/scaffold и 8 export tests passed. Native rollup ожидается.
