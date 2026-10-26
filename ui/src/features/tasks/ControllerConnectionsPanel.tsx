import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Badge, Button, Card, Code, Group, Stack, Text, Title } from "@mantine/core";
import type { ControllerConnections, NetworkRpcObservation, WorkerInfo } from "../../runtime/contracts";
import { interpretCatalog } from "../../runtime/discovery";

export function controllerConnectionsSupported(info: WorkerInfo): boolean {
  const catalog = interpretCatalog(info);
  return catalog.kind === "known" && ["winbridge.extensions", "winbridge.controller-connections"].every((id) =>
    catalog.catalog.supported.some((ref) => ref.id === id && ref.version === 1));
}
const time = (ms: number | null) => ms === null ? "ещё не было" : new Date(ms).toLocaleString();
function Rpc({ value }: { value: NetworkRpcObservation }) {
  return <Text size="xs">Обработчики RPC: {value.inFlight} / {value.limit} · принято {value.admitted} · завершено {value.completed} · отклонено до обработки {value.rejected}</Text>;
}

export function ControllerConnectionsPanel({ info, connected }: { info: WorkerInfo; connected: boolean }) {
  const [snapshot, setSnapshot] = useState<ControllerConnections | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const supported = controllerConnectionsSupported(info);
  const context = info.fencedRequests?.context;
  const scope = `${context?.instanceId}:${context?.journalIncarnation}:${info.generation}:${info.build?.buildId}`;
  useEffect(() => {
    if (!supported || !connected || !context) { setBusy(false); return; }
    let disposed = false;
    setBusy(true); setError(null);
    void invoke<ControllerConnections>("controller_connections", { request: { context: { ...context } } }).then((page) => {
      if (disposed) return;
      if (page.schemaVersion !== 1 || page.workerGeneration !== info.generation
        || page.provider.buildId !== info.build?.buildId || page.context.instanceId !== context.instanceId
        || page.context.journalIncarnation !== context.journalIncarnation) throw new Error("Область или поколение worker изменились. Обновите сведения worker; старый срез не перенесён.");
      setSnapshot(page);
    }).catch((reason) => { if (!disposed) setError(String(reason)); }).finally(() => { if (!disposed) setBusy(false); });
    return () => { disposed = true; };
  }, [scope, connected, supported, refresh]);
  if (!supported) return null;
  const page = snapshot?.context.instanceId === context?.instanceId
    && snapshot?.context.journalIncarnation === context?.journalIncarnation
    && snapshot?.workerGeneration === info.generation && snapshot?.provider.buildId === info.build?.buildId ? snapshot : null;
  const stale = !connected || !!error;
  return <Card withBorder data-testid="controller-connections"><Stack gap="xs">
    <Group justify="space-between"><Title order={3}>Соединения с Debian</Title>
      <Button onClick={() => setRefresh((n) => n + 1)} disabled={!connected || !context || busy} loading={busy}>Обновить соединения</Button></Group>
    <Text size="sm">Каждый controller подключается отдельно. Очередь и история задач общие; разрыв связи не отменяет принятое задание.</Text>
    {!connected && <Alert color="yellow">Нет свежей связи с worker. Сохранённый срез не подтверждает текущее состояние соединений.</Alert>}
    {error && <Alert color="red" title="Соединения не проверены">{error} Последний полученный срез сохранён.</Alert>}
    {!page && <Text size="sm">{busy ? "Читается состояние соединений…" : "Срез соединений ещё не получен."}</Text>}
    {page && <>
      <Text size="xs">{stale ? "Сохранённый срез" : "Срез worker"}: {time(page.observedAtMs)}. Кнопка «Обновить» получает новое наблюдение.</Text>
      <Rpc value={page.rpc} />
      <Text size="xs" c="dimmed">Счётчики сетевых RPC ведутся с запуска worker. Это обработчики запросов, а не число заданий. После тайм-аута слот занят до фактического окончания обработчика.</Text>
      {page.connections.length === 0 && <Text size="sm">Управление из Debian не настроено. Локальные задачи доступны через worker.</Text>}
      {page.connections.map((link) => <Card withBorder padding="sm" key={link.id} data-testid={`controller-link-${link.id}`}>
        <Stack gap={4}>
          <Group><Text fw={600}>{link.id}</Text>{page.primary === link.id && <Badge>Primary · локальная сводка</Badge>}
            <Badge color={stale ? "gray" : link.connection.state === "connected" ? "teal" : "yellow"}>На момент среза: {({ connected: "подключён", connecting: "подключается", disconnected: "отключён" } as const)[link.connection.state]}</Badge></Group>
          <Code>{link.connection.url}</Code>
          <Text size="xs">Попыток подключения: {link.attempts}. Последнее подтверждение: {time(link.connection.lastConfirmedAtMs)}.</Text>
          <Text size="xs">Подключение: {time(link.lastConnectedAtMs)} · разрыв: {time(link.lastDisconnectedAtMs)}</Text>
          <Text size="xs">Controller: {link.connection.controllerNodeId ?? "не получен"} · generation {link.connection.controllerGeneration ?? "не получена"} · session {link.connection.sessionId ?? "не создана"}</Text>
          <Rpc value={link.rpc} />
          {link.connection.error && <Text size="sm" c="red">Причина: {link.connection.error}</Text>}
          {link.lastError && link.lastError !== link.connection.error && <Text size="sm">Последняя ошибка предыдущего соединения: {link.lastError}</Text>}
        </Stack>
      </Card>)}
    </>}
  </Stack></Card>;
}
