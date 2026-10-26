# UI/UX: пользователь видит и понимает действие

## Принцип

Интерфейс отвечает на вопросы: кто попросил действие, над каким проектом/снимком, что именно запущено, что происходит сейчас, чем закончится или закончилось и какое действие доступно пользователю. Transport ACK и запуск процесса не показываются как завершение задачи.

Tray companion скрывает окно только после сохранения всех форм/настроек и при исправном
tray. Явный quit сохраняет их по тому же правилу. Popup не забирает focus и открывает
конкретную task/workflow/application page; открытие/скрытие не подменяет «Прочитано». Offline/неизвестные
счётчики показываются явно, ошибка cursor/presentation остаётся видимой.
Поведение и defaults описаны в [companion](guides/companion.md).

## Информационная архитектура

| Экран | Содержимое и действия |
| --- | --- |
| Обзор | Связь Debian/Windows, активные задачи, очередь, требующее внимания; без смешивания offline и failed. |
| Проекты | Windows root, source path Debian, shared mapping, recipes и разрешение direct commands; setup preview. |
| Задачи | Поиск/фильтры по project, инициатору, времени, status; последняя свежесть; cancellation и новые retry tasks. |
| Детали задачи | Commit/snapshot, workflow steps, фактическая команда/argv/cwd, output, exit code, artifacts, process/log links. |
| Журнал действий | Кто/что/когда, RPC→task→step correlation, config changes, delivery и attention; экспорт. |
| Настройки | Local address/mapping, behaviour worker на закрытие UI, retention/quotas, звуки/quiet hours. |

## Setup без непонятных выборов

1. Выбрать controller Debian и worker Windows. Показать реальную достижимость и адреса, вместо просьбы понимать TCP термины.
2. Выбрать shared folder mapping. Показать `/mnt/hgfs/shared` и `K:\VMs\shared` как два пути к общим данным и результат согласованного probe.
3. Выбрать отдельный Windows workspace root. Показать пример конечного snapshot path, объём и разделение исходников/caches/runtime данных.
4. Зарегистрировать project: source, root, recipes и direct commands в этом проекте. Это сохранённое разрешение для обычных задач агента, без confirmation каждой команды.
5. Выполнить явно выбранную маленькую fixture проверку: доставка, выполнение, stdout, журнал. Показать каждый результат и следующий шаг.

Ошибки setup конкретны: «Windows не видит manifest в выбранной shared папке», «Этот каталог уже занят другим проектом», «Executable cargo.exe не найден». К ошибке приложить поле/путь и доступное действие.

## Task workbench

Верхняя часть: название, project, snapshot/commit, инициатор, status, время, свежесть данных. Ниже — timeline и вкладки «Команда», «Вывод», «Файлы», «Приложение/лог». Типичный шаг читается одной строкой; подробности доступны без скрытия самой команды.

Команда показывается как executable, отдельный массив аргументов и cwd; recipe name дополняет, а не заменяет фактические параметры. Shell/script виден явно. Sensitive values редактируются в UI/export, без скрытия самого факта передачи параметра. Доступно копирование параметров; формат отображения не используется для обратного исполнения.

Для PowerShell `-EncodedCommand` основной вид показывает декодированный UTF-16LE
сценарий с объяснением кодирования. Точные исходные argv доступны отдельно; нельзя
выдавать их декодированный вид за другую выполненную команду. Ошибка декодирования
видна явно. Это требование пользователя от 2026-10-07, реализация относится к этапу 5.

Форма запоминает executable, исходный текст JSON argv, cwd, timeout и выбранный рецепт отдельно для worker/project/root. После открытия desktop заполняет прежние поля, включая незавершённую правку JSON, и выбирает последний использованный зарегистрированный проект. Видны состояния сохранения и ошибка; закрытие окна ждёт записи последних правок. Восстановление не запускает команду. Файл `winbridge-desktop-state.local.json` хранится рядом с exe; worker config остаётся отдельно.

Output: независимые stdout/stderr, поиск, severity filters, pause autoscroll, новые фрагменты, byte offset и признаки gaps. Task output читается bounded pages с sequence cursor; desktop удерживает последние 65 536 Unicode символов каждого потока, точный loaded byte count и явные gaps. Virtual scroll отрисовывает до 28 строк, включая overscan; фильтр и pause относятся к загруженному хвосту. Очистка worker output сбрасывает также frozen view. Копирование/экспорт показывает, что выбрано: весь сохранённый output или фильтрованная часть. Application logs пока используют прежний bounded tail.

Доставка: capture, pack, shared write, validation/import и ready for build — отдельные шаги. Progress bytes/files точный, если известен размер; иначе indeterminate progress без придуманного процента. Пользователь может увидеть snapshot manifest и какие files будут заменены.

## Состояния и доступные действия

| Ситуация | Сообщение | Действие |
| --- | --- | --- |
| Queued | «Задача в очереди; ожидает workspace» | Отмена до запуска, посмотреть причину ожидания. |
| Running | «Сборка выполняется; последние данные N секунд назад» | Смотреть output, отменить дерево процессов. |
| Controller offline | «Связь потеряна; последнее подтверждение — запуск» | Reconnect/reconcile той же задачи; не обозначать её как failed. |
| Failed | «Сборка завершилась с exit code 101; запуск не выполнялся» | Ошибка/output, исправить и создать новую task. |
| Indeterminate | «Результат не удалось восстановить после падения worker» | Проверить состояние приложения/артефакты; повтор только осознанно. |
| Shared folder unavailable | «Снимок ещё не импортирован: путь недоступен» | Проверить mapping, повторить import; не начинать build. |
| Cancel requested | «Запрошена отмена; ожидается завершение процессов» | Показать реальное окончание, timeout или неполную отмену. |
| Empty state | «Проекты ещё не зарегистрированы» | Прямое действие «Добавить проект», без технических заглушек. |

Last known status + connection freshness — два независимых поля. Уведомление запроса отмены не превращает running в cancelled раньше подтверждения worker. Retry создаёт новую задачу со ссылкой на прежнюю; повтор request ID возвращает прежнюю запись.

Feature-ветка multi-controller: «Соединения с Debian» показывает все настроенные
адреса и primary, который используется локальной readiness сводкой. У каждой связи
свои состояние, последнее подтверждение, session, attempts и последняя ошибка.
Общие/per-controller RPC counters не называются количеством заданий: фоновые Info
тоже являются запросами, а timeout не освобождает ещё работающий handler.
Данные явно помечены временем среза; кнопка «Обновить соединения» читает новый.
При потере worker или ошибке чтения прошлые данные сохраняются с предупреждением,
без объявления текущего connected. Старые ответы другого поколения не отображаются
в новой области. Старый worker использует прежнюю сводку и не получает неизвестных
запросов. Chromium/Linux IPC проверены; это ещё не native Windows gate панели.

## Attention и tray

Popup/звук содержит проект, короткую причину и действие «Открыть задачу», «Открыть workflow» или «Открыть приложение». Нормальная успешная сборка может быть тихой; blocked workflow требует внимания по настройке пользователя. Повторяющиеся события объединяются. Quiet hours, volume, severity, acknowledged и missed attention доступны в UI.

Первый реализованный сценарий 2026-10-07 — [журнал внимания](guides/attention.md): pending count,
страницы по 50, severity/причина/проект и task link, отдельная кнопка «Прочитано».
Открытие задачи не подтверждает просмотр; acknowledgement живёт у worker и сохраняется
после перезапуска UI. Tray/popup/sound/quiet hours прошли автоматический Windows gate.
Workflow/application events добавлены 2026-10-08: ошибка import не требует Task ID,
workflow failure показывает шаг/причину, runtime crash не переписывает успешный
startup. Target открывается по точному ID, даже вне recent list.
[Новый native gate](done/stage0-7/stage5/windows-workflow-attention-gate-2026-10-08.md) пройден автоматически.

Tray показывает активные задачи и attention count. Закрытие окна скрывает UI. «Закрыть интерфейс» и «Остановить worker» — разные команды; остановка показывает последствия для текущих tasks. Не требовать admin права для обычного runtime.

## Качество и расширяемость

В «Совместимость компонентов» readonly readiness сводка показывает отдельно
support, проверенные сборки, проверенную пару и готовность именованного профиля.
Build/scope/route/policy change объясняет несовпадение сохранённой квитанции;
успех прежней проверки остаётся историей. Task links открывают реальные сохранённые
задачи; связи из другой journal incarnation не перепривязываются. Disconnect
или истёкший cache не показывает текущий enabled. [A3.2c](plans/architecture-transition/readiness-projection.md)
[проверен на actual installed Windows pair](plans/architecture-transition/windows-operation-pages-installed-2026-10-15.md).

Статус передаётся текстом и значком, а не только цветом. Поддержать keyboard navigation, focus states, responsive layout, readable contrast и длинные пути/аргументы. После reconnect не сбрасывать раскрытые steps, фильтры, выбранный task и scroll position.

Новая capability обязана определить label, parameters view, events, progress, output/result, доступные actions и failure recovery. Общие status badges/timeline/renderers переиспользуются; предметный renderer добавляется в `ui/src/features`.

## Что уже можно посмотреть

`npm run dev:ui` открывает browser prototype и просмотр сохранённого TaskReport на localhost. Fixtures явно подписаны; browser не выполняет команды. Tauri использует worker adapter: форма project/recipe/direct, recent tasks, реальные параметры/timestamps, journal timeline, output paging и cancel. При потере ответа submit сохраняет исходные параметры/key для безопасного повторного запроса. Соединение и последний task status отображаются независимо. [Native Windows gate пройден](done/stage0-7/stage1/windows-gate-2026-10-05.md); UI integration test дополнительно проверяет сохранение формы и закрытие через файловый IPC shim к настоящему Unix worker.

На этапе 2 native UI дополнительно показывает controller URL/node/session, connected/connecting/disconnected, время последнего heartbeat и точную ошибку handshake/timeout. При недоступном local worker эти данные помечены как последние известные; task status остаётся независимым. Stop/restart controller с running task проверен UI integration на настоящей WebSocket сессии в Debian. Windows runtime gate и три новых native UI наблюдения подтверждены пользователем: этап 2 завершён, [доказательства](done/stage0-7/stage2/windows-session-gate-2026-10-05.md).

На этапе 3 native workbench показывает preview manifest (100 files/page), commit, обе mapping стороны, local generation, durable imports с phases/progress/reasons/cancel. Command form сохраняет optional snapshot ID и выполняет explicit pinned build; preview честно отделён от actual verified import. Capture/publish вызывается CLI агента; browser demo остаётся read-only. Windows runtime gate пройден, UI исправления приняты пользователем, этап закрыт.

На этапе 4 «Цикл разработки» связывает выбранный snapshot с editable JSON профилем import/build/start/readiness/log. Принятый профиль, step IDs, operation links и события читаются из worker journal. Потеря ACK предлагает повтор frozen запроса; восстановление профиля не исполняет команду. UI отделяет завершённый startup от состояния приложения, показывает PID/readiness/error и явную остановку. Log panel хранит byte cursor, декодирует UTF-8 incrementally, показывает reset при rotate/truncate/rewrite и время последнего чтения. Недоступность worker обозначает последние известные данные. Незаконченный JSON workflow draft и именованные editable profiles сохраняются по worker/project/root; каталог artifacts показывает проверенные файлы, source commit и SHA-256. stdout/stderr/log можно фильтровать и приостановить просмотр. Worker/desktop показывают build identity отдельно от generation; native визуальную проверку пользователь отложил.

Исправления 2026-10-06: published snapshot выбирается из списка worker с поиском по ID/commit, размером ZIP, metadata errors, refresh и страницами по 100. Delivery project/snapshot сохраняются по worker/project/root отдельно от command form; восстановление не запускает операции. Close дожидается всех изменённых форм; failed save удерживает окно открытым. Непроверенная сборка объясняет необходимость успешного импорта и сохраняет техническую причину. `snapshot-build` выводит диагностику компиляции; Unicode результат fixture находится в отдельной задаче запуска, на вкладке stdout/stderr. [Windows runtime evidence](done/stage0-7/stage3/windows-snapshot-gate-2026-10-06.md); новые UI исправления проверены Unix integration и ожидают native подтверждения.

Реализованы [история и task diagnostic](guides/history-diagnostics.md): поиск/фильтры,
переход к точным старым ID, Ctrl+K/Ctrl+Shift+P и размеры журнала. Command/output
в отчёте исключены по умолчанию; перед отдельным сохранением виден editable preview
с маскированием. UI подтверждает файл только после readback bytes/SHA-256; lost ACK
не считается успехом. [Настройки/retention](guides/settings-retention.md) позволяют
сохранить checked worker config с ручным restart и очистить только старый output
после preview. Automatic cleanup default off и включается явно. Незаписанные
правки удерживают окно открытым; Tab/Enter доступны в навигации разделов и controls.

## Delta-доставка этапа 6

Preview показывает полный целевой состав и отдельно ID/hash базы, число/объём
локально переиспользуемых файлов и ZIP bytes. Старое поколение сохраняется;
проверка каждого нового файла обязательна. Фаза «Копирование и проверка файлов
из базового snapshot» видна в import history вместе с сохранённой summary базы.
Недоступная база объясняет необходимость явного полного импорта. Lost ACK
повторяет прежний delta-метод/параметры/ключ; выбор snapshot не выполняет импорт.

### Возобновление доставки

Preview обозначает resume-capable package и объясняет explicit new import после
обрыва. Старый key только сверяет прежний результат. Worker Info определяет
поддержку; UI не предлагает schema 3 import старому worker. Import detail отдельно
показывает проверенные ZIP bytes из local cache и shared, фазу проверки/сборки ZIP
и точную ошибку блока. Resume не обозначается успехом до verified generation.


## Общий журнал A3.3 — проверен 2026-10-15

Task/import/workflow/application показываются вместе со scoped ID, typed status,
причиной, revision/event и занятыми ресурсами проекта. Одна страница — один
согласованный срез SQLite. Следующая страница сохраняет состав admissions,
«Обновить общий журнал» начинает новую выборку. Изменённые, но ещё не применённые
фильтры отмечены явно. Детали содержат команду/output; общий список их не дублирует.
При потере связи последний срез сохраняется с предупреждением; просмотр не
запускает работу. Старый worker получает сообщение об отсутствии поддержки,
новый контракт ему не отправляется. Windows candidate прошёл 48 tests и desktop compile; actual installed MCP/IPC/reconnect и renderer также проверены автоматически.


## Общая нагрузка A4.1 — проверена

[Native gate](plans/architecture-transition/windows-work-scheduling-gate-2026-10-15.md)
и [actual installed renderer gate](plans/architecture-transition/windows-work-scheduling-installed-2026-10-15.md)
пройдены. Рабочий лимит автоматически не включается.

Сводка показывает включён ли общий лимит, занятые слоты, ожидание и неопределённое
освобождение. Причины ожидания сохраняются в общем журнале. Workflow занимает work
slot через child; Ready app продолжает работать вне этого budget. Это не общий
счётчик Windows процессов. Saved snapshot отделён от свежей связи и read-only
archive. Неподдерживаемая metadata не выдаётся за активную политику.

## Очередь и пауза A4.2 — проверена

[Семантика и восстановление](guides/queue-control.md). Root count отличается от
work slots: один workflow считается один раз. Приём и запуск приостанавливаются
отдельно; accepted tasks, cancel/output и уже начатые исполнители продолжаются.
UI показывает реальную ревизию и время среза, не оптимистичный результат кнопки.
Lost ACK сохраняет исходную команду до отправки и после открытия окна; восстановление
не исполняет запрос. Stale revision требует нового явного решения. Unknown scope,
offline и read-only запрещают управление. [69 native tests и desktop compile](plans/architecture-transition/windows-queue-control-gate-2026-10-16.md)
прошли; [actual installed управление и renderer](plans/architecture-transition/windows-queue-control-installed-2026-10-17.md) также проверены.

## Общие ресурсы A4.3 — кандидат

[Семантика и config](guides/resource-scheduling.md). Inventory показывает объявленные
capacities, used/uncertain units и владельцев со scoped operation/lease IDs.
Набор выдаётся целиком, причина ожидания сохраняется в журнале. Build/cache и
process resources имеют разные lifetimes при application Ready. Offline/inspection,
unknown metadata и truncated inventory отмечаются явно. [80 native tests/desktop compile и actual Windows renderer](plans/architecture-transition/windows-resource-scheduling-gate-2026-10-17.md) прошли; installed новый контракт ожидает update. Production policy не включается автоматически.
