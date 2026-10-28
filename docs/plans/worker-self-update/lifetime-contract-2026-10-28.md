# Lifetime: контракт и чистая модель

2026-10-28. Продолжение [native OS spike](windows-lifetime-spike-2026-10-28.md)
и [плана lifetime](activation-lifetime-2026-10-28.md). Это data-only foundation:
receiver, runtime keeper, SQL admission и public install этим шагом не включаются.
Следующий [ledger22](lifetime-ledger-2026-10-28.md) реализуется отдельно;
упомянутые ниже schema21 и935 tests относятся к этому DTO/FSM checkpoint.

## Контракт

В `crates/protocol/src/update/activation_lifetime.rs` добавлены четыре DTO:
mode, binding, command и receipt; TypeScript генерируется из тех же Rust типов.
Единственный mode — `supervised_quarantine`. Режимов normal/unmanaged нет.
Нет нового RPC action, capability, private marker или версии advertised support;
worker19/updater21/private5/supervisor26/view18 пока прежние.

Binding содержит update/attempt/launch, original updater owner/epoch, ожидаемую
root revision AwaitingController, command ID controller, domain-separated SHA-256
точных controller/local7 records, keeper PID/birth/host/session, scoped activation
Job, compatibility того же child и authority worker journal. Вместо нового
рекурсивного вложения service/controller commands используются плоские ссылки.
Перед использованием они сверяются с **полным** сохранённым physical6 → writer →
service → local7 → confirmed controller graph. Отдельно корректный JSON binding
не считается доказательством этого graph или kernel custody.

Command сохраняет original deadline controller/activation: новый timeout не
начинается. Receipt связывает hash всего command, тот же keeper/Job, process,
Store incarnation/epoch, config/build/schemas и mode. Меняться может только время
наблюдения compatibility, причём entry должен произойти внутри исходного окна.
Unknown поля, normal mode, boolean вместо counter и alias предыдущего command ID
отклоняются.

## Состояния

`apps/update/src/activation/lifetime.rs` содержит чистую FSM:

```text
Issued(0) → Confirmed(1)
Issued(0) → Indeterminate(1) → Confirmed(2)
```

Command immutable на каждом переходе; timestamps монотонны, исходное окно не
продлевается. Неизвестный ACK не разрешает повторный send. Подтверждение может
сохраниться после deadline как исторический факт, только если сам entry receipt
относится к исходному окну. Такой факт не создаёт live permit, owner, root commit,
normal dispatch или maintenance release. Полный causal prefix потребуется SQL
reader; одиночный JSON revision2 не заменяет две предыдущие записи.

## Следующее подключение

1. Добавить updater22 lifetime intent/current/history: immutable command/key,
   CAS, атомарный readback всех связанных records, строгая causal history.
   Production API сначала только read/inspect; test SQL writers не выдавать
   за fresh permission. Старый schema21 archive сохранять через consistent copy;
   actual Windows21→22/old21 refusal — отдельный gate перед новым full Main.
2. Fresh admission принимает original resident guard, pinned config, exact
   current root AwaitingController/owner epoch и неизменный live startup budget.
   Existing возвращает данные; холодное чтение никогда не возвращает permit.
3. Подключить same-child receiver и normal IPC observation. Только затем
   выбирать private6/actor27/view19; не рекламировать их по наличию DTO.
4. Resident keeper должен удержать original Job/process/channel, но после
   проверенного commit освободить file pins и updater journal authority для
   следующего поколения. Это consuming role transition, не `mem::forget` guard.
   Next-update delegation, maximum2 keepers, final root commit/scoped release,
   U2 faults/U3 UI/U4 actual self-update ещё впереди.

## Проверки

Шесть focused model tests проверяют exact graph/keeper binding, initial window,
7 подмен retained graph,16 подмен flat command,7 подмен receipt, unknown mode/fields
и late fact после unknown ACK без replay. Это синтетические DTO/FSM проверки;
native OS proof и Main26 имеют свои отдельные pins.

Workspace с `winbridge-worker/update-validation`:935 passed/0 failed/17 ignored,
124 test groups. Linux и Windows MSVC workspace/all-target Clippy `-D warnings`
прошли; TypeScript/typecheck, generated bindings, fmt/diff и scaffold317 также.
Логи/измерение дисков: `.winbridge/review-2026-10-28/lifetime-model-*`.
Debian перед проверками10,381,496,320 bytes, Windows K: через shared24,996,601,856;
Windows build/RPC tasks не запускались. Readonly Info подтвердил installed0.2.0/
generation946f1128, active0, maintenance/recovery errors=null. Runtime204 не менялся.
