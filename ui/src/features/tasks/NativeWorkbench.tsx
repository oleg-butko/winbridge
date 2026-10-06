import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Card, Code, Group, NumberInput, Select, Stack, Text, Textarea, TextInput, Title } from "@mantine/core";
import { expectResult, workerCall } from "../../runtime/adapter";
import { emptyCommandForm, loadCommandDrafts, sameDraftScope, saveCommandDraft, saveDeliveryDraft, saveWorkflowProfile } from "../../runtime/commandDrafts";
import type { CommandForm } from "../../runtime/commandDrafts";
import type { CommandDraft, DeliveryDraft, OutputChunk, TaskRecord, TaskSummary, TaskSubmission, WorkerInfo } from "../../runtime/contracts";
import { SnapshotWorkbench } from "../snapshots/SnapshotWorkbench";
import { WorkflowWorkbench } from "../workflows/WorkflowWorkbench";
import type { WorkflowSnapshot } from "../workflows/WorkflowWorkbench";
import { TaskDetail, statuses } from "./TaskDetail";

function TaskSession({ id }: { id: string }) {
  const [task, setTask] = useState<TaskRecord | null>(null);
  const [chunks, setChunks] = useState<OutputChunk[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [confirmedAt, setConfirmedAt] = useState<number | null>(null);
  const [cancelling, setCancelling] = useState(false);
  useEffect(() => {
    let disposed = false;
    let afterSequence = 0;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const record = expectResult(await workerCall({ method: "get_task", taskId: id }), "task").task;
        const page = expectResult(await workerCall({ method: "read_output", taskId: id, afterSequence }), "output").chunks;
        if (disposed) return;
        setTask(record); setConfirmedAt(Date.now()); setError(null);
        if (page.length) { afterSequence = page.at(-1)!.sequence; setChunks((previous) => [...previous, ...page]); }
      } catch (error) { if (!disposed) setError(String(error)); }
      if (!disposed) timer = setTimeout(() => void poll(), 750);
    }
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, [id]);
  async function cancel() {
    setCancelling(true);
    try { setTask(expectResult(await workerCall({ method: "cancel", taskId: id }), "task").task); }
    catch (error) { setError(String(error)); }
    finally { setCancelling(false); }
  }
  return <Stack>
    {error && <Alert color="yellow" title="Нет свежего подтверждения worker">{error}<Text mt="xs">Последнее подтверждение: {confirmedAt === null ? "ещё не получено" : new Date(confirmedAt).toLocaleString()}. Последний известный статус сохранён; повторный запуск не выполняется.</Text></Alert>}
    {task ? <>
      <Group justify="space-between"><Text size="xs" c="dimmed">Последнее подтверждение: {confirmedAt && new Date(confirmedAt).toLocaleTimeString()}</Text>
        <Button color="orange" variant="light" disabled={!["queued", "running"].includes(task.status) || task.cancelRequested} loading={cancelling} onClick={() => void cancel()}>Отменить задачу</Button>
      </Group>
      <TaskDetail task={task} chunks={chunks} />
    </> : <Text>Читается сохранённая задача…</Text>}
  </Stack>;
}

export function NativeWorkbench() {
  const [info, setInfo] = useState<WorkerInfo | null>(null);
  const [workflowSnapshot, setWorkflowSnapshot] = useState<WorkflowSnapshot | null>(null);
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [form, setForm] = useState<CommandForm>(emptyCommandForm);
  const { recipeId, executable, argumentsJson: argv, workingDirectory: cwd, timeoutSeconds: timeout } = form;
  const [drafts, setDrafts] = useState<CommandDraft[] | null>(null);
  const [deliveryDrafts, setDeliveryDrafts] = useState<DeliveryDraft[] | null>(null);
  const [workflowProfiles, setWorkflowProfiles] = useState<import("../../runtime/contracts").WorkflowProfile[] | null>(null);
  const [desktopBuild, setDesktopBuild] = useState<import("../../runtime/contracts").BuildIdentity | null>(null);
  const [loadedScope, setLoadedScope] = useState<string | null>(null);
  const loadedScopeRef = useRef<string | null>(null);
  const saveQueue = useRef<Promise<void>>(Promise.resolve());
  const scopedSaves = useRef(new Map<string, Promise<void>>());
  const saveSequence = useRef(0);
  const [saveStatus, setSaveStatus] = useState("Поля пока не сохранены");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [closeError, setCloseError] = useState<string | null>(null);
  const [closing, setClosing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [connected, setConnected] = useState(false);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [pending, setPending] = useState<TaskSubmission | null>(null);
  const selectedRef = useRef(selectedId); selectedRef.current = selectedId;
  useEffect(() => { let disposed = false; void invoke<import("../../runtime/contracts").BuildIdentity>("desktop_build").then((build) => { if (!disposed) setDesktopBuild(build); }).catch(() => {}); return () => { disposed = true; }; }, []);
  useEffect(() => {
    let disposed = false;
    void loadCommandDrafts().then((state) => {
      if (!disposed) { setDrafts(state.drafts); setDeliveryDrafts(state.deliveryDrafts); setWorkflowProfiles(state.workflowProfiles ?? []); }
    }).catch((error) => {
      if (!disposed) { setDrafts([]); setDeliveryDrafts([]); setSaveError(String(error)); }
    });
    return () => { disposed = true; };
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested(async (event) => {
      event.preventDefault();
      setClosing(true);
      try { await Promise.all(scopedSaves.current.values()); await getCurrentWindow().destroy(); }
      catch (error) { setClosing(false); setCloseError(String(error)); }
    }).then((stop) => { if (disposed) stop(); else unlisten = stop; })
      .catch((error) => { if (!disposed) setSaveError(String(error)); });
    return () => { disposed = true; unlisten?.(); };
  }, []);
  useEffect(() => {
    let disposed = false; let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const info = expectResult(await workerCall({ method: "info" }), "info").info;
        const tasks = expectResult(await workerCall({ method: "list_tasks" }), "tasks").tasks;
        if (disposed) return;
        setInfo(info); setTasks(tasks); setConnected(true); setConnectionError(null);
        if (!selectedRef.current && tasks.length) setSelectedId(tasks[0].taskId);
      } catch (error) { if (!disposed) { setConnected(false); setConnectionError(String(error)); } }
      if (!disposed) timer = setTimeout(() => void poll(), 1000);
    }
    void poll(); return () => { disposed = true; clearTimeout(timer); };
  }, []);
  useEffect(() => {
    if (!info || drafts === null) return;
    setProjectId((id) => {
      if (info.projects.some((project) => project.id === id)) return id;
      const latest = [...drafts].reverse().find((draft) => draft.workerInstanceId === info.instanceId
        && info.projects.some((project) => project.id === draft.projectId && project.workspaceRoot === draft.workspaceRoot));
      return latest?.projectId ?? info.projects[0]?.id ?? null;
    });
  }, [info, drafts]);
  const project = info?.projects.find((project) => project.id === projectId);
  const controller = info?.controllerConnection;
  const controllerFresh = connected && controller?.state === "connected";
  const scope = info && project ? { workerInstanceId: info.instanceId, projectId: project.id, workspaceRoot: project.workspaceRoot } : null;
  const scopeKey = scope ? JSON.stringify(scope) : null;
  useEffect(() => {
    if (!scope || drafts === null || loadedScopeRef.current === scopeKey) return;
    const saved = drafts.find((draft) => sameDraftScope(draft, scope));
    const restored = saved ? { ...saved, recipeId: project?.recipes.some((recipe) => recipe.id === saved.recipeId) ? saved.recipeId : null } : emptyCommandForm;
    setForm(restored); setLoadedScope(scopeKey); loadedScopeRef.current = scopeKey;
    saveSequence.current += 1;
    setSaveStatus(saved ? "Поля восстановлены из локального файла" : "Поля пока не сохранены");
    // scopeKey includes all identity fields; polling the same worker must not reset edits.
  }, [scopeKey, drafts]);
  const formReady = scopeKey !== null && loadedScope === scopeKey;
  function updateForm(change: Partial<CommandForm>) {
    if (!scope || !formReady) return;
    const next = { ...form, ...change };
    setForm(next);
    const draft: CommandDraft = { ...scope, ...next };
    setDrafts((saved) => [...(saved ?? []).filter((previous) => !sameDraftScope(previous, scope)), draft]);
    const sequence = ++saveSequence.current;
    setSaveStatus("Сохранение полей…"); setSaveError(null); setCloseError(null);
    const pendingSave = saveQueue.current.catch(() => {}).then(() => saveCommandDraft(draft));
    saveQueue.current = pendingSave;
    scopedSaves.current.set(`command:${scopeKey}`, pendingSave);
    void pendingSave.then(() => {
      if (sequence === saveSequence.current) setSaveStatus("Поля сохранены");
    }).catch((error) => {
      if (sequence === saveSequence.current) { setSaveStatus("Поля не сохранены"); setSaveError(String(error)); }
    });
  }
  function persistDelivery(draft: DeliveryDraft): Promise<void> {
    setDeliveryDrafts((saved) => [...(saved ?? []).filter((previous) => previous.workerInstanceId !== draft.workerInstanceId
      || previous.projectId !== draft.projectId || previous.workspaceRoot !== draft.workspaceRoot), draft]);
    setCloseError(null);
    const pendingSave = saveQueue.current.catch(() => {}).then(() => saveDeliveryDraft(draft));
    saveQueue.current = pendingSave;
    scopedSaves.current.set(`delivery:${JSON.stringify([draft.workerInstanceId, draft.projectId, draft.workspaceRoot])}`, pendingSave);
    return pendingSave;
  }
  function persistWorkflow(profile: import("../../runtime/contracts").WorkflowProfile): Promise<void> {
    setWorkflowProfiles((saved) => [...(saved ?? []).filter((p) => p.workerInstanceId !== profile.workerInstanceId
      || p.projectId !== profile.projectId || p.workspaceRoot !== profile.workspaceRoot || p.name !== profile.name), profile]);
    setCloseError(null);
    const pendingSave = saveQueue.current.catch(() => {}).then(() => saveWorkflowProfile(profile));
    saveQueue.current = pendingSave;
    scopedSaves.current.set(`workflow:${JSON.stringify([profile.workerInstanceId, profile.projectId, profile.workspaceRoot, profile.name])}`, pendingSave);
    return pendingSave;
  }
  async function send(request: TaskSubmission) {
    setSubmitting(true); setPending(request); setError(null);
    try {
      const response = await workerCall({ method: "submit", request });
      if (response.kind === "error") { setPending(null); throw new Error(response.message); }
      const task = expectResult(response, "task").task;
      setSelectedId(task.taskId); setPending(null);
    } catch (error) { setError(String(error)); }
    finally { setSubmitting(false); }
  }
  async function submit() {
    try {
      if (!project) throw new Error("Выберите зарегистрированный проект");
      const argumentsList: unknown = JSON.parse(argv);
      if (!Array.isArray(argumentsList) || !argumentsList.every((value) => typeof value === "string")) throw new Error('Аргументы должны быть JSON-массивом строк, например ["--flag", "значение с пробелами"]');
      if (!Number.isFinite(Number(timeout)) || Number(timeout) <= 0) throw new Error("Укажите положительный timeout в секундах");
      if (!recipeId && !executable.trim()) throw new Error("Укажите executable или выберите рецепт");
      await send({ protocolVersion: info!.protocolVersion, projectId: project.id, requestId: crypto.randomUUID(), idempotencyKey: crypto.randomUUID(), timeoutMs: Math.round(Number(timeout) * 1000),
        snapshotId: form.snapshotId, command: recipeId ? { kind: "recipe", recipeId, arguments: [] } : { kind: "direct", executable, arguments: argumentsList, workingDirectory: cwd } });
    } catch (error) { setError(String(error)); }
  }
  return <Stack gap="lg">
    <Alert color={connected ? "teal" : "yellow"} title={connected ? `Worker подключён · ${info?.platform} · ${info?.instanceId}` : "Worker недоступен"}>
      {connected ? "Данные поступают из локального журнала worker. Закрытие окна не отменяет принятую задачу." : connectionError ?? "Ожидается соединение с локальным worker."}
      {info && <Text size="xs" mt="xs">{info.runtimeValidation}</Text>}
      {info && <Text size="xs">Версия worker: {info.build ? `${info.build.version} · commit ${info.build.sourceCommit ?? "не указан"}${info.build.sourceDirty ? " · изменённые исходники" : ""} · build ${info.build.buildId}` : "этот выпуск не передаёт сведения о сборке"} · generation {info.generation}</Text>}
      <Text size="xs">Версия desktop: {desktopBuild ? `${desktopBuild.version} · commit ${desktopBuild.sourceCommit ?? "не указан"}${desktopBuild.sourceDirty ? " · изменённые исходники" : ""} · build ${desktopBuild.buildId}` : "сведения о сборке ещё не получены"}</Text>
    </Alert>
    <Alert color={controllerFresh ? "teal" : "yellow"} title={controllerFresh ? "Controller Debian подключён" : !info ? "Состояние controller пока неизвестно" : controller ? connected && controller.state === "connecting" ? "Подключение к controller Debian…" : "Нет свежего соединения с controller Debian" : "Управление из Debian не настроено"}>
      {controller ? <>
        <Text size="sm">{controller.url} · {controller.controllerNodeId ?? "node controller ещё не получен"}</Text>
        <Text size="xs">Последнее подтверждение: {controller.lastConfirmedAtMs === null ? "ещё не получено" : new Date(controller.lastConfirmedAtMs).toLocaleString()} · session: {controller.sessionId ?? "ещё не создана"}</Text>
        {!connected && <Text size="sm">Worker недоступен; сохранены последние известные данные соединения.</Text>}
        {controller.error && <Text size="sm">{controller.error}</Text>}
        {!controllerFresh && <Text size="sm">Принятая задача принадлежит worker и продолжает выполняться. Автоматического повторного запуска нет.</Text>}
      </> : <Text size="sm">{info ? "Для связи указать локальный ws:// адрес Debian в worker.controllerUrl. Локальные задачи доступны через worker." : "Ожидаются данные локального worker о соединении с Debian."}</Text>}
    </Alert>
    {info?.snapshotImportAvailable && <SnapshotWorkbench info={info} connected={connected} closing={closing} drafts={deliveryDrafts} persist={persistDelivery} onWorkflowSnapshot={info.workflowAvailable ? setWorkflowSnapshot : undefined} />}
    {info?.workflowAvailable && <WorkflowWorkbench info={info} connected={connected} closing={closing} profiles={workflowProfiles} persist={persistWorkflow} snapshot={workflowSnapshot} onTask={(id) => { setSelectedId(id); requestAnimationFrame(() => document.getElementById("native-task-detail")?.scrollIntoView({ behavior: "smooth" })); }} />}
    <Card withBorder><Stack>
      <Title order={3}>Запустить команду</Title>
      <Select label="Зарегистрированный проект" value={projectId} onChange={setProjectId} data={info?.projects.map((p) => ({ value: p.id, label: p.id })) ?? []} disabled={submitting || pending !== null || closing || drafts === null} />
      {project && <Text size="xs" c="dimmed">Рабочий корень: {project.workspaceRoot}</Text>}
      {info?.snapshotImportAvailable && <TextInput label="Snapshot ID для сборки" description="Пустое поле использует корень проекта. Для snapshot worker проверит исходники перед запуском; cwd задаётся внутри поколения." value={form.snapshotId ?? ""} onChange={(e) => updateForm({snapshotId:e.currentTarget.value || undefined})} disabled={pending !== null || !formReady || closing} />}
      <Select label="Способ запуска" value={recipeId ? `recipe:${recipeId}` : "direct"} onChange={(id) => updateForm({ recipeId: id?.startsWith("recipe:") ? id.slice(7) : null })} data={[
        ...(project?.allowDirectCommands ? [{ value: "direct", label: "Прямая команда" }] : []),
        ...(project?.recipes.map((r) => ({ value: `recipe:${r.id}`, label: `Рецепт: ${r.id}` })) ?? []),
      ]} disabled={submitting || pending !== null || !formReady || closing} />
      {recipeId ? <Code block>{JSON.stringify(project?.recipes.find((r) => r.id === recipeId)?.command, null, 2)}</Code> : <>
        <TextInput label="Executable" placeholder="powershell.exe или полный путь к .exe" value={executable} onChange={(e) => updateForm({ executable: e.currentTarget.value })} disabled={pending !== null || !formReady || closing} />
        <Textarea label="Аргументы — JSON-массив строк" value={argv} onChange={(e) => updateForm({ argumentsJson: e.currentTarget.value })} autosize minRows={2} disabled={pending !== null || !formReady || closing} />
        <TextInput label="Рабочая папка относительно корня проекта" value={cwd} onChange={(e) => updateForm({ workingDirectory: e.currentTarget.value })} disabled={pending !== null || !formReady || closing} />
      </>}
      <NumberInput label="Timeout, секунд" value={timeout} onChange={(value) => updateForm({ timeoutSeconds: String(value) })} min={0.1} max={86400} disabled={pending !== null || !formReady || closing} />
      <Text size="xs" c="dimmed">{saveStatus}. Восстановление формы не запускает команду.</Text>
      {saveError && <Alert color="yellow" title="Не удалось сохранить поля команды">{saveError}</Alert>}
      {closeError && <Alert color="yellow" title="Окно осталось открытым: поля не сохранены">{closeError}
        <Button mt="sm" variant="light" onClick={() => { void getCurrentWindow().destroy().catch((error) => setCloseError(String(error))); }}>Закрыть без сохранения</Button>
      </Alert>}
      {error && connected && <Alert color="red" title="Запрос не завершён">{error}</Alert>}
      {pending && !submitting && <Alert color="yellow" title="Приём задачи не подтверждён">
        Worker мог сохранить задачу до потери ответа. Повтор отправит прежние параметры и тот же ключ, чтобы получить ту же задачу.
        <Button mt="sm" onClick={() => void send(pending)}>Проверить приём — повторить тот же запрос</Button>
      </Alert>}
      <Button onClick={() => void submit()} loading={submitting} disabled={!connected || pending !== null || !project || !formReady || closing || (!recipeId && !project.allowDirectCommands)}>Передать задачу worker</Button>
    </Stack></Card>
    <Select label="Последние 10 задач" value={selectedId} onChange={setSelectedId} searchable data={tasks.map((t) => ({ value: t.taskId, label: `${t.projectId} · ${statuses[t.status].label} · ${t.taskId}` }))} />
    {selectedId ? <div id="native-task-detail"><TaskSession key={selectedId} id={selectedId} /></div> : <Text c="dimmed">Задач пока нет. Выберите проект и передайте команду worker.</Text>}
  </Stack>;
}
