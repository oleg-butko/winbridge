# U1: exact artifact set и durable receipt

2026-10-18. Trusted library; runtime replacement, health и public update отключены.

Проверка читает exact bytes уже связанного compiler report. Typed schema-1 parser
требует успешную сборку, правильные source commit/manifest, четыре фиксированных
имени exe, по одному summary/query на каждое имя, правильные роли, OS/architecture,
версии и clean build identities. Текущие файлы payload сверяются по File ID,
размеру, SHA-256 и static query capability marker. Unknown/duplicate/extra paths,
неизвестные поля, failed command/query, неподтверждённый cleanup и смена файлов
отклоняются. Пути из report только сравниваются с заранее вычисленными локальными
путями; поиск и исполнение по report-controlled path отсутствуют.

Четыре readonly Windows file handles удерживаются вместе и запрещают write/delete
во время проверки и записи receipt. Повторные source/report/root/file проверки
обнаруживают смену состояния. Это доверенная локальная сборка, не подпись и не
sandbox для произвольного чужого кода. Metadata query не заменяет реальный запуск
нового worker и проверку его journal/health.

`candidate::verify_and_record` требует нынешние updater owner/epoch, Verified
source, живую SourceHeld lease, Completed successful compiler и физический
cleanup его дерева. Immutable artifact-set receipt и parent transition
Building → ValidatingCandidate записываются в **одной транзакции**. Receipt
привязан к source/build digests, точному report, payload directory, четырём
файлам, parent revision и времени наблюдения. Duplicate возвращает исторический
receipt; это не новое разрешение и не подтверждение текущей свежести файлов.

Для readonly проверки файлов отдельный execution permit/Intent не нужен:
повторное чтение не создаёт внешних эффектов. После неудачной metadata transaction
допустима новая проверка тех же файлов; compiler при этом не запускается.
Raw BuildSucceeded/CandidateAccepted не обходят driver и будущие health proofs.
CandidateVerified остаётся false. Verified artifacts означают начало проверки
кандидата, а не разрешение остановить старый worker или заменить runtime.

Updater schema 7 добавляет `candidate_artifact_sets`, с immutable/retained
triggers. Миграция 6 → 7 требует owner lock, verified consistent backup и epoch
fence. Старые compiler/report receipts сохраняются без перепривязки. Safe abort
дополнительно закрепляет digest artifact receipt; отсутствующее значение в старой
истории не дописывается задним числом. Worker schema остаётся 15; legacy RPC и
supervisor v1–v4 wire shapes не меняются. [Mailbox v5](artifact-actor.md) добавлен
следующим блоком; его native gate ещё впереди.

Standalone readonly команда:

```text
winbridge-update inspect-candidate-files ABS_SOURCE MANIFEST_SHA256 ABS_STAGE REPORT_SHA256
```

Она нужна для build-only diagnostics и native validation. Проверяет точный report
и реальные четыре exe, возвращает `journalBound: false`, `candidateVerified: false`,
`runtimeApplied: false`; не создаёт journal и ничего не исполняет. Build-only
сценарий вызывает её после create-only publication candidate-report.json и
сохраняет отдельные readable command/proof files. Trusted coordinator принимает
только собственный managed compiler/report receipt, не standalone proof.

Тесты ledger используют явно synthetic query receipts и не утверждают executable
health. Отдельный native full build проверяет actual four exe queries и затем
standalone parser на этих же actual файлах. Следующий actor gate должен связать
source/build/artifact driver; позднее необходимы real worker startup, journal
rehearsal, exact shutdown и activation. Native готовность текущего блока пока
не подтверждена; подробности текущих проверок — в handoff.

Schema-1 build report нельзя расширять несовместимыми обязательными полями без
нового контракта. Для будущих обновлений новая build script обязана сохранять
старый формат, который понимает работающий updater, до явного согласования нового.
Worker не должен автоматически мигрировать независимую updater DB при activation:
ещё работающий прежний supervisor владеет её прежним контрактом. Его совместимый
readonly handshake и следующая миграция после release — отдельная задача U2.
