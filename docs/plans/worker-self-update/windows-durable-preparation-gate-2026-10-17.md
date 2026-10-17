# U1 journal/ZIP preparation: native Windows gate

2026-10-17. [Реализованная граница](durable-preparation.md), [общий план](README.md).
**Passed для journal/ZIP/staging; U1 целиком и самообновление остаются открытыми.**
Installed worker не останавливался, рабочие src/exe/config не заменялись; рабочая
БД не открывалась напрямую. Validation task/output сохранял штатный worker.

## Воспроизводимые входы

- Clean code: `93978bc985d47999a2a8c286142becef8061fa18`.
- ZIP: `WinBridge/sources/winbridge-20261017T135009Z-93978bc-e7a0f552.zip`.
- ZIP SHA: `2e45459409a82ea5023d7e8a334ac9508a1262a3ba097fb939d9fa5fbf061c63`;
  4,449,324 bytes, 484 source files / 8,449,779 decompressed bytes.
- Manifest SHA: `fc60f7e05717875b47618effa54bb43b743894c2b81234862571400edb04b2b2`.
- Run: `20261017-095021-2614d81f`; task `975d53b8-3862-49c3-bcfb-3639155c8afa`, succeeded/0.
- Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261017-095021-2614d81f`.
- report.json SHA: `62c90e148a049f565aaa84a8b1103f2fdde0d386a3c0c1f5829ae569e8790071`.
- update-archive-proof.json SHA: `1c39c34db025da3ab33525e145d74b1f56ca58fb95fee2de2fce5fe5cdce0965`.
- update-candidate-proof.json SHA: `7b623c7c86f613ccde61239b9f9328f194714b62e38cbad6a620c2cb8cb25bab`.

`report.ready.json` и каждый artifact повторно проверены по size/SHA из Debian.
Native orchestrator сверил parent task, source identity, candidate flags и cache receipts.

## Результаты

| Проверка | Результат |
| --- | --- |
| Source policy/FSM, включая partial source failure barrier | 8 passed |
| Updater ZIP/CLI/preflight | 7 ZIP + 6 source preflight passed |
| Durable acceptance/CAS/receipts/owner/crash | 15 passed, включая subprocess helper |
| Frozen legacy protocol | 6 passed |
| Native updater build с bundled SQLite | Exit 0 |
| PowerShell create-only payload и generated-output junction guard | Passed |
| Actual exported ZIP: archive/manifest/per-file bytes/SHA, readonly | Passed |
| Full workspace + desktop custom-protocol release build | Exit 0 |
| Source readback, actual worker build identity, четыре payload hashes | Passed |

Итого **42 native tests**, плюс script guards и полный candidate build.
Worker candidate build `1792245084573315400` идентифицирует clean `93978bc` и
указанный manifest. Payload: `...\20261017-095021-2614d81f\update-candidate\payload`.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| winbridge-worker.exe | 16,315,904 | `f0114b9de60d88849043c438f7617d45917c34a19453faa38636c979f755fefe` |
| winbridge-fixture.exe | 309,760 | `182e5acd6047b30f9fcaed50b121d83ce5dec36a96caa11a6b1f106267c2f0f2` |
| winbridge-desktop.exe | 14,054,912 | `49d18ed47d0a6987fe579897b91c199dd6558c1edf1905bbfc9f9125386f2f17` |
| winbridge-update.exe | 4,227,072 | `8486ff7810878559e276732c62823cfbe98bc3e519470308235670e4c0b45e07` |

`runtimeApplied=false`, `candidateVerified=false`, `windowsOpened=false`.
Build/staging не подтверждает installed startup, migration, IPC handoff или controller
confirmation. Эти gates появятся при реализации supervisor/maintenance/apply.

## Ревью и локальные проверки

До native gate исправлены:

- Strict event decoding: internally tagged unit event пропускал extra fields;
  adjacent tag/content и regression теперь отклоняют их.
- SourceApplying failure не освобождает installation до reconciliation.
- Windows fs2 contention распознаётся по native lock error, не только WouldBlock.
  Lock handle запрещает удаление на Windows; callbacks проверяют file identity.
- Required SQL writes проверяют affected rows и actual record/event readback до ACK.
  Abort, RAISE(IGNORE), corrupted event и CAS conflict откатывают всю transaction.
- Native test compiler cache тоже переиспользуется под lease, без наследования state/proof.

Локально passed: **382 full workspace tests**; после final journal readback refinement
повторены **28 updater tests**, Clippy и Windows updater cross-check. Core update 8,
schema/TS-feature Clippy, generated contracts/check, TypeScript/build:ui/scaffold,
8 source export tests, Python compile и PowerShell parser/stage checks также passed.
UI behavior не менялось; native UI observation этого пакета не требуется.

## Сохранение рабочего runtime и места

Installed до/после: clean `1f3aa9f642871cd3960d2f2b123b9738462ab4b0`,
build `1792228466051801600`, generation `994671b4-e10c-469c-8515-b2aa71a4e3dc`.
Readonly health: activeTaskCount 0, journalRecoveryRequired/maintenanceError отсутствуют.
Installed exe/config не копировались; updater не открывал worker journal.

Все compiler caches происходят от terminal run `20261017-061527-f4639971`,
четыре native groups которого passed; его поздний staging failure не используется
как test evidence. Receipt SHA
`22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`.
Test target, workspace target и desktop target переиспользованы под leases; source,
temporary test state, staging и reports текущего прогона новые.

Windows free before/after: **38,396,719,104 / 37,591,474,176 bytes**.
Debian initial/final: **27,204,505,600 / 23,421,390,848 bytes**; incremental выключен,
основной target переиспользовался. Порог 6 GiB соблюдён; cleanup не понадобился.
Windows validation directories, reports, configs и пользовательские файлы не удалялись.

Desktop можно закрыть; установленный worker оставить работающим. Ручное обновление
сейчас не нужно. Далее — source lease/per-file transaction, infrastructure launch,
maintenance/drain/quiesce/startup guard и idempotent IPC receipts в двух журналах.
U1–U4 открыты; remote self-update capability не объявлять до gates/bootstrap.
