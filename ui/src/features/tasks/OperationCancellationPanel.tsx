import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Card, Group, Stack, Text } from "@mantine/core";
import type { HistoryKind, OperationCancellation, WorkerInfo } from "../../runtime/contracts";
import { interpretCatalog } from "../../runtime/discovery";

export function OperationCancellationPanel({ info, connected, kind, operationId, updateKey = "" }: { info: WorkerInfo; connected: boolean; kind: HistoryKind; operationId: string; updateKey?: string }) {
  const [page, setPage] = useState<OperationCancellation | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const catalog = interpretCatalog(info);
  const supported = catalog.kind === "known" && ["winbridge.extensions", "winbridge.operation-cancellation"].every((id) => catalog.catalog.supported.some((c) => c.id === id && c.version === 1));
  const context = info.fencedRequests?.context;
  const scope = `${context?.instanceId}:${context?.journalIncarnation}:${info.generation}:${info.build?.buildId}:${kind}:${operationId}`;
  useEffect(() => {
    if (!supported || !context || !connected) { setBusy(false); return; }
    let disposed = false;
    setBusy(true); setError(null);
    void invoke<OperationCancellation>("operation_cancellation", { request: { context: { ...context }, query: { kind, operationId } } }).then((value) => {
      if (disposed) return;
      if (value.schemaVersion !== 1 || value.workerGeneration !== info.generation || value.provider.buildId !== info.build?.buildId
        || value.reference.context.instanceId !== context.instanceId || value.reference.context.journalIncarnation !== context.journalIncarnation
        || value.reference.kind !== kind || value.reference.operationId !== operationId) throw new Error("Область или операция изменились; сведения об отмене не перенесены.");
      setPage(value);
    }).catch((reason) => { if (!disposed) setError(String(reason)); }).finally(() => { if (!disposed) setBusy(false); });
    return () => { disposed = true; };
  }, [scope, connected, supported, refresh, updateKey]);
  const current = page?.reference.context.instanceId === context?.instanceId && page?.reference.context.journalIncarnation === context?.journalIncarnation
    && page?.workerGeneration === info.generation && page?.provider.buildId === info.build?.buildId && page?.reference.kind === kind && page?.reference.operationId === operationId ? page : null;
  return <Card withBorder data-testid="operation-cancellation"><Stack gap="xs">
    <Group justify="space-between"><Text fw={600}>Инициатор отмены</Text><Button size="compact-xs" variant="light" loading={busy} disabled={!connected || !context || !supported || busy} onClick={() => setRefresh((n) => n + 1)}>Обновить сведения об отмене</Button></Group>
    {!supported && <Text size="sm">Эта версия worker не предоставляет сведения об инициаторе отмены.</Text>}
    {!connected && <Text size="sm" c="dimmed">Worker недоступен. Сохранённые сведения сейчас не подтверждены.</Text>}
    {error && <Alert color="yellow">Не удалось прочитать инициатора отмены: {error}</Alert>}
    {current && (current.intent ? <>
      <Text size="sm">{current.intent.actor === null ? "Источник запроса не указан; возможно внутреннее действие worker." : current.intent.actor.kind === "local" ? "Локальный вход worker" : `Controller ${current.intent.actor.controllerId} · ${current.intent.actor.controllerNodeId}`}</Text>
      <Text size="xs">Первый принятый запрос отмены: {new Date(current.intent.requestedAtMs).toLocaleString()}.</Text>
      {current.intent.inheritedFrom && <Text size="xs">Отмена передана из {current.intent.inheritedFrom.kind === "workflow" ? "workflow" : "приложения"} {current.intent.inheritedFrom.operationId}.</Text>}
      <Text size="xs" c="dimmed">Записанное намерение отмены не подтверждает остановку процесса. Фактический результат смотрите в статусе задачи.</Text>
    </> : <Text size="sm">Сохранённой записи об инициаторе отмены нет. Это не доказывает, что отмена никогда не запрашивалась.</Text>)}
    {current && <Text size="xs" c="dimmed">{!connected || error ? "Последний полученный срез" : "Срез worker"}: {new Date(current.observedAtMs).toLocaleString()}.</Text>}
    {supported && !current && <Text size="sm">{busy ? "Читаются сведения об отмене…" : "Сведения ещё не получены."}</Text>}
  </Stack></Card>;
}
