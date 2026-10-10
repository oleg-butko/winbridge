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
