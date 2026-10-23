# Durable candidate support: schema16 foundation

Продолжение: [actor18/view10](candidate-support-actor.md) добавлен после этой
foundation; его native Main gate пока pending. Production drain binding не готов.

2026-10-23. Добавлены модель, SQLite ledger и trusted Rust driver проверки
контрактов candidate updater. Это отдельная prerequisite после worker health,
перед будущим production drain. **Actor и обязательный допуск drain ещё не
подключены; public install остаётся выключен.** Supervisor17/view9 и worker16
не изменены. Этот документ дополняет [policy](updater-compatibility.md).

## Что сохраняется

`apps/update/src/candidate_support.rs` задаёт strict serde FSM:
Admitted → Creating → ResumeGranted → Running → Captured → Finished.
Каждая запись содержит immutable intent: installation/scope/update/attempt,
launch/owner/epoch, root revision, source/artifact/health/config digests,
точный updater image/descriptor и worker config. Точный executor и его
`UpdaterDriverContracts` фиксируются при admission. Историческая assessment
пересчитывается по этому снимку, а не по поддержке будущей версии программы.

Результаты: Checked, Unsupported, Rejected, Cancelled, Indeterminate.
Checked означает проверенный report, **не разрешение установить**: blockers
сохраняются отдельно, отсутствующие activation/controller/desktop drivers
по-прежнему запрещают production установку. Validation executor и candidate
также запрещены policy. FSM проверяет causal history, PID/birth/session/host,
resume grant/ACK, cleanup и финальное состояние worker. Неизвестный PID после
создания процесса не считается подтверждённым cleanup.

`apps/update/src/journal/candidate_support.rs` добавляет schema16:
`candidate_support_intents`, `candidate_support_records`,
`candidate_support_history`. Immutable/retention triggers, CAS, history replay
и readback связывают запись с исходными health/artifacts/source. Intent
сохраняется до процесса; root остаётся CandidateVerified с прежней revision.
Существующая запись возвращается как observation без нового permit.
Повторное открытие журнала не создаёт право повторного исполнения.

Fresh `CandidateSupportPermit` нельзя сериализовать или клонировать.
`candidate_support/driver.rs` принимает только этот permit и использует
`artifact::run_probe_observed`: intent до CreateProcess, actual PID/Job до resume,
ограниченный output, timeout, подтверждённый cleanup. Команда фиксирована:
`--runtime-support`, 10 секунд. Отсутствующий marker даёт Unsupported без запуска
неизвестного флага. Точные artifact/config/executor pins удерживаются до receipt;
source lease и health/live worker проверяются до и после query.

Abort не освобождает source при pending/unknown support: добавлены physical
cleanup и transactional barriers. Ошибка записи terminal receipt сохраняет
неопределённость, автоматический replay запрещён.

## Проверки и граница доказательств

Локально: workspace674/0 (16 ignored), пять model и пять SQL tests, policy6,
Linux и Windows all-target Clippy, generated contracts/TypeScript/fmt прошли.
Synthetic15→16 тест проверяет consistent backup, сохранение root и fencing epoch;
ignored writes откатывают receipt целиком. Synthetic тест не доказывает работу
нового драйвера с реальным Windows процессом.

[Windows gate25/0](windows-candidate-support-gate-2026-10-23.md) пройден:
13 ready files twice,36 query +22 migration mutations; actual15→16/refusal.
Readonly перепроверка той же terminal task тоже прошла. Новый journal-bound
driver через supervisor Main этим gate не исполнялся.

Добавлен mode `--candidate-support-migration` в
`scripts/validate-update-support-native.py` / `-CandidateSupportMigration` в
`scripts/test-update-support.ps1`. Он собирает обе updater flavors, выполняет
query tests, новые metadata/policy tests и migration/refusal на **новой
SQLite Backup API копии** actual retained schema15 replacement history.
Original history, manifest и updater image сохраняются. Старый15 намеренно
исполняет только известные artifact/journal команды для проверки refusal16.
Получено25 native tests, семь test groups, девять commands; terminal task и
независимый verifier подтвердили результат.

## Следующий блок

1. Native metadata/query/migration gate закрыт, immutable pins сохранены в
   отчёте выше. Он **не проверяет** исполнение нового journal-bound driver через Main.
2. Добавить новый versioned supervisor action/view (предположительно18/10),
   admission-before-ACK и единственный blocking workslot. Существующие frozen
   actions/views и health contract не менять. Добавить support-active в abort.
3. Подключить production drain через отдельный атомарный binding support→drain;
   frozen v1 DrainIntent молча не расширять. Перед drain заново проверить root,
   owner, source/health/config/images и отсутствие blockers.
4. Native Main gate на schema16 candidate: durable blockers должны появиться
   **до** отправки BeginDrain; old worker остаётся доступен и SourceHeld.
   Проверить duplicate/lost ACK/cold reopen, stale evidence и query/receipt faults.
5. Далее activation из resident replacement guard, local health/controller
   receipts, оставшиеся U2 faults, U3 desktop и U4 public/bootstrap.

Не заявлять whole updater готовым по metadata/migration gate. Установленный
worker d052c6c пока не требует ручного обновления.
