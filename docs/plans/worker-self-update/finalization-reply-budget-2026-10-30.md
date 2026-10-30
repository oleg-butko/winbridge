# Ответ V8 после обычного IPC deadline

2026-10-30, .204/main. Source worker19/updater23. Продолжение
[original factory/Main30](finalization-factory-2026-10-30.md). Полный positive
Main30 ещё не подтверждён; permanent keeper и public installation не готовы.

## Второй Main30

Run20261030-051739-2b5177f8/taskbb0fc4f8-c4fe-43ac-a587-7786c5e823a3,
source baf8e3f → targete5cdceb завершился failed/exit1.68 ready files дважды
проверены: report SHA c4e93fc694485c7b63e9c53acba9be06984c6099345f1d0ae7e6dc1d2f25f306.
Known owned Job закрыт; runtime архивирован в case runtime-archive-2f472def-
3896-4f8b-ab5f-045c76c4d052. Повторять этот task/key или принимать его за green нельзя.

Исправление capture сработало: после V7 осталось7500ms вместо2399ms; root decision
known committed при оставшихся2737ms из ORIGINAL10s. Sender затем потерял ответ
по обычному10s IPC timeout, independently прочитал Active и вывел Confirmed,
custody=true/pinsRetired=true/attempted=true. Это partial evidence из настоящего
source, но финальный proof не сформирован: harness сделал unwrap отсутствующего
4-byte reply header. Config atomic replacement и task/dedup ещё не выполнялись.
Не выдавать этот результат за полный independently audited physical V8 proof.

Installed source48/gen946 прежний, active0/errors=null. K free17,647,632,384 before /
16,432,107,520 after. Новых окон не было; рабочий worker оставить запущенным.

## Изменение

Общий server timeout10s мог закрыть reply раньше специально прерываемого header.
Введён отдельный bounded V8 reply wait30s. Чтение входного кадра ограничено прежними
10s; остальные IPC requests используют ORIGINAL absolute deadline, а не новые10s
после чтения. V8 handler проверяет envelope/OS peer и bounded admission до ожидания
исходного координатора; malformed/foreign/busy requests не получают accepted owner.
Потеря клиента по-прежнему не отменяет принятое и не повторяет SQL.

Normal parent sender сохраняет свой обычный10s observation timeout и recovery
через independent Active. Только validation fault hook ждёт header до30s, закрывает
тот же единственный V8 stream после4 bytes и проверяет результат новым readonly
соединением. Startup240s и original V7 freshness10s не менялись. Ошибка reply теперь
попадает в bounded diagnostic message; отсутствие header получает понятную ошибку.

Новый regression использует настоящий local IPC/OS peer, задерживает один accepted
ответ за обычные10s и получает его без второй delivery. Synthetic lifecycle выдаёт
только Indeterminate, никогда не притворяется native Active/SQL release. Проверка
выделена из frozen mailbox26 namespace в отдельный module: Main30 выбирает её явно,
старый MailboxGate сохраняет4 cases и исторический native26 результат не меняется.

Проверки перед следующим source export:1037 workspace tests/17 ignored,
Linux/MSVC workspace all-target Clippy/update-validation, contracts/fmt/diff passed.
Логи `.winbridge/finalization-main30-review-204/reply-budget-*`. У regression также
есть соседний incomplete V8 frame: он закрывается по прежним10s, в то время как
accepted reply остаётся жив. Усиленный вариант passed:1 test,10.23s, оба IPC условия проверены. До нового native green
не объявлять config/task/keeper доказанными.


Clean producer pin: `15d0b982079a0aecb1ab8cb37ffbcca5cecd7b45`, `winbridge-20261030T093705Z-15d0b98-c743b0f8.zip`,
SHA256 `866fbcae01113b058440749260bdf390b605b173e9a6a274c80d63a17f13188b`, 6929543 bytes, built UI.
Следующий documentation-only checkpoint — отдельный target того же кода.
