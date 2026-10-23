# Readonly runtime support candidate updater

2026-10-23. Реализована и [проверена на Windows](windows-updater-support-gate-2026-10-23.md)
query foundation перед будущим production drain. Она не
меняет updater15/worker16 journal и supervisor17/view9, не открывает config/store
и не выдаёт permission на install. Public update по-прежнему выключен.
**Durable health/approval integration ещё не подключена.** Матрица replacement
faults завершилась на предыдущем immutable archive e567827, без этой функции;
[native60/0 result](windows-runtime-replacement-faults-gate-2026-10-23.md) не является
проверкой новой query.

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
использует immutable e567827 и не подтверждает query. Отдельный query gate
прошёл **8/0**,9 ready files twice/36 negative mutations; durable approval/drain
enforcement ещё не подключены.

## Что делать дальше

### Короткий native gate завершён

Реализация возобновлена после checkpoint11e506a. Добавлен отдельный approved
сценарий без повторения полного build/drain/replacement pipeline:

- `scripts/test-update-support.ps1` собирает standard и validation updater в
  общем cache под одной lease, копирует их в разные новые каталоги и проверяет
  обе CLI версии с намеренно испорченным config. Рабочий runtime не затрагивает.
- `apps/update/tests/updater_support_native.rs` запускает trusted inspector на
  обеих реальных сборках, проверяет source/image/process binding и отсутствие
  побочных файлов. Actual retained updater5fbc из positive replacement gate
  должен дать Unsupported по отсутствию marker **без запуска старого exe**.
- `scripts/validate-update-support-native.py` проверяет full prior proof/pins и
  terminal reference task до нового submit; готовый отчёт сверяет дважды, затем
  проверяет actual proof и negative mutations. `--verify-run` наблюдает ту же
  задачу после timeout/разрыва, не создаёт новую. Pending run/request сохраняются
  атомарно до submit; lost ACK без Task ID восстанавливается readonly history/get,
  с полным сравнением frozen payload, лимитами страниц/времени и без resend.
  Временный state/config не
  заимствуется из рабочего worker, берётся лишь readonly executable reference.

Windows all-target Clippy прошёл; local CLI3/0 и Python admission/recovery7/0
прошли. [Native8/0 gate](windows-updater-support-gate-2026-10-23.md) также пройден,
task d97ea580/run20261023-190500-afa1564e terminal succeeded/0. Он проверяет discovery;
durable approval/drain enforcement и activation не реализованы этим сценарием.
После native gate review исправлен pre-ACK gap в Debian observer; readonly lookup
проверен на том же actual accepted Task ID. Это не новый Windows execution run.
Команда после clean source export:

```bash
python3 scripts/validate-update-support-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/NEW_CLEAN_SOURCE.zip \
  --reference-pin .winbridge/update-preparation/replacement-main-native-verified.json \
  --cache-run 20261017-061527-f4639971
```

Перед submit сообщить: установленный worker нужен, desktop не нужен, окон не
будет. Измерения диска сохраняются до сборки, минимум6 GiB. Retained runtime5fbc
не удалять и не обновлять. После результата сверить proof/ready files и только
потом интегрировать support в новый durable контракт до production drain.

### Durable integration

Следом реализуется [versioned compatibility admission](updater-compatibility.md).
Pure production policy/typed blockers добавлены отдельно после native producer
f1e2f10; она не является journal-bound approval и не входит в его Windows proof.

Перед production drain сохранить проверенный support в immutable evidence,
связанной с candidate ImageEvidence/descriptor/source/health и owner scope. Проверять
совместимость всех требуемых contracts, не просто matching SQLite schema. Validation
build не должен стать production target. Отсутствующие/новые неподдержанные версии
объяснять до остановки worker, без fallback и без автоматического запуска unknown
flags. Native actual query и marker-negative checks закрыты; следующий Windows
gate должен проверять durable contract, а не повторять discovery без новых причин.
После реализации activation обновить declaration по действительному execution
contract, а не заранее. Подробности основной очередности — в
[runtime-replacement-actor](runtime-replacement-actor.md).
