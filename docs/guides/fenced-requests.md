# Запросы с закреплённым журналом

Это candidate A2.4.2b.1. Установленный worker пока обновлять не нужно;
процедура восстановления новой пары ещё готовится.

1. Прочитайте `info`. Если есть `fencedRequests`, сохраните его `context`
   вместе с создаваемым запросом и idempotency key.
2. Для нового запроса используйте contract 2. Пример envelope:

   ```json
   {
     "requestContractVersion": 2,
     "context": {
       "instanceId": "значение из info",
       "journalIncarnation": "значение из info"
     },
     "request": {
       "method": "submit",
       "request": {
         "protocolVersion": 1,
         "requestId": "новый-correlation-id",
         "projectId": "зарегистрированный-проект",
         "idempotencyKey": "ключ-логической-операции",
         "timeoutMs": 60000,
         "command": {
           "kind": "recipe",
           "recipeId": "зарегистрированный-рецепт",
           "arguments": []
         }
       }
     }
   }
   ```

3. CLI: `winbridge-controller --config CONFIG invoke-v2 ENVELOPE.json`.
   MCP: `winbridge_invoke_fenced` с этим envelope в arguments.
4. Сохраните Task ID из `response.task`, читайте status/output до terminal outcome.
   `outcome: responded` и exit 0 CLI означают получение ответа RPC, не успех команды.
5. Если ответ потерян, сохраните весь envelope. Повтор использует исходные
   context/key. Новый Info или обновлённый config не заменяют incarnation
   ожидающего запроса. `unconfirmed` — исход пока неизвестен.

При отказе по другой incarnation не направляйте прежний request в новую область.
Новый запрос после recovery — отдельное явное действие после безопасного cutover.
Историю прежней пары сохраняют в архиве. Автоматического fallback на v1 нет.
Maintenance/config save/cleanup доступны только через local Windows IPC.

[Контракт, ревью и границы готовности](../plans/architecture-transition/fenced-requests.md).
