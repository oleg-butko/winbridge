# Readonly updater support: Windows gate пройден

2026-10-23. Task `d97ea580-8adb-43ff-bf71-224d0d9b222c`, run
`20261023-190500-afa1564e`, terminal **succeeded / exit0**.
**8 native tests /4 test groups**, 9 ready files проверены дважды,
36 намеренных искажений actual proof отклонены. Это отдельный короткий gate,
без повторения source/build/drain/replacement pipeline и без установки.

## Точная проверенная версия

| Поле | Значение |
| --- | --- |
| Producer | `f1e2f102dc04d40b4704dbaa7a3f29113f2d307e` |
| Source ZIP | `winbridge-20261023T190444Z-f1e2f10-d0bafcc6.zip` |
| ZIP SHA256 | `b3cee2ed3bbf9d31571e6ebcca36050d155b671b2dce4e9d0c49a726a07bc42a` |
| Source manifest SHA256 | `2407a108d0172a75ddcb0fcae5b46fc70691858b15a43e38ab12f34b8879aaf6` |
| `report.json` SHA256 | `f0ca8b7185edf57d4ae2a65d10c22e8043be862691fc2f2983702da11f8871a0` |
| `updater-support-proof.json` SHA256 | `4beb813fddcadcaa607eea310ccc6b3caca6a8bdab16588939eec9cabbb429d4` |
| Standard image SHA256 | `5d39f565634969e2524dfb590d5a519f51056a43546f16ba9f1c5d42d3df4ff2` |
| Validation image SHA256 | `b08c9b0fdbc4e0ca98f2bde72c893185c7a031787935330d8628e6b7a3e64307` |
| Retained old updater SHA256 | `e964676e8e80d2a9747a2d5f1b1453205c29c17966695ee0ff6c9a4b4efa2b81` |

Обе новые сборки updater journal15, queryContract1; worker16/supervisor17/view9
не менялись. Retained old image взят из полностью проверенного
[positive replacement gate](windows-runtime-replacement-main-gate-2026-10-23.md),
run20261023-163606-15675844, target5fbc. Его source/manifest, полный prior proof,
image bytes/File ID/path и terminal prior task сверены до нового submit.

## Что доказано

1. Standard и validation builds скопированы в разные новые каталоги под одним
   native cache lease. Их File IDs, bytes и SHA различаются; actual descriptors
   связаны с одним clean source. Отдельные CLI tests прошли по3 на каждую flavor.
2. Рядом с каждым exe лежит намеренно испорченный local config. Direct CLI и
   managed inspector работают без чтения config/journal; его bytes/File ID
   остаются прежними, в каталоге только exe и исходный config.
3. Report возвращает реальную flavor, backup15/preparation16/replacement17.
   Activation/controllerConfirmation/desktopUpdate остаются null: несуществующие
   execution paths не рекламируются. Journal schema сама не означает поддержку.
4. Managed inspector сверяет exact image/descriptor, actual PID/birth/host/session
   и подтверждённый cleanup. Binding unit test прошёл на Windows; deliberate
   process/build/zero-version substitutions отвергаются.
5. Actual старый updater5fbc не содержит нового marker. Inspector возвращает
   Unsupported до любого artifact/support query этого exe. Он не запускается,
   его image bytes/File ID сохраняются. Unknown-flag fallback отсутствует.
6. Independent verifier принимает actual proof, затем отвергает36 mutations:
   source/flavor/schema/role/image/process/config/cleanup, fabricated authority,
   activation и изменение retained reference. Ready files перечитаны дважды.

Это доказательство **discovery**, не durable candidate approval/drain/activation.
Новая pure production policy добавляется после immutable producer f1e2f10 и
этим native прогоном не подтверждается. Её дальнейшая интеграция описана в
[updater compatibility](updater-compatibility.md).

## Отчёт, окружение и продолжение

Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261023-190500-afa1564e`;
Debian: `/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-190500-afa1564e`.
Local pins/counts: `.winbridge/workflow-validation/20261023-190500-afa1564e/support-verified.json`.
Observer log: `.winbridge/update-preparation/updater-support-native.log`.
Readonly recheck той же terminal задачи:

```bash
python3 scripts/validate-update-support-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261023T190444Z-f1e2f10-d0bafcc6.zip \
  --reference-pin .winbridge/update-preparation/replacement-main-native-verified.json \
  --cache-run 20261017-061527-f4639971 \
  --verify-run 20261023-190500-afa1564e
```

Эта команда не пересылает task. После observation timeout всегда сверять тот же
Task ID; новый submit лишь для новой исправленной isolated попытки.
После native gate review найден и исправлен pre-ACK gap Debian observer:
run/request fsync/atomic save теперь предшествуют submit. Если Task ID не получен,
readonly history/get находит exact frozen payload; отсутствие/неполная история
не вызывает resend. Python7/0 и actual readonly lookup того же d97ea580 прошли;
изменение observer не меняет зафиксированные native producer/proof hashes.

Windows free35,185,926,144→34,444,357,632 bytes; Debian26,390,867,968→26,388,832,256.
Кэш20261017-061527-f4639971 reused, минимум6 GiB соблюдён; очистки не было.
Рабочий installed d052 unchanged, connected/active0, maintenance/recovery errors
отсутствуют, generation8f8d1fb9-e5b1-4984-a17e-d067db46268e прежняя.
Desktop можно закрыть; установленный worker оставить. UI-окон не было.

Следующий блок — versioned durable support admission и gating до production drain,
потом activation/local health/controller confirmation и U3/U4. Public install off;
native query не разрешает применять файлы или продолжать историческую replacement.
