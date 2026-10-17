# U1: применение исходников с сохранением прежних файлов

2026-10-17. Следующий пакет после [worker coordination](worker-coordination.md).
Библиотечный driver реализован; native Windows gate пока ожидается. Supervisor,
локальный update IPC, shutdown, установка binaries и bootstrap ещё не готовы.
Рабочий runtime не обновляется. Архитектурный переход A4 остаётся отложенным.

## Порядок действий

`apps/update/src/source.rs` получает текущего владельца updater journal и свежую
проверенную информацию worker через `SourceControl`. Это trusted interface:
сохранённый JSON/отчёт не даёт права менять исходники. Сетевой capability и
source-write CLI отсутствуют. Будущий IPC adapter должен проверять peer и deadline.

1. Под SourceHeld проверить installation scope, worker build/config, generation,
   incarnation, updater owner/epoch/revision, baseline и входящий ZIP/manifest/commit.
   Source/runtime не пересекаются; transaction находится на том же локальном томе.
2. В `runtime/update/<id>/source-transaction` создать новые каталоги `preimages`,
   `prepared`, `held`. Скопировать и проверить только изменяемые manifest-owned
   файлы. `preimages` хранит ожидаемую прежнюю версию, `prepared` — новую.
   Конфигурация, `.env`, `.git`, output, journal и build caches не копируются.
3. Сохранить immutable plan/hash и directory identities. Prepared означает
   готовность staging; исходники пока не менялись.
4. В одной SQLite transaction перевести parent update в SourceApplying, source
   transaction — в Applying. Worker SourceHeld/startup guard остаётся активным.
5. Для каждого файла записать durable intent до файлового эффекта. Проверить
   актуальность lease/owner/config/путей. Прежний файл переместить в `held` без
   перезаписи существующего назначения, проверить hash. Новую версию переместить
   из `prepared` в исходники также без перезаписи, проверить hash. Только затем
   сохранить immutable receipt и проверить её readback перед ACK.
6. Manifest заменить последним, после проверки всех target files и состава дерева.
   Повторить полную проверку исходников. В одной DB transaction подтвердить Verified
   и parent Building. Это **не подтверждение сборки или установки**. SourceHeld
   автоматически не снимается: проверенная сборка/handoff ещё впереди.

Add/Replace/Remove работают по одному плану; Keep не записывается. Создание
недостающих родительских каталогов покрывается intent соответствующего файла;
пустые каталоги после сбоя автоматически не удаляются. Число файлов/размеры/объём
декодируемого плана ограничены. Manifest hashes связаны с принятым запросом.

## Гонки, сбои и восстановление

Локальная правка до preparation/apply блокирует выполнение. Если она произошла
между проверкой и перемещением, фактические bytes сохраняются в `held`, ожидаемая
версия — отдельно в `preimages`; несовпадение останавливает операцию. Если после
перемещения пользователь создал файл назначения, он сохраняется: no-clobber move
отклоняется. Это не sandbox от враждебного процесса того же пользователя; границы
scope/link/junction/hardlink и проверки непосредственно перед эффектами обязательны.

При известной ошибке до первой записи parent становится FailedBeforeCutover;
после начала SourceApplying — ManualIntervention. Source transaction становится
Blocked. Если запись ошибки не подтверждена, результат явно сообщает об этом:
Applying и worker barrier нельзя считать снятыми. При process crash остаются
Applying, intents, receipts, исходные файлы/staging и worker startup guard.
Новый owner/epoch не получает права повторить старую файловую операцию.

**Автоматических rollback и replay нет.** Повтор завершённого вызова возвращает
исторический Verified без эффектов. Prepared/Applying/Blocked — разные состояния:
Applying/Blocked повторно не исполняются. Даже если все target bytes уже совпали,
отсутствующая квитанция не превращается в успех. Исправление/снятие guard требует
отдельного reconciliation; этот пакет даёт диагностику и материалы восстановления.

`journal source-status <id> [offset] [limit]` читает согласованную страницу plan,
состояния Planned/Intent/Applied, общий applied count и phase. `source-inspect`
дополнительно сравнивает source/preimage/held с pins, проверяет baseline/target и
показывает точные пути сохранённых файлов. Оба readonly, без owner acquisition,
запуска worker, создания БД, исправления файлов или вывода содержимого конфигурации.
Page: 1–100 файлов. Изменение root identity блокирует filesystem inspection;
journal status остаётся доступным для чтения сохранённых путей и истории.

## Физические гарантии и журнал

Windows использует MoveFileExW с WRITE_THROUGH, без REPLACE_EXISTING/COPY_ALLOWED;
Linux — renameat2(RENAME_NOREPLACE) с fsync родителей. Unsupported OS/filesystem
отклоняется без fallback на overwrite/copy-delete. Bytes staging/preimages проходят
sync и hash readback. [Windows API](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw),
[renameat2](https://man7.org/linux/man-pages/man2/rename.2.html),
[fsync](https://man7.org/linux/man-pages/man2/fsync.2.html).

Нет общей атомарной транзакции SQLite+filesystem или гарантии атомарного переключения
целого source tree при power loss. Поэтому сохраняются intents и startup barrier;
перед сборкой/передачей владения нужна полная повторная проверка. Process-crash tests
не являются доказательством всех вариантов сбоя питания/контроллера диска.

Updater schema **2**, application ID WBUP: source_transactions, source_plans,
source_file_intents, source_file_receipts. Plans/intents/receipts immutable;
phase/revision и parent FSM обновляются атомарно, ignored writes не дают ACK.
Schema 1 читается без миграции. Миграция требует свободный owner lock, создаёт
согласованный `VACUUM INTO updates-v1-<timestamp>-<uuid>.sqlite`, проверяет schema,
installation metadata и quick_check, синхронизирует backup, затем атомарно добавляет
таблицы/schema и меняет epoch/owner. Ошибка откатывает schema/epoch; backup сохраняется.
Прежний updater должен отказать на schema 2 до записи. Worker schema остаётся 15;
его журнал updater не открывает. Source DTO не меняют legacy RPC schema.

## Повторное ревью

Перед native gate проверены границы source effects, raw FSM bypass, atomic parent/
source phase changes и отрицательные ACK. Уточнения реализации: накопительный
лимит декодирования плана до выделения больших buffers; уникальные paths и manifest
строго последним с hashes принятого запроса; affected-row/readback для каждого intent,
receipt и финального CAS; owner lock и verified SQLite backup до migration. Ошибка
доставки Prepared останавливает operation до source writes; ошибка доставки уже
Verified не отменяет сохранённый результат. Не заявляются automatic rollback,
защита от враждебного same-user процесса или полная power-loss атомарность.

405 workspace tests passed; 11 source tests повторены после style fixes. Clippy,
Windows workspace/tests cross-check, TS contracts/typecheck/build:ui/scaffold и
Python/PowerShell syntax прошли. Это не заменяет ожидаемый native Windows gate.

## Проверки и дальнейшая работа

Tests используют отдельные реальные Worker instances и собственные ZIP/config/DB.
Проверяются success/manifest-last, сохранение local/generated данных, local edits,
staging/config tamper, replacement root, stale owner/lease, ignored intent/receipt/
final CAS, ошибка progress и шесть process-crash boundaries. Reopen не повторяет
эффекты, worker startup guard сохраняется. Migration проверяет lock, backup/history,
epoch и rollback; native gate запускает прежний schema-1 updater на новой тестовой БД.

Windows gate использует установленный worker только как транспорт разрешённой
тестовой задачи. Все новые source/runtime/state находятся в isolated validation
root; desktop не запускается. Candidate build/staging четырёх exe проверяется
отдельно, без установки. Compiler caches переиспользуются под lease; history,
reports и Windows validation directories сохраняются.

Дальше: самостоятельный infrastructure supervisor вне managed Job Objects,
аутентифицированный bounded local IPC, graceful shutdown с exact-process identity,
затем isolated replacement/startup/reconnect gates. Только после U1–U3 готовится
первоначальная установка capability. Source transaction сама по себе этого не даёт.
