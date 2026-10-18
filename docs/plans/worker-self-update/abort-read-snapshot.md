# U1: согласованное readonly чтение отмены

2026-10-18. Full local workspace run выявил sporadic отказ source actor:
`Corrupt: Abort receipt missing`. Операция могла быть сохранена корректно:
inspector делал SELECT header и SELECT receipts вне одной транзакции. Commit
driver между ними давал header revision 0 и history до revision 1, что ошибочно
трактовалось как повреждение.

`inspect_abort` теперь использует readonly SQLite transaction для всех проверок
одного snapshot. Истории schema 5/6/7 читаются совместимо; writer ownership,
actual receipt validation и source/physical guards не ослабляются. Повреждённая
история всё ещё даёт Corrupt. Тот же audit подтвердил, что supervisor/build/source
public inspections уже используют транзакции.

Deterministic regression использует два настоящих WAL connections и выполняет
commit Verified между header и history SELECT. Raw чтение воспроизводит Corrupt;
production snapshot возвращает прежний согласованный Intent; следующий обычный
inspect видит новый Verified. Это проверка реального interleaving, не retry или
игнорирование ошибки. Source actor, на котором найден отказ, после исправления
passed. Native rollup нового блока ещё впереди.
