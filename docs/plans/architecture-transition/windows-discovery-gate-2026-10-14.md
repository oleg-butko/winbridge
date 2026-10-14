# A3.1 — native discovery gate и финальное ревью

Дата: 2026-10-14. [Контракт и local evidence](discovery.md).
**A3.1 завершён**: финальный gate прошёл 203 native tests.
Полный A3 открыт; дальше [A3.2 paired readiness](paired-readiness.md).
Ниже сохранены остановки/исправления и evidence каждого прогона.

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

## Авторизованный повтор и отсутствующий cache

Пользователь подтвердил автоматическое исправление/повтор и уточнил: выбирать
рекомендованный вариант в таких ситуациях самостоятельно. Это правило сохранено
в AGENTS.md; реальная проблема установленного worker по-прежнему останавливает
зависящие Windows действия.

Run `20261014-053002-115eeed9`, source `509dabd`, task
`d7a90728-2abf-4f8e-837d-67e638253f6e`, failed/exit 1, zero test groups:
старый source manifest не найден. Read-only inspection task
`96fc7226-90fb-49dd-bdc5-4f594f3abfe7` succeeded/exit 0: весь owner directory
`20261014-042057-8075f8f1` уже отсутствует, включая compiler cache. Free Windows
bytes 37,407,731,712 (~34.8 GiB). WinBridge не удалял этот каталог.

Сценарий больше не требует старых source files для compiler cache. Если cache
отсутствует, выбирается только новый `test-build-cache` текущего run, отчёт хранит
requested receipt и `cache_missing`, driver проверяет свежий scoped directory.
Не-directory/reparse/busy cache не превращается в fresh fallback. Fixed local
volume, exclusive lease и 8 GiB free-space threshold сохраняются; старые
каталоги не очищаются. PowerShell тестирует missing/existing-without-source/file/
linked ancestor; 10 Python tests проверяют provenance и explicit scoped fallback.
Автоматический fresh повтор готовится, installed worker остаётся healthy/idle.

## Успешный fresh gate и последующее ревью

Source `f8c5676f583147549f16d147e5786e11fa266afa`, archive
`winbridge-20261014T094331Z-f8c5676-d84c66a2.zip`, SHA-256
`21e418eed764f5a916935a00e52c1a1810b59227f3d435430e5a4263da43ee75`.
Run `20261014-054619-d5d99c71`, task
`1ada176b-7b02-47fa-aed1-1d94e1a7381a`, PID 2784, succeeded/exit 0.
UTC 09:46:26–09:52:40. **202 native tests**, все 7 групп и desktop compile passed;
10 artifacts size/SHA verified, report SHA-256
`458c5d075140c1445925a317078bbf4bb71b570e7c33b0afaef14587f5075698`.
Candidate build ID `1791971228556180000`, manifest SHA-256
`0226cc4e1c94e9eb789b0a790bf74edc1a6503c8866d702fece46fba7577effb`.

Candidate named pipe Info содержит build-bound catalog. Настоящий reference exe
schema 6 с новым config/state вернул читаемый Info без catalog/fenced scope,
подтверждённо остановлен; actual older-schema refusal также passed. Installed
worker generation/session unchanged, idle, без maintenance/journal errors.
Окон не было, desktop можно оставить закрытым, рабочий worker оставить.

Cache missing подтверждён report: requested owner `20261014-042057-8075f8f1`,
`buildCacheReuseSkipped: cache_missing`; фактический новый cache находится в
`20261014-054619-d5d99c71/test-build-cache`. Windows free before 37,373,976,576,
after 33,064,738,816 bytes (~30.8 GiB); fresh build добавил ~4.01 GiB вместе с
native desktop dependencies. Debian ~23 GiB free, incremental выключен.

Финальное ревью нашло несовпадение Rust/TS для omitted nullable build fields:
UI теперь нормализует missing → null так же, как Rust Option; 24 common cases
passed. Также Responded/Error с foreign/null scope мог считаться подтверждённым
Rust transport: теперь все Responded требуют исходный scope, explicit rejection/
unconfirmed остаются error-only. Есть реальный RpcSender regression без retry.
286 workspace tests, UI checks и Windows-target Clippy прошли. Этот protocol fix
проверяется новым native повтором с уже созданным cache; до него A3.1 не закрыт.

## Финальный scope-guard gate — passed

Clean source `ec75bcb675f1a33019362e36d8f448bc7f5971d4`, archive
`winbridge-20261014T103212Z-ec75bcb-6d8f1556.zip`, SHA-256
`1ec4c6f8ecd01c29b271a862c6f43276e41241f1fdb2ae84b28274bd91f28226`.
Run `20261014-063318-85934278`, task
`f3712202-462e-4c13-9a4f-5dbfb26495f5`, PID 23624, succeeded/exit 0.
UTC 10:33:24–10:38:42. **203 native tests**, 7 групп и desktop check passed;
10 artifacts size/SHA verified, report SHA-256
`1afda5f25e7adc8820e4306c96be5d7200bbdb8339cce5a5e9aead11ebe51baa`.
Candidate build ID `1791974033135349800`, manifest SHA-256
`c1d8af838926c5740e1a4cf720b34572e7b431973a1004b3d09816fe8e5976c4`.

Новое regression проверило foreign/null scope error через actual RpcSender:
confirmation отвергнуто, transport не повторил и не перепривязал request.
Все остальные runtime/restore/refusal/dedup cases также passed. Reference Info
missing catalog корректно прочитан; reference runtime подтверждённо остановлен.
Installed worker generation/session unchanged и activeTaskCount 0; app history
показывает stopped, активных приложений нет. Desktop закрыт, окон не было.

Cache из `20261014-054619-d5d99c71` переиспользован, source/state/report новые.
Windows free before 33,483,112,448, after 32,785,203,200 bytes (~30.5 GiB),
добавлено ~0.65 GiB вместо второго полного cache. Runtime/state/reports сохранены.

Local final review: 286 workspace tests, Linux/Windows-target Clippy, 24 shared
Rust/TS metadata cases, bound request tests, typecheck/build/contracts и script
checks passed. A3.1 закрыт. Это доказательство isolated candidate и mixed reader
compatibility, не pair-tested installed route. Для A3.2 нужен новый installed
worker по постоянному exe path; [инструкция](../../validation/architecture-worker-update.md).
