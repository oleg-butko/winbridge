# Updater13 →14: реальная история и отказ прежнего executable

2026-10-22. Этот gate отделён от [happy preparation v16](windows-runtime-preparation-main-gate-2026-10-22.md)
и от synthetic schema13 fixture. Новый сценарий добавлен, native результат ещё
не получен. Production updater schema14/worker16/supervisor v16/view8 не меняются.

## Источник и границы

Readonly reference — реальный successful13 final backup run
`20261021-155003-a33e1e12`, source8bd7629. Его12 ready files повторно сверены;
proof SHA `27d84e4ff254f371292c4a8e5be600da7c16022601e44427d5cb32d606fc70cb`.
Source commit проверен по Git: updater schema13. Reference runtime:
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation\20261021-155003-a33e1e12\shutdown-main-cases\None\runtime`.
Manifest SHA `9e1ffee9240e7472f9bce74bd2e251ab0a8f01e9a598a8220dab8966b6cdecb7`;
update ID `3522a9c3-a481-4ab1-bc15-1747ec1a675d`.

Оригинальный reference никогда не открывается updater writer, не мигрируется и
не получает нового owner. SQLite backup API создаёт consistent clone в новой
изолированной Windows validation directory; обычного копирования DB/WAL нет.
Installed runtime/config/journal не используются. Test не открывает UI и не
требует новых исходников или запуска desktop у пользователя.

## Что доказывает native test

1. Manifest bytes совпадают с известным SHA; actual old updater image/descriptor
   совпадают с updater role в manifest и исходным source commit/manifest SHA.
2. Реальная13 БД содержит прежний update, exact shutdown и Verified backup.
   Снимок всех SQL rows включает backup intent/current/history, исключает только
   изменяемую installation authority и новые14 preparation tables.
3. Consistent clone мигрирует13→14 с одним увеличением epoch. Все прежние rows,
   typed shutdown/backup records и installation/scope сохраняются; новые tables пусты.
4. Создан проверенный backup13 исходной clone-БД с теми же rows/epoch/integrity.
5. Новый owner получает существующий backup только как observation. Старый Exited/
   Verified не создаёт fresh preparation permit и не восстанавливает OS custody.
6. Actual old13 executable вызывает `journal init` и `journal status` с checked
   config новой изолированной БД14: оба отказываются `UnsupportedSchema`, DB/WAL
   bytes не меняются. Original reference DB/nonempty WAL также остаются прежними.

Raw manifest text включается в public proof: это metadata четырёх ролей, без
config или private control. Verifier независимо сверяет SHA raw bytes и связывает
actual updater SHA с manifest/source. Он также проверяет exact schemas, epoch,
preservation/no-permit/refusal outcomes. Данные не являются permission на restore,
replay или автоматический запуск исторического worker.

## Повтор и продолжение

Новый mode существующих runners — `-PreparationMigrationOnly` /
`--preparation-migration-only`. Прежний12→13 mode остаётся frozen.
Input pin создаётся из проверенного прежнего public proof и хранится локально:
`.winbridge/update-preparation/preparation-schema13-reference-pin.json`.

```bash
python3 scripts/validate-update-shutdown-primitives-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/<NEW-CLEAN-ARCHIVE>.zip \
  --cache-run 20261017-061527-f4639971 \
  --preparation-migration-only \
  --reference-receipt .winbridge/update-preparation/preparation-schema13-reference-pin.json
```

Это шаблон новой задачи, не readonly recheck завершённого run. Применять только
после clean export, проверки места и сообщения о required worker/desktop/windows.
Source/old references/report pins и terminal status сохранить отдельно.
После этого продолжаются actual preparation IO faults и consuming replacement;
gate миграции не заменяет остальные U2.2/U3/U4 проверки.

Код: `apps/update/tests/runtime_preparation_migration.rs`,
`scripts/test-update-shutdown-primitives.ps1`,
`scripts/validate-update-shutdown-primitives-native.py`.
