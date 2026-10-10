# Ревью этапа 6 — 2026-10-10

После перезапуска Debian прочитаны актуальные контракты full/delta, publisher,
local base reuse, staging/publication, worker import/recovery и desktop output.
Git worktree был чистым. Controller запущен; actual Windows Info подтвердил clean
`af90ec0`, WorkspaceDelta, здоровый журнал и отсутствие активных задач.

В delta новых важных ошибок не обнаружено: reused files проверяются по actual
SHA-256/size, target остаётся отдельным поколением, новый tree проверяется полностью,
ready/schema согласованы, cancellation проходит через checkpoints. Повтор готового
target не зависит от сохранности shared/base. Regression tests выполняются вместе
с workspace suite; уже пройденный delta native gate не повторяется.

В output найдены два существенных недостатка:

- Desktop удерживал весь output в массивах чисел, заново декодировал его на каждом
  poll и только затем обрезал отображение. Исправлено incremental UTF-8 decoding и
  ограниченным Unicode tail; cursor/loaded bytes сохраняются отдельно от текста.
- Просмотр и загрузка отчётов не проверяли offsets между фрагментами. Пропуск мог
  склеить разделённые bytes в другой Unicode символ. Теперь gaps обозначаются
  явно и сбрасывают decoder; malformed page не меняет cursor, а отчёт с заявленным
  полным output и gaps отвергается до render.
- Frozen output оставался виден после retention cleanup. При новом
  outputRemovedAtMs view теперь пересоздаётся вместе с filter/pause state.

Продолжение этапа 6 — [пакетное чтение и виртуализация output](output-batching-plan.md).
ReadOutput DTO/sequence semantics сохраняются; byte/JSON/row budgets ограничивают
страницу вместо прежних 16 rows. Отдельная Windows проверка сравнивает старый и
новый worker в новых тестовых каталогах. Working runtime не обновляется сценарием.
Resume snapshot delivery остаётся незавершённым.

135 workspace tests, Clippy с `-D warnings`, Windows all-targets compilation,
TypeScript и unit checks Unicode buffer прошли. Browser integration также прошла:
actual 20 000 Unicode lines на поток, lost read ACK с прежним cursor, bounded tails,
не более 28 DOM rows, filter/pause. [Native gate завершён](windows-output-gate-2026-10-10.md):
11 tests, три пары reference/current, same bytes, pages 17→11, restart/dedup/control.
Дополнительная browser проверка frozen cleanup и malformed complete report
прошла после заключительного UI fix. Cross-compile не обозначается Windows run.

## Продолжение ревью — pinned chunk/resume

Новая schema 3 закрепляет все block hashes в manifest pin; непроверенные/частичные
блоки остаются в staging. Каждый cache read повторно проверяет actual bytes;
whole ZIP и file/base hashes проверяются до immutable publication. Отмена/timeout
во время cache verification сохраняют свою точную причину. Во время ревью устранены
два риска: journal events могли разрастаться при смене фаз каждого блока, а
chunk index ошибочно попадал бы в счётчик source files. История фиксирует первое
появление каждой фазы, payload progress не выдаётся за число source files.

143 workspace tests прошли; final Clippy/Windows all-targets/DTO/TypeScript/build
и browser integration проходят. В новой UI проверке исправлено устаревшее
ожидание теста: после выбора resume package сохранён именно новый выбранный ID.
Это ошибка тестового ожидания, восстановление формы работает корректно.
Actual native Windows resume gate ещё ожидается. Никакие рабочие exe/state
или прежние shared packages/reports не менялись; исходный Git tree был чистым.
