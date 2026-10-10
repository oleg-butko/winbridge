# Явное возобновление доставки snapshot

Реализация 2026-10-10; automatic Windows gate пока ожидается.
[Контракт и проверки](snapshot-resume-plan.md).

```sh
winbridge-controller --config controller.local.json publish-resumable PROJECT REVISION
# Необязательная pinned база: ZIP содержит только изменённые файлы.
winbridge-controller --config controller.local.json publish-resumable PROJECT REVISION BASE_SNAPSHOT
```

Ответ содержит ready и monotonic publication timings. Альтернатива для MCP —
`winbridge_publish_resumable` с projectId/revision и необязательным baseSnapshotId.
Импорт использует прежний ImportSubmission и `import` (`import-delta` для delta).
Новый worker показывает `snapshotResumeAvailable: true` в Info. Старые worker
не понимают schema 3: обновите worker или опубликуйте legacy `publish` / `publish-delta`.
Новый runtime не требуется для чтения старых schema 1/2 packages.

Manifest schema 3 содержит `payloadChunks: {chunkBytes: 1048576, sha256: [...]}`.
Количество hashes точно равно ceil(payloadBytes / chunkBytes); последний блок
может быть короче. Ready закрепляет SHA-256 **всех** manifest bytes, включая hashes
блоков и optional delta/base pin. Полный payload SHA-256 и files sizes/hashes
остаются обязательными. ID `r-…` учитывает проект, commit и optional base pin;
он отличается от legacy `s-…` / `d-…`.

Новый Import ID получает отдельный staging. Завершённые блоки хранятся локально:
`<workspace>/.winbridge/payload-cache/<manifestSha256>/<index>.chunk`.
Каждый блок проверяется по actual bytes и sync перед переносом из staging.
Частичный блок, отмена или потеря процесса не создают готовый cache entry.
Проверяются links/reparse points/hardlinks, как и для остальных source путей.

После обрыва выберите тот же пакет и нажмите «Импортировать пакет»:
новый key создаёт новый Import ID и повторно проверяет сохранённые блоки.
Старый key возвращает старый статус/ID, даже если это cancelled/interrupted.
Ни startup worker, ни восстановление UI, ни reconnect не возобновляют принятый
running filesystem
mutation автоматически. Перед extract проверяется assembled ZIP SHA-256;
перед publication проверяются все target files и delta base. Сборка остаётся явной.

ImportRecord.transfer и UI показывают payloadBytes, chunkBytes, reusedBytes и
sharedBytes. Счётчики описывают **завершённые проверенные** блоки; недописанные
байты не включаются. Проверка отсутствия лишнего tail использует отдельный EOF
probe. Это объём ZIP из HGFS, а не сетевой traffic или metadata reads.
На успешном импорте reusedBytes + sharedBytes = payloadBytes. История фаз не
разрастается по количеству блоков; текущий прогресс обновляется отдельно.

При повреждении cache import останавливается с номером блока и причиной.
Данные сохраняются; явная legacy full publication того же commit создаёт другой
package/pin и обходит cache. Автоматической очистки здесь нет: cache занимает
ещё один payload ZIP наряду со staging ZIP и распакованными source generations.
Управление сроком жизни source cache требует отдельной согласованной политики.
Resume экономит повторное чтение HGFS после обрыва; первая доставка включает
дополнительную локальную запись/проверку. Рост общей скорости не обещается.
