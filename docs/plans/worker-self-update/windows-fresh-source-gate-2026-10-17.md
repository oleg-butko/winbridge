# Windows gate U1: fresh SourceControl и отзывчивый journal

2026-10-17. Clean source `3e01e56dbaf3d303daedda70d1058c96d789748c`.
[Контракт](fresh-source-control.md). **82 tests / 13 groups**, полный candidate build
четырёх exe и 21 checksummed artifacts passed. Run `20261017-175317-171ce504`,
parent task `f9018010-5548-4892-a3eb-5ec2516b9f82` succeeded/0.

- Report: `K:\VMs\shared\WinBridge\workflow-validation\20261017-175317-171ce504`.
- Report SHA: `e48db4bb337af5aa0ea42b3d189d34d3161d110e223d14d059fda03853c7239a`.
- ZIP: `winbridge-20261017T215247Z-3e01e56-561f7c37.zip`, 511 files,
  4,589,622 bytes; SHA `c7b0dbba74ac5e44e63ec549203e761e903fc29e455085bdc5151c58488528c4`.
- Source manifest SHA: `a145602cfb89f1a600c866234c4d51aae353e4f56c0d96efe2d7f1d672920a15`.

На настоящей Windows source transaction применила отдельную fixture через
process-authenticated worker Info: fresh PID/birth/build/context/config binding,
Verified/Building и отрицательные проверки изменений tuple/config. Другая проверка
намеренно задержала verifier: настоящий Info IPC ответил до продолжения чтения,
lease осталась Pending без work slot, изменённая authority отклонила итоговый grant.
Свежий запрос после этого завершился нормально.

Прежние source/crash, drain/quiesce, atomic work reservation, supervisor/lost ACK,
busy pipe, legacy contracts и старые binary refusal checks также прошли. Happy path
worker-control использовал actual exported WinBridge source readonly. Schema worker/
updater для проверенного source — 15/3. Автоматического source/build driver пока нет.

Installed clean `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние; final Info healthy/idle.
Рабочие source/runtime/config/journal не заменялись, GUI не запускался.
Desktop можно закрыть, worker оставить. Windows free before/after:
28,010,450,944 / 27,720,368,128 bytes; Debian after verification: 34,723,389,440 bytes.
Original compiler cache `20261017-061527-f4639971` использован под leases;
Windows validation directories не удалялись. Source/status proofs новые.

Блок fresh SourceControl проверен. Следующий — managed compiler attempt/cleanup,
затем source/build orchestration и exact shutdown. U1–U4 остаются открытыми;
ручной bootstrap пока не требуется, A4 отложен.
