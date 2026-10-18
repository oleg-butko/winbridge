# Windows gate U1: managed compiler

2026-10-18. Clean source `8990bd7f056f061615345dd6e3d17ae5da07154e`.
[Контракт](compiler-driver.md). **95 tests / 13 groups**, полный candidate build
четырёх exe и 22 checksummed artifacts passed. Run `20261018-034803-ef13a3c8`,
parent task `5d1982b9-6537-44d9-824f-d0ec70e92523` succeeded/0.

- Report: `K:\VMs\shared\WinBridge\workflow-validation\20261018-034803-ef13a3c8`.
- Report SHA: `afec9321ac09b0869cf38620d9e71613025ad13cdd3cd4be09f318e0f8e10e63`.
- ZIP: `winbridge-20261018T074739Z-8990bd7-f9cd8e99.zip`, 516 files,
  4,614,730 bytes; SHA `02513f185fda575c7e6d1eb21a0de814177e806a8db10f3e08939e474747bf6b`.
- Source manifest SHA: `ffb010559fddc7af96a0a54a229ac5825975c2f478dd90c9b86194ac6ce70586`.
- Candidate worker build ID: `1792310049631113000`.

На настоящей Windows проверены intent/PID/Job/resume receipts, success/dedup,
nonzero exit, timeout дерева, независимые stdout/stderr и квота, ignored journal
writes, изменения source/config, owner restart и corruption receipt history.
Owner-crash test оставил долговечный Running record и source guard; повтор не
запустил новый процесс. `update-build-proof.json` подтверждает Completed/Succeeded,
exit 0, verified cleanup и сохранённую worker lease. Это проверка compiler fixture;
полная сборка candidate отдельно выполнена build-only сценарием. Ни один из этих
результатов не является разрешением установить candidate.

Updater schema 4; worker schema 15. Старый schema-3 updater отказался открыть schema 4
без записи. Прежние source/control/supervisor/legacy checks также passed.

Installed clean `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние; final Info healthy/idle.
Рабочие source/runtime/config/journal не заменялись. UI-окон не было;
desktop можно закрыть, worker оставить работать. Windows free before/after:
29,884,207,104 / 27,965,808,640 bytes; Debian after verification: 36,538,798,080 bytes.
Original compiler cache `20261017-061527-f4639971` переиспользован под leases;
Windows validation directories не удалялись.

Блок compiler проверен. До production pipeline отдельно закрывается crash gap между
созданием suspended процесса и назначением Job, затем source/build orchestration,
artifact verification, coordinated abort/release и exact shutdown. U1 целиком открыт,
U2–U4 впереди, ручной bootstrap пока не требуется. A4 отложен.
