# Replacement metadata schema15: Windows gate пройден

2026-10-23. Task `e292eb2f-33b6-4d7f-b281-a954b5f24d63`, run
`20261023-091331-b6f35ac8`, terminal succeeded/exit0. **11 native tests /2 groups**:
6 aggregate model +5 SQL ledger/migration tests. Все6 ready files проверены
независимо дважды; counts повторно прочитаны из actual test logs.

Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261023-091331-b6f35ac8`
(`/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-091331-b6f35ac8`).
`report.json` SHA `8df8a648f16d0dd24d7e796c9eaa7be381236a0e45ff4390f389b25cdf49dddb`.

Clean producer `5fbc1cf2e37086dad8e65667ed3af31b1a16d9f7`, archive
`winbridge-20261023T091226Z-5fbc1cf-b4ac44a5.zip`, SHA
`3891ad4689779c2eb2507b70be4ad5d2f599cf4816fb4b882d64f2de0006c6fb`,
manifest SHA `98e52e10e99e25fbc8cf9864b4af3e4d83419e8a1297cbcd330dc61e5c4546f3`.
Режим `--runtime-replacement-ledger-only` / `-RuntimeReplacementLedgerOnly`;
updater15/worker16, supervisor v16/view8 не менялся.

## Проверенный объём

SQL tests проверяют все22 causal steps, stale/duplicate CAS, immutable terminal
rows/history, rollback coupled admission/current/history writes при RAISE(IGNORE),
reopen issued rename с сохранением uncertainty и tampered history. Эти file receipts
synthetic; SQL test не выполняет rename и не конструирует OS guard.

Migration case consistent-backup/open14→15 сохраняет настоящий root/update record,
проверяет backup14 integrity и прежний epoch, увеличение current epoch и пустые
replacement tables. Это generated14 fixture: **в нём нет реального Prepared graph
и не запускается старый14 executable**. Для них нужен следующий самостоятельный
actual retained14→15/refusal gate. Положительный fresh admission с оригинальным
kernel guard и consuming handoff также ещё не доказаны actual pipeline.

[Trusted admission code](runtime-replacement-ledger.md) удерживает pins через commit,
выдаёт non-Serde permit только fresh caller и позволяет лишь отметить uncertainty.
Raw rename-success API/actor/IO отсутствуют, public install отключён. Metadata gate
не подтверждает ни FileSystem effects, ни весь U2.2/updater. Старые schema14 gates
сохраняют свою область и не считаются проверкой15.

До export прошли package216/0 (6 ignored), workspace647/0 (16 ignored), обе
all-target Clippy с update-validation, Rust fmt, Python7/0/py_compile и scaffold240.
Review добавил full worker-config identity и pinned candidate payload directory;
source/previous/prepared files и exact directory set проверяются перед admission.

Windows available37,968,691,200→37,962,076,160 bytes; Debian при независимой сверке
26,806,845,440 bytes. Approved cache reused, Windows directories не удалялись.
Desktop/candidate build и тестовых UI окон не было. Installed d052 worker connected,
active0, maintenance/recovery errors отсутствуют; runtime/config/journal не менялись.
Desktop можно закрыть, worker оставить; ручное обновление сейчас не требуется.

Полные pins/storage/result:
`.winbridge/update-preparation/replacement-ledger-native-verified.json`.
Дальше actual14→15/refusal с прежним Prepared history, consuming original custody,
trusted rename callbacks/IO/actor, positive admission gate и activation/U3/U4.
