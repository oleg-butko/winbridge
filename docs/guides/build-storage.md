# Место для сборок и проверок

Перед тяжёлым прогоном проверить `df -h /home/user1/winbridge` и свободное место
на Windows по `report.json → storage`. При запасе менее 6 GiB не начинать новые
сборки. Отчёты, journals, configs и пользовательские файлы сохраняются.

Локальные последовательные проверки запускаются с `CARGO_INCREMENTAL=0`.
Используйте основной `target`, чтобы повторно использовать зависимости.
Не создавайте новый полный cache для каждой небольшой проверки.
Для освобождения места достаточно удалить неиспользуемые `target/**/incremental`
только при отсутствии активного cargo/rustc. Эти файлы восстанавливаются сборкой.
Работающий controller и его exe сохраняются; Windows runs без отдельного
указания пользователя не удаляются.

Очистка Debian 2026-10-14: удалены только четыре incremental-каталога основного
Rust workspace и native desktop. Освобождено 20,810,207,232 bytes (~19.4 GiB),
доступно 28,755,800,064 bytes (~26.8 GiB) вместо 7,945,592,832 bytes.
Исходники, настройки, БД, логи, reports и работающий controller не изменялись.

Последний Windows gate b.1: после сборки оставалось 32,443,744,256 bytes
(~30.2 GiB); [native report](../plans/architecture-transition/windows-fenced-request-gate-2026-10-13.md).
Это измерение конкретного прогона, а не обещание текущего свободного места.

Для full CAS повтора после исправления используйте
`--task-cas-reuse-build-cache <run-id>`: [проверка receipt и lease](../validation/windows-workflow-validation.md).
Новые исходники, test-temp/state и report отдельные; Cargo target один.
Failed тест не становится passed из-за reuse cache. Windows каталоги не удаляются.
