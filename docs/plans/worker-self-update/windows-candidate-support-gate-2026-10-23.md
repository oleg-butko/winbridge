# Candidate support schema16: Windows gate пройден

2026-10-23. Task **f88c0185-6665-424c-8e07-92d0867612ad**, run **20261023-224925-0eae733c**:
terminal **succeeded / exit0**. **25 native tests /7 test groups /9 commands**,
13 ready files проверены дважды; независимый verifier отверг **36 query +
22 migration mutations**. Readonly verify-run повторно проверил ту же terminal
task без submit. Installed runtime не менялся, GUI-окон не было.

## Проверенная версия

| Поле | Значение |
| --- | --- |
| Producer | 7186bb652a6148b2866dc966a055f041de2d67a9 |
| Source ZIP | winbridge-20261023T224917Z-7186bb6-59eeb744.zip |
| ZIP SHA256 | 94ed85726985c32ee6678e9bc1d1c013c4f9813df2be6fd7206a7642cf695a49 |
| Source manifest SHA256 | e4740f078caf2743768e78c90246df3b5bdb308fd69195821ae03ae000ff3484 |
| report.json SHA256 | f4218decd2ee2bcdeaea7664001fdae8560080aa99e99f6e550bc64f6930eae3 |
| updater-support-proof.json SHA256 | 48b499a9ebf467e04ffc84526034ea0efd5d65f8a15d60d13ec1a56ed97d5ee5 |
| candidate-support-migration-proof.json SHA256 | 92739281417b9238feea8ef789be490e60bf27237ca83c813b062a7a3feeb6d1 |
| Reference proof SHA256 | e3511ebfee87f81fcd9a60f7dbd7affdca3d83c3297fd6a5c46d1f02b929dec5 |
| Reference replacement canonical SHA256 | 500d61a64c1efd33a80212560f5bbdda284609832643c31ad39abd01704e38ba |

Новый updater journal16; worker journal16/supervisor17/view9 не менялись.
Reference взят из actual positive replacement15 run20261023-163606-15675844,
update f0c6e634-1376-44cc-88ea-0e516143399e, installed target5fbc. Exact reference
image/manifest/File ID/descriptor и старый terminal task сверены до нового submit.

## Что выполнено и доказано

| Test group | Passed |
| --- | --- |
| Standard updater CLI | 3 |
| Validation updater CLI | 3 |
| Strict support binding | 1 |
| Actual support queries / old marker negative | 1 |
| Candidate support model / SQL ledger | 10 |
| Frozen-context compatibility policy | 6 |
| Actual retained15→16 migration / old15 refusal | 1 |

Обе flavors собраны и скопированы в разные isolated image directories под одной
cache lease. Broken adjacent config игнорируется query, image/PID/Job cleanup
binding проверен. Query возвращает schema16 и настоящие versions drivers,
отсутствующие activation/controller/desktop contracts остаются null.
Marker-negative старый updater не запускается **в discovery сценарии**.

Migration integration отдельно запускает старые известные artifact/journal
команды. SQLite Backup API создаёт новую локальную копию actual15 history;
original DB/nonempty WAL/image/manifest не меняются. После открытия новым16:

- Installation/scope/update/lastSequence сохранены; epoch **2→3**, schema **15→16**.
- Actual FilesMoved22 replacement, shutdown, backup и preparation истории прежние.
- Digest всех прежних таблиц (кроме installation) до/после/в migration backup:
  8534ba1bd7e78b1e4360a220be4220e7e478f888ab80f524e5b9cc8e6112a630.
- Migration backup проходит integrity_check и содержит прежние schema15/epoch2.
- Три новые candidate_support tables пусты. Сохранённый старый supervisor/lease
  не создаёт fresh admission; replacement view physicalCustodyHeld=false.
- Actual old15 journal init и status отвергают16 с UnsupportedSchema; DB/nonempty
  WAL новой копии не меняются. Cold history не становится physical permission.

Verifier проверяет source, terminal task, ordered commands, test selection,
ready files, full proof bindings, затем искажения actual proofs в памяти.
runtimeApplied=false, installedRuntimeApplied=false, windowsOpened=false.

## Граница доказательства и продолжение

Это actual Windows **query/model/ledger/policy/migration** gate. Он не исполняет
новый journal-bound candidate-support driver через supervisor Main и не доказывает
обязательный pre-drain enforcement. Новый actor route пока отсутствует; production
install выключен. Не объявлять whole updater готовым по этому результату.

Следом новый versioned actor/action/view, обязательный support→drain binding и
actual Main blocked-before-drain gate. Затем authorized activation из resident
replacement guard, local health/controller receipts, remaining U2 faults, U3/U4.
[Полная инструкция следующей сессии](resume-2026-10-23-candidate-support.md).

## Сохранённые результаты и окружение

Shared: /mnt/hgfs/shared/WinBridge/workflow-validation/20261023-224925-0eae733c
(Windows mapping описан в handoff).
Local pins: .winbridge/workflow-validation/20261023-224925-0eae733c/support-verified.json.
Observer: .winbridge/update-preparation/candidate-support-native.log.
Readonly повтор: candidate-support-native-readonly-recheck.log там же.
Команда recheck в [handoff](resume-2026-10-23-candidate-support.md).

Disk bytes: Windows **33080188928→32755130368**;
Debian **25611313152→25609728000**.
Минимум6 GiB соблюдён, cache20261017-061527-f4639971 переиспользован,
очистки Debian/Windows не было.

Installed d052 остался connected/active0, generation8f8d1fb9 прежняя,
maintenance/recovery errors=null, coverage complete. Worker оставить работать;
desktop можно закрыть. Active native/Cargo runs после закрытия нет.
