# Readonly runtime support candidate updater

2026-10-23. Реализована query foundation перед будущим production drain. Она не
меняет updater15/worker16 journal и supervisor17/view9, не открывает config/store
и не выдаёт permission на install. Public update по-прежнему выключен.
**Durable health/approval integration ещё не подключена.** Матрица replacement
faults сейчас работает на предыдущем immutable archive e567827, без этой функции.

## Контракт и вызов

`winbridge-update --runtime-support` возвращает strict `UpdaterRuntimeSupport`
schema1/queryContract1: artifact descriptor/build/target, actual process stamp,
updater journal schema, build flavor Standard/Validation и версии поддержанных
контрактов. У Windows объявлены final backup15, preparation16, replacement17.
У Debian эти native contracts отсутствуют. Activation, controller confirmation
и desktop update пока **null** на обеих платформах: их исполнение ещё не реализовано.
Никакого `canInstall`/health approval report не содержит.

Отдельный marker `WinBridge.UpdaterRuntimeSupport.contract-1` позволяет отказаться
от запуска неизвестного флага у старого exe. `updater_support::inspect` получает
exact ImageEvidence + ArtifactDescriptor из frozen trusted candidate set:

1. Сверить updater role и полный SHA/File ID/bytes/canonical path под immutable pin.
2. Если нового marker нет — вернуть Unsupported (`None`), **не запускать** exe.
3. Через существующий bounded managed probe сверить actual artifact descriptor,
   затем получить support от того же pinned image.
4. Проверить strict поля, actual PID/birth/host/session, build/source descriptor,
   nonzero declared contracts, отсутствие stderr и неизменный image после query.

Результат содержит `readOnly`, `cleanupVerified`, `journalBound:false`,
`candidateVerified:false`. Он остаётся data observation; его нельзя превратить
в original shutdown/replacement custody или свежий install permit.

## Код и проверки

- DTO/marker: `crates/protocol/src/update/updater_support.rs`.
- Query/inspector: `apps/update/src/updater_support.rs`.
- Early CLI до config: `apps/update/src/main.rs`.
- Executable tests: `apps/update/tests/updater_support.rs` — readonly CLI с
  испорченным config рядом, bounded actual image query, marker absence, stale
  image/role и неверные flags. Unit binding test отвергает чужой process/build
  и нулевые версии. TypeScript генерируется из Rust.

Local updater package **225 passed /0 failed /6 ignored**; сюда входят3 integration
tests и binding unit test. Linux/Windows all-target Clippy с update-validation
прошли; Rust fmt и generated TypeScript совпадают. Native fault task63232320
использует immutable e567827 и не подтверждает query. **Нового native Windows
support gate ещё не было**, durable approval/drain enforcement не подключены.

## Что делать дальше

Перед production drain сохранить проверенный support в immutable evidence,
связанной с candidate ImageEvidence/descriptor/source/health и owner scope. Проверять
совместимость всех требуемых contracts, не просто matching SQLite schema. Validation
build не должен стать production target. Отсутствующие/новые неподдержанные версии
объяснять до остановки worker, без fallback и без автоматического запуска unknown
flags. Добавить native actual query и marker-negative checks в следующую Windows
проверку после завершения текущей матрицы; не занимать общий build cache параллельно.
После реализации activation обновить declaration по действительному execution
contract, а не заранее. Подробности основной очередности — в
[runtime-replacement-actor](runtime-replacement-actor.md).
