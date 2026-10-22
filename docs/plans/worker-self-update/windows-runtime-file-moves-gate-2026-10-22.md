# Runtime file moves и FSM: Windows gate пройден

2026-10-22. Task `07e45433-4782-43c1-8527-8b3217b100c3`, run
`20261022-203417-4db1d2dc`, succeeded/exit0. **9 tests / 2 command groups**:
три actual Win32 filesystem tests и шесть data FSM/batch tests.
Все шесть ready files сверены независимо дважды.

Report: `K:\VMs\shared\WinBridge\workflow-validation\20261022-203417-4db1d2dc`
(`/mnt/hgfs/shared/WinBridge/workflow-validation/20261022-203417-4db1d2dc` в Debian).
SHA-256 `report.json`:
`889de8d2b79153bb6bc151eb91dc539eb2dd9e0986ba4e6ff0d9466dfa18a6db`.
Local verification: `.winbridge/update-preparation/runtime-move-native-verified.json`.

Producer clean `6f060f0abe0c29abc325d18779d736cc52b72ff4`, ZIP
`winbridge-20261022T203358Z-6f060f0-c3c330a0.zip`, SHA
`bfb2c1addbc14764a1a03ecf454b3e54450a486b86c3cb169ac9859d5ee16f9c`.
Использован существующий cache `20261017-061527-f4639971`; full desktop/candidate
build не выполнялся. Установленный worker d052 не менялся; UI-окон не было.

Native tests проверили фактический перенос файла с кириллицей/non-BMP символом
в имени: source path исчез, File ID/hash/size сохранились, write sharing запрещён
до и после move. Проверены occupied destination с ошибкой80/183 и сохранением bytes,
отказ второй OS попытки, неверные source ID/hash/size/parent и занятый no-delete pin.
API использует один source handle, absolute UTF-16 path и NULL RootDirectory;
все ancestor directories удерживаются no-delete handles до readback.

Первые три native tests предыдущего run `20261022-202619-3d8f4020`, task
`3fc7949a-e9fe-441f-b0e8-2b72608d0822`, дали2/3/overall failed. Relative RootDirectory
дал error87. Тест collision сначала принимал любой отказ, поэтому его прежний pass
не был доказательством no-overwrite; теперь он требует именно80/183. Failed report
и файлы сохранены. Исправление не добавляет runtime fallback или повтор эффекта.

Data tests проверили порядок intent/receipt, exact rename identity, отсутствие
переходов после неопределённого исхода, JSON reopen, immutable scope, независимые
IDs пяти файлов и публикацию manifest последним. Они не создают physical custody.

Windows available57640415232→57641054208 bytes; validation directories не удалялись.
Все тестовые процессы завершены. Desktop можно открывать/закрывать, installed
worker оставить работать. Затем отдельно запущен full backup fault gate;
его результаты этот документ не подменяет.

Это primitive/model gate. Replacement binding/ledger, consuming custody handoff,
файловый driver, activation и public installation ещё не реализованы.
[План следующего блока](runtime-replacement.md).
