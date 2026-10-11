# Windows output gate — 2026-10-10

Автоматический прогон `20261010-042006-6c67052d` завершён: 11 actual Windows tests,
четыре checksum-verified artifacts, outer task
`ddf23ba0-c6b3-4eb8-ae3d-14511f320a79` succeeded / exit 0.
Desktop закрыт, новых окон не было. Installed worker/config/journal не заменялись;
тестовые worker с отдельными SID+instance pipes завершены.

Исходники: clean `b980ac0bccac9a1730c832e180b629103bb5d229`, архив
`winbridge-20261010T081954Z-b980ac0-c546bda5.zip`, SHA-256
`669700f71397b265a2511cbe68dfd4b454023bab9cc478965a7d00279dce90b4`.
Тестовый worker manifest SHA-256
`cd070f80a78034abb8cbe99ef9ba07edacb2567df840d50b5bffcd36f19e4d1d`.
Reference — clean installed `af90ec0`; его exe запущен с новым тестовым config/state.
Общие Cargo fixture и IPC driver одинаковы, сборки debug. Порядок: три reference,
затем три current; cache/background variation не устранены.

## Проверенное поведение

- Byte/JSON/row budgets: worst-case 255-byte JSON, tiny rows, сохранение offsets и
  каждой sequence, историческая row больше raw target, resume после partial cursor.
- Настоящий named-pipe IPC: три flood tasks на версию, ровно 1 MiB `x` на каждый
  поток плюс известный stdout header. SHA-256 тела каждого потока
  `8f990ba0b577b51cf009ea049368c16bbda1b21e1b93be07a824758bb253c39b`.
  Captured total совпадает с task.outputBytes; same-key submit не дублирует запуск.
- После остановки только isolated worker новый worker читает оставшиеся rows по
  прежнему cursor; completed task и key сохранены, success fixture запущен один раз.
- Managed tree cancellation и explicit attention работают на обеих версиях.
  Дополнительные runtime tests проверяют quota/drain, timeout, cancel, linked paths,
  persisted outcome и recovery.

## Измерения

| Показатель | Reference | Current |
| --- | ---: | ---: |
| Непустые страницы 2 MiB + header, каждый повтор | 17 | 11 |
| Read RPC ms, три повтора | 1967.72 / 1960.72 / 1920.15 | 1773.51 / 2070.54 / 1900.94 |
| Median Read RPC ms | 1960.72 | 1900.94 |
| Median read + interleaved Info + local validation ms | 2642.87 | 2523.60 |
| Info median / p95 / max ms | 1.66 / 2.72 / 2.80 (51 samples) | 1.66 / 2.55 / 2.93 (33 samples) |
| Explicit cancel observed ms, один sample | 49.57 | 67.96 |
| Explicit attention RPC ms, один sample | 6.88 | 7.61 |

Количество страниц уменьшилось на 35%; tiny-row test выдаёт 1024 rows вместо 16.
Время чтения улучшилось примерно на 3% по median, но ranges пересекаются: общего
ускорения pipeline или статистически подтверждённой latency регрессии/её отсутствия
эти три пары и одиночные control samples не доказывают. Baseline 4161 ms измерял
Debian → WebSocket → Windows, поэтому напрямую с native IPC не сравнивается.
Основное UI улучшение — bounded persistent buffer и DOM, независимо от task quota.

## Доказательства и границы

Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261010-042006-6c67052d`
(Debian `/mnt/hgfs/shared/WinBridge/workflow-validation/20261010-042006-6c67052d`).

- `report.json`: SHA-256 `a73633f445734e78bceb9ee28799c74fec97a5db9db478b30d31472f09c9629c`.
- `output-proof.json`: SHA-256 `8ce31629d426da0c6eb1b042be7d579dc126d79165c1dbd324aae6918d157907`;
  raw timings, source identity, byte hashes и control results.
- `report.ready.json`: SHA-256 `5cb6081849e876cdb44dde99b457d09135447647facbc6a0152a33f051de8fa2`.
- Два Cargo logs, команды/argv/exit code в report. Bootstrap request и installed
  worker proof сохранены в `.winbridge/workflow-validation/<run>` Debian.

Installed generation осталась `c182793f-5391-4bf8-9e70-9c94b8b24a5c`, healthy,
activeTaskCount=0. Рабочий worker оставить запущенным, desktop необязателен.

Browser integration с настоящим Unix worker отдельно проверяет 20 000 Unicode
lines/stream, lost read ACK без пропуска cursor, exact loaded bytes, ограниченный
tail, максимум 28 DOM rows, filter/pause, frozen-view reset после cleanup и отказ
«полного» отчёта с byte gaps. Native визуальная проверка
нового UI этим backend сценарием не заявляется. Snapshot chunk/resume ещё не реализован.
