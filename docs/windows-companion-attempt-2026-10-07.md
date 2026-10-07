# Windows companion: незавершённый gate 2026-10-07

После review/plan commit `bbba964` и implementation commit `c1e9402` запущен один
автоматический isolated native сценарий. Gate **не пройден**; дальнейшая реализация
и повтор остановлены по инструкции пользователя при неожиданном поведении.

- Clean source: `c1e94026603f854e263693a9d6b82dacb3f897a1`.
- ZIP: `winbridge-20261007T095527Z-c1e9402-ba703261.zip`, SHA-256
  `1ea69f71111183dcbd69b537f3b264e26ce14fd915e2fec1b53e28a1f7a1b563`.
  Manifest включает собранный frontend; на Windows npm не запускался.
- Outer task: `b0817b9d-be0f-42fb-98c1-6824bdaf7f5b`, failed, exit 1.
- Shared report: `WinBridge/workflow-validation/20261007-055534-610692b2`.
- `report.json` SHA-256:
  `289508a901690e9d7d19816aae672a2636804e122dde9f1eb8244b08e42270e2`.
- 21 native core tests и 9 attention tests passed; отдельные worker и desktop
  с custom-protocol frontend собраны offline, все четыре Cargo группы exit 0.
  Их logs и report.json совпадают с bytes/SHA в ready manifest.

Initial native desktop phase завершилась без `companion-initial.json`.
`companion-report.json` содержит passed=false и «Native companion initial failed;
see retained logs/report». Единственная сохранённая строка desktop stderr:
WebView2 `Failed to unregister class Chrome_WidgetWin_0. Error = 1411`.
Это не устанавливает причину завершения; desktop exit code и progress checkpoints
в текущем harness не сохранены. Tray/popup/sound/task navigation/restart нельзя
считать подтверждёнными по compilation или отсутствующему фазовому отчёту.

Ready manifest дополнительно содержит неверный размер `companion-worker.stderr.log`:
0 вместо фактических 181 bytes; SHA содержимого совпадает. Валидация правильно
отклонила весь gate. Возможны задержка metadata HGFS и незавершённый redirect drain;
точная причина пока не подтверждена. Manifest и существующие shared reports не менялись.

Installed worker по-прежнему `523fc5b`, generation
`bff7f8da-af71-432b-83af-739a020d9e37`, connected; read-only info после failure успешен.
Рабочие exe/config/journal не заменялись. Harness остановил только собственные тестовые
PIDs в finally; generated test runtime и logs удержаны для диагностики.

После уточнения у пользователя: добавить checkpoints/args/exit-code evidence,
разобрать раннее завершение desktop и стабилизировать publication bytes/hash после
закрытия writer handles. Не ослаблять process-tree ownership, WebView sandbox или
другие security boundaries для обхода без отдельного согласования. Повторять только
новый companion сценарий; завершённые предыдущие gates не затрагивать.
