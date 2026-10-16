import { useState } from "react";
import { Alert, Badge, Button, Group, Stack, Text } from "@mantine/core";
import { invoke } from "@tauri-apps/api/core";
import type { QueueControlObservation, QueueControlRequest, QueueControlResponse, QueueMode, WorkerInfo } from "../../runtime/contracts";
import { interpretQueueControl } from "../../runtime/discovery";
import { nativeRuntime } from "../../runtime/adapter";
import { readQueueChange, validateQueueResponse } from "../../runtime/queueControl";
import type { QueueChange as Change } from "../../runtime/queueControl";

type Send = (request: QueueControlRequest) => Promise<QueueControlResponse>;
const localSend: Send = (request) => invoke("queue_control", { request });
const label: Record<QueueMode, string> = { running: "Приём и запуск разрешены", admission_paused: "Приём приостановлен", dispatch_paused: "Запуск приостановлен", paused: "Приём и запуск приостановлены" };
const pausedAdmission = (mode: QueueMode) => mode === "admission_paused" || mode === "paused";
const pausedDispatch = (mode: QueueMode) => mode === "dispatch_paused" || mode === "paused";
const nextMode = (admission: boolean, dispatch: boolean): QueueMode => admission ? (dispatch ? "paused" : "admission_paused") : (dispatch ? "dispatch_paused" : "running");

export function QueueControlPanel({ info, connected, send = localSend }: { info: WorkerInfo; connected: boolean; send?: Send }) {
  const current = interpretQueueControl(info);
  const storageKey = `winbridge.queue-command.v1:${info.instanceId}:${info.nodeId}`;
  const [restored] = useState(() => {
    try {
      const bytes = localStorage.getItem(storageKey);
      if (bytes === null) return { request: null, error: "" };
      const request = readQueueChange(JSON.parse(bytes));
      if (!request) throw new Error("Сохранённая команда имеет неизвестный формат; исходная запись сохранена.");
      return { request, error: "" };
    } catch (e) { return { request: null, error: String(e) }; }
  });
  const [local, setLocal] = useState<QueueControlObservation | null>(null);
  const [pending, setPending] = useState<Change | null>(restored.request);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(restored.error);
  const [message, setMessage] = useState("");
  const matches = local && current && local.context.instanceId === current.context.instanceId && local.context.journalIncarnation === current.context.journalIncarnation
    && (["version", "sourceCommit", "sourceDirty", "sourceManifestSha256", "buildId"] as const).every((key) => local.provider[key] === current.provider[key]);
  const view = matches && BigInt(local.revision) > BigInt(current.revision) ? local : current;
  if (!view) return <Alert title="Управление очередью" color="yellow">Контракт очереди не подтверждён этим worker. Управление недоступно.
    {pending && <Text>Сохранённая команда {pending.command.commandId} не повторяется автоматически.</Text>}</Alert>;
  const interactive = send !== localSend || nativeRuntime;
  const samePendingScope = !pending || (pending.context.instanceId === view.context.instanceId && pending.context.journalIncarnation === view.context.journalIncarnation);
  const disabled = !interactive || !connected || view.readOnly || busy || pending !== null || !!restored.error;
  function clearPending() {
    try { localStorage.removeItem(storageKey); setPending(null); }
    catch (e) { setError(`Не удалось убрать сохранённую команду после ответа worker: ${String(e)}. Квитанция и параметры команды сохранены для проверки.`); }
  }
  async function dispatch(request: QueueControlRequest, original: Change) {
    setBusy(true); setError(""); setMessage("");
    try {
      const reply = await send(request);
      validateQueueResponse(reply, request, info);
      if (reply.result.kind === "unconfirmed") throw new Error(reply.result.message);
      if (reply.result.kind === "rejected") {
        setError(`Запрос отклонён: ${reply.result.code}. ${reply.result.message}`);
        if (request.command.kind === "set" && ["revision_conflict", "invalid_request", "key_conflict"].includes(reply.result.code)) clearPending();
        return;
      }
      setLocal(reply.result.observation);
      const receipt = reply.result.receipt;
      if (!receipt) { setMessage("Квитанция ещё не найдена. Доступен повтор только сохранённой команды."); return; }
      if (receipt.commandId !== original.command.commandId || receipt.fromRevision !== original.command.expectedRevision || receipt.mode !== original.command.mode) {
        throw new Error("Квитанция принадлежит другим параметрам команды; исходная команда сохранена.");
      }
      setMessage(`Команда подтверждена квитанцией ревизии ${receipt.toRevision}. Текущая ревизия: ${reply.result.observation.revision}.`);
      clearPending();
    } catch (e) { setError(`Результат не подтверждён: ${String(e)}. Автоматического повтора нет.`); }
    finally { setBusy(false); }
  }
  function change(mode: QueueMode) {
    if (!view || disabled) return;
    const request: Change = { queueControlVersion: 1, context: view.context, command: { kind: "set", commandId: crypto.randomUUID(), expectedRevision: view.revision, mode } };
    try {
      const bytes = JSON.stringify(request); localStorage.setItem(storageKey, bytes);
      if (localStorage.getItem(storageKey) !== bytes) throw new Error("Запись команды не подтверждена");
    } catch (e) { setError(`Команда не отправлена: не удалось сохранить её для восстановления. ${String(e)}`); return; }
    setPending(request); void dispatch(request, request);
  }
  return <Alert data-testid="queue-control" title="Очередь запросов" color={!connected || view.mode !== "running" || pending ? "yellow" : "blue"}><Stack gap="xs">
    <Group><Badge>{label[view.mode]}</Badge><Text size="sm">Незавершённых запросов: {view.pendingRoots}{view.policy ? ` / ${view.policy.maxPendingRoots}` : "; общий лимит не задан"}.</Text></Group>
    <Text size="sm">Task, import и workflow учитываются по одному разу. Дочерние шаги workflow повторно лимит не занимают. При паузе запуска работы, чей запуск уже начат, продолжаются; отмена и чтение результатов доступны.</Text>
    {view.policy?.maxPendingRootsPerProject != null && <Text size="sm">Лимит на проект: {view.policy.maxPendingRootsPerProject}.</Text>}
    {view.projects.map((p) => <Text size="xs" key={p.projectId}>{p.projectId}: {p.pendingRoots} незавершённых запросов.</Text>)}
    {view.blockers.map((b, i) => <Text size="sm" key={i}>{b.code === "admission_paused" ? "Новые запросы отклоняются до подтверждения приёма; повтор уже принятого ключа работает." : b.code === "dispatch_paused" ? "Новые исполнители ждут возобновления запуска." : b.code === "global_limit" ? `Достигнут общий лимит ${b.limit}.` : `Достигнут лимит проекта ${b.projectId}: ${b.limit}.`}</Text>)}
    <Group>
      <Button size="xs" disabled={disabled} onClick={() => change(nextMode(!pausedAdmission(view.mode), pausedDispatch(view.mode)))}>{pausedAdmission(view.mode) ? "Возобновить приём" : "Приостановить приём"}</Button>
      <Button size="xs" disabled={disabled} onClick={() => change(nextMode(pausedAdmission(view.mode), !pausedDispatch(view.mode)))}>{pausedDispatch(view.mode) ? "Возобновить запуск" : "Приостановить запуск"}</Button>
    </Group>
    <Text size="xs">Ревизия {view.revision}; последнее изменение {view.changedAtMs ? new Date(view.changedAtMs).toLocaleString() : "ещё не выполнялось"}. Срез {new Date(view.observedAtMs).toLocaleString()} · {view.context.instanceId} / {view.context.journalIncarnation}.</Text>
    {!connected && <Text fw={600}>Нет свежей связи: показан последний срез, управление недоступно.</Text>}
    {view.readOnly && <Text fw={600}>Журнал только для чтения. Изменение очереди запрещено.</Text>}
    {!interactive && <Text size="sm">Управление доступно в desktop.</Text>}
    {error && <Alert color="yellow" title="Состояние команды">{error}</Alert>}
    {message && <Text size="sm">{message}</Text>}
    {pending && <Stack gap="xs"><Text size="xs">Сохранена команда {pending.command.commandId}, исходная ревизия {pending.command.expectedRevision}, область {pending.context.instanceId} / {pending.context.journalIncarnation}.</Text>
      {!samePendingScope && <Text>Область журнала изменилась. Сохранённая команда не переносится в новую область.</Text>}
      <Group><Button size="xs" disabled={!interactive || !connected || busy || !samePendingScope} onClick={() => void dispatch({ queueControlVersion: 1, context: pending.context, command: { kind: "observe", commandId: pending.command.commandId } }, pending)}>Проверить квитанцию</Button>
        <Button size="xs" disabled={!interactive || !connected || busy || view.readOnly || !samePendingScope} onClick={() => void dispatch(pending, pending)}>Повторить исходную команду</Button></Group>
      {!samePendingScope && <Button size="xs" onClick={() => { try { clearPending(); } catch (e) { setError(String(e)); } }}>Убрать сообщение старой области</Button>}
    </Stack>}
  </Stack></Alert>;
}
