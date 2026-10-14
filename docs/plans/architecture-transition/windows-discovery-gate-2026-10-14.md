# A3.1 — native discovery gate: остановка сценария

Дата: 2026-10-14. [Контракт и local evidence](discovery.md).
Native gate **не пройден**; полный A3 открыт. A2 остаётся завершённым.

## Прогон

Clean source `ec300d37a136bfe30368ce5ac0a909c507158ef5`, archive
`winbridge-20261014T091405Z-ec300d3-9cbbf9e3.zip`, SHA-256
`39948842d28ec4cda15868dae483e9aff214045bb9059247b3ea6b1261585ca1`.
Run `20261014-051509-c518d0b8`, task
`cdfb65e3-53fa-4225-8e55-40ad6dfe5240`, PID 10516, failed/exit 1.
Report/ready: один artifact проверен по размеру/SHA-256, zero test groups.
Ошибка: `Prior build cache no longer exists` до компиляции/тестовых процессов.

Выбранный предыдущий run `20261014-044152-738951fb` сам заимствовал Cargo target
у `20261014-042057-8075f8f1`. Driver передал предыдущий report ID как owner cache,
хотя собственного `test-build-cache` у этого run нет. Дополнительная проверка
receipt после failure скрыла первичную ошибку; её порядок также исправлен.
Это ошибка сценария автора, не Windows worker или исследуемого runtime.

## Исправление и граница продолжения

Driver теперь разрешает цепочку checksum-verified parent receipts до настоящего
owner; сравнивает parent build/report hash и recorded directory, запрещает cycles,
traversal и цепочки >32. Windows получает только owner ID и pinned receipt,
не произвольный path. Terminal task/project и exclusive cache lease проверяются
прежним способом. Успешный A2 report остаётся evidence A2, cache не даёт test proof.

Девять Python tests passed, включая фактически случившуюся chained reuse ошибку,
подмену parent receipt, неверный directory и cycle. На реальных отчётах previous
run разрешился к `20261014-042057-8075f8f1`. Native повтор пока не запущен:
соблюдается условие пользователя остановиться и уточнить при новом неожиданном сбое.

Installed worker generation `02939593-8adf-4878-a346-473092697a4a` unchanged,
idle, без journal/maintenance errors. Desktop закрыт, окон не было; рабочий
worker оставить запущенным. Обновление installed worker для этого исправления
не требуется; build cache/reports/configs не удалялись.
