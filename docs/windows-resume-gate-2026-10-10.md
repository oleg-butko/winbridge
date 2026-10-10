# Этап 6: automatic native Windows chunk/resume gate

2026-10-10. Рабочий Windows worker clean af90ec0 только запускал одобренный
сценарий; его exe/config/journal не заменялись. Desktop закрыт, новых окон нет.
Новый worker собирался из clean `38f4e5de4b8c6efeeb8524c589218b08b96eb1af`
в отдельном local runtime. Его manifest SHA-256:
`f1e1b78456deeafbd1c64b45e19916427f65f9930465dad4c825d7281e18a438`.

Source archive: `winbridge-20261010T091640Z-38f4e5d-4690e508.zip`, SHA-256
`1646f6fc2f88a6f1fe764084b4303d4c1156eaf63a0a89a9f5ed04341ad4f923`.
`validate-workflow-native.py --resume-only` опубликовал fresh fixture в Debian,
затем Windows импортировал **тот же ready/manifest/blob pin**. Доставка размещена
в `WinBridge/resume-validation/20261010-051707-f5ebc3d7`, report —
`K:\VMs\shared\WinBridge\workflow-validation\20261010-051707-f5ebc3d7`.

- Outer task `9ab94af0-988f-4086-961d-4451cc85e162`: succeeded, exit 0.
- **39 native tests**: core metadata/policy, шесть HGFS block/corruption/cancel/
  mount/delta/junction tests, owned worker crash/restart/dedup/build и семь
  process-tree/timeout/output runtime regression tests.
- Шесть report artifacts прочитаны обратно в Debian; их actual bytes/SHA-256
  совпали с ready. Report SHA-256
  `1b84f21af9050d83cdd6102b5697d8f39ba17a5fb73f9f34b680fe7ffcfc2cb6`;
  proof SHA-256 `3a0d251a2efa0d39b7fc38f5f1d3af13d70ab4232e9156f52581a938c8556019`;
  ready SHA-256 `be71f2f0d170730bdf95949546d596064a602678d9e062d306059d67df37d313`.

## Actual resume и результат

Payload: **33 555 078 bytes**, 33 блока по максимум 1 МиБ.
Snapshot `r-005a7d1f547784fa187471e47fa55fb6a7aa9bc5c0969aafe974509dfd6e5482`,
fixture commit `38d491ed8bba789ef66e513e44920a060f392204`.
Manifest SHA-256 `9fb3b1175555f0252dcd565a50177aabc1cda5240bb685d9dca31dcae49a790f`;
payload SHA-256 `ee975c95a2b3a1a2802331d8b42a6c0cbf5bd16ccf4dc9708b1041fac5b084f4`;
actual 32 MiB + 31 blob SHA-256
`59a6392a3ecfb9c633fbeefbd8e91a15fd13ab544ffc6d47909a93b1a42c8244`.

| Сценарий | Проверено из cache | Проверено из shared | Observed import |
| --- | ---: | ---: | ---: |
| Полная schema-3 доставка в новом runtime | 0 | 33 555 078 | 17 158,97 ms |
| Явный импорт после cancel/crash | 25 165 824 | 8 389 254 | 13 388,86 ms |

Повторное чтение ZIP из HGFS сократилось на **75%** (с точностью до последнего
короткого блока). Это один sequential pair с разным cache state; 22% observed
time reduction не выдаётся за воспроизводимый общий speedup. Control samples
также descriptive, без утверждения отсутствия всех возможных latency regressions.
Первая публикация включает вычисление block hashes и полную ZIP проверку:
Debian capture 3 649,18 ms, local verify 6 474,08 ms, shared write 3 140,02 ms,
readback 3 096,93 ms, total 16 482,37 ms. Времена разных хостов не вычитались.

Отмена импорта `42335bd9-adb0-45ec-bb8c-991fa473c712` сохранила четыре блока;
cancel ACK 6,53 ms, наблюдение terminal cancelled 9,96 ms. Тот же key вернул
тот же cancelled ID. Следующий import `267a0a2c-bfbb-40f3-a6df-0cbcc302a8b1`
использовал эти четыре блока и получил ещё 20 до **планового kill только owned
test worker**. После restart тот же ID стал interrupted; повтор key не делал
filesystem work, изменённый pin с тем же key отвергнут.

Новый explicit key дал `031f989a-c10a-46f2-ad0b-3ecd1b7e95d6`, использовал
24 блока, проверил assembled ZIP и весь tree. Прежние records/staging сохранились
во время теста; никакая внешняя команда не повторялась после неизвестного исхода.
Build task `4a68b33f-1fc8-4e0e-a7fa-945740d9a4e8`: succeeded/exit 0, тот же
manifest pin, offline Cargo target вне sources. Повтор key вернул ту же build task;
запуск её artifact вывел `Проверенный resume → Windows build ✓`.

Первые 256 Info samples в running imports: full median/p95/max
1,70/2,14/6,08 ms; resumed 1,71/2,51/5,23 ms. Explicit attention к отдельной
fixture task во время imports: 3,20 и 6,37 ms. Это local native IPC, не Debian
WebSocket latency. Import events остались bounded: 10/11 вместо записи фаз
для каждого блока. Tests завершили свои workers, installed generation
`c182793f-5391-4bf8-9e70-9c94b8b24a5c` осталась healthy/idle.

## Первый прерванный gate

`20261010-051248-8c1f0d94`, outer task
`8399240c-162a-4a6d-a65b-bc0f4cbd41c5`, сохранил failed report. Пять HGFS
tests прошли, шестой не создал test junction: cmd трактовал '/' в mixed-separator
path как switch. В `38f4e5d` исправлен harness и введены unique native instance IDs;
сам junction check повторён и прошёл. Ошибки рабочего worker не было, update
installed runtime не требовался. Прежний report не изменён.

Этап 6 закрыт: baseline/Git batch, delta/cache, output batching/virtualization
и explicit snapshot resume прошли gates. SocketDelivery не требуется по этим
измерениям: control остаётся отзывчивым, resume устраняет повторный HGFS read;
другой adapter и дополнительная list virtualization выбираются при новых данных.
Автоматическая source-cache cleanup остаётся отдельной политикой.
