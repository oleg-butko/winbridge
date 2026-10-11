# Этап 6: возобновление ZIP из проверенных блоков

2026-10-10. Следующий сценарий после измеренного output gate — явный повтор
доставки большого snapshot без повторного чтения уже доставленных байтов из HGFS.

Schema 3 добавляет manifest-pinned SHA-256 каждого последовательного блока ZIP
по 1 МиБ (последний короче), сохраняя полный payload SHA-256 и все file pins.
Новая публикация opt-in: `publish-resumable PROJECT REVISION [BASE_SNAPSHOT]`.
Отдельный ID не пересекается с прежними full/delta packages. Schema 1/2 не меняются.
RPC import и idempotency semantics прежние; старый worker отклоняет неизвестный
формат обычной ошибкой импорта. Новые поля read model необязательны; UI явно
сообщает, если текущий worker не поддерживает resume.

Worker сохраняет блок только после actual size/hash check и sync, в локальном
project cache с ключом manifest SHA-256. Частичный блок остаётся в staging
конкретного Import ID. Новый **явный** импорт с новым key повторно проверяет
кэшированные блоки, читает недостающие из shared по offset и собирает новый ZIP
в собственном staging. Перед extract проверяется весь ZIP; file hashes и
immutable generation publication остаются прежними. Delta может использовать
ту же доставку ZIP, отдельно проверяя pinned local base.

Cancel, timeout, loss of mount и crash не запускают import/build заново.
История прежнего импорта и staging сохраняются; повтор прежнего key только
возвращает прежний результат. Worker restart помечает незавершённый импорт
interrupted. Полный legacy package того же commit — явный fallback при повреждении
кэша; автоматической очистки и ремонта пользовательских каталогов нет.

Проверки: chunk metadata bounds/coverage, cancel inside block, explicit resume
и точные прочитанные/reused bytes, local/shared corruption, whole ZIP hash,
changed manifest pin, traversal/links/junctions, delta/full equivalence,
durable dedup и actual owned worker crash/recovery. Automatic Windows gate
использует новый runtime и journal; установленный worker только запускает сценарий.
Desktop не нужен, новых окон не будет. До завершения gate сценарий не закрывается.
