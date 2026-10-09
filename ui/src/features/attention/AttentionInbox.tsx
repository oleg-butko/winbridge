import { useEffect, useState } from "react";
import { Alert, Badge, Button, Card, Checkbox, Group, ScrollArea, Stack, Text, Title } from "@mantine/core";
import type { AttentionEventPage, AttentionTarget, WorkerInfo } from "../../runtime/contracts";
import { expectResult, workerCall } from "../../runtime/adapter";

const severity = {
  info: { label: "Сообщение", color: "blue" },
  warning: { label: "Требует проверки", color: "yellow" },
  error: { label: "Ошибка", color: "red" },
};

export function AttentionInbox({ info, connected, onTarget }: {
  info: WorkerInfo; connected: boolean; onTarget: (target: AttentionTarget) => void;
}) {
  const [page, setPage] = useState<AttentionEventPage | null>(null);
  const [after, setAfter] = useState(0);
  const [history, setHistory] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ackError, setAckError] = useState<{ id: string; message: string } | null>(null);
  const [acknowledging, setAcknowledging] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const scope = `${info.instanceId}:${info.nodeId}`;
  useEffect(() => { setPage(null); setAfter(0); setError(null); setAckError(null); }, [scope, info.generation]);
  useEffect(() => {
    if (!info.attentionAvailable || !connected) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        let normalized: AttentionEventPage;
        if (info.attentionTargetsAvailable) normalized = expectResult(await workerCall({ method: "list_attention_events", afterSequence: after, pendingOnly: !history }), "attention_events").page;
        else {
          const legacy = expectResult(await workerCall({ method: "list_attention", afterSequence: after, pendingOnly: !history }), "attention_list").page;
          normalized = { ...legacy, records: legacy.records.map(({taskId, ...r}) => ({...r, target:{kind:"task" as const, taskId}})) };
        }
        if (!disposed) { setPage(normalized); setError(null); }
      } catch (error) { if (!disposed) setError(String(error)); }
      if (!disposed) timer = setTimeout(() => void poll(), 1500);
    }
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, [scope, info.generation, info.attentionAvailable, info.attentionTargetsAvailable, connected, after, history, revision]);
  async function acknowledge(id: string) {
    setAcknowledging(id); setAckError(null);
    try {
      if (info.attentionTargetsAvailable) expectResult(await workerCall({ method: "acknowledge_attention_event", attentionId: id }), "attention_event");
      else expectResult(await workerCall({ method: "acknowledge_attention", attentionId: id }), "attention");
      setRevision((value) => value + 1);
    } catch (error) { setAckError({ id, message: `Ответ на подтверждение не получен: ${String(error)}. Повтор использует то же событие и сохраняет время первого подтверждения.` }); }
    finally { setAcknowledging(null); }
  }
  return <Card id="native-attention" tabIndex={-1} withBorder><Stack>
    <Group justify="space-between"><Title order={3}>Требует внимания</Title>
      <Badge color={page?.pendingCount ? "orange" : "gray"}>{!info.attentionAvailable ? "Требуется обновление" : page ? `${page.pendingCount} неподтверждённых` : "Читается журнал"}</Badge>
    </Group>
    <Text size="sm">События хранятся у worker при закрытом окне. Открытие страницы не подтверждает просмотр; нажмите «Прочитано» отдельно.</Text>
    {!info.attentionAvailable ? <Alert color="yellow">Журнал внимания требует следующего общего обновления worker и desktop.</Alert> : <>
      {(!connected || error) && <Alert color="yellow" title="Нет свежего подтверждения журнала внимания">{error ?? "Worker недоступен; показаны последние известные события."}</Alert>}
      {ackError && <Alert color="yellow">{ackError.message}<Button mt="sm" variant="light" loading={acknowledging === ackError.id} disabled={!connected || acknowledging !== null} onClick={() => void acknowledge(ackError.id)}>Проверить подтверждение</Button></Alert>}
      <Checkbox label="Показать также подтверждённые события" checked={history} onChange={(event) => { setHistory(event.currentTarget.checked); setAfter(0); setPage(null); }} />
      <ScrollArea.Autosize mah={360}><Stack gap="sm">
        {page?.records.map((record) => <Card key={record.attentionId} withBorder data-attention-id={record.attentionId}>
          <Group><Badge color={severity[record.severity].color}>{severity[record.severity].label}</Badge><Text fw={600}>{record.projectId}</Text></Group>
          <Text mt="xs">{record.message}</Text>
          <Text size="xs" c="dimmed">{new Date(record.createdAtMs).toLocaleString()} · {({task_outcome:"Результат задачи",workflow_outcome:"Результат workflow",application_outcome:"Состояние приложения",agent_request:"Запрос агента"} as Record<string,string>)[record.origin] ?? record.origin} · {record.target.kind === "task" ? `Task ID: ${record.target.taskId}` : record.target.kind === "workflow" ? `Workflow ID: ${record.target.workflowId}` : `Application ID: ${record.target.applicationId}`}</Text>
          <Group mt="sm"><Button size="xs" variant="light" onClick={() => onTarget(record.target)}>{record.target.kind === "task" ? "Открыть задачу" : record.target.kind === "workflow" ? "Открыть workflow" : "Открыть приложение"}</Button>
            {record.acknowledgedAtMs === null ? <Button size="xs" data-testid={`ack-attention-${record.attentionId}`} loading={acknowledging === record.attentionId} disabled={!connected || acknowledging !== null} onClick={() => void acknowledge(record.attentionId)}>Прочитано</Button>
              : <Text size="xs">Прочитано: {new Date(record.acknowledgedAtMs).toLocaleString()}</Text>}
          </Group>
        </Card>)}
      </Stack></ScrollArea.Autosize>
      {page && !page.records.length && <Text c="dimmed">{after ? "На этой странице больше нет событий. Вернитесь к первым событиям." : history ? "Журнал внимания пока пуст." : "Нет событий, ожидающих подтверждения."}</Text>}
      <Group><Button variant="subtle" disabled={after === 0} onClick={() => { setAfter(0); setPage(null); }}>Первые события</Button>
        <Button variant="light" disabled={!page?.hasMore || !connected} onClick={() => { if (page) { setAfter(page.nextSequence); setPage(null); } }}>Следующие 50 событий</Button>
      </Group>
    </>}
  </Stack></Card>;
}
