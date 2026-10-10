# Ревью и следующий потребитель общих обработчиков — 2026-10-10

Новая сессия после reboot: исходное дерево clean 58f3ee2. Controller запущен
с прежним локальным config; Windows worker clean fa7e4d7, generation
372266b6-3dee-4d87-9de2-ccab896dbcf8, activeTaskCount=0, ошибок maintenance/journal нет.
Desktop закрыт. Пользовательские configs, рабочий Netwatch и installed runtime
не меняются.

Проверены раскрытие профилей, bounds Info, frozen command/origin, workflow pins,
same-key lookup до новой child acceptance, restart/config-change и файловые
проверки перед process creation. Новых важных ошибок в этих путях не обнаружено.
Это целевое ревью, а не утверждение об отсутствии всех возможных ошибок.

Найдена ошибка build identity: без export manifest Git искал checkout вверх по
родительским каталогам. Вложенный source generation мог получить чужой commit;
при отсутствии Git значение sourceDirty оставалось false. Исправление требует
совпадения Git top-level с корнем исходников; неизвестное происхождение больше
не считается clean. Regression компилирует настоящий build.rs и проверяет own
checkout, dirty changes, вложенный generation и unversioned directory.

## Следующий сценарий этапа 7

Второй реальный потребитель после Netwatch — сам WinBridge worker. Два отдельных
тестовых проекта используют общий recipe profile build-service и локальные
рецепты запуска, поскольку условия готовности приложений различаются. Общий
профиль вызывает читаемый build script из проверенного snapshot, с фиксированными
RuntimeDirectory/LogDirectory/TargetDirectory вне исходников. Script и дополнительный
validation commit каждого clone сохраняются в evidence; production repositories
и runtime не редактируются. Netwatch isolation уже согласована пользователем.

Проверить автоматически на настоящей Windows: точные Debian ready/manifest pins,
import → recipe build → artifact hash → start → readiness → log, происхождение
рецептов и expanded paths для обоих проектов, отсутствие повторного запуска при
same key, явную остановку деревьев, restart тестового coordinator и сохранённую
историю. Вторая worker application имеет свой config/instance/SQLite и не
подключается к Debian; её настоящий IPC также проверяется. Рабочий worker только
запускает сценарий. Desktop/test windows не требуются, новые firewall rules не нужны.

Результат должен подтвердить существующие границы CommandHandler/runner и
workspace-sync: adapters раскрывают команды/проверяют файлы; один worker владеет
ACK, lease, journal, cancel и recovery. Новая очередь, transport/RPC, dynamic
plugin loader и SDK не нужны этому потребителю. Этап не считается завершённым
до проверенного native report. Проверки отмены/crash/profile edits предыдущего
сценария переиспользуются как regression.
