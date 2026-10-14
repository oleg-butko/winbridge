# A3.2b — runtime producer и CLI/MCP readiness

Дата: 2026-10-14. После [A3.2a](readiness-foundation.md).
Пакет реализован локально; полный paired gate ещё впереди. **A3.2/A3 открыт.**

## Контракт

Controller-local IPC contract 1 отделён от frozen worker RPC 1/2. Три операции:
`contract_readiness`, `verify_pair`, `get_verification`. CLI `readiness QUERY.json`,
`verify-pair REQUEST.json`, `verification KEY`; MCP соответствующие отдельные tools.
Readiness возвращает exact observed identity, session/controller generation,
четыре факта, typed причины и явное имя test profile. Unknown contract не вызывает
worker или legacy fallback. Receipt ID сравнивает историческую квитанцию с
фактическими build/schema/scope/route/policy. Existing A2 admission не меняется.

`verify_pair` получает identity из observation и stable verificationKey.
Оба сохраняются у caller при создании операции; повтор не заменяет их.
First known profile — fixture/RPC-2 admission. Он проверяет envelope, task/duplicate,
output/exit, stale scope rejection и неизменность route. Это не доказательство
семантики всех будущих RPC methods или новых execution permissions.

## Проверки фактических binaries

Consumer proof создаёт настоящий CLI/MCP process integration test: executable
identity и SHA-256, actual WS/IPC route, schema/policy/scope, unsupported no-dispatch
и disconnect no-enable. Deployment запускает test с
`WINBRIDGE_CONSUMER_PROOF_DIRECTORY=<controller state>/consumer-build-verifications-v1`.
Runtime принимает только evidence текущего clean build и текущего executable;
missing/changed proof отвергается до Windows effects.

Provider verifier запускает **установленный exe**, с другим instance ID и новым
локальным journal. Config не содержит controllerUrl; UI и outbound connections
отсутствуют. Проверяются actual binary description/hash, IPC Info/schema/policy,
success+Unicode output, duplicate без нового physical process, stale scope до
dispatch, cancel root+descendant и cleanup. Actual working runtime не заменяется.
Сценарий compiled into controller и читаемо показывается в аргументе `-Command`.

Initial actual provider gate прошёл:
`20261014-180817-61117dcf`, task `8621c915-0dfe-4bfd-b69a-49ce96af958c`, succeeded/0.
Source `b2aeb9f`, actual installed build ID `1792012729498364300`, report SHA-256
`41762fee8f8131d0ae4c49bde3a737cc1511e0a882eecfe942daca2bd9fe344f`.
Все семь checks passed; working build/generation/scope прежние.

## Сохранение и восстановление

Create-only checksum/readback plan сохраняет original identity и оба frozen
FencedRequests до dispatch. Worker task keys детерминированы по stable verification
family/key. ACK/task links и terminal record immutable; partial/corrupt/symlink/FIFO
не интерпретируются как success. Supervisor дедуплицирует concurrent same-key calls.
Get/readiness ничего не запускают. После restart сохранённый plan требует явного
same-key вызова; worker journal сверяет ту же задачу, scope не перепривязывается.
Known failure/unknown outcome не создают success receipt и не запускают команду
автоматически. CLI/MCP acceptance означает операцию, а не готовность контракта.

Post-implementation review: unknown outcome сохраняется отдельным immutable
interruption record, а не terminal failure. Явный same-key retry продолжает
сверку прежних task keys; без supervisor сохранённый plan показывается как
`indeterminate`, поскольку ACK мог потеряться до сохранения Task ID. Known terminal
failure не повторяется. Drop guard освобождает in-memory key, panic executor
попадает в supervisor как unknown outcome. Report/ready schema проверяется явно;
consumer proof создаётся с 0700/0600 и fsync parent directory.

Следом — actual paired producer gate, reconnect/reopen и UI projection через
negotiated readonly metadata. Windows worker `b2aeb9f` уже даёт required policy/schema
inputs. Новое представление для desktop ещё не реализовано; полноту A3.2 не объявлять.
