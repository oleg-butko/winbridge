-- Synthetic historical schema. Never run against a user's journal.
BEGIN IMMEDIATE;
CREATE TABLE tasks (id TEXT PRIMARY KEY, project TEXT NOT NULL, key TEXT NOT NULL, payload TEXT NOT NULL, record TEXT NOT NULL, UNIQUE(project,key));
CREATE TABLE output (task TEXT NOT NULL REFERENCES tasks(id), sequence INTEGER NOT NULL, stream TEXT NOT NULL, offset INTEGER NOT NULL, bytes BLOB NOT NULL, PRIMARY KEY(task,sequence));
INSERT INTO tasks(id,project,key,payload,record) VALUES (
  '00000000-0000-4000-8000-000000000002',
  'legacy-project',
  'legacy-task-1',
  '{"protocolVersion":1,"requestId":"","projectId":"legacy-project","idempotencyKey":"legacy-task-1","timeoutMs":60000,"command":{"kind":"direct","executable":"K:\\fixtures\\tool.exe","arguments":["success","Привет ✓","a & b","$value"],"workingDirectory":"src"}}',
  '{"taskId":"00000000-0000-4000-8000-000000000002","submission":{"protocolVersion":1,"requestId":"submission-correlation-1","projectId":"legacy-project","idempotencyKey":"legacy-task-1","timeoutMs":60000,"command":{"kind":"direct","executable":"K:\\fixtures\\tool.exe","arguments":["success","Привет ✓","a & b","$value"],"workingDirectory":"src"}},"resolvedCommand":{"executable":"K:\\fixtures\\tool.exe","arguments":["success","Привет ✓","a & b","$value"],"workingDirectory":"K:\\fixtures\\src","timeoutMs":60000},"status":"running","acceptedAtMs":1000,"startedAtMs":1001,"finishedAtMs":null,"processId":null,"exitCode":null,"reason":null,"cancelRequested":false,"outputBytes":0,"outputTruncated":false,"events":[{"sequence":1,"occurredAtMs":1000,"status":"queued","summary":"Task accepted and committed to the local journal"},{"sequence":2,"occurredAtMs":1001,"status":"running","summary":"Process launch intent committed before spawning"}]}');
PRAGMA user_version=1;
COMMIT;
