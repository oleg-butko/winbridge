# Архив и новая пара после восстановления журнала

Candidate A2.4.2b; до завершения Windows gate установленный runtime не менять.
Процедура сохраняет неизвестный исход старых команд, а не повторяет их.

1. Остановите source worker и запустите с его config `serve-recovery`: постоянный
   latch и режим чтения. Restore SQLite выполняется после latch с сохранением
   внешних markers и execution-domain registry.
2. Закройте inspection worker. С тем же exe выполните:

   ```powershell
   & 'K:\_proj-26\winbridge\runtime\winbridge-worker.exe' --config 'SOURCE_CONFIG' prepare-recovery 'NEW_LOCAL_DIRECTORY'
   ```

3. Ответ указывает archivePath, новый workerConfigPath, contexts и Prepared/Blocked.
   Blocked объясняет missing proof; пустая история не доказывает остановку
   процессов. Подготовленный worker ещё не запускается.
4. С **новым** config выполните `activate-recovery`. Unknown/Occupied запрещают
   activation. После подтверждения запустите `--config NEW_CONFIG serve`.
   Повтор activation этого же результата не создаёт нового эффекта.
5. Явно настройте одинаковый новый instance ID в controller/worker, controller
   URL и переподключите агентов. Desktop local config указывает на новый worker
   config. Exe paths остаются постоянными.
6. Новые запросы используют [закреплённый context и contract 2](fenced-requests.md).
   Не подставляйте новый context в старый pending envelope. Отсутствие key в
   архиве не разрешает считать старую команду неисполненной и повторять её.

Source journal, archive, markers и registry сохраняются для последующих проверок.
Прежняя история читается через source `serve-recovery`; закрывайте его перед
preparation/activation или restart target, поскольку source проверяется под
exclusive lock. Отдельный archive viewer — A3. Историческое Running/Ready не
подтверждает живой процесс.

Не удаляйте gate/retirement files. Legacy без полного witness не активируется
автоматически. Unix не подтверждает Windows процессы. Пока поддерживается
schema 12; older backup schema требует compatible inspection binary и не
мигрируется как действующий journal.

[Контракт и ревью](../plans/architecture-transition/recovery-pair.md).

Архив сохраняет SQLite history и записанный stdout/stderr. Внешние application log,
artifact files и workspaces целиком не копируются: сохранённый путь не является
исторической копией файла. При чтении внешних файлов важны их identity/hash;
archive read model A3 будет показывать эти ссылки отдельно от сохранённого output.
