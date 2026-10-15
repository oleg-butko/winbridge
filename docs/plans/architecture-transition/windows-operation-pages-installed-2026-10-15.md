# A3 завершён: установленный Windows read model, 2026-10-15

Worker `a8cf019c3f381bfd56006eadbb824a19553fae81`, build `1792055908607920500`,
generation `69b65ecc-fe9d-42f2-a4cc-210b75127a66`, прежняя journal incarnation
`5a8eb173-4193-4332-b00f-67e3850b2c97`. Controller clean `545166d` с прежним exact
consumer proof; после проверки serve session `75826`, WS session
`01823f6a-0e4c-480e-a45b-a38ec3f1da03`. Worker здоров/idle, desktop не нужен.

Actual MCP pair `paired-20261015-052844-9b1fae89` passed, обе задачи succeeded/0:
`71da6a6f-02ce-4cf4-a3c0-0407bbac0aef`, `4b6324a4-fca9-451f-9c15-490534a2aa48`.
Receipt `628a4076d68b9e8862436dcbc19fccf52174eb0ba937214c94906903e0a63223`
остаётся действительным после перезапуска того же controller binary.

Gate `operations-20261015-053018-7d937961`, parent Task ID `21b7e911-c197-4c7a-af3a-50eea99ad9bc`,
PID `14640`, succeeded/0. Report SHA-256
`d21dbc9d6bb8643a7b1f19f6d2569424def45cabbf214b0bfb73785e8ba3485e`. Результаты:

- MCP/CLI прочитали 173 записи всех четырёх видов. Приём probe между страницами
  не изменил исходную выборку; refresh нашёл новую задачу. Повтор того же ключа
  сохранил Task ID и единственный процесс.
- Чужая scope, изменённые фильтры старого cursor и неизвестный контракт отвергнуты.
- Настоящий Windows named pipe показал running task и held project lease;
  во время остановки controller локальная страница оставалась доступной.
- Reconnect сохранил Task ID/PID/worker/journal и прежний cursor. После terminal
  commit страница показала succeeded и освобождённый ресурс; UTF-8 output и exit 0
  прочитаны через MCP.
- Production OperationPagePanel получил данные через настоящий MCP → WS → Windows.
  Browser на Debian проверил ресурсы/revisions, пагинацию, применение фильтра,
  exact task navigation и disabled controls при устаревшей локальной свежести.
  Browser bridge допускает только readonly operation query; Windows UI окна не
  создавались. Это проверка renderer, не native window automation. Native desktop
  ранее прошёл [48-test candidate gate](windows-operation-pages-gate-2026-10-15.md).

Оригинальные запросы сохраняются до submit. `--finish` только сверяет исходную
задачу/report, не повторяет работу. Отчёты находятся в shared
`K:\VMs\shared\WinBridge\workflow-validation\operations-20261015-053018-7d937961`,
local evidence — одноимённый каталог `.winbridge/architecture-transition`.
Рабочие config/runtime/journal не редактировались; очищать Windows не потребовалось.
Полный A3 и A3.3 завершены. Далее [A4.1](work-scheduling.md).
