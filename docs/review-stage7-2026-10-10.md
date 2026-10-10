# Ревью перед этапом 7 — 2026-10-10

Исходное дерево clean bad7bcb. Controller подключён; read-only Info показывает
installed Windows worker af90ec0, generation c182793f-5391-4bf8-9e70-9c94b8b24a5c,
activeTaskCount=0, maintenanceError/journalRecoveryRequired=null. Рабочий runtime
не заменяется; desktop для backend проверок не требуется.

Проверены schema-3 metadata bounds, actual block/whole ZIP hashes, cache paths,
staging publication, cancel checkpoints, transfer counters, project lease,
recovery и same-key admission. Новых важных ошибок в этих путях не обнаружено.
Частичные/повреждённые bytes остаются unbuildable; старый ключ возвращает старую
запись; crash не запускает внешнюю команду заново. Native evidence этапа 6 остаётся
в [resume gate](windows-resume-gate-2026-10-10.md). Это целевое ревью, не утверждение
об отсутствии всех возможных ошибок. Исправлены противоречивые статусы в README.

Первый потребитель интерфейса этапа 7 — два зарегистрированных проекта с общим
профилем рецептов. Сейчас раскрытие direct/recipe и файловая policy объединены
в WorkerConfig::resolve; отдельный runner module пока только README. Новый
статический интерфейс разрешения команд должен переиспользоваться обычными
tasks и workflow steps, сохраняя единственный worker journal/process service.

Профили остаются внутри локального worker config и применяются после ручного
restart. Не вводятся внешние config includes, наследование/скрытые overrides,
новые RPC или dynamic plugins. Итоговые executable/argv/cwd и источник рецепта
показываются до submit и сохраняются с accepted task. Историческая команда не
меняется после редактирования профиля. Проверка: два проекта, fixed recipes,
запрет direct, duplicate key/conflict, cancel tree, restart/recovery и старые DTO.
Автоматический Windows прогон использует отдельные exe/state; installed worker
только запускает одобренный сценарий. Новый ручной gate не назначается.

При интеграции обнаружена важная ошибка queued workflow: accepted submission
с recipe ID не закреплял определение, и после config edit/restart мог использовать
новые аргументы. Исправлено recipePins до ACK и сравнение перед стартом дочерних
операций. Уже принятые tasks/история остаются frozen; старые queued recipe workflow
без pins требуют нового явного запроса, поскольку прежние параметры неизвестны.
Changed-profile и legacy regression tests добавлены; успешный profile build также
проверяет provenance дочерней task. При ошибке terminal commit admission блокируется
тем же journal fault механизмом, что в прежних workflows.

Новая reuse возможность ограничивает раскрытый Info metadata до 512 KiB с JSON
escaping: маленький общий config не может раздуть ответ сверх transport frame.
Unknown bindings/collisions/argv limits проверены; Settings оставляет malformed
nested JSON в raw editor вместо падения controls. Bindings не переинтерпретируют
подставленные строки; локальные recipes сохраняют literal strings.

Проверка recipe pins выполняется и перед новой child acceptance в recovered
running workflow. Уже принятая child сначала сверяется по durable key, чтобы
не скрыть indeterminate outcome после crash. Проверен точный crash window после
завершённого import и до build acceptance: изменение рецепта не запускает build.

151 full workspace tests и final 24 workflow tests, Clippy/Windows all-targets,
DTO/TypeScript/build/scaffold/fmt проходят. Full browser integration прошла.
Первый browser attempt остановился на explicit quit сразу после restart worker:
Settings ещё перечитывал generation. Harness теперь ждёт текущую settings generation
и окончание чтения, затем проверяет именно отказ tray. Дополнительный ошибочный
case-sensitive текст ожидания исправлен; финальный прогон passed. Тестовая
синхронизация не подменяет проверку close; новых Windows ручных действий нет.

Итог: 152 full workspace tests проходят. [Automatic Windows gate](windows-profiles-gate-2026-10-10.md)
passed: 68 native tests, пять проверенных по SHA-256 artifacts, source clean fa7e4d7.
Installed worker healthy/idle прежней generation. Controller обновлён до этого
проверенного кода после gate, reconnect успешен. Первый сценарий этапа 7 завершён.
