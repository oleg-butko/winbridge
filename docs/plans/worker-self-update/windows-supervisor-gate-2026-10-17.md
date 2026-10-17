# U1: native Windows gate отдельного supervisor

2026-10-17. [Контракт и ограничения](supervisor.md). **Passed: 76 native tests /
12 groups**, actual source ZIP, staging checks и полный build четырёх exe.
Готова библиотека запуска/передачи владения. Production worker adapter, source-control
IPC, shutdown и установка ещё не реализованы; полное самообновление U1–U4 открыто.

## Идентичность и отчёты

- Clean code: `64bb3b59f233ed8466c18d43aeaeb3f65e3a7aca`; основной supervisor commit `128f9eb`,
  отдельный desktop lockfile исправлен в `429e14f`, busy-pipe guard — в `64bb3b5`.
- Run: `20261017-161030-d0a06df8`; worker task `6480fd5d-8e09-4740-8c3d-d1d9bc8874ce`, succeeded / exit 0.
- Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261017-161030-d0a06df8`.
- ZIP: `sources/winbridge-20261017T201022Z-64bb3b5-e5f4df1d.zip`, 4,555,597 bytes,
  SHA-256 `c7be93855fa693c695e67da1a29cfbda7f22f8482649a7c9bfe68cd445783dbf`.
- Manifest SHA-256 `4ceca6efd4f5d7c3d80d7761c6a116e12acc051668d28269c1219155b5f83971`;
  505 files / 8,826,900 source bytes.
- Native test provider build `1792267858661225100`;
  candidate worker build `1792268060282199000`.
  Оба clean и привязаны к тому же commit/manifest.

Все 19 artifacts проверены по bytes/SHA-256 из `report.ready.json`.

| JSON artifact | SHA-256 |
| --- | --- |
| report.json | `52f4000b3b4242dded4a7fbf2ab809f0a6b08bdcd8a6da8467df8620ffb8a335` |
| source-transaction-proof.json | `4efc29e8dcc0b8cd4fe3e5134844dd4c659af0258c53a32de30c61f70eebb215` |
| supervisor-proof.json | `1e62e60502af274656825813cffab5b8c5e6a142818896c31a391f2d8d54c5f6` |
| supervisor-source-proof.json | `896b33d68936ef085d2ad30aa1726a442906caa3956d9de19b980fc5267b5626` |
| update-archive-proof.json | `b69a358f49ef716f9b0c6649e38ae62b30256250092227c9b11221c87cb13f08` |
| update-candidate-proof.json | `5cb74a3d1f83759476de4d32d6897c7cb7a0731bbf7217dbebc8f7900b8fa015` |
| update-coordination-proof.json | `efad21ceca779be65747dc5196c6f119c50825e59fab0a92767e18b75f095a39` |

## Проверки

| Group | Passed tests |
| --- | ---: |
| update-source-policy-and-fsm | 9 |
| update-native-preflight | 46 |
| update-legacy-contracts | 6 |
| update-verifier-build | 0 |
| update-worker-durable-coordination | 5 |
| update-worker-native-coordination | 3 |
| update-previous-worker-refusal | 1 |
| update-platform-no-clobber | 1 |
| update-previous-updater-refusal | 1 |
| update-infrastructure-domain | 2 |
| update-supervisor-staged-source | 1 |
| update-supervisor-ipc | 1 |

В 46 updater tests входят 6 supervisor tests и 12 source transaction tests.
Проверены config/image tamper, неверный peer/nonce, устаревший epoch, ignored
intent/PID/Active writes, падение до spawn receipt/до handoff и после Active,
потерянный ACK без повторного запуска, exclusive owner и атомарный no-effects Stop.
Schema 1/2 мигрируют с consistent backup и fencing; прежний schema-2 updater
отказал на schema 3 без записи. Schema-14 worker также отказал на schema 15.

Native busy-pipe тест удерживает единственный экземпляр, проверяет ожидание
соединения и отсутствие отправленных байтов. Ожидание допускается только до
отправки запроса, в пределах общего 3-second deadline; повтор команды отсутствует.

В crash proof parent PID 14916 завершился, child PID 21332 сохранил Active/owner
epoch 2 после parent epoch 1; birth/host/session и spec совпали в Active/Stopped.
Второй owner отвергнут. Source/runtime effects всё время отключены.

Отдельный smoke над настоящим распакованным WinBridge прошёл Ready → Active → Stop:
505 файлов проверены до/после, manifest не изменился. Подготовка и запуск до Ready
заняли 8.113 s; весь smoke — 14.34 s с проверками исходников. Это измерение данного
прогона, а не гарантия времени запуска при другой нагрузке.

## Граница Job Objects

Launcher/child не входят в указанный execution domain тестового worker.
Попытка запуска из managed domain отвергается; закрытие собственного domain
родителя не завершает supervisor. Breakaway flags не используются.

`externalJobObserved: true`: вся проверка остаётся внутри внешнего Job установленного
worker, который выполняет validation task. Этот gate доказывает исключение из
указанного domain и переживание тестового parent exit. Он не проверяет production
worker shutdown и не доказывает возможность пережить уничтожение произвольного
внешнего Job. Перед production install нужны реальный worker adapter и проверенная
policy окружения; неизвестное окружение должно отложить обновление.

## Candidate build и исправления сценария

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| winbridge-worker.exe | 16,479,232 | `bfe7006ac969727ef4b71bd6e9fa6d8c5229863af47a0f023207e29f6fd26340` |
| winbridge-fixture.exe | 309,760 | `4a9fb36620f027fe094e560cf82ea0690abfb99d3994ea543f3ddcfb461578b9` |
| winbridge-desktop.exe | 13,962,240 | `55806f8a35083a6609739d5d7e63bee5961027fbe1ebeb48d8fbb93679e1ca8c` |
| winbridge-update.exe | 5,620,224 | `f8f27adcd444923bc067fa4d39b15fd9e53e609d5ffd7a5c51295b09cf19ad43` |

Actual ZIP проверен current updater; source hashes сверены до/после сборки.
Native Tauri desktop собран с отдельным locked dependency graph.
`runtimeApplied: false`, `candidateVerified: false`, `windowsOpened: false`:
это build/staging evidence, а не production install/reconnect gate.

Первый запуск `20261017-160448-3fa4781c` отменён агентом после обнаружения
несинхронизированного desktop lockfile. Второй `20261017-160609-ed269fa6` выявил
ERROR_PIPE_BUSY при немедленном соединении после отвергнутого peer. Исправлены
lockfile и bounded connection guard, добавлен отдельный native regression test.
Итоговый gate выполнен заново из clean commit; прежние directories сохранены.

Compiler caches переиспользованы из original owner `20261017-061527-f4639971`,
receipt SHA `22c40d278d595a82038d027184d3d706383d6ebf629fc65b3570fe6baf046bb5`.
Terminal task, checksummed report и exclusive leases проверены. Source/state/proof
новые; результат старых tests не наследуется. Windows directories не удалялись.

Windows K free before/after: 31,665,422,336 /
30,528,774,144 bytes. Debian перед серией native gates
28,938,813,440 bytes; final 28,935,110,656 bytes. Все измерения выше 6 GiB.
Удалены 164 неиспользуемых старых Debian test executables: 10,669,626,464 bytes.
Working controller/main binaries, runtime/config/journals/reports сохранены.
`CARGO_INCREMENTAL=0`; основной target переиспользован.

Installed worker до/после: clean `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc` прежние. Итоговый Info healthy/idle:
activeTaskCount 0, maintenanceError/journalRecoveryRequired/updateCoordination null.
Рабочий worker оставить запущенным; desktop можно закрыть. Обновление установленной
версии сейчас не требуется. Следующий пакет — worker-owned launcher, source-control
IPC и exact shutdown; перед source/build нужен атомарный work/resource reservation.
