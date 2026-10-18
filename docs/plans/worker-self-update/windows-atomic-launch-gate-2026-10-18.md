# Windows gate U1: атомарное назначение compiler Job

2026-10-18. Clean source `11566cbc691afa99ac64bd91d7fc2a72d32289ad`.
[Контракт](atomic-executable.md). **102 tests / 14 groups**, полный candidate build
четырёх exe и 24 checksummed artifacts passed. Run `20261018-040345-92ee23bc`,
parent task `81f6284b-339e-45ef-a4eb-2fe478ffd334` succeeded/0.

- Report: `K:\VMs\shared\WinBridge\workflow-validation\20261018-040345-92ee23bc`.
- Report SHA: `9116f547607ab540d83aece91620cdac33f6f9a8fdce07c3ca81bfefb40ee334`.
- ZIP: `winbridge-20261018T080334Z-11566cb-ab3623fb.zip`, 519 files,
  4,628,460 bytes; SHA `7815199d8ff0960e46a5effdb43d74fa9ea9799d948f039003078cefee885ea4`.
- Source manifest SHA: `c027c69cd3a23ace3f23a4069ea1fd90f58c44fd72860f7005b55bdeac13486b`.
- Candidate worker build ID: `1792310937230250000`.

Семь новых Windows tests проверили argv/Unicode/stdio, Job membership до resume,
закрытие до resume, ordered domain/private Jobs, отклонение UNC/некорректных команд,
дополнительный suspend count и explicit handle inheritance. Crash владельца сразу
после CreateProcess до PID receipt оставил пустой Job и не выполнил user code.
Следующий [source admission](source-admission.md) rollup дополнительно проверяет
exact root exit в этом crash test; это усиление ещё не входит в данный отчёт.

Прежние compiler/source/supervisor/control/legacy tests passed. Updater/worker
schema 4/15. Полный candidate build выполнен отдельно build-only сценарием:
ни он, ни compiler fixture proof не разрешают установку runtime.

Installed clean `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние; final Info healthy/idle.
Рабочие source/runtime/config/journal не заменялись. UI-окон не было;
desktop можно закрыть, worker оставить работать. Windows free before/after:
26,389,467,136 / 26,163,970,048 bytes; Debian after verification: 36,498,391,040 bytes.
Original compiler cache `20261017-061527-f4639971` переиспользован под leases;
Windows validation directories не удалялись.

Блок atomic compiler launch проверен. Следом source admission/actor orchestration,
artifact verification, coordinated abort/release и exact shutdown, затем U2–U3.
U1 целиком открыт, bootstrap пока не требуется; A4 отложен.
