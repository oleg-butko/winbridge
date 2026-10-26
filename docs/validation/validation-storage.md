# Что занимает место в Windows workflow-validation

Сессия2026-10-26, работа в `perf/run-speed-2026-10-25`; updater не развивать.
Корень `K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation`
содержит **смешанные каталоги прогонов**, а не только временные файлы.
Удаление целого старого run может потерять повторно используемый cache и
эталонные executable/state, на которые ссылаются последующие проверенные отчёты.

## Фактическое измерение

Readonly inventory task `bd30afe7-3879-4492-9542-b2b6d92ad9e6`,
`.winbridge/storage-inventory/20261026-115118-c285f7e2/inventory.json`:
63 entries,118331 files, **37.6631GiB логических размеров**.
Ошибок чтения нет. Две ссылки в `update-stage-test` старых runs
`20261020-051354-bd565b6c` и `20261020-053153-bde36139` пропущены;
это не обход их targets. Hard links считаются на каждом пути, а не по уникальным
физическим блокам. Размеры могут изменяться после инвентаризации.
Первый inventory task684b90f1 завершился failed из-за суммирования PowerShell
dictionaries; он ничего не изменял и не является результатом измерения.

| Роль по структуре | GiB | Как обращаться |
| --- | ---: | --- |
| `test-build-cache` |15.1402|Повторно используется; очищать выбранный cache только без активной сборки/lease, следующая сборка станет холодной|
| `winbridge` source snapshots + SDK caches |10.8349|Внутри бывают `target` и desktop `target`; не считать весь каталог чистым source или tmp|
| `shutdown-main-cases` |7.9305|Retained updater fixtures: проверить зависимости от pinned reports, сохранять нужные эталоны|
| `test-temp` / `tmp-test-temp` на первом уровне runs |0.0529|Около54MiB; другие вложенные tmp могут входить в не классифицированные данные|
| Остальные файлы/каталоги |3.7046|Не классифицированы, автоматически сохранять; failed/старый run сам по себе не означает disposable|

**25.3395GiB находятся в одном run `20261017-061527-f4639971`:**
`test-build-cache`16256668312 bytes(15.1402GiB) и
`winbridge`10951384963 bytes(10.1991GiB).
Это действующий original compiler cache owner, который используют текущие
проверки и эксперименты. Внутри source directory лежат workspace
`target` и `apps/desktop/src-tauri/target` для SDK build.
Удаление только временных файлов не освободит основную часть38GiB.
Роли присваиваются непосредственным дочерним элементам каждого run; это не
полная рекурсивная классификация назначения всех файлов. Размер каждой такой
группы при этом измеряется рекурсивно, без перехода по reparse points.

## Новая маркировка

- Главный native workflow создаёт `tmp-test-temp` вместо `test-temp` и
  `README.storage.txt` в каталоге run. Legacy `test-temp` при diagnostic reuse
  сохраняется. Guide создаётся через CreateNew и не заменяет ручной текст.
- Новые `run-windows-perf-probe.py` work/helper каталоги:
  `tmp-build-matrix-<run>`, `tmp-desktop-release-<run>`,
  `tmp-worker-idle-<run>`, `tmp-build-profile-check-<run>`,
  `tmp-perf-script-<run>`. Work получает guide после возврата script;
  TEMP внутри новых build probes также `tmp-test-temp`.
- Существующие runs и эталонные пути не переименовываются. `test-build-cache`
  тоже остаётся на месте: cache provenance проверяет его точный путь.
- Старый profile-v1/profile-hotspots tooling сохраняет прежние имена. Не считать
  весь namespace уже переведённым на новую раскладку.

`tmp-` означает роль временных рабочих данных. Для удаления всё равно нужны
terminal результат task, отсутствие его процессов/leases и отсутствие ссылок
из сохраняемых reference reports. При неизвестном исходе сохранять.
README не является подтверждением terminal результата или полномочием cleanup.
Никакой автоматической очистки здесь нет. Windows каталоги в этой задаче
не удаляются; tiny fixture cleanup касается только новых собственных Temp files.

Для дальнейшей раскладки рекомендуются отдельные `cache/`, `references/`,
`reports/`, `tmp/`, но это отдельная миграция scripts и cache/proof contracts.
Переименование уже проверенных каталогов не заменяет такую миграцию.

## Инструменты

На Windows:

```powershell
pwsh -NoProfile -File scripts/inspect-windows-validation-storage.ps1 -Root 'K:\_proj-26\winbridge\workspaces\snapshot-fixture\workflow-validation'
```

Из Debian через существующий controller:

```sh
python3 scripts/inspect-validation-storage-native.py --controller-config /home/user1/winbridge/.winbridge/controller-snapshot.local.json
```

Script только читает metadata, не открывает содержимое journal/config/credentials,
не следует junctions/reparse points и не удаляет файлы. Роли — подсказки по
структуре имён, не подтверждённый граф references. Сохраняет request до submit;
при неизвестном acceptance не пересылать заново. Для того же task использовать
`--observe <saved-submission.json>`. Raw inventory/output не коммитить.

`scripts/test-validation-storage.ps1` проверяет exact sizes, пропуск junctions,
сохранение неизвестных данных/ручного README, новый tmp и legacy path selection.
Native fixture taskfe02435c-c1ce-47e6-ab36-603daa888512 succeeded/0.
Linux cache provenance regressions:10 passed. Полный Rust/native gate не повторять
из-за одной маркировки: compiler/runtime logic здесь не меняется.

Новый prefix/guide проверен быстрым native build-profile-check, без Cargo/UI:
run20261026-115618-35aa3ac4/taske8103ef1-50d5-45da-9683-766bcefeab46,
succeeded/0, ready report проверен twice. Создан новый
`tmp-build-profile-check-20261026-115618-35aa3ac4`, README затем проверен
отдельным task. Также прошли3 desktop experiment verifier regressions,
Python syntax/scaffold286. Полный workflow с новым TEMP ещё не запускался;
его отдельный selector и совместимость legacy paths проверены Windows fixture.

В корне Windows workflow-validation созданы **только новые**
`README.storage.txt` и `STORAGE.inventory-20261026-115118-c285f7e2.json`.
Task533ff50c-ebe0-4103-bcae-8b2aad186042 succeeded/0, copied hashes verified;
существующие файлы не заменяются. README SHA256
`d91fa6311b086ec383e32f1d9282a4f3bc4bc9085e2c2656b41d46a348774892`,
inventory SHA256
`9ece769718a19270bbae7ba0dca4daa0d0bfaeb888fcccb082e3add4abbd59df`.
Delivery `.ready` и исходные тексты сохранены в WinBridge shared namespace
`storage-inventory/20261026-120056-9ff6cd82`;
native request/terminal/output — `.winbridge/storage-guide/20261026-120056-9ff6cd82`.
