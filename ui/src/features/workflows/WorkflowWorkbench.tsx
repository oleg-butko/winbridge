import { useEffect, useRef, useState } from "react";
import { Alert, Badge, Button, Card, Code, Group, Select, Stack, Table, Text, Textarea, Timeline, Title } from "@mantine/core";
import { expectResult, workerCall } from "../../runtime/adapter";
import type { ApplicationRecord, ApplicationStatus, LogCursor, WorkerInfo, WorkflowRecord, WorkflowStepStatus, WorkflowSubmission } from "../../runtime/contracts";
import { statuses } from "../tasks/TaskDetail";

export type WorkflowSnapshot = { projectId: string; snapshotId: string; manifestSha256: string };
const steps: Record<string, string> = { import: "Импорт", build: "Сборка", start: "Запуск процесса", readiness: "Проверка готовности", log: "Подключение лога" };
const stepStates: Record<WorkflowStepStatus, string> = { pending: "Ожидает", running: "Выполняется", succeeded: "Подтверждено", failed: "Ошибка", cancelled: "Отменено", skipped: "Пропущено", indeterminate: "Исход неизвестен" };
const appStates: Record<ApplicationStatus, { label: string; color: string }> = { starting: { label: "Ожидается готовность", color: "blue" }, ready: { label: "Приложение готово", color: "teal" }, stopping: { label: "Остановка запрошена", color: "orange" }, stopped: { label: "Приложение остановлено", color: "gray" }, exited: { label: "Приложение завершилось", color: "gray" }, failed: { label: "Ошибка приложения", color: "red" }, indeterminate: { label: "Исход приложения неизвестен", color: "yellow" } };
const terminalApp = (app: ApplicationRecord) => !["starting", "ready", "stopping"].includes(app.status);

function ApplicationLog({ applicationId, connected }: { applicationId: string; connected: boolean }) {
  const [content, setContent] = useState("");
  const [path, setPath] = useState("");
  const [offset, setOffset] = useState(0);
  const [reset, setReset] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState<number | null>(null);
  const cursor = useRef<LogCursor | null>(null);
  const decoder = useRef(new TextDecoder());
  useEffect(() => { cursor.current = null; decoder.current = new TextDecoder(); setContent(""); setReset(null); setPath(""); setConfirmed(null); }, [applicationId]);
  useEffect(() => {
    if (!connected) return;
    let disposed = false; let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const page = expectResult(await workerCall({ method: "read_application_log", applicationId, cursor: cursor.current, maxBytes: 65536 }), "log").page;
        if (disposed) return;
        if (page.resetReason) { decoder.current = new TextDecoder(); setContent(""); setReset(page.resetReason); }
        const text = decoder.current.decode(new Uint8Array(page.bytes), { stream: true });
        cursor.current = page.cursor; setPath(page.path); setOffset(page.cursor.offset); setConfirmed(Date.now()); setError(null);
        setContent((previous) => [...previous + text].slice(-65536).join(""));
      } catch (error) { if (!disposed) setError(String(error)); }
      if (!disposed) timer = setTimeout(() => void poll(), 750);
    }
    void poll(); return () => { disposed = true; clearTimeout(timer); };
  }, [applicationId, connected]);
  return <Stack gap="xs">
    <Title order={4}>Лог приложения</Title>
    <Text size="xs">{path || "Ожидается путь зарегистрированного лога"} · UTF-8 · offset {offset}</Text>
    <Text size="xs">Показаны последние 65 536 символов. Подтверждение: {confirmed === null ? "ещё не получено" : new Date(confirmed).toLocaleTimeString()}</Text>
    {(!connected || error) && <Alert color="yellow" title="Нет свежего чтения лога">{error ?? "Worker недоступен; сохранены последние данные."}</Alert>}
    {reset && <Alert color="orange">Курсор лога сброшен: {reset}. Показан новый файл или содержимое после изменения.</Alert>}
    <Code block>{content || "Новых записей пока нет"}</Code>
  </Stack>;
}

export function WorkflowWorkbench({ info, connected, closing, snapshot, onTask }: { info: WorkerInfo; connected: boolean; closing: boolean; snapshot: WorkflowSnapshot | null; onTask: (id: string) => void }) {
  const [profile, setProfile] = useState("");
  const [flows, setFlows] = useState<WorkflowRecord[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [record, setRecord] = useState<WorkflowRecord | null>(null);
  const [applications, setApplications] = useState<ApplicationRecord[]>([]);
  const [appId, setAppId] = useState<string | null>(null);
  const [application, setApplication] = useState<ApplicationRecord | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  const [pending, setPending] = useState<WorkflowSubmission | null>(null);
  const [busy, setBusy] = useState(false);
  const selectedRef = useRef(selected); selectedRef.current = selected;
  const appRef = useRef(appId); appRef.current = appId;
  const snapshotKey = snapshot ? JSON.stringify([info.instanceId, snapshot, info.projects.find((p) => p.id === snapshot.projectId)?.workspaceRoot]) : null;
  useEffect(() => {
    if (!snapshot) return;
    const project = info.projects.find((p) => p.id === snapshot.projectId); if (!project) return;
    const separator = info.platform === "windows" ? "\\" : "/";
    const join = (...parts: string[]) => [project.workspaceRoot.replace(/[\\/]$/, ""), ...parts].join(separator);
    setProfile(JSON.stringify({ name: `${project.id} · build/start`, ...snapshot, timeoutMs: 180000,
      build: project.recipes.some((r) => r.id === "snapshot-build") ? { kind: "recipe", recipeId: "snapshot-build", arguments: [] }
        : { kind: "direct", executable: "cargo", arguments: ["build", "--locked", "--offline", "--target-dir", join("build-cache")], workingDirectory: "." },
      start: { kind: "direct", executable: join("build-cache", "debug", `snapshot-fixture${info.platform === "windows" ? ".exe" : ""}`), arguments: ["serve", join("logs", "app.log"), "ready"], workingDirectory: "." },
      readiness: { stdoutContains: "READY Привет ✓", timeoutMs: 10000 }, logPath: "logs/app.log" }, null, 2));
  }, [snapshotKey]);
  useEffect(() => {
    let disposed = false; let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const flows = expectResult(await workerCall({ method: "list_workflows" }), "workflows").workflows;
        const id = selectedRef.current ?? flows[0]?.workflowId;
        const flow = id ? expectResult(await workerCall({ method: "get_workflow", workflowId: id }), "workflow").workflow : null;
        const list = expectResult(await workerCall({ method: "list_applications" }), "applications").applications;
        const applicationId = appRef.current ?? flow?.applicationId;
        const current = applicationId ? expectResult(await workerCall({ method: "get_application", applicationId }), "application").application : null;
        if (!disposed) { setFlows(flows); setRecord(flow); setApplications(list); setApplication(current); setPollError(null); }
      } catch (error) { if (!disposed) setPollError(String(error)); }
      if (!disposed) timer = setTimeout(() => void poll(), 750);
    }
    void poll(); return () => { disposed = true; clearTimeout(timer); };
  }, []);
  async function send(request: WorkflowSubmission) {
    setBusy(true); setPending(request); setError(null);
    try {
      const response = await workerCall({ method: "submit_workflow", request });
      if (response.kind === "error") { setPending(null); throw new Error(response.message); }
      const accepted = expectResult(response, "workflow").workflow;
      setSelected(accepted.workflowId); setAppId(null); setRecord(accepted); setPending(null);
    } catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  }
  async function submit() {
    try {
      if (profile.length > 65536) throw new Error("Профиль workflow превышает 64 KiB");
      const parsed: unknown = JSON.parse(profile);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("Профиль должен быть JSON-объектом");
      await send({ ...parsed, protocolVersion: info.protocolVersion, requestId: crypto.randomUUID(), idempotencyKey: crypto.randomUUID() } as WorkflowSubmission);
    } catch (error) { setError(String(error)); }
  }
  async function cancel() {
    if (!record) return; setBusy(true);
    try { setRecord(expectResult(await workerCall({ method: "cancel_workflow", workflowId: record.workflowId }), "workflow").workflow); }
    catch (error) { setError(String(error)); } finally { setBusy(false); }
  }
  async function stop() {
    if (!application) return; setBusy(true);
    try { setApplication(expectResult(await workerCall({ method: "stop_application", applicationId: application.applicationId }), "application").application); }
    catch (error) { setError(String(error)); } finally { setBusy(false); }
  }
  function restoreProfile() {
    if (!record) return;
    const { protocolVersion: _version, requestId: _request, idempotencyKey: _key, ...saved } = record.submission;
    setProfile(JSON.stringify(saved, null, 2));
  }
  return <Card withBorder><Stack>
    <Title order={3}>Цикл разработки</Title>
    <Text size="sm">Импорт → сборка → запуск → готовность → лог. Ошибка блокирует следующие шаги. Worker сохраняет принятый профиль и ID; закрытие окна или потеря controller не повторяет команды.</Text>
    <Text size="sm">Выберите snapshot в «Доставка исходников» и нажмите «Использовать в workflow». Шаблон настроен для snapshot-fixture; его параметры можно изменить или восстановить из истории.</Text>
    <Textarea label="Параметры workflow — JSON" description="Проверьте команды, общий timeout, сигнал готовности в stdout и путь UTF-8 лога внутри проекта. Готовность подтверждается сигналом этого процесса. Приложение работает до явной остановки или своего завершения." value={profile} onChange={(e) => setProfile(e.currentTarget.value)} autosize minRows={5} maxRows={15} disabled={busy || pending !== null || closing} />
    <Button onClick={() => void submit()} loading={busy} disabled={!connected || !profile || pending !== null || closing}>Запустить workflow</Button>
    {pending && <Alert color="yellow" title="Приём workflow не подтверждён">Повтор использует те же параметры и ключ; новую цепочку не создаёт.
      <Button onClick={() => void send(pending)} loading={busy} disabled={!connected || closing}>Проверить приём workflow</Button></Alert>}
    {error && <Alert color="yellow" title="Операция workflow не подтверждена">{error}</Alert>}
    {(pollError || !connected) && <Alert color="yellow" title="Нет свежего состояния workflow">{pollError ?? "Worker недоступен; показаны последние известные записи."}</Alert>}
    <Select label="Последние 10 workflow" value={selected ?? record?.workflowId ?? null} onChange={(id) => { setSelected(id); setAppId(null); }} data={flows.map((f) => ({ value: f.workflowId, label: `${f.submission.name} · ${statuses[f.status].label} · ${f.workflowId}` }))} searchable />
    {record && <>
      <Group><Badge color={statuses[record.status].color}>{statuses[record.status].label}</Badge><Text>Workflow ID: {record.workflowId}</Text></Group>
      <Text size="xs">Snapshot {record.submission.snapshotId} · manifest {record.submission.manifestSha256}</Text>
      {record.reason && <Alert color={statuses[record.status].color}>{record.reason}</Alert>}
      <Group><Button variant="light" onClick={restoreProfile} disabled={busy || pending !== null || closing}>Заполнить профиль из workflow</Button>
        <Button color="orange" variant="light" onClick={() => void cancel()} disabled={!connected || busy || record.cancelRequested || !["queued", "running"].includes(record.status) || closing}>Отменить workflow</Button></Group>
      {record.cancelRequested && !record.finishedAtMs && <Text>Запрошена отмена; ожидается остановка дочерних операций.</Text>}
      <Table.ScrollContainer minWidth={500}><Table><Table.Thead><Table.Tr><Table.Th>Шаг</Table.Th><Table.Th>Состояние</Table.Th><Table.Th>Результат</Table.Th></Table.Tr></Table.Thead><Table.Tbody>
        {record.steps.map((step) => <Table.Tr key={step.stepId}><Table.Td>{steps[step.name] ?? step.name}<Text size="xs">{step.stepId}</Text></Table.Td><Table.Td>{stepStates[step.status]}</Table.Td><Table.Td>{step.reason}
          {step.operationId && <Text size="xs">Operation ID: {step.operationId}</Text>}
          {step.operationId && ["build", "start"].includes(step.name) && <Button size="xs" variant="light" onClick={() => onTask(step.operationId!)}>Открыть задачу {step.name}</Button>}
        </Table.Td></Table.Tr>)}
      </Table.Tbody></Table></Table.ScrollContainer>
      <Timeline active={record.events.length - 1}>{record.events.map((event) => <Timeline.Item key={event.sequence} title={statuses[event.status].label}><Text size="sm">{event.summary}</Text></Timeline.Item>)}</Timeline>
    </>}
    <Title order={4}>Приложения</Title>
    <Select label="Последние 10 приложений" value={appId ?? application?.applicationId ?? null} onChange={setAppId} data={applications.map((a) => ({ value: a.applicationId, label: `${a.projectId} · ${appStates[a.status].label} · ${a.applicationId}` }))} />
    {application && <>
      <Group><Badge color={appStates[application.status].color}>{appStates[application.status].label}</Badge><Text size="sm">PID: {application.processId ?? "ещё не подтверждён"}</Text></Group>
      <Text size="xs">Application ID: {application.applicationId} · workflow {application.workflowId} · task {application.taskId}</Text>
      <Text size="sm">Готовность: {application.readyAtMs === null ? "не подтверждена" : new Date(application.readyAtMs).toLocaleString()}</Text>
      {application.reason && <Alert color={appStates[application.status].color}>{application.reason}</Alert>}
      <Button color="orange" variant="light" onClick={() => void stop()} disabled={!connected || busy || terminalApp(application) || application.status === "stopping" || closing}>Остановить приложение</Button>
      <ApplicationLog key={application.applicationId} applicationId={application.applicationId} connected={connected} />
    </>}
  </Stack></Card>;
}
