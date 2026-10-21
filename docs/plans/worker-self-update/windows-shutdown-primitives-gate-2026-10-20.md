# Windows: примитивы подтверждённой остановки

2026-10-20 America/New_York (UTC run 20261021-034430-116fd4ff).
Task `3970a744-a5ef-4ecc-97a3-3b8a56ce9075`: succeeded, exit 0.

| Группа | Тестов | Что подтверждено |
| --- | ---: | --- |
| shutdown-process-pin | 1 | Exact live birth/host/session, retained process handle, выход собственного test child. |
| shutdown-data-state-journal | 12 | Типизированные intent/ACK/exit data, CAS/history/reopen, rollback при отказе записи, Windows file pins и instance lock. |
| shutdown-worker-acceptance | 4 | Commit-before-permit, immutable original receipt, duplicate observation, backup/migration и отказ active старой lease. |
| shutdown-worker15-refuses16 | 1 | Реальный предыдущий worker отказывается от нового журнала без изменения bytes. |
| shutdown-updater11-refuses12 | 1 | Реальный предыдущий updater отказывается от нового журнала без изменения bytes. |

Все 19 native tests прошли. В девяти файлах `report.ready.json` независимо
проверены размер и SHA-256, затем проверка повторена. Report SHA-256:
`e176d5d90b61dc24cfdfc50e475e04fd8d3a15dce1ff7c8c79a00adfcc19e92f`.
Shared: `WinBridge/workflow-validation/20261021-034430-116fd4ff`.

Проверен чистый source `c704a881ed163c0856bd1a2683ec1156731b8e2d`,
`winbridge-20261021T034421Z-c704a88-9f51aba2.zip`, archive SHA-256
`8178f7f124ee5e8824216a8f1d206d9d12453de0426c8739111096a544916160`.
Участвуют source schemas worker16/updater12. Использованы заранее сохранённые
proven schema15/11 reference binaries из C3b, hashes/query descriptors проверены
ещё раз. В scope не входит full positive migration joined health/drain из11.

Сценарии: `scripts/test-update-shutdown-primitives.ps1` и
`scripts/validate-update-shutdown-primitives-native.py`. Использован прежний
test-build-cache `20261017-061527-f4639971`, свежие source/state/temp/report. Свободно
43,198,169,088 → 42,924,994,560 bytes Windows; перед запуском проверен порог 6 GiB.
Candidate/desktop не собирались, окон нет, runtime не устанавливался. Test child
останавливался только в собственном process test. Installed worker `d052c6c`,
generation `78e9bbe1-8cb9-4ad3-93cf-371e981801ad`, остался здоровым и idle после gate.

Это проверка примитивов. Synthetic ledger tests не доказывают полную остановку
actual worker Main через independent supervisor. Новый stop control, ACK retirement,
cold receipt verifier, combined gate и последующие replacement/activation/rollback
ещё нужны. U1–U4 и public auto-update не закрыты.
