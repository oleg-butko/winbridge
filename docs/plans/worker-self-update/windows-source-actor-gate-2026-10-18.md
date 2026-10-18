# Windows gate U1: source preparation actor

2026-10-18. Clean source `fc1965c359c6648e53936d7680537c0987b2f518`.
[Actor](source-actor.md), [source admission](source-admission.md).
**109 tests / 14 groups**, полный candidate build четырёх exe и 25 checksummed
artifacts passed. Run `20261018-043430-4fccfd01`, parent task
`49cc5f0d-2757-4e4e-aa7b-078d32cfab2d` succeeded/0.

- Report: `K:\VMs\shared\WinBridge\workflow-validation\20261018-043430-4fccfd01`.
- Report SHA: `3a1097fe21fc2f470f91972ed927e93294c33f05bf4495e9725b39e658409eb2`.
- ZIP: `winbridge-20261018T083418Z-fc1965c-fcdf088e.zip`, 523 files,
  4,646,139 bytes; SHA `ecf1d5494b8a82fdfce917254b9a4362c1d75d8712333d29f0c34f45639de3fb`.
- Source manifest SHA: `478185beffd9ca92ce497930478dd2581eae3a7b912e4755304f0476502093ca`.
- Candidate worker build ID: `1792312803204708700`.

Отдельный supervisor на Windows принял Preparing, проверил fixture ZIP, получил
реальный worker source grant, применил Cargo.toml/manifest и сохранил Verified/Building.
Mailbox отвечал, пока test worker задерживал control reply; duplicate не запускал
другой driver, Stop при работе отклонялся. SourceHeld/work slot сохранялись после
source success. Explicit fixture cleanup не является production abort/release API.
Missing policy отклонена до intent, повреждённый ZIP не менял исходники и не
повторялся по тому же запросу. `source-preparation-proof.json` фиксирует эти факты.
Это отдельная fixture source; рабочий WinBridge src не применялся.

Три source admission tests также passed: commit-before-dispatch, dropped permit/
owner restart и единственный effect permit при двух concurrent admissions.
Atomic owner-crash test дополнительно подтвердил exact root exit, Empty Job и
отсутствие user code; `atomic-launch-proof.json` содержит rootExited=true.
Прежние compiler/source/crash/supervisor/control/legacy/refusal tests passed.
Updater/worker schema 4/15; config 2/internal source IPC 2 не меняют legacy RPC.

Installed clean `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние; final Info healthy/idle.
Рабочие source/runtime/config/journal не заменялись. UI-окон не было;
desktop можно закрыть, worker оставить работать. Windows free before/after:
25,554,857,984 / 26,192,838,656 bytes; Debian after verification: 33,531,867,136 bytes.
Original compiler cache `20261017-061527-f4639971` переиспользован под leases;
Windows validation directories не удалялись.

Source actor/admission проверены. Полный candidate build по-прежнему выполняется
отдельным build-only сценарием; компиляция через actor, artifact verification,
coordinated abort/release, exact shutdown и U2–U3 остаются следующими блоками.
U1 целиком открыт, bootstrap пока не требуется; A4 отложен.
