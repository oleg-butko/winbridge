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
Clippy, fmt, Python runner9 и scaffold270 passed. Настоящий Windows release gate
пока **не запускался**.
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
