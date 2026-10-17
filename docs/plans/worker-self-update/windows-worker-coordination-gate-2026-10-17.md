# U1 worker coordination: automatic Windows gate

2026-10-17. [Реализованная граница](worker-coordination.md),
[общий план](README.md). Clean code `b0d57308a1b935ef3395066c551efca51375bde6`.
**Passed: 52 native tests / 7 groups, полный build/staging четырёх exe, 11 проверенных
report artifacts.** Source apply, shutdown и installed self-update не выполнялись.

## Вход и воспроизводимость

ZIP `winbridge-20261017T142939Z-b0d5730-a09474f1.zip`, SHA
`fb9af35607f3ff28d4b643d3e268cb94241d578db9e17a15f672557a144781ea`.
493 source/UI files, 8,579,375 uncompressed bytes; archive 4,487,740 bytes.
Manifest SHA `8d3dac06a42240cc58289c68e806dd579d3cc0dea7e1cbcd58b2365166753dac`.

Run `20261017-102946-eea3af7d`, parent task
`6804eb6f-dff5-4c24-91b7-72f0d40b4710` succeeded / exit 0.
Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261017-102946-eea3af7d`.
Report SHA `0c358b3047b02e53d2028b2344e62866b3534467919c6a30a5287a120c7cc91a`.
Coordination proof SHA
`8a909f564702b4dde9d9ceedc19ee739c8c6a7bd496f2fd8cf281e13d0eddcdc`.
Candidate proof SHA
`8467f96da884c7488d9ebfd17403e7b4c1dd70e60b31d5185ab6a4632f3f8865`.
Archive proof SHA
`fe294e6f0939612209b21a319d0d625d3ff2644bbd4d8fec037e0c9eb95853cf`.
`report.ready.json` hashes/bytes каждого artifact проверены Debian consumer.

Вызов из Debian:

```bash
python3 scripts/validate-workflow-native.py \
  --controller-config .winbridge/controller-snapshot.local.json \
  --archive /mnt/hgfs/shared/WinBridge/sources/winbridge-20261017T142939Z-b0d5730-a09474f1.zip \
  --update-preparation-only \
  --update-preparation-reuse-run 20261017-061527-f4639971 \
  --update-reference-worker 'K:\_proj-26\winbridge\runtime\winbridge-worker.exe'
```

Только compiler caches переиспользованы под lease из original owner run
`20261017-061527-f4639971`, receipt SHA
`22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`.
Source/state/staging/reports новые; результаты прежних tests не наследовались.
Скопированный reference exe SHA
`f462add07d0d0e2a64cae0da4c055149a4c92d9b394efcad96bb680bb02ce251`;
запуск использовал отдельный тестовый config/journal.

## Что подтвердилось

| Группа | Результат |
| --- | --- |
| update-source-policy-and-fsm | 9 tests: source policy и оба lifecycle FSM, все пять категорий blockers. |
| update-native-preflight | 28 tests: журнал updater, ZIP staging, faults/epochs/receipts и Windows junction guards. |
| update-legacy-contracts | 6 tests: frozen legacy codec и совместимость optional Info. |
| update-verifier-build | Native updater executable собран. |
| update-worker-durable-coordination | 5 tests: concurrent acceptance/CAS, ignored writes, retained receipts, workflow children при drain, physical Unknown и cleanup freeze. |
| update-worker-native-coordination | 3 tests: real managed processes/IPC, config pending/admin guard, startup до migration и lost marker/recovery guard. |
| update-previous-worker-refusal | 1 test: старый exe отказался от schema 15, тестовый DB остался побайтно прежним. |

Actual IPC producer build `1792247444091453100`, generation
`ce4b8dd0-4d58-4727-90f5-f8bd07cb7471`, incarnation
`917ea0e5-ecf2-4ae4-876e-cc24e0156339`.
Все шесть observations привязаны к clean code и manifest выше.
При SourcePending новая source task отклонялась, прежняя завершалась после явной
отмены. При SourceHeld другой проект продолжал task
`ba4b89ba-2a70-4070-b3aa-27786cd2f854` с прежним PID **9352**.
Draining не отменял его; только явный cancel позволил закрыть dispatch.
Quiesced подтверждён Windows execution domain scan. Периодическая reconciliation
во время freeze не создала journal fault. Output/Info и прежний queue receipt читались.

Изменённый source не прошёл Release; после восстановления проверенных байтов Release
сохранил пользовательскую DispatchPaused. Новая task ждала явного user resume.
Прежний release receipt не удалил marker следующего update.
Malformed guard блокировал schema-14 migration до backup; active DB lease блокировала
startup даже после потери marker, inspection и recovery/new-pair не снимали её.

Actual source ZIP и PowerShell create-only/junction guards passed. Candidate worker
build `1792247503961887500` соответствует clean commit/manifest. Worker, fixture,
desktop и updater собраны/staged; `runtimeApplied=false`, `candidateVerified=false`.
Последний flag означает, что полный install/handoff verification ещё впереди.

Production React renderer passed по checksummed `update-coordination-proof.json`:
пять phase labels, source/project scope, offline stale, исчезновение Released metadata,
unknown version/scope и readonly mode. Это headless проверка production renderer на
Windows observations; native tray/popup automation не выполнялась и окон не открывала.

## Рабочая установка и место на диске

Installed worker остался clean `1f3aa9f642871cd3960d2f2b123b9738462ab4b0`, build
`1792228466051801600`, generation `994671b4-e10c-469c-8515-b2aa71a4e3dc`.
Final readonly check: connected, activeTaskCount 0, maintenance/recovery errors нет.
Installed source/runtime/config не заменялись. Обычную validation task записал
сам worker в свой journal; побайтная неизменность рабочего журнала не заявляется.

Windows free before/after: **36,320,370,688 / 34,816,147,456 bytes**.
Debian before/final: **23,415,459,840 / 22,994,698,240 bytes**;
measurements сохранены локально.
Ненужные Windows directories не удалялись; Debian cache cleanup не потребовалась.
Desktop можно закрыть, working worker оставить. Новый bootstrap сейчас не нужен.
U1 целиком открыт: дальше per-file source transaction, независимый supervisor,
local authenticated IPC и exact shutdown/handoff; затем U2–U4.
