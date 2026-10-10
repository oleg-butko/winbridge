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

Продолжение этапа 6 — [пакетное чтение и виртуализация output](output-batching-plan.md).
ReadOutput DTO/sequence semantics сохраняются; byte/JSON/row budgets ограничивают
страницу вместо прежних 16 rows. Отдельная Windows проверка сравнивает старый и
новый worker в новых тестовых каталогах. Working runtime не обновляется сценарием.
Resume snapshot delivery остаётся незавершённым.

135 workspace tests, Clippy с `-D warnings`, Windows all-targets compilation,
TypeScript и unit checks Unicode buffer прошли. Browser integration также прошла:
actual 20 000 Unicode lines на поток, lost read ACK с прежним cursor, bounded tails,
не более 28 DOM rows, filter/pause. Native gate публикуется после завершения;
cross-compile не обозначается Windows run.
