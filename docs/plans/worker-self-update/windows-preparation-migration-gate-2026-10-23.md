# Actual updater13 →14 и old13 refusal: gate пройден

Отчёт зафиксирован2026-10-23; run `20261023-020641-5eca976a` выполнялся ещё
2026-10-22 America/New_York. Task `4c252e55-f681-4f8f-87d2-05a18e410d28`,
succeeded/exit0. **1 native integration test**,6 ready files сверены независимо
дважды,19 искажений actual proof отклонены.

Report directory: `K:\VMs\shared\WinBridge\workflow-validation\20261023-020641-5eca976a`
(`/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-020641-5eca976a`).

| Файл | SHA-256 |
| --- | --- |
| report.json | `91d31fd34d8c6bf422478e3c27691d92d49147d50b23d88260e05fdafa6f2ca4` |
| runtime-preparation-migration-proof.json | `b0170b450836050de75af84a51242a90da3522000265112ac4aa8b44c3c4529f` |

Producer clean `6286ad947afecfeeee56f2d38f334fd0af544320`, архив
`winbridge-20261023T020618Z-6286ad9-f5f0ef36.zip`, SHA
`d4419f7be87151ac2fb32281b126a2c43b94829f25f382a0d5dcebeadd11e6f6`,
manifest SHA `d76e77a64b8cef4e9c1a446aa407818998a89dd56a84cef80574efd56059c000`.

Readonly actual13 reference: run20261021-155003-a33e1e12/source8bd7629,
proof SHA `27d84e4ff254f371292c4a8e5be600da7c16022601e44427d5cb32d606fc70cb`.
Exact manifest SHA `9e1ffee9240e7472f9bce74bd2e251ab0a8f01e9a598a8220dab8966b6cdecb7`
связан с actual updater SHA
`f2af5e6de9e97e4e9a53bd75ba5501c5d5bc5c525325faaa6f853330ac18143a`.
Исходный updater schema13 проверен по Git и реальному descriptor/known manifest.

В новую local directory через SQLite backup API consistent-cloned реальный
shutdown/Verified-backup13 graph. Migration13→14 сохранила все старые raw SQL rows,
включая backup intents/current/history и typed shutdown/backup. Installation/scope
сохранились, migration epoch2→3, backup13 имеет исходные rows/epoch и integrity ok.
Все14 preparation tables пусты. Новый owner получил исторический backup только
как Existing observation; fresh preparation без original guard не допущена.

Actual old13 executable запущен с checked config только новой изолированной БД14:
`journal init/status` отказали UnsupportedSchema, bytes DB/nonempty WAL сохранились.
Оригинальный reference всё время readonly; его DB/WAL/info не изменились.
Verifier независимо проверил exact raw manifest bytes/SHA, executable/source и
все schema/epoch/preservation/no-permit/refusal связи.19 negative mutations включают
подмену raw manifest/updater SHA, schemas/epoch и ложные preservation/custody flags.

Windows available41,419,579,392→41,235,996,672 bytes, Debian после проверки
27,126,972,416 bytes. Cache20261017-061527-f4639971 reused, directories не удалялись.
Desktop/candidate build и UI окна не нужны. Installed d052 worker unchanged/healthy,
оставить его работать; desktop можно открывать/закрывать. Local Windows all-target
Clippy/fmt/Python compile/scaffold passed.

Это закрывает **actual13→14 migration/refusal**, но не preparation fault matrix,
replacement, activation или U3/U4. [Сценарий и повтор](preparation-schema-migration.md).
Полный public proof/pins: `.winbridge/update-preparation/preparation-migration-native-verified.json`.
