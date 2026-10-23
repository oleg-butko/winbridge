# Replacement transaction data: Windows gate пройден

2026-10-23. Task `f1e772de-7a28-4ddd-9164-ed3c8a8434f8`, run
`20261023-084849-45c8a2e5`, terminal succeeded/exit0. **12 native tests /2 groups**:
6 per-file exchange tests и6 aggregate replacement transaction tests. Все6 ready
files проверены независимо дважды; counts заново прочитаны из actual test logs.

Отчёт: `K:\VMs\shared\WinBridge\workflow-validation\20261023-084849-45c8a2e5`
(`/mnt/hgfs/shared/WinBridge/workflow-validation/20261023-084849-45c8a2e5`).
`report.json` SHA `eac21af9768d909800f8404f55e3fddb950853f671b80dd51b358f2320c5b3ec`.

Clean producer `7f3a952d920fc80db705b7db084d80fff8461e7b`, archive
`winbridge-20261023T083803Z-7f3a952-5babd8d5.zip`, SHA
`305bf46218ce577ace3010214ba90c9914bfc379e7ea8671c391edf5654d56fc`,
manifest SHA `647f6540e7902e747ca11e94041e5a159718fbc4a45820994626fb8749859688`.
Режим `--runtime-replacement-only` / `-RuntimeReplacementOnly`, generated fixtures,
без old executable reference, desktop/candidate build и UI окон.

Проверена [data-only модель](runtime-replacement.md): exact Prepared snapshot и
distinct attempt, retained-directory intent/receipt до file steps, manifest-last,
все22 causal transitions и запрет skipped CAS/alias/changed receipt. Terminal
uncertainty сохраняет prefix, JSON reopen не даёт новых разрешений. Strict fields,
nested uncertainty и неверные directory/file identities отвергаются.

Это **native execution data tests**, не actual rename и не ownership handoff.
Ledger/fresh permit/consuming custody/IO/actor/activation и U3/U4 ещё не готовы.
Schema14/worker16/supervisor v16/view8 не изменились; public install отключён.
Прежний [Win32 file adapter gate](windows-runtime-file-moves-gate-2026-10-22.md)
сохраняет отдельную область реального one-attempt rename.

Windows available39,674,974,208→39,667,503,104 bytes; Debian при сверке
26,870,960,128 bytes. Approved cache reused последовательно после завершения
preparation faults; никаких Windows directories не удалялось. Installed d052
worker connected/idle/healthy и не менялся. Desktop можно закрыть, worker оставить.

Полные pins/storage/result: `.winbridge/update-preparation/replacement-model-native-verified.json`.
Native/Cargo runs этого блока завершены. Следом durable replacement ledger,
fresh original-custody admission и consuming handoff, затем owned file driver.
