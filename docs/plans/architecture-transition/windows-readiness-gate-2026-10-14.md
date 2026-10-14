# A3.2a — Windows readiness foundation gate

Дата: 2026-10-14. [Контракт и ревью](readiness-foundation.md).
**A3.2a завершён; полный A3.2/A3 открыт.** Реализованы inputs/evaluator/store;
producer квитанций, runtime readiness API и новые execution gates ещё впереди.

## Проверенное состояние

Installed Windows worker после обновления пользователя: clean `f128c9a`,
build ID `1791975551027624700`, generation `a1412884-4287-4bb2-a24c-3c2018d39e25`,
incarnation `5a8eb173-4193-4332-b00f-67e3850b2c97`.
Actual MCP task `44f71380-dede-45eb-835a-c80aaf018648` succeeded/exit 0;
repeat key вернул тот же ID, stale scope не передан. Это smoke, не enable.

First candidate `5c8165d`: run `20261014-093621-0b2d5b5e`, task
`689a777d-1445-4554-b2fd-a744e07b5447`, **38 native tests + desktop check** passed.
Ревью после gate исправило schema/policy hashing при serde_json `preserve_order`.
Recursive sorted keys и golden digest проходят в обоих feature configurations.

## Финальный native gate

- Source `b2aeb9f75051e958fd24f704c9396ac635ea623e`, clean archive
  `winbridge-20261014T135448Z-b2aeb9f-7bf64364.zip`, SHA-256
  `09f5c70b8e47620133ba85f90b34db808f827450d0fc3f1e90d261c905600d7f`.
- Shared run `20261014-095509-b06820e8`, task
  `72efda31-bd14-4bef-a7d8-cb1c84d4c8d6`, succeeded/exit 0.
- Four groups: pure readiness 5, protocol/wire 16, actual Info/scope/settings 18;
  **39 native tests**, desktop compile, all groups exit 0.
- Candidate build ID `1791986150506264500`, source manifest SHA-256
  `c047427bffb73441db7e7b5a2ea4f01d0e4b554247e898b6b953e72a23c9d63e`.
- Actual Windows/Unix contract-schema hash совпал:
  `dd86d35b8b9ed59aca0bb438491e906b9c25c69970f0121226fdee99b98e9a12`.
- `policy-proof.json` связывает provider build, loaded policy, schemas и context;
  шесть report artifacts сверены по bytes/SHA. Report SHA-256
  `dfa0574e77d1018f199eae4521cebe51b5b879589237421e6e51754dbf5aa227`.
- UTC 13:55:18–13:56:38. Cache owner `20261014-054619-d5d99c71` reused;
  source/state/report новые. K: free 26,375,323,648 → 26,323,025,920 bytes
  (~24.5 GiB); Windows directories не удалялись.
- Installed worker build/generation/session прежние после обоих gates;
  activeTaskCount 0, maintenance/recovery errors отсутствуют. UI окон не было.

Local baseline: **300 workspace tests** до canonical fix; затем focused
readiness/store/protocol checks, golden schema с jsonschema ± preserve_order,
Linux/Windows-target Clippy и fmt passed. UI build/typecheck, 24 catalog + 12
policy shared Rust/TS fixtures, contracts/scaffold, PowerShell parse и 10 cache
verifier tests passed. Не повторялся полный A2 runtime gate.

Debian cleanup удалил 261 obsolete ELF test binaries: 14,702,763,128 bytes
(~13.7 GiB); две последние сборки каждого test source и используемые файлы
сохранены. Production executables, configs, DB, reports, backups не затронуты.
Receipt `.winbridge/architecture-transition/a32-test-cache-cleanup.json`;
Debian free ~31 GiB. Все новые Cargo вызовы с CARGO_INCREMENTAL=0.

## Следующий пакет и граница

A3.2b требует фактических `projectPolicies`/`contractSchemas` от installed provider.
У `f128c9a` этих inputs нет; не восстанавливать их из disk config, source commit
или результатов другого candidate. [Инструкция обновления](../../validation/architecture-worker-update.md).
Новые поля additive; journal schema 12 и existing A2 admission не меняются.
Worker остаётся по постоянному firewall path. Desktop можно оставить закрытым;
manual UI checklist не требуется. Исторический gate не объявляет installed pair
новой версии pair-tested: её проверит producer A3.2b после обновления.
