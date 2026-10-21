# Windows: readonly receipt после выхода

Run `20261021-043442-51698f37`, task `9f70a474-5fdd-48e8-8aae-50a313e97172`:
succeeded, exit 0. Четыре теста в трёх группах прошли:

| Группа | Тестов | Scope |
| --- | ---: | --- |
| shutdown-reader-cli | 1 | Actual CLI/client, занятая instance lock и освобождённый fixture owner, сохранённые DB/epoch/pending guard, отсутствие recovery marker, отказ SHA/foreign state/different attempt. |
| shutdown-reader-report | 1 | Exact helper PID/input/config/state/schema/receipt и readonly flags; подмена отклоняется. |
| shutdown-reader-v6-contract | 2 | Версия/контекст/state/lease/receipt, отсутствие скрытого exit permission. |

Семь ready files независимо проверены по размеру и SHA-256 дважды.
Report SHA-256: `84bd67e52d0fe5f1c3674fd33af4674a42227409c9af3a91c8b2ea7df1817aad`.
Shared: `WinBridge/workflow-validation/20261021-043442-51698f37`.

Чистый source `a294e03100712ecf5a3e5ed836ef008bf084b3c4`:
`winbridge-20261021T043231Z-a294e03-6d95065c.zip`, archive SHA-256
`d0f79b40da71e624087d4ac2531ed56d30e018de24839ef70c4f210ca00d3607`.
Original worker/updater schemas16/12; reuse test-build-cache
`20261017-061527-f4639971`, свежие source/state/report.
Windows free 40,189,407,232 → 39,856,279,552 bytes, порог 6 GiB проверен до сборки.
Сценарии — существующий primitive runner с `--cold-reader-only`/`-ColdReaderOnly`.
Ранее пройденные 19 primitive tests здесь не повторялись.

Использованы synthetic intent metadata и actual reader exe/owned probe. Полный
actual worker Main stop/ACK/callback retirement этим не подтверждён. Нужны
consuming supervisor driver и combined gate. Reader не запускает worker заново,
не мигрирует, не изменяет markers и не восстанавливает process/owner permission.

Installed `d052c6c`, generation `78e9bbe1-8cb9-4ad3-93cf-371e981801ad`, после gate
здоров, idle0, recovery coverage complete, maintenance/recovery errors отсутствуют.
Runtime не обновлялся, окон нет. U1–U4 и public auto-update остаются незавершёнными.
