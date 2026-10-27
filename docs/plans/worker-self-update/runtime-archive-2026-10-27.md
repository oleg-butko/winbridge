# Архивирование собственного inactive test runtime

2026-10-27. Продолжение [FixtureLayout](fixture-layout-2026-10-27.md).
Public updater выключен; installed worker0.1.1/source59249e6 не меняется.

## Реализовано

`scripts/lib/test-activation-archive.ps1` принимает только original resident
reservation с ещё открытым script slot lease после release временного image pin.
Closed reservation/cold JSON не открывают старый updater и не принимают custody.
Это сохранение test files, не новое разрешение на activation/Store/epoch.

Проверяются точный run/runtime/production scope, SHA и fixed path retirement
proof, native exit flag, actual named Jobs и текущие CIM processes. Недоступный
process/Job не считается завершённым. Неизвестные top-level entries, reparse,
hard links, занятые файлы, другой том или существующий archive destination
вызывают отказ. Volume проверяется до нового plan/pending. Directory ancestor
pins удерживают source и destination namespace; каждый payload проверяется
по Win32 file ID и SHA, включая дерево и пустые directories.

В прежнем run создаётся immutable `plan.json`; перед каждым переносом сохраняется
`move-N-issued.json`, после проверки destination — `move-N-done.json`.
`MoveFileExW` использует только WRITE_THROUGH, без replace/copy fallback.
Все файлы/configs/serialized paths сохраняются без переписывания. Runtime folders
переносятся целиком; validation directories не удаляются. Base owner/lease остаются
на месте; следующий run получает прежний exe path и новый reservation ID.

`winbridge-activation-archive.pending.json` блокирует повторное использование
неполного slot. Потерянный результат разрешается только наблюдением того же ID/
bytes в destination, без второго Move. Issued+source present считается
indeterminate и не повторяется. Продолжение требует прежнего resident reservation;
cold automatic recovery/возобновление чужого updater не добавлены.

Маленький C# interop в `test-activation-archive-native.cs` используется только
Windows test script: file IDs/link count, directory pins, no-clobber move и query
Job. Отдельный FixtureJob — явный kill-on-close owner policy test, не archive API.
Production Rust platform/process authority не заменяются этим interop.

## Native retirement hook

`update_shutdown` сохраняет actual old Main Job name. После retained Job empty,
drain stdout/stderr и проверки activation Job он записывает
`runtime-retirement.json` для external runtime, в том числе после execution-body
panic с успешно подтверждённой очисткой Job. Артефакт связывает reservation/
marker SHA/source/run/runtime/jobs, `archivalOnly=true`, `newActivationPermit=false`.
`executionBodyCompleted` не обозначает завершение всех последующих audits.
Main без external runtime сохраняет прежнее поведение.

Hook прошёл MSVC all-targets/update-validation Clippy и fmt. **Actual native Main
с этим hook ещё не запускался**. Компонентные policy tests ниже используют свой
настоящий fixture Job и не являются Main25/миграцией/полным updater.

## Проверки

Final run **20261027-074433-9d3bc04c**, task
**90d968a3-f026-4357-b397-9a2d3b45abcc**, succeeded/0:

- legacy slot6 + reservation11 + **archive12 checks**;
- actual live Job, несмотря на retirement flag, запрещает move;
- actual file/occupied archive collision сохраняет обе стороны;
- unknown file/hard link/junction/open data handle не меняются;
- injected lost reply после настоящего OS rename оставляет plan/blocker;
  прежний reservation продолжает по фактам destination без повторного эффекта;
- IDs/bytes/Unicode filename/serialized config paths сохранены;
- новый reservation использует тот же executable path.

Data в `opaque-state.bin` — fixture bytes, **не SQLite/migration proof**.
Источник —9 pinned worktree files на основе76ede7a, не clean Rust source export.
Manifest SHA `0655fed53a8313a0121938186edcb92ec7b438df6737aa289eee786c7e2c5a40`;
archive plan SHA `b73ad8db70206df3c260dcebff0fa54e02cad3c4882a500ae46d39bf66f4d371`;
exact stdout/check names/counts/typed flags и9 source files проверены на Debian
дважды. Evidence `.winbridge/update-preparation/archive-slot-policy-20261027-074433-9d3bc04c/`
(`request`, `accepted`, `terminal`, `chunks`, `info-before/after`, `source`, `verified.json`).
Observer `archive-policy-collision-observer.log`; verifier
`verify-archive-policy.py` / `archive-policy-verified-final.log`.

Ранние positive runs bdd6fd96-9a57-4a61-b4d3-b00787b60717 и
b3b80a60-b48e-4468-8c81-c82ba910cd5a проверяли6+11+10; они не подменяют final12.
Отдельный readonly audit старшего archive также прошёл: task
e57f282c-19d5-41c4-83a6-9583def712f4,5 files/3 directories, receipts и другой
current reservation, Job empty. Final readonly audit также succeeded/0:
**a5b4b745-5043-4029-be74-1e23ef583a09**,5 files/3 directories/5 move receipts,
source IDs/bytes/plan SHA подтверждены, новый reservation отличается, старый Job empty.
Evidence `archive-readonly-20261027T074617-819aec/verified.json`,
`archive-native-audit-final.log`; проверка не переносит файлы и не запускает Main.

Windows free33,518,620,672 →33,511,972,864 bytes. Debian перед cross-check
22,738,157,568 bytes. Тяжёлых Windows Rust/UI builds не было. Old validation dirs
не очищались; новый policy test сохраняет свои unknown/hardlink/junction fixtures.
Installed SHA536d8637…/generation936909c6/incarnation5a8eb173 прежние, active0,
maintenance/recovery errors=null. Controller5103 сохранён, desktop не нужен.

## Следующее

Подключить helper к full gate, передавая native retirement proof после Cargo и
удерживая original script lease на всё время. При отсутствии/неизвестном exit
proof оставить slot заблокированным, не создавать новый worker автоматически.
Нельзя вместо Main25 снова ограничиться component checks.

Далее: deterministic fixture instance ID + отдельный controller1144, exact
validation worker build перед reservation, firewall wait900s в old normal Main
**до supervisor admission**, отдельный внешний timeout. Actual Main25 dispatch
сразу после writer confirmation; проверить configured URL/no session, same child/
Store/lock/generation, один epoch+1, историю/output/pause/shutdown, root
AwaitingController, duplicates/lost ACK/cold no-replay и actual Job exit. Затем
independent verifier/negative mutations, actual retained19→20/refusal, controller/
lifetime/commit/release и оставшиеся U2–U4. Public install остаётся off.
