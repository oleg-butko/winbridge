# Windows gate настроек и output retention — 2026-10-09

Прогон завершён автоматически через installed worker без его замены. Рабочий
desktop пользователь закрыл до сессии; появились два последовательных TEST окна
отдельного runtime. Пользователю до запуска объяснено, что их не нужно трогать;
после подтверждено закрытие окон и необходимость оставить working worker запущенным.

## Источник и проверенный отчёт

- Clean source: `adbda09261a151f6de80bce3cf1fdda7901bf502`.
- ZIP: `winbridge-20261009T082348Z-adbda09-9618e0d6.zip`.
- ZIP SHA-256: `aeb822aa29ebcafaaa6d32cc57e8151ee9f5878cd22bfc236a60a14b713f69a6`.
- Shared run: `K:\VMs\shared\WinBridge\workflow-validation\20261009-042415-34160b98`.
- Outer task: `56ee9fa9-b87d-4392-84c9-efcfd70a70c6`, succeeded, exit 0.
- Report SHA-256: `1bfb9e0e9b59ff58d4a050e4a3ec29b24819b1c50ba723b2209d21ba783de5df`.
- Все 19 artifacts из ready marker проверены по фактическим bytes/SHA-256.

## Реальное исполнение на Windows

9 maintenance tests проверили actual running fixture/PID во время config save,
save/restart/reconciliation, invalid identity/URL/limits и external edit conflict;
transaction rollback, pending attention, active/indeterminate tasks и workflow/app
links; 50+5 cleanup batches, сохранение recent output/history/keys, opt-in automatic
cleanup и consistent schema 5→6 backup. 23 core policy/persistence tests и 4
diagnostic report tests также прошли. Последние включают Windows junction guard
и bounded readback 2 GiB collision после [ревью](review-stage5-2026-10-09.md).
Offline worker и actual desktop собраны успешно.

Native UI первая фаза:

- Изменение полного config через реальную React форму удержало окно при quit
  до сохранения; затем actual local IPC сохранил ровно показанные JSON bytes.
- Прежний config backup совпал с исходником; UI показал restartRequired.
- Task `4db27e2e-410d-4c83-8754-b7910bbf9b6e` сохранила running/PID **19068**;
  active quota/policy не изменились до restart.
- UI cleanup preview показал 0 eligible tasks и выключенную кнопку удаления:
  fresh/running task не очищалась. Fixture затем явно cancelled.

Вторая фаза перезапустила только isolated worker и открыла новый TEST desktop:
active quota 4096, automatic=true, restartRequired=false; UI/file/worker совпали.
Новая test generation: `88d39766-10fb-4d38-bb9b-81fa08709f03`.
Оба desktop exit 0; workerSurvivesQuit=true, cleanupErrors=[].

Удаление непустой партии и lost ACK **через UI** проверены actual Unix browser
integration; native Windows worker deletion/rollback/dedup проверены backend tests.
Эти виды проверки не подменяют друг друга и не выдаются за ручные UI наблюдения.

## Рабочий runtime и готовность

Installed worker остался source `1982de8`, generation
`750f3499-f4a9-42c6-aa43-13d248abb245`, healthy, activeTaskCount=0.
Его config, SQLite, приложения и старые shared файлы не заменялись и не очищались.
Установка новых возможностей требует общего обновления worker/desktop через
`prepare-windows-snapshot.ps1`; пользовательский worker пока остаётся запущенным.

Этап 5 завершён в принятом scope: retention очищает output, metadata/dedup/attention
сохраняются; automatic policy default off. Dirty source и performance расширения
не входили в этот gate. Следующий этап — измерения времени/объёма перед оптимизацией.
