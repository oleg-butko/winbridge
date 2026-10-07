# UI/UX: пользователь видит и понимает действие

## Принцип

Интерфейс отвечает на вопросы: кто попросил действие, над каким проектом/снимком, что именно запущено, что происходит сейчас, чем закончится или закончилось и какое действие доступно пользователю. Transport ACK и запуск процесса не показываются как завершение задачи.

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

Output: независимые stdout/stderr, поиск, severity filters, pause autoscroll, новые фрагменты, byte offset и признаки gaps. Virtualization и paging появляются до больших logs. Копирование/экспорт показывает, что выбрано: весь сохранённый output или фильтрованная часть.

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

## Attention и tray

Popup/звук содержит проект, короткую причину и действие «Открыть задачу». Нормальная успешная сборка может быть тихой; blocked workflow требует внимания по настройке пользователя. Повторяющиеся события объединяются. Quiet hours, volume, severity, acknowledged и missed attention доступны в UI.

Tray показывает активные задачи и attention count. Закрытие окна скрывает UI. «Закрыть интерфейс» и «Остановить worker» — разные команды; остановка показывает последствия для текущих tasks. Не требовать admin права для обычного runtime.

## Качество и расширяемость

Статус передаётся текстом и значком, а не только цветом. Поддержать keyboard navigation, focus states, responsive layout, readable contrast и длинные пути/аргументы. После reconnect не сбрасывать раскрытые steps, фильтры, выбранный task и scroll position.

Новая capability обязана определить label, parameters view, events, progress, output/result, доступные actions и failure recovery. Общие status badges/timeline/renderers переиспользуются; предметный renderer добавляется в `ui/src/features`.

## Что уже можно посмотреть

`npm run dev:ui` открывает browser prototype и просмотр сохранённого TaskReport на localhost. Fixtures явно подписаны; browser не выполняет команды. Tauri использует worker adapter: форма project/recipe/direct, recent tasks, реальные параметры/timestamps, journal timeline, output paging и cancel. При потере ответа submit сохраняет исходные параметры/key для безопасного повторного запроса. Соединение и последний task status отображаются независимо. [Native Windows gate пройден](windows-gate-2026-10-05.md); UI integration test дополнительно проверяет сохранение формы и закрытие через файловый IPC shim к настоящему Unix worker.

На этапе 2 native UI дополнительно показывает controller URL/node/session, connected/connecting/disconnected, время последнего heartbeat и точную ошибку handshake/timeout. При недоступном local worker эти данные помечены как последние известные; task status остаётся независимым. Stop/restart controller с running task проверен UI integration на настоящей WebSocket сессии в Debian. Windows runtime gate и три новых native UI наблюдения подтверждены пользователем: этап 2 завершён, [доказательства](windows-session-gate-2026-10-05.md).

На этапе 3 native workbench показывает preview manifest (100 files/page), commit, обе mapping стороны, local generation, durable imports с phases/progress/reasons/cancel. Command form сохраняет optional snapshot ID и выполняет explicit pinned build; preview честно отделён от actual verified import. Capture/publish вызывается CLI агента; browser demo остаётся read-only. Windows runtime gate пройден, UI исправления приняты пользователем, этап закрыт.

На этапе 4 «Цикл разработки» связывает выбранный snapshot с editable JSON профилем import/build/start/readiness/log. Принятый профиль, step IDs, operation links и события читаются из worker journal. Потеря ACK предлагает повтор frozen запроса; восстановление профиля не исполняет команду. UI отделяет завершённый startup от состояния приложения, показывает PID/readiness/error и явную остановку. Log panel хранит byte cursor, декодирует UTF-8 incrementally, показывает reset при rotate/truncate/rewrite и время последнего чтения. Недоступность worker обозначает последние известные данные. Незаконченный JSON workflow draft и именованные editable profiles сохраняются по worker/project/root; каталог artifacts показывает проверенные файлы, source commit и SHA-256. stdout/stderr/log можно фильтровать и приостановить просмотр. Worker/desktop показывают build identity отдельно от generation; native визуальную проверку пользователь отложил.

Исправления 2026-10-06: published snapshot выбирается из списка worker с поиском по ID/commit, размером ZIP, metadata errors, refresh и страницами по 100. Delivery project/snapshot сохраняются по worker/project/root отдельно от command form; восстановление не запускает операции. Close дожидается всех изменённых форм; failed save удерживает окно открытым. Непроверенная сборка объясняет необходимость успешного импорта и сохраняет техническую причину. `snapshot-build` выводит диагностику компиляции; Unicode результат fixture находится в отдельной задаче запуска, на вкладке stdout/stderr. [Windows runtime evidence](windows-snapshot-gate-2026-10-06.md); новые UI исправления проверены Unix integration и ожидают native подтверждения.
