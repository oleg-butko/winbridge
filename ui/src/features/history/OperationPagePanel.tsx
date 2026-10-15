import { useEffect, useState } from "react";
import { Alert, Badge, Button, Card, Group, Select, Stack, Text, TextInput, Title } from "@mantine/core";
import { invoke } from "@tauri-apps/api/core";
import type { HistoryEntry, HistoryKind, OperationObservation, OperationPage, OperationPageRequest, WorkerInfo } from "../../runtime/contracts";
import { interpretCatalog } from "../../runtime/discovery";
import { statuses } from "../tasks/taskStatus";

const kinds: Record<HistoryKind, string> = { task: "Задача", import: "Импорт", workflow: "Workflow", application: "Приложение" };
const applications = { starting: "Запускается", ready: "Готово", stopping: "Останавливается", stopped: "Остановлено", exited: "Завершилось", failed: "Ошибка", indeterminate: "Исход неизвестен" };
export function OperationPagePanel({ info, connected, onOpen }: { info: WorkerInfo; connected: boolean; onOpen: (entry: HistoryEntry) => void }) {
  const [project, setProject] = useState("");
  const [kind, setKind] = useState<HistoryKind | null>(null);
  const [search, setSearch] = useState("");
  const [request, setRequest] = useState<OperationPageRequest | null>(null);
  const [page, setPage] = useState<OperationPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmed, setConfirmed] = useState<number | null>(null);
  const catalog = interpretCatalog(info);
  const supported = catalog.kind === "known" && catalog.catalog.supported.some((ref) => ref.id === "winbridge.operation-read-model" && ref.version === 1);
  const context = info.fencedRequests?.context;
  useEffect(() => {
    if (!request || !connected || !supported) { setBusy(false); return; }
    let disposed = false;
    setBusy(true); setError(null);
    void invoke<OperationPage>("operation_page", { request }).then((value) => {
      if (!disposed) { setPage(value); setConfirmed(Date.now()); }
    }).catch((reason) => { if (!disposed) setError(String(reason)); }).finally(() => { if (!disposed) setBusy(false); });
    return () => { disposed = true; };
  }, [request, connected, supported]);
  function refresh() {
    if (!context) return;
    setPage(null); setConfirmed(null);
    setRequest({ context: { ...context }, query: { projectId: project || null, kind, search, limit: 25, cursor: null } });
  }
  function open(record: OperationObservation) {
    if (!context || record.reference.context.instanceId !== context.instanceId || record.reference.context.journalIncarnation !== context.journalIncarnation) return;
    const state = record.state.status;
    const workflow = record.related.find((ref) => ref.kind === "workflow");
    onOpen({ operationId: record.reference.operationId, kind: record.reference.kind, projectId: record.projectId, label: record.label, status: state,
      createdAtMs: record.acceptedAtMs, finishedAtMs: record.finishedAtMs, workflowId: workflow?.operationId ?? null });
  }
  return <Card withBorder id="operation-pages" data-testid="operation-pages"><Stack gap="xs">
    <Title order={3}>Общий журнал операций</Title>
    <Text size="sm">Статусы, причины и ресурсы каждой страницы читаются вместе. Новые операции появятся после «Обновить»; следующие страницы сохраняют исходную выборку. Вывод открывается в подробностях операции.</Text>
    {!supported && <Text size="sm">Worker пока не поддерживает согласованные страницы журнала.</Text>}
    <Group align="end"><TextInput label="Проект (пусто — все)" value={project} onChange={(e) => setProject(e.currentTarget.value)} maxLength={80} />
      <Select label="Вид операции" clearable data={Object.entries(kinds).map(([value, label]) => ({ value, label }))} value={kind} onChange={(value) => setKind(value as HistoryKind | null)} />
      <TextInput label="Поиск по ID, проекту и названию" value={search} onChange={(e) => setSearch(e.currentTarget.value)} maxLength={200} />
      <Button onClick={refresh} disabled={!connected || !supported || !context || busy}>Обновить общий журнал</Button></Group>
    {!connected && <Alert color="yellow">Нет свежей связи с worker. Сохранённая страница не подтверждает текущее состояние.</Alert>}
    {request && (project !== (request.query.projectId ?? "") || kind !== request.query.kind || search !== request.query.search) && <Text size="sm">Фильтры изменены. Нажмите «Обновить общий журнал», чтобы применить их.</Text>}
    {error && <Alert color="red" title="Страница не получена">{error}<Text size="xs">Исходный курсор сохранён. При смене области или фильтров начните новый просмотр кнопкой «Обновить».</Text></Alert>}
    {page && <>
      <Text size="xs">Область: {page.context.instanceId} / {page.context.journalIncarnation}. Срез: {new Date(page.observedAtMs).toLocaleString()}. Получено: {confirmed ? new Date(confirmed).toLocaleString() : "—"}. {page.readOnly && "Журнал открыт в режиме чтения."}</Text>
      {page.records.length === 0 && <Text size="sm">Операций по этой выборке нет.</Text>}
      {page.records.map((record) => <Card key={`${record.reference.kind}:${record.reference.operationId}`} withBorder padding="xs">
        <Group justify="space-between"><Text fw={600}>{record.projectId} · {kinds[record.reference.kind]} · {record.label}</Text><Badge>{record.state.family === "execution" ? statuses[record.state.status].label : applications[record.state.status]}</Badge></Group>
        <Text size="xs">ID {record.reference.operationId} · revision {record.revision} · событие {record.lastEventSequence ?? "нет"}</Text>
        {record.reason && <Text size="sm">{record.reason}</Text>}
        {record.resources.map((resource) => <Text key={resource.leaseId} size="xs">Ресурс проекта: {resource.scope === "project_mutation" ? "изменение проекта" : "жизненный цикл приложения"}; {resource.state === "held" ? "занят" : "освобождение не подтверждено"}; владелец {resource.owner.operationId}. {resource.reason}</Text>)}
        {(record.textTruncated || record.resourcesTruncated) && <Text size="xs">Сводка ограничена; полные сведения доступны в деталях.</Text>}
        <Button size="compact-xs" variant="subtle" onClick={() => open(record)} disabled={!connected}>Открыть операцию {record.reference.operationId}</Button>
      </Card>)}
      <Button variant="light" disabled={!page.nextCursor || !connected || busy || !request} onClick={() => { if (request && page.nextCursor) setRequest({ ...request, query: { ...request.query, cursor: page.nextCursor } }); }}>Следующая страница общего журнала</Button>
    </>}
  </Stack></Card>;
}
