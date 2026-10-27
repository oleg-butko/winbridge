import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Card, Group, Stack, Text } from "@mantine/core";
import type { HistoryKind, OperationAttribution, WorkerInfo } from "../../runtime/contracts";
import { interpretCatalog } from "../../runtime/discovery";

export function OperationAttributionPanel({ info, connected, kind, operationId }: { info: WorkerInfo; connected: boolean; kind: HistoryKind; operationId: string }) {
  const [page, setPage] = useState<OperationAttribution | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const catalog = interpretCatalog(info);
  const supported = catalog.kind === "known" && ["winbridge.extensions", "winbridge.operation-attribution"].every((id) => catalog.catalog.supported.some((c) => c.id === id && c.version === 1));
  const context = info.fencedRequests?.context;
  const scope = `${context?.instanceId}:${context?.journalIncarnation}:${info.generation}:${info.build?.buildId}:${kind}:${operationId}`;
  useEffect(() => {
    if (!connected || !supported || !context) { setBusy(false); return; }
    let disposed = false;
    setBusy(true); setError(null);
    void invoke<OperationAttribution>("operation_attribution", { request: { context: { ...context }, query: { kind, operationId } } }).then((value) => {
      if (disposed) return;
      if (value.schemaVersion !== 1 || value.workerGeneration !== info.generation || value.provider.buildId !== info.build?.buildId
        || value.reference.context.instanceId !== context.instanceId || value.reference.context.journalIncarnation !== context.journalIncarnation
        || value.reference.kind !== kind || value.reference.operationId !== operationId) throw new Error("Область или задача изменились; сведения об авторе не перенесены.");
      setPage(value);
    }).catch((reason) => { if (!disposed) setError(String(reason)); }).finally(() => { if (!disposed) setBusy(false); });
    return () => { disposed = true; };
  }, [scope, connected, supported, refresh]);
  const current = page?.reference.context.instanceId === context?.instanceId && page?.reference.context.journalIncarnation === context?.journalIncarnation
    && page?.workerGeneration === info.generation && page?.provider.buildId === info.build?.buildId && page?.reference.kind === kind && page?.reference.operationId === operationId ? page : null;
  return <Card withBorder data-testid="operation-attribution"><Stack gap="xs">
    <Group justify="space-between"><Text fw={600}>Источник первоначального запроса</Text><Button size="compact-xs" variant="light" disabled={!connected || !supported || !context || busy} loading={busy} onClick={() => setRefresh((n) => n + 1)}>Обновить автора</Button></Group>
    {!supported && <Text size="sm">Эта версия worker не предоставляет сведения об авторе.</Text>}
    {!connected && <Text size="sm" c="dimmed">Worker недоступен. Сохранённые сведения сейчас не подтверждены.</Text>}
    {error && <Alert color="yellow">Не удалось прочитать автора: {error}</Alert>}
    {current && <>
      {current.creator === null ? <Text size="sm">Автор в журнале неизвестен. Старым записям он не назначается при повторе запроса.</Text>
        : <><Text size="sm">{current.creator.kind === "local" ? "Локальный вход worker" : `Controller ${current.creator.controllerId} · ${current.creator.controllerNodeId}`}</Text>
          <details><summary>Подключение при создании</summary><Text size="xs">Worker generation: {current.creator.workerGeneration}</Text>
            {current.creator.kind === "controller" && <Text size="xs">Controller generation: {current.creator.controllerGeneration} · session {current.creator.sessionId}</Text>}</details></>}
      {current.inheritedFrom && <Text size="xs">Источник унаследован от workflow {current.inheritedFrom.operationId}.</Text>}
      <Text size="xs" c="dimmed">Повтор запроса через другой controller сохраняет исходного автора. Прочитано: {new Date(current.observedAtMs).toLocaleString()}.</Text>
    </>}
    {supported && !current && <Text size="sm">{busy ? "Читаются сведения об авторе…" : "Сведения ещё не получены."}</Text>}
  </Stack></Card>;
}
