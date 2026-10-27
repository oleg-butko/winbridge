# Main25 после multi-controller merge — 2026-10-27

**Итог: повтор полностью подтверждён,140 native tests/52 files twice/34 negative
audits.** Первый неполный run ниже сохранён как история; окончательный результат
и pins — в конце документа. Actual retained19→20 проверяется отдельно.

## Контекст и границы

`feat/multi-controller` 9877b9e слит в main (merge 6a0abb6), review fixes
967c6a1 и актуальный checkpoint b4bbc9d опубликованы. Полный review:
[multi-controller merge](../../analysis/multi-controller-merge-review-2026-10-27.md).
Installed worker остаётся 0.1.1 / source59249e6 / EXE SHA-256
`536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712`.
Рабочий controller .203:1143/PID5103 сохранён, worker подключён к .203.

Main25 использует отдельный, уже разрешённый пользователем стабильный EXE slot
`K:\_proj-26\winbridge\runtime-test-204` и временный controller .203:1144.
Desktop и тестовые UI-окна не нужны. Старый slot `runtime-test` на .203 и его
pending conservation plan не трогать. Никаких firewall/system changes.

## Первый прогон: native success, independent gate incomplete

Run `20261027-191956-0a3bd8ae`, task
`3eec91f0-98f2-4438-850e-3eda858bc6e1`: succeeded / exit0.
Producer b4bbc9deea9fcc02a2f07447e6c1b84a8903a133:

- `sources/winbridge-20261027T191856Z-b4bbc9d-b0fa124a.zip`, SHA-256
  `203fa7cadecf880a5b132c2cdf48a07b1ed217873c5bc3311b9f58a7d7a9d063`;
- manifest `03b2f6d47a9c151789bd8b07cc37ac76c6aa635b0ebee2a78e8b9129ffd2fa07`;
- target48c834c00c658f11feec066c9ae4650a4e6cff7a,
  `source-archives/winbridge-20261027T183112Z-48c834c-d7b3d320.zip`, SHA-256
  `d11deff1be30d9b376c64417a0b700498eb85f24d154c7533edcad0b143f0bdd`.

Shared reports: `WinBridge/workflow-validation/<run>`; local metadata/output:
`.winbridge/update-preparation/main25-204-<run>`.
Profile явно `worker19-updater20`; отдельный cold worker17→19 test passed12.03s.
Свежий network handshake действительно прошёл (не выводится из разрешения EXE).
Actual Main прошёл replacement, same-child local service, readonly IPC/history,
pause, exit и owned Job retirement; native test body671.81s.
Core independent audit прошёл24 negative mutations,49 files прочитаны дважды.
Report SHA-256 `a97a4fa5062a24cdc8302a5a9ccf491af0db8118d529bdd182b3f5def97d5c06`.

**Full gate не пройден:** отсутствуют три независимых native trace markers.
Нельзя дорисовать их после завершения или ослабить verifier. Кроме того, старый
selector не учитывал, что `--nocapture` вставляет diagnostics между test name
и финальным `ok`; проверку выбора исправили без повторного исполнения.

## Причина и исправление

Validation hook выводил trace-input из `runtime.parent()`. Это работает для
`case/runtime`, но для external slot искало файл рядом с корнем проекта,
тогда как fixture записывал его в case. Hook возвращал None без ошибки.

Теперь external fixture явно пишет и читает input/markers внутри
`runtime/update/<canonical-update-id>`. Исторический `case/runtime` сохраняет
свой layout, одновременные scoped и legacy inputs отклоняются. Чужой sibling
у external slot не принимается. Все прежние input/build/owner/record pins
сохраняются; private validation не даёт нового admission или process grant.
Native Main25 сам требует три маркера с точным history5/history6, lock boundary
и command identity. Экспорт читает их также после lease-owned архивации runtime.
Selector сверяет полную Cargo argument vector и единственный успешный libtest
summary, допускает interleaved diagnostics. Public schemas/контракты не меняются.

Проверки исправления: Python8/0, portable hook tests5/0, MSVC all-targets
update-validation Clippy (workspace, затем worker после новых assertions), fmt.
Полный updater library test:251 passed/0 failed/4 штатно ignored, body33.88s.
Новый actual gate пока не засчитан.

При review independent reader дополнительно связаны input owner/update/attempt/
runtime/config с history5; writer marker обязан иметь тот же process/build и
exact input bytes/SHA, что обе lock boundaries. Ранее некоторые скоординированные
подмены вспомогательных marker fields могли остаться незамеченными. Reader теперь
имеет10 отдельных negative mutations и portable regression (Python9/0).
Тот же audit прошёл на трёх retained actual Main24 markers без Windows replay.
Это усиление readonly проверки, исходные reports не меняются.

## Отдельный readonly audit завершённого архива

Task `58727e1e-f29b-4234-9b8e-e7c37f329175` succeeded/0. Проверены actual native
file identities и bytes всех10 entries:996 files/152 directories; issued/done
move receipts, complete marker, original runtime marker и retirement proof pins.
Все named Jobs пусты, процессов в slot нет. Остались только исходные два файла
base lease; отсутствие EXE здесь штатно после архивации, не повод менять lease.
Plan SHA `7d48ee79388f7649334dceebc4fac94262cc08b58389f7561c8d7b130216229c`, archive:
`workspaces/snapshot-fixture/workflow-validation/tmp-main25-204-20261027-191956-0a3bd8ae/shutdown-main-cases/runtime-archive-2b270321-47a5-4c80-a441-e3282409f670`.
Новый activation permit этим чтением не выдавался. Installed worker unchanged,
active0, recovery/maintenance errors=null. Windows free23,834,763,264 bytes;
Debian free13,922,856,960 bytes перед повтором (оба больше6GiB).

## Как продолжить

1. Закоммитить fixes, экспортировать новый clean producer с built UI.
2. Повторить `scripts/run-update-service-main-native.py` с новым archive,
   тем же target48c834c, `--cache-run 20261017-061527-f4639971`,
   `--profile worker19-updater20`, released worker17 hash выше,
   `--test-runtime-directory 'K:\_proj-26\winbridge\runtime-test-204'`,
   `--controller-config .winbridge/controller-snapshot.local.json`,
   `--controller-binary target/debug/winbridge-controller`.
3. Новый run/state/instance/key; не переигрывать старую задачу. До admission
   дождаться actual network handshake, firewall budget900s. При observer failure
   читать сохранённый task ID, не отправлять заново неизвестный запрос.
4. Full verifier должен проверить все три actual markers, cold17→19,
   same-child IPC, history и24 negatives; затем readonly archive audit.
5. Следом actual retained updater19→20 с копией настоящего journal и refusal
   старого updater19. Не подменять исторический18→19 test актуальной константой20.
   Далее controller/lifetime/commit/release и U2–U4 основного updater plan.

Ни source0.2.0, ни native success первого прогона не означают release/install
или готовое самообновление. Рабочий worker0.1.1 оставлять запущенным, desktop
можно держать закрытым.

## Повтор выполнен: полный Main25 подтверждён

Run `20261027-195930-24f64ab8`, task
`85b38bd5-3a77-4e7f-83ac-b044c25e806f`: succeeded/0. Native140 tests,
37 test groups/41 commands, все exit0. Original normal Main/candidate pipeline,
same-child state/writer/local service, actual normal IPC/history, pause/refusal,
cold non-replay и owned process/Job retirement проверены. Это local service gate,
не whole-update commit/release и не установленное самообновление.

| Pin | Значение |
| --- | --- |
| Producer | `e03bb2a5c859c6d0236f776183350c55d92fe976` |
| Producer ZIP | `sources/winbridge-20261027T195915Z-e03bb2a-45f82268.zip` |
| ZIP SHA | `ed20cb7a7763316f14527f1993c94098b931d975d54d8657f4c5d5b6cdaf338f` |
| Manifest SHA | `bc5372406c759b8efc99163805afbe5b23d017ce9a056653763b06491a41b9b4` |
| Target | `48c834c00c658f11feec066c9ae4650a4e6cff7a`, ZIP/SHA выше |
| Report SHA | `1e7720e6b6cac1e8b86ab6255b87b2a2910de65015f1ebe95f598b785d7bfa02` |
| Proof SHA | `006458167065a5ed7de616b776431f01132ff73740349ed23be027818bf6e6cf` |
| Observations SHA | `190b9114e750b768216cec5f1ae2a3c11645829f6621b9852f9e331ee751f9c7` |

Full independent reader1e267f8 принял24 core+10 original marker negative audits,
все3 native barriers и отдельный cold worker17→19 proof.52 ready files прочитаны
дважды. Наличие и source/owner/process/input bytes каждого marker проверены;
нет восстановления пропущенных событий задним числом.1237 controller observations
связаны с actual preflight session `9c6a0ac8-52db-41b5-968f-084b60d5dccc`.
Main body660.70s; old exit339.942s/final backup351.350s/replacement497.920s от
начала fixture; Ready61.606s/StateOwned79.429s от activation. Window240s не продлевался.

Readonly повтор audit, **без Windows исполнения**:

```bash
python3 scripts/verify-update-service-main-native.py --profile worker19-updater20 \
  --run-info .winbridge/update-preparation/main25-204-20261027-195930-24f64ab8/run-info.json \
  --source-archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261027T195915Z-e03bb2a-45f82268.zip \
  --target-archive /mnt/hgfs/shared/WinBridge/source-archives/winbridge-20261027T183112Z-48c834c-d7b3d320.zip
```

Дополнительный postflight task `3f5f3365-16d3-47b6-8a6a-ce5c0b5e6e41` succeeded/0:
archived10 entries/1000 files/152 directories, actual native identities/bytes,
issued/done/complete receipts и original marker/retirement pins сверены. Plan SHA
`48047b3fc3238b61e3482b018a696b07b7b8fd2425a976e8610587db70f031c0`, archive
`workspaces/snapshot-fixture/workflow-validation/tmp-main25-204-20261027-195930-24f64ab8/shutdown-main-cases/runtime-archive-7c511752-1903-4bcb-8ff8-6bdcb793b554`.
Все named Jobs пусты, test slot содержит только исходный base lease. Fresh permits
не выдавались. Test controllerPID125411 остановлен,1144 свободен. Working controller
PID5103 и installed worker0.1.1/gen9b924962/incarnation5a8eb173 неизменны/active0.
Windows report free23,724,810,240→23,387,516,928 bytes; postflight23,387,734,016.

Тот же postflight проверил синтаксис нового migration PS (`scriptExecuted=false`)
и восстановил два exact old ZIP из Windows retained copies в
`WinBridge/source-archives/recovered-5ad74c42ecb2` (create-new, source SHA unchanged).
Следующий [actual retained19→20](retained19-to20-2026-10-27.md) тоже завершён;
далее следующий
controller/lifetime/commit/release сценарий. Production runtime и public install
по-прежнему не обновлялись.
