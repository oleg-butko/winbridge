# U1: compiler intent до actor ACK

2026-10-18. Trusted compiler API разделён на metadata admission и исполнение.
`admit_build` проверяет current updater owner, saved Verified source/scope, command
и локальное свободное место. Fresh SQLite insert Intent возвращает одноразовый
BuildPermit. Повтор возвращает только Observed; concurrent insert не выдаёт второй
permit. Record нельзя превратить в permit через deserialize или clone.

Admission не принимает сохранённую worker lease за live permission. Перед процессом
BuildPermit.execute заново проверяет source manifest, directory identity, config,
актуальную worker lease и owner/revision. После callback проверка source и ≥6 GiB
свободного места повторяется перед OS create. Затем действует прежняя последовательность
atomic suspended Job creation → PID/resume receipts → execution → verified cleanup.

Permit можно передать blocking driver после ACK. Если он потерян или owner изменился,
Intent сохраняется; restart/повтор ключа не создаёт новый процесс. Ошибка до execution
также не разрешает переиграть Intent. Readonly inspection и worker guard сохраняются;
следующий coordinated abort driver должен отдельно подтвердить допустимый cleanup.
Синхронный execute_build сохраняет прежние live проверки до admission и использует
тот же permit path. Успешная сборка остаётся отдельной от candidate verification.

31 source/compiler tests passed, в том числе пять новых: admission/one-use grant,
dropped permit/owner restart, source edit после ACK,
offline worker и concurrent admissions. Helper записывает начало user code только
в generated target/compiler-runs.log; проверяется ровно один запуск либо его отсутствие.
Linux/Windows Clippy/tests cross-check и fmt passed. Native rollup впереди, schemas
updater/worker прежние 4/15. Actor compiler command/local build policy ещё не подключены.
Следом они, bounded output read, artifacts/abort, exact shutdown и U2–U3.

Реализация actor/policy и дальнейшая схема 5 описаны в [compiler actor / abort](compiler-actor-abort.md); перечисленная выше schema 4 относится к admission commit.
