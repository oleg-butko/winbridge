# Следующий этап: исходный updater становится постоянным keeper

**Текущий этап,2026-10-30/.204:** source worker19/updater24 реализует выбор
Keeper и consuming retirement исходного координатора. На Windows новый путь ещё
не выполнялся; готовность сквозного обновления не заявлена. Main30/19/23 отдельно
успешен (run20261030-093727-eb1f2df9); сведения ниже о ранних прогонах исторические.

### Реализованный переход

После уже Confirmed finalization исходный guard получает свежий normal Active
capture. Не-Serde permit допускает одну транзакцию: immutable keeper selection и
supervisor Active→Keeper, active-slot освобождён; root/lease/finalization не меняются.
Схема24 добавляет только keeper_selections; actual23 copy migration имеет отдельный
профиль Keeper23To24 и обязательный native gate, ещё не выполненный.

Известный COMMIT даёт opaque KeeperSelectionCommit. Он не означает закрытого writer
или освобождённой блокировки. Исходный actor передаёт тот же journal, тот же Job,
readers и обе authority Arc в CoordinatorRetirement. Проверяются physical identity,
original known selection, отсутствие лишних владельцев. Эти две Arc — внутренние
ссылки writer/control, не ограничение числа Debian-агентов. При close error остаются
тот же journal, authority и Job; RetryWriterClose повторяет только закрытие известного
writer. Неизвестный SQL outcome удерживает ресурсы без replay/adoption.

Только успешный consuming close позволяет удалить обе authority Arc и перейти в
ActivationKeeperCustody. Он содержит ProcessCustody/binding, без SQLite, authority
или config/image verification pins. Отдельный keeper-control v1 принимает Observe,
RetireCoordinator (повтор — только observation) и RetryWriterClose. Request не
содержит path/executable/нового permission; старый SupervisorRequest не расширен.
Endpoint/nonce/actual IPC server PID проверяются, чужие scope/version/корреляция
отклоняются. Keeper ждёт listener.accept().await без SQL/hash polling в idle.

При ревью исправлена гонка SQL commit/driver completion: запись Keeper уже могла
существовать при ещё выполняющемся JoinHandle. Прежний Active-record invariant
уничтожил бы исходный Job. В PreparingKeeper authority-проверка принадлежит исходной
транзакции; completed attempt сразу передаётся в keeper loop. Validation-only hold
после известного COMMIT принудительно открывает этот промежуток в Main31.

### Проверки и продолжение

1043 workspace tests/17 ignored passed;9 focused keeper tests (включая5 SQL и
реальный IPC с9 отказами), Linux/MSVC all-target Clippy, contracts generation,
TypeScript/scaffold/UI build/fmt/diff passed. После усиления native task stage
Windows integration Clippy также passed. Python observers:2 keeper tests/19 negative
mutations и3 migration tests/22 negative mutations; frozen finalization/resident/
22→23 migration regressions passed. Synthetic reader tests не являются native proof.
Перед workspace Debian17,444,675,584/shared15,319,457,792bytes; очистки не было.

Далее выполнить run-update-keeper-migration-native.py (actual Main30 archive23→24,
consistent copy/backup, старый23 init/status обязаны отказать), затем отдельный
run-update-keeper-main-native.py, profile worker19-updater24. Main31 проверяет actual
role/close, получение OS-lock новым owner при живом прежнем Job, повтор Retire без SQL,
Info и одну fenced cmd task/dedup после смены owner epoch,3s CPU sample keeper и
cleanup/cold view. Gates подготовлены, пока не выполнены. Имена/профили Main25–30
не подменяются; Main30 по умолчанию по-прежнему строго19/23.

После Main31: version-neutral bootstrap следующего coordinating actor вне worker
activation Job, одна admission/max2 keeper, known-empty выход старого; затем U2–U4.
Public install off, установленный source48/gen946 не менялся. Рабочий worker оставить,
desktop не нужен. Никаких Windows validation directories не удалено.

2026-10-30, .204/main. План после [Main30/factory](finalization-factory-2026-10-30.md).
Keeper conversion пока **не реализована**. Начат первый необходимый примитив:
`UpdateJournal::close_owned` проверяет исходную authority и при ошибке возвращает
то же соединение и directory. Три real SQLite tests passed: native unfinalized
statement даёт SQLITE_BUSY без потери journal/lock; stale authority отказ не закрывает
connection; live transaction не откатывается молча при закрытии. Положительный retry закрывает SQLite при ещё удерживаемом owner lock,
и только отдельный Drop authority позволяет новому owner получить следующий epoch.
Runtime вызова/передачи в keeper пока нет; schema остаётся23.
Linux/MSVC updater all-target Clippy/update-validation с -D warnings, fmt/diff
passed;3 targeted tests passed. Логи `keeper-*` в
`.winbridge/finalization-main30-review-204/`. Перед сборкой Debian18,553,077,760,
Windows(shared)19,567,517,696 bytes free. На Windows этот новый close primitive
пока не выполнялся; параллельный Main30 использует frozen c19e1ec/8fdcc42 без него.

## Какой ресурс ещё не освобождён

FinalizationGuard уже хранит только ProcessCustody и обычные immutable данные;
verification/config files закрыты. Но coordinating `supervisor::run_inner` удерживает
UpdateJournal в `idle` и ДВА Arc на UpdateAuthority: `control_authority` и
`idle.1`. Освободить один Arc недостаточно. SQL-соединение должно быть закрыто при
ещё удерживаемой authority; затем должны исчезнуть все её Arc и owner file lock.
Root Committed/Active receipt сами этого не доказывают. `supervisor::active` также
продолжает резервировать single_supervisor, поэтому следующее update сейчас blocked.

## Рекомендуемый вертикальный сценарий

1. Отдельный внутренний контракт, current view и typed consuming owner. Переход
   разрешён только оригинальному FinalizationHeld с независимо подтверждённым Active,
   живыми child/Job/keeper и свежим normal-IPC observation. Cold view или saved Confirmed
   не создают владельца, токен retirement или разрешение на новый процесс.
2. Явная SupervisorPhase::Keeper вместо Stopped. Это меняет durable semantics:
   updater schema24, consistent retained23→24 copy/backup gate и отказ old23 reader
   до любых изменений. Main30/23 остаётся frozen, следующий native gate отдельный.
   Исторический Keeper record — факт роли, не доказательство актуальной OS custody.
3. Отдельная transaction под original coordinator/authority связывает exact finalization
   command/receipt, root Committed, original process/Job и следующую supervisor revision.
   Обе границы проверяют fresh capture и actual physical owner. Обычный
   `advance_supervisor` не становится способом обойти этот trusted переход.
4. При known COMMIT закрыть writable journal через consuming fallible close API,
   сохраняющий Connection/директорию при ошибке. После успешного close отпустить оба
   исходных Arc. Перед переходом проверить отсутствие work/runtime reads/pending
   admissions и лишних authority refs; при ошибке сохранить Job и объяснить причину.
5. Передать целый ProcessCustody минимальному keeper loop. Он продолжает drain обоих
   streams и владение Job, не содержит journal/authority/config/file verification pins.
   Его обычный status — readonly OS/process/output data. Конец coordinating actor
   не должен разрушить Job; потеря самого keeper по-прежнему закрывает именно его Job.
6. Native: тот же worker/generation/Store/Job остаётся Active после SQL close/lock
   release; отдельный owner приобретает updater lock без takeover и без убийства
   worker. Реальная task/dedup, config atomic replace и idle CPU проверяются после
   перехода. Полное owned Job cleanup остаётся обязательным для test slot archival.

## Неизвестный исход и ошибки

COMMIT error не превращается в Known permit по чтению сохранённой строки. Исходный
pending owner сохраняет actual custody и журнал/authority; никакой cold adoption,
повторного перехода, release/V8/launch. Для его reconciliation потребуется явно
описанный consuming original-only путь; обычный control loop не должен увидеть
изменившийся durable supervisor record и аварийно уничтожить живой Job.

Close error возвращает то же соединение; authority остаётся удержанной. Лишняя Arc,
потеря child, stale owner, clock rollback или истёкший capture дают Unconfirmed с
реальным ресурсным статусом. Нельзя сообщать «lock released», проверив только row
или strong_count одного локального Arc. Проверка новым владельцем на Windows —
отдельное фактическое доказательство. Отсутствие writable connection и отсутствие
OS Job custody — разные вещи.

## Следующее поколение

После keeper conversion нужен version-neutral bootstrap/delegation protocol:
logical current worker и physical parent keeper различны. Новый keeper создаётся
старым вне worker execution domain; не ослаблять Job limits и не добавлять breakaway.
Одна durable admission, максимум два keeper, old выходит только после known empty
старого Job. В инфраструктурном API уже есть `require_parent_outside`; сам worker,
запущенный в activation Job, не может использовать прежний direct spawn путь.

Minimal bootstrap не должен требовать от старого keeper понимания будущей SQLite
schema. Новая updater authority принадлежит новому coordinating actor, старый keeper
не получает её обратно. Новый публичный install нельзя объявлять готовым до проверки
двух поколений, U2 fault matrix, U3 UI/attention и U4 installed update.

## Диагностический output перед keeper conversion

Исправлен старый дефект Capture: после первых64KiB diagnostics tail переставал
обновляться и мог скрыть последнюю ошибку. Теперь первые64KiB остаются для native
аудита, а отдельные последние4096 bytes дают актуальные хвосты до512 Unicode chars.
Оба reader по-прежнему дочитывают весь поток; размер состояния ограничен. Весь prefix
больше не копируется ради короткого status. Усилен существующий concurrent drain
regression:2MiB+разные Unicode footers, owner move, точные read counts, bounded tails
и сохранённый prefix.3 portable tests и MSVC updater all-target Clippy passed.
Это ещё не keeper runtime; source23 и schema/контракты не изменились. Текущий второй
Main30 использует frozen baf8e3f→e5cdceb, без этого отдельного output fix.
