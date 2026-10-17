# Windows gate U1: worker-owned supervisor/control

2026-10-17. Clean source `c7052999a3115fdeb24c8f8b45f008f01ff644c3`.
[Контракт](worker-control.md). **80 tests / 13 groups**, полный candidate build
четырёх exe и 21 checksummed artifacts passed. Run `20261017-174249-58eff6c0`,
parent task `6dbfdff2-a789-43f6-bb90-06f1da3c75bd` succeeded/0.

- Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261017-174249-58eff6c0`.
- Report SHA-256: `58484ae7fc24a2d78f743aac90e48341759cd69df4ef0a59207566c74defdfda`.
- ZIP: `winbridge-20261017T214239Z-c705299-ab7e90d2.zip`, 508 files, 4,580,099 bytes;
  SHA `f9330854feaeafbcf9550384f5d3c85337ef4af9069a864c344fa86af9e0d6d3`.
- Source manifest SHA: `2476f21dd314bebfe4a6298659ff2a40f473378149a4d7225552c236e127d11c`.

## Доказательства

Actual Windows worker library запустила detached supervisor из своего execution
domain, сохранила exact build/config/context/PID binding и получила Active handoff.
Production external-environment/fixed-path guard отклонил тестовое окружение;
private fixture не обходила внешний validation Job. Полное production shutdown
этим прогоном не доказано.

Реальный child через отдельный named pipe получил Pending → Held → Released.
SourceHeld атомарно занял единственный work slot: другая задача осталась queued
без PID; повтор Acquire вернул исходный receipt без второго slot. После release
slot освободился. Actual source manifest до/после одинаковый. Launch: 8,551 ms,
source grant: 2,876 ms. Отдельный staged-source smoke достиг Ready за 8,746 ms.

Forged peer, stale epoch/generation/build/config/incarnation/launch, config byte
changes, ignored receipt write и преждевременный drain отклонены. Stop при Held
не разрешён; после verified release supervisor завершился. Прежние crash/source,
worker drain/quiesce, migration/refusal, busy pipe и legacy wire tests прошли.
Текущие worker/updater schema — 15/3.

## Сохранение среды

Установленный clean worker `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние; final Info healthy/idle.
Рабочие source/runtime/config/journal не заменялись, GUI не запускался.
Desktop можно закрыть, установленный worker оставить работать.

Windows free before/after: 28,252,860,416 / 28,264,001,536 bytes.
Debian after checks: 34,772,668,416 bytes. Удалены только 348 устаревших Debian
ELF test executables: 12,165,789,064 bytes. Main/running binaries и данные сохранены;
Windows validation directories не удалялись. Original compiler cache owner
`20261017-061527-f4639971` переиспользован под leases; state/proofs новые.

Блок worker launcher/lease IPC проверен. U1 целиком остаётся открытым:
fresh SourceControl/file driver, compiler ownership, exact shutdown; затем U2–U3
и один ручной bootstrap. Публичное включение, source apply через supervisor и
runtime install пока отсутствуют. A4 отложен.
