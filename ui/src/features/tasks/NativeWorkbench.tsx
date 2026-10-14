import { bindWorkerRequest } from "../../runtime/requestContext";
import type { BoundWorkerRequest } from "../../runtime/requestContext";
import { useCallback, useEffect, useRef, useState } from "react";
import { SettingsPanel } from "../settings/SettingsPanel";
import { AttentionInbox } from "../attention/AttentionInbox";
import { CompanionPanel } from "../attention/CompanionPanel";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Card, Code, Group, NumberInput, Select, Stack, Text, Textarea, TextInput, Title } from "@mantine/core";
import { expectResult, workerCall, workerMutation, workerBoundCall } from "../../runtime/adapter";
import { emptyCommandForm, loadCommandDrafts, sameDraftScope, saveCommandDraft, saveDeliveryDraft, saveWorkflowProfile } from "../../runtime/commandDrafts";
import type { CommandForm } from "../../runtime/commandDrafts";
import type { CommandDraft, DeliveryDraft, TaskRecord, TaskSummary, TaskSubmission, WorkerInfo, CompanionStatus, NotificationPreferences, AttentionTarget } from "../../runtime/contracts";
import { catalogSummary } from "../../runtime/discovery";
import { OutputBuffer } from "./outputBuffer";
import { SnapshotWorkbench } from "../snapshots/SnapshotWorkbench";
import { WorkflowWorkbench } from "../workflows/WorkflowWorkbench";
import { HistoryPanel } from "../history/HistoryPanel";
import type { HistoryEntry } from "../../runtime/contracts";
import type { WorkflowSnapshot } from "../workflows/WorkflowWorkbench";
import { closeDeadline, CloseTimeout, desktopEvent } from "../../runtime/desktopEvents";
import type { ClosePhase, CloseSource, DesktopLogStatus } from "../../runtime/desktopEvents";
import { TaskDetail, statuses } from "./TaskDetail";

function TaskSession({ id, readOnly, requestContext }: { id: string; readOnly: boolean; requestContext?: import("../../runtime/contracts").RequestContext }) {
  const [task, setTask] = useState<TaskRecord | null>(null);
  const [output, setOutput] = useState(() => new OutputBuffer().snapshot());
  const [error, setError] = useState<string | null>(null);
  const [confirmedAt, setConfirmedAt] = useState<number | null>(null);
  const [cancelling, setCancelling] = useState(false);
  useEffect(() => {
    let disposed = false;
    let buffer = new OutputBuffer();
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      let backlog = false;
      try {
        const record = expectResult(await workerCall({ method: "get_task", taskId: id }), "task").task;
        const page = expectResult(await workerCall({ method: "read_output", taskId: id, afterSequence: buffer.snapshot().afterSequence }), "output").chunks;
        if (disposed) return;
        setTask(record); setConfirmedAt(Date.now()); setError(null);
        if (record.outputRemovedAtMs) buffer = new OutputBuffer();
        else { buffer.append(page); backlog = page.length > 0; }
        if (!backlog && !["queued", "running"].includes(record.status)) buffer.finish();
        setOutput(buffer.snapshot());
      } catch (error) { if (!disposed) setError(String(error)); }
      if (!disposed) timer = setTimeout(() => void poll(), backlog ? 10 : 750);
    }
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, [id]);
  async function cancel() {
    setCancelling(true);
    try { setTask(expectResult(await workerMutation({ method: "cancel", taskId: id }, requestContext), "task").task); }
    catch (error) { setError(String(error)); }
    finally { setCancelling(false); }
  }
  return <Stack data-confirmed-task-id={task?.taskId}>
    {error && <Alert color="yellow" title="Нет свежего подтверждения worker">{error}<Text mt="xs">Последнее подтверждение: {confirmedAt === null ? "ещё не получено" : new Date(confirmedAt).toLocaleString()}. Последний известный статус сохранён; повторный запуск не выполняется.</Text></Alert>}
    {task ? <>
      <Group justify="space-between"><Text size="xs" c="dimmed">Последнее подтверждение: {confirmedAt && new Date(confirmedAt).toLocaleTimeString()}</Text>
        <Button color="orange" variant="light" disabled={readOnly || !["queued", "running"].includes(task.status) || task.cancelRequested} loading={cancelling} onClick={() => void cancel()}>Отменить задачу</Button>
      </Group>
      <TaskDetail task={task} outputState={output} />
    </> : <Text>Читается сохранённая задача…</Text>}
  </Stack>;
}

export function NativeWorkbench() {
  const [info, setInfo] = useState<WorkerInfo | null>(null);
  const [workflowSnapshot, setWorkflowSnapshot] = useState<WorkflowSnapshot | null>(null);
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [workflowNavigation, setWorkflowNavigation] = useState<{sequence:number;workflowId:string;applicationId:string|null} | null>(null);
  const [importNavigation,setImportNavigation] = useState<{sequence:number;importId:string}|null>(null);
  async function openHistory(entry:HistoryEntry) {
    try {
      if(entry.kind==="import") {
        setImportNavigation((previous)=>({sequence:(previous?.sequence??0)+1,importId:entry.operationId}));
        requestAnimationFrame(()=>{const detail=document.getElementById("native-import-detail");detail?.scrollIntoView();detail?.focus({preventScroll:true});});
      } else if(entry.kind==="application") {
        const workflowId=entry.workflowId??expectResult(await workerCall({method:"get_application",applicationId:entry.operationId}),"application").application.workflowId;
        openAttention({kind:"application",applicationId:entry.operationId,workflowId});
      } else if(entry.kind==="workflow") openAttention({kind:"workflow",workflowId:entry.operationId});
      else openAttention({kind:"task",taskId:entry.operationId});
    }catch(e){setError(String(e));}
  }
  function openAttention(target: AttentionTarget) {
    if (target.kind === "task") setSelectedId(target.taskId);
    else setWorkflowNavigation((previous) => ({sequence:(previous?.sequence ?? 0)+1, workflowId:target.workflowId, applicationId:target.kind === "application" ? target.applicationId : null}));
    requestAnimationFrame(() => {
      const detail = document.getElementById(target.kind === "task" ? "native-task-detail" : "native-workflow-detail");
      detail?.scrollIntoView(); detail?.focus({preventScroll:true});
    });
  }
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
  const [closeReady, setCloseReady] = useState(false);
  const closingRef = useRef(false);
  const closeQuit = useRef(false);
  const closeHandler = useRef<(quit: boolean, source: CloseSource) => Promise<void>>(async () => {});
  const [desktopLog, setDesktopLog] = useState<DesktopLogStatus | null>(null);
  const settingsBlocked = useRef(false);
  const blockSettingsClose = useCallback((blocked:boolean)=>{settingsBlocked.current=blocked;},[]);
  const [companion, setCompanion] = useState<CompanionStatus | null>(null);
  const [preferences, setPreferences] = useState<NotificationPreferences | null>(null);
  const [preferencesError, setPreferencesError] = useState<string | null>(null);
  const [preferencesStatus, setPreferencesStatus] = useState("Настройки ещё не загружены");
  const preferencesSequence = useRef(0);
  const [companionError, setCompanionError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [connected, setConnected] = useState(false);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [pending, setPending] = useState<BoundWorkerRequest<{ method: "submit"; request: TaskSubmission }> | null>(null);
  const selectedRef = useRef(selectedId); selectedRef.current = selectedId;
  useEffect(() => {
    let disposed = false;
    const heartbeat = () => {
      desktopEvent("heartbeat");
      void invoke<DesktopLogStatus>("desktop_log_status").then((status) => { if (!disposed) setDesktopLog(status); }).catch(() => {});
    };
    const error = () => desktopEvent("error");
    heartbeat(); const timer = setInterval(heartbeat, 10000);
    window.addEventListener("error", error); window.addEventListener("unhandledrejection", error);
    return () => { disposed = true; clearInterval(timer); window.removeEventListener("error", error); window.removeEventListener("unhandledrejection", error); };
  }, []);
  useEffect(() => { let disposed = false; void invoke<import("../../runtime/contracts").BuildIdentity>("desktop_build").then((build) => { if (!disposed) setDesktopBuild(build); }).catch(() => {}); return () => { disposed = true; }; }, []);
  useEffect(() => {
    let disposed = false;
    void loadCommandDrafts().then((state) => {
      if (!disposed) { setDrafts(state.drafts); setDeliveryDrafts(state.deliveryDrafts); setWorkflowProfiles(state.workflowProfiles ?? []); setPreferences(state.notificationPreferences); setPreferencesStatus("Настройки загружены из локального файла"); }
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
      await closeHandler.current(false, "titlebar");
    }).then((stop) => { if (disposed) stop(); else { unlisten = stop; setCloseReady(true); } })
      .catch((error) => { if (!disposed) setSaveError(String(error)); });
    return () => { disposed = true; unlisten?.(); };
  }, []);
  async function requestClose(quit: boolean, source: CloseSource = "panel", skipSaves = false) {
    desktopEvent(skipSaves ? "close.force" : "close.request", source, quit);
    if (closingRef.current) { desktopEvent("close.busy", source, quit); return; }
    closingRef.current = true; closeQuit.current = quit; setClosing(true);
    const started = performance.now(); let phase: ClosePhase = "settings";
    try {
      if(!skipSaves && settingsBlocked.current)throw new Error("Сохраните или отмените изменения конфигурации и дождитесь завершения очистки в разделе «Проекты, соединение и хранение».");
      if (!skipSaves) {
        phase = "saves"; desktopEvent("close.wait", source, quit, phase, scopedSaves.current.size);
        await closeDeadline(Promise.all(scopedSaves.current.values()), "saves");
        desktopEvent("close.saved", source, quit, phase);
      }
      phase = "native"; desktopEvent("close.invoke", source, quit, phase);
      await closeDeadline(invoke("companion_close", { quit }), "native");
      desktopEvent("close.done", source, quit, phase, 0, Math.round(performance.now() - started));
      setCloseError(null);
    } catch (error) {
      desktopEvent(error instanceof CloseTimeout ? "close.timeout" : "close.failed", source, quit, phase, 0, Math.round(performance.now() - started));
      setCloseError(String(error));
      requestAnimationFrame(() => document.getElementById("desktop-close-error")?.scrollIntoView({ block: "center" }));
    }
    finally { closingRef.current = false; setClosing(false); }
  }
  closeHandler.current = requestClose;
  useEffect(() => {
    let disposed = false; let timer: ReturnType<typeof setTimeout>;
    let openSequence = 0; let quitSequence = 0;
    async function poll() {
      try {
        const status = await invoke<CompanionStatus>("companion_status");
        if (disposed) return;
        setCompanion(status); setCompanionError(null);
        if (status.openTaskSequence > openSequence) {
          openSequence = status.openTaskSequence;
          const target = status.openTarget ?? (status.openTaskId ? {kind:"task" as const,taskId:status.openTaskId} : null);
          if (target) openAttention(target);
        }
        if (status.quitRequestSequence > quitSequence) { quitSequence = status.quitRequestSequence; void closeHandler.current(true, "tray"); }
      } catch (error) { if (!disposed) setCompanionError(String(error)); }
      if (!disposed) timer = setTimeout(() => void poll(), 750);
    }
    void poll(); return () => { disposed = true; clearTimeout(timer); };
  }, []);
  function persistPreferences(next: NotificationPreferences) {
    setPreferences(next); setPreferencesError(null); setCloseError(null);
    const sequence = ++preferencesSequence.current; setPreferencesStatus("Сохранение настроек…");
    const pendingSave = saveQueue.current.catch(() => {}).then(() => invoke<void>("save_notification_preferences", {preferences:next}));
    saveQueue.current = pendingSave; scopedSaves.current.set("notifications", pendingSave);
    void pendingSave.then(() => { if (sequence === preferencesSequence.current) setPreferencesStatus("Настройки сохранены"); }).catch((error) => { if (sequence === preferencesSequence.current) { setPreferencesError(String(error)); setPreferencesStatus("Настройки не сохранены"); } });
  }
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
  const executionReady = connected && !info?.journalRecoveryRequired;
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
  async function send(request: TaskSubmission, bound?: BoundWorkerRequest<{ method: "submit"; request: TaskSubmission }>) {
    const pinned = bound ?? bindWorkerRequest({ method: "submit", request }, info?.fencedRequests?.context);
    setSubmitting(true); setPending(pinned); setError(null);
    try {
      const response = await workerBoundCall(pinned);
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
  return <Stack gap="lg" data-close-ready={closeReady}>
    <nav aria-label="Разделы рабочего окна"><Group gap="xs">{[
      ["native-command","Команда"],["native-attention","Внимание"],["native-history","История"],["native-settings","Настройки"],["native-companion","Tray и уведомления"]
    ].map(([id,label])=><Button key={id} variant="subtle" size="xs" onClick={()=>{const target=document.getElementById(id);target?.scrollIntoView({block:"start"});target?.focus({preventScroll:true});}}>{label}</Button>)}</Group></nav>
    <Alert color={connected ? "teal" : "yellow"} title={connected ? `Worker подключён · ${info?.platform} · ${info?.instanceId}` : "Worker недоступен"}>
      {connected ? "Данные поступают из локального журнала worker. Закрытие окна не отменяет принятую задачу." : connectionError ?? "Ожидается соединение с локальным worker."}
      {info && <Text size="xs" mt="xs">{info.runtimeValidation}</Text>}
      {info && <details><summary>Совместимость компонентов</summary><Text size="xs">{!connected && "Последние сохранённые сведения; worker сейчас недоступен. "}{catalogSummary(info)}</Text></details>}
      {info && <Text size="xs">Версия worker: {info.build ? `${info.build.version} · commit ${info.build.sourceCommit || "не указан"}${!info.build.sourceCommit ? " · происхождение исходников не подтверждено" : info.build.sourceDirty ? " · изменённые исходники" : ""} · build ${info.build.buildId}` : "этот выпуск не передаёт сведения о сборке"} · generation {info.generation}</Text>}
      <Text size="xs">Версия desktop: {desktopBuild ? `${desktopBuild.version} · commit ${desktopBuild.sourceCommit || "не указан"}${!desktopBuild.sourceCommit ? " · происхождение исходников не подтверждено" : desktopBuild.sourceDirty ? " · изменённые исходники" : ""} · build ${desktopBuild.buildId}` : "сведения о сборке ещё не получены"}</Text>
    </Alert>
    <Alert color={controllerFresh ? "teal" : "yellow"} title={controllerFresh ? "Controller Debian подключён" : !info ? "Состояние controller пока неизвестно" : controller ? connected && controller.state === "connecting" ? "Подключение к controller Debian…" : "Нет свежего соединения с controller Debian" : "Управление из Debian не настроено"}>
      {controller ? <>
        <Text size="sm">{controller.url} · {controller.controllerNodeId ?? "node controller ещё не получен"}</Text>
        <Text size="xs">Последнее подтверждение: {controller.lastConfirmedAtMs === null ? "ещё не получено" : new Date(controller.lastConfirmedAtMs).toLocaleString()} · session: {controller.sessionId ?? "ещё не создана"}</Text>
        {!connected && <Text size="sm">Worker недоступен; сохранены последние известные данные соединения.</Text>}
        {controller.error && <Text size="sm">{controller.error}</Text>}
        {!controllerFresh && <Text size="sm">{info?.journalInspection ? "Доступна диагностика журнала; состояние прежних процессов не подтверждено." : "Принятая задача принадлежит worker и продолжает выполняться. Автоматического повторного запуска нет."}</Text>}
      </> : <Text size="sm">{info ? "Для связи указать локальный ws:// адрес Debian в worker.controllerUrl. Локальные задачи доступны через worker." : "Ожидаются данные локального worker о соединении с Debian."}</Text>}
    </Alert>
    {info && <AttentionInbox key={`${info.instanceId}:${info.nodeId}`} info={info} connected={connected} onTarget={openAttention} />}
    {info && <HistoryPanel key={`history:${info.instanceId}:${info.nodeId}`} info={info} connected={connected} closing={closing} onOpen={(entry)=>void openHistory(entry)} />}
    {info && <SettingsPanel key={`settings:${info.instanceId}:${info.nodeId}`} info={info} connected={connected} closing={closing} onBlocked={blockSettingsClose} />}
    {closeError && <Alert id="desktop-close-error" color="yellow" title="Окно осталось открытым: закрытие не подтверждено">{closeError}
      <Button mt="sm" variant="light" disabled={closing} onClick={() => void requestClose(closeQuit.current, "recovery", true)}>Закрыть без сохранения</Button>
    </Alert>}
    <CompanionPanel status={companion} preferences={preferences} closing={closing} error={preferencesError} companionError={companionError} preferencesStatus={preferencesStatus} persist={persistPreferences} close={(quit) => void requestClose(quit)} desktopLog={desktopLog} />
    {info?.journalRecoveryRequired && !info.journalInspection && <Alert color="red" title="Ошибка журнала · новые запуски приостановлены">{info.journalRecoveryRequired}<Text mt="xs">Старый статус задачи в журнале не подтверждает, что процесс ещё работает. Восстановите доступ к локальному журналу и перезапустите worker; неизвестная команда не запускается повторно.</Text></Alert>}
    {info?.journalInspection && <Alert id="journal-inspection" color="orange" title="Диагностика восстановления · только чтение">
      <Text>{info.journalInspection.reason}</Text>
      <Text mt="xs">История, output и события внимания доступны. «Выполняется» и «Готово» здесь — сохранённые записи, а не подтверждение работающего процесса. Схема журнала: {info.journalInspection.schemaVersion}.</Text>
      <Text mt="xs">Worker сохраняет блокировку вне базы данных. Обычный перезапуск не разрешает исполнение; не удаляйте journal-recovery.required для продолжения работы.</Text>
      <Text mt="xs">Прежняя история остаётся архивом. Новая пара готовится отдельно; запуск разрешается после проверки прежней области исполнения. Агенты переподключаются явно с новым контрактом запросов.</Text>
      {info.recoveryCoverage && <Text mt="xs">Учёт области исполнения: {info.recoveryCoverage.reason}</Text>}
    </Alert>}
    {info?.resourceRecovery && <Alert id="resource-recovery" color="yellow" title={`Восстановление: ${info.resourceRecovery.unresolvedCount} ресурсов требуют проверки`}>
      <Text size="sm">Остановка прежнего исполнителя не подтверждена. Новые операции в этих проектах ждут освобождения ресурса; прежние команды повторно не запускаются.</Text>
      <Stack mt="xs" gap="xs">{info.resourceRecovery.resources.map((resource) => <div key={`${resource.operationKind}:${resource.operationId}`}>
        <Text size="sm">Проект {resource.projectId} · {resource.operationKind === "import" ? "импорт" : "задача"} <Code>{resource.operationId}</Code></Text>
        <Text size="sm">{resource.reason}</Text>
        {!resource.physicalBinding && <Text size="xs" c="dimmed">Проверяемая принадлежность Windows-процесса не сохранена; одного PID недостаточно.</Text>}
      </div>)}</Stack>
      {info.resourceRecovery.truncated && <Text size="sm" mt="xs">Показаны первые 50 ресурсов.</Text>}
    </Alert>}
    {info && (info.snapshotImportAvailable || info.journalInspection) && <SnapshotWorkbench navigation={importNavigation} info={info} connected={connected} closing={closing} drafts={deliveryDrafts} persist={persistDelivery} onWorkflowSnapshot={info.workflowAvailable ? setWorkflowSnapshot : undefined} />}
    {info && (info.workflowAvailable || info.journalInspection) && <WorkflowWorkbench navigation={workflowNavigation} info={info} connected={connected} closing={closing} profiles={workflowProfiles} persist={persistWorkflow} snapshot={workflowSnapshot} onTask={(id) => { setSelectedId(id); requestAnimationFrame(() => document.getElementById("native-task-detail")?.scrollIntoView({ behavior: "smooth" })); }} />}
    <Card id="native-command" tabIndex={-1} withBorder><Stack>
      <Title order={3}>Запустить команду</Title>
      <Select label="Зарегистрированный проект" value={projectId} onChange={setProjectId} data={info?.projects.map((p) => ({ value: p.id, label: p.id })) ?? []} disabled={submitting || pending !== null || closing || drafts === null} />
      {project && <Text size="xs" c="dimmed">Рабочий корень: {project.workspaceRoot}</Text>}
      {project?.recipeProfile && <Alert color="blue" title={`Общий профиль: ${project.recipeProfile.label}`}><Text size="sm">{project.recipeProfile.description}</Text><Text size="xs">ID: {project.recipeProfile.id}. Изменения конфигурации применяются после ручного перезапуска worker.</Text></Alert>}
      {(info?.commandHandlers?.length ?? 0) > 0 && <Text size="sm">{info?.commandHandlers?.find((handler) => handler.id === (recipeId ? "recipe" : "direct"))?.description}</Text>}
      {recipeId && <Text data-testid="recipe-origin" size="sm">Источник рецепта: {project?.recipes.find((r) => r.id === recipeId)?.origin?.profileLabel ?? "локальная конфигурация проекта"}. Ниже раскрыты параметры; cwd задаётся относительно выбранного корня или snapshot.</Text>}
      {info?.snapshotImportAvailable && <TextInput label="Snapshot ID для сборки" description="Пустое поле использует корень проекта. Для snapshot worker проверит исходники перед запуском; cwd задаётся внутри поколения." value={form.snapshotId ?? ""} onChange={(e) => updateForm({snapshotId:e.currentTarget.value || undefined})} disabled={pending !== null || !formReady || closing} />}
      <Select label="Способ запуска" value={recipeId ? `recipe:${recipeId}` : "direct"} onChange={(id) => updateForm({ recipeId: id?.startsWith("recipe:") ? id.slice(7) : null })} data={[
        ...(project?.allowDirectCommands ? [{ value: "direct", label: "Прямая команда" }] : []),
        ...(project?.recipes.map((r) => ({ value: `recipe:${r.id}`, label: `Рецепт: ${r.id}` })) ?? []),
      ]} disabled={submitting || pending !== null || !formReady || closing} />
      {recipeId ? <Code block>{JSON.stringify(project?.recipes.find((r) => r.id === recipeId)?.command, null, 2)}</Code> : <>
        <TextInput label="Executable" placeholder="powershell.exe или полный путь к .exe" value={executable} onChange={(e) => updateForm({ executable: e.currentTarget.value })} disabled={pending !== null || !formReady || closing} />
        <Textarea data-testid="command-arguments" label="Аргументы — JSON-массив строк" value={argv} onChange={(e) => updateForm({ argumentsJson: e.currentTarget.value })} autosize minRows={2} disabled={pending !== null || !formReady || closing} />
        <TextInput label="Рабочая папка относительно корня проекта" value={cwd} onChange={(e) => updateForm({ workingDirectory: e.currentTarget.value })} disabled={pending !== null || !formReady || closing} />
      </>}
      <NumberInput label="Timeout, секунд" value={timeout} onChange={(value) => updateForm({ timeoutSeconds: String(value) })} min={0.1} max={86400} disabled={pending !== null || !formReady || closing} />
      <Text size="xs" c="dimmed">{saveStatus}. Восстановление формы не запускает команду.</Text>
      {saveError && <Alert color="yellow" title="Не удалось сохранить поля команды">{saveError}</Alert>}
      {error && connected && <Alert color="red" title="Запрос не завершён">{error}</Alert>}
      {pending && !submitting && <Alert color="yellow" title="Приём задачи не подтверждён">
        Worker мог сохранить задачу до потери ответа. Повтор отправит прежние параметры и тот же ключ, чтобы получить ту же задачу.
        <Button mt="sm" disabled={!executionReady} onClick={() => void send(pending.request.request, pending)}>Проверить приём — повторить тот же запрос</Button>
      </Alert>}
      <Button onClick={() => void submit()} loading={submitting} disabled={!executionReady || pending !== null || !project || !formReady || closing || (!recipeId && !project.allowDirectCommands)}>Передать задачу worker</Button>
    </Stack></Card>
    <Select label="Последние 10 задач" value={selectedId} onChange={setSelectedId} searchable data={tasks.map((t) => ({ value: t.taskId, label: `${t.projectId} · ${statuses[t.status].label} · ${t.taskId}` }))} />
    {selectedId ? <div id="native-task-detail" tabIndex={-1}><TaskSession key={selectedId} id={selectedId} readOnly={!!info?.journalInspection} requestContext={info?.fencedRequests?.context} /></div> : <Text c="dimmed">Задач пока нет. Выберите проект и передайте команду worker.</Text>}
  </Stack>;
}
