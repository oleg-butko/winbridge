# Selective instance-lock release: physical primitive

2026-10-24 America/New_York, после [Main22 receiver112/0](windows-activation-process-gate-2026-10-24.md).
Это следующий library primitive, **не подключённый state writer handoff**.
Updater18/worker16/supervisor22/view14 прежние, public install off.

## Задача и контракт

После Ready original supervisor ещё удерживает instance lock. Нельзя просто
unlock и оставить `LockedShutdownState`, будто он по-прежнему владеет lock.
`LockedShutdownState::release_for_handoff_checked` потребляет этот объект и
освобождает только его собственный instance handle. Directory/journal/lock pins
остаются открытыми; exact FileID до/после проверяются. Новый
`ReleasedShutdownState` умеет только читать observation и revalidate identities.
Он не открывает SQLite, не приобретает successor lock, не удаляет guard и не
даёт startup permission. Обратного преобразования в locked custody нет.

Caller обязан заранее сохранить HandoffIssued, аутентифицировать того же child
и в checkpoint проверять original live authority/deadline. После potentially
slow identity I/O checkpoint вызывается непосредственно перед unlock. После
unlock выполняются fact/identity checks. Ошибка не запускает retry/relock.

| Ошибка | Сохранившиеся ресурсы | Знание об original lock |
| --- | --- | --- |
| BeforeRelease | Прежний `LockedShutdownState`. | `Some(true)`: OS unlock не вызывался. |
| ReleaseUnconfirmed | Те же pins после неуспешного ответа unlock. | `None`: не выводить фактическое владение из ошибки OS. |
| AfterRelease | Released state с теми же pins. | `Some(false)`: наш unlock подтверждён; новый owner этим не подтверждён. |

Failure owner не Clone/Serde и не превращается в новую release attempt. Его
нужно удерживать при ошибке. Drop освобождает лишь свои ресурсы; это не rollback
и не разрешение повторно запускать child. Observation — сохранённая идентичность,
не доказательство содержимого SQLite или текущей successor authority.

## Код и проверки

- `apps/update/src/shutdown/state_handoff.rs`: adapter, released/failure types,
  typed failure phase; exports через state/shutdown modules.
- Успех проверяет настоящий lock до release и acquisition exact file после,
  unchanged journal bytes и то, что Drop прежних pins не освобождает новый lock.
- Отказы до/после checkpoint проверяют реальные права другого handle, а не
  только значение enum.
- OS error проверен при двух фактических исходах: lock ещё удержан / уже
  освобождён. Оба возвращают unknown, не fake rollback.
- Unix tests подменяют journal name файлом с теми же bytes до/после release;
  другой FileID не принимается.
- Windows test дополнительно проверяет rename/delete denial для всех retained
  journal/lock names при успехе и при failure после release.

Local focused6 и package280/0 (6 ignored) passed. Linux/MSVC validation all-target
Clippy, fmt, Python runner9 и scaffold270 passed. **Windows release gate7/0
пройден**, task7799e394-45bc-4f20-82ce-2dc065a27345/run20261025-024054-42fcdb0c.
Logs/measurements `.winbridge/update-preparation/activation-state-release-*`.

## Короткий Windows gate

Новые `--state-lock-handoff-only` / `-StateLockHandoffOnly` в существующем
shutdown-primitives runner выбирают5 release tests +2 original state-pin
regressions. Generated local fixtures, без old binary reference/полного build/
runtime replacement/UI. Retained cache20261017-061527-f4639971, exclusive lease,
порог6GiB и CARGO_INCREMENTAL=0 сохраняются. Диски измерить перед новым run.

Observer сохраняет exact request с fsync и `primitives-run.json` **до ACK**,
потом тот же metadata с task ID. При разрыве наблюдения inspect эту task;
не создавать новый run. Если ACK не получен, проверить request SHA и восстановить
ту же task через `validate-update-support-native.py::recover_task`, а не submit
нового request. Этот старый primitives observer пока не имеет `--verify-run`;
terminal evidence повторно проверять readonly по сохранённым metadata/ready files.
Для нового режима counts независимо читаются из обеих native logs; ready bytes
и ready manifest перепроверяются после audit.

После clean source commit/export:

~~~bash
python3 scripts/validate-update-shutdown-primitives-native.py \
  --archive /mnt/hgfs/shared/WinBridge/sources/PRODUCER.zip \
  --cache-run 20261017-061527-f4639971 --state-lock-handoff-only
~~~

Нужен только installed worker, desktop не нужен, окон не будет. Объявить это до
submit; после terminal явно сообщить, что worker оставить. Не очищать Windows
validation directories. Это primitive gate, не actual interprocess writer transfer.

## После положительного physical gate

Подключить consuming transition к original ExitedStateCustody/process owner так,
чтобы views после release больше не показывали held lock. Затем private handshake,
acquisition того же file ID и отдельная атомарная activation-транзакция worker
с новой authority/receipt. Требования и найденные ограничения —
[в review state handoff](runtime-activation.md#review-следующего-state-handoff-после-actor22-2026-10-24).
Ни существующий Prepared-only checker, ни обычные startup/write barriers не
ослаблять. Local/controller/lifetime/commit и U2/U3/U4 остаются впереди.

## Положительный Windows gate

- Task `7799e394-45bc-4f20-82ce-2dc065a27345`, run `20261025-024054-42fcdb0c`,
  succeeded/exit0,5 release +2 original-pin tests.6 ready files checked twice;
  counts независимо подтверждены native logs, observer exit0.
- Clean producer `da1c7b6bdd1c5c1edfaae7c6e3619b4a4a15e54e`, ZIP
  `winbridge-20261025T024051Z-da1c7b6-cf81690d.zip`, SHA
  `dfa77c771219226374e8e1d1109348731756e185ea32007a058d64a42e606eb1`, manifest
  `266e8bc306fa159b7e197a2d1b943c58438a23c595071e05d108528b2a855bab`.
- Report SHA `668720de2be7f1da3c61b45c63d9e220e5e5121eaa1c47d54b5e826bf9e6f4b5`,
  ready SHA `9187a26bb416b35a07e0d0fc15d4b7d54b38cf1951729dbb1a2b016e41cef46f`.
- Local pin `.winbridge/workflow-validation/20261025-024054-42fcdb0c/state-lock-handoff-verified.json`,
  observer log `.winbridge/update-preparation/activation-state-release-native.log`.
- Shared report `/mnt/hgfs/shared/WinBridge/workflow-validation/20261025-024054-42fcdb0c`.
- Windows free13,133,565,952→13,134,938,112 bytes; Debian26,317,541,376 after.
  Перед новым build измерить заново; Windows cleanup не выполнялся.
- Fresh installed Info: прежний d052/generation8f8d1fb9-e5b1-4984-a17e-d067db46268e,
  connected/active0/maintenanceError=null/journalRecoveryRequired=null,
  recoveryCoverage.complete=true. Desktop можно закрыть, installed worker оставить.

Review не выявил важных ошибок: успешный released object и каждая failure ветвь
сохраняют именно original pins; unknown не имеет conversion в held/released
success; Drop старых pins не снимает чужой новый lock. Это real physical adapter,
не проверка межпроцессной передачи writer. Следующий native gate должен использовать
того же authenticated child/original supervisor и проверить HandoffIssued→StateOwned,
сохраняя явную границу между physical state ownership и Store authority/local health.

## Следующий implementation checkpoint

Существующая ledger18 `StateOwned` receipt подтверждает actual process, прежние
directory/journal/lock IDs и новый generation. Её следует получать после
настоящего acquisition в том же child. Она не доказывает новую JournalAuthority
epoch или writable Store; это отдельная следующая транзакция/local verification.
Не добавлять epoch/permission в frozen receipt задним числом.

Порядок: новый private state-handoff contract рядом с frozen Challenge/Ready1;
проверка его поддержки до unlock; HandoffIssued до освобождения original lock;
exact child acquires тот же lock и возвращает physical StateOwned; parent
сохраняет факт, сохраняя released pins/old process/runtime resources. Отдельный
typed owner обязан заменить прежнее предположение «original state held».
Failure/unknown никогда не возвращают Ready guard для повторной передачи.
Обычный serve по-прежнему заблокирован, Store writer/dispatch не запускаются.

Для actual state-transfer gate потребуется **новая clean receiver fixture**:
fixture1c23d48 имеет только Ready1 и для следующей команды не подходит.
После receiver/library source checks экспортировать fixture, затем distinct
actor/harness producer и новый isolated Main gate. Пройденные112/0 и7/0 не replay.
Далее writable authority receipt/worker schema policy, local/controller/lifetime,
remaining U2 и U3/U4. Installed worker сейчас обновлять не требуется.
